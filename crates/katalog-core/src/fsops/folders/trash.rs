//! Web "move a folder to the trash": the models go to the trash one by one through the
//! journalled single-model delete, in blocks of at most 200 (the same bound as every other
//! batch); the catalog rows of the folders follow once no model is left below them.
//! Directories on disk are never touched here, and the trash format is the one the desktop
//! reads, including the `folder_trash_origin` marker that lets a restore rebuild the folders.
use super::*;
use std::collections::HashSet;

/// Frozen list of the models a folder trash will move, handed out in blocks.
#[derive(Debug)]
pub struct FolderTrashSnapshot {
    folder_id: i64,
    ids: Vec<i64>,
    offset: usize,
}
impl FolderTrashSnapshot {
    pub fn folder_id(&self) -> i64 {
        self.folder_id
    }
    pub fn ids(&self) -> &[i64] {
        &self.ids
    }
    /// Model ids of the next block; empty once every model has been handled.
    pub fn next_block_ids(&self) -> &[i64] {
        let end = self.offset.saturating_add(200).min(self.ids.len());
        self.ids.get(self.offset..end).unwrap_or(&[])
    }
    pub fn remaining(&self) -> usize {
        self.ids.len().saturating_sub(self.offset)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTrashBlock {
    pub results: Vec<ItemResult>,
    pub remaining: usize,
    /// Set by the last block: catalog rows of folders that were removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed_folder_count: Option<usize>,
    /// Folders that stay in the catalog because a model below them could not be moved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept_folder_count: Option<usize>,
    /// The folder rows could not be removed; the models are in the trash all the same.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_error: Option<Error>,
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    /// Checks the folder like a catalog removal does (the catalog's own folder and anything
    /// above it is refused) and freezes the ids of its active models.
    pub fn folder_trash_snapshot(&mut self, folder_id: i64) -> Result<FolderTrashSnapshot> {
        let folder = self.folder_state(folder_id)?;
        self.context.locate(&folder.path)?;
        let before = self.db.with_conn(|c| removal_tree(c, folder_id))?;
        self.protect_folder_removal(folder_id, &before)?;
        let ids = before
            .files
            .iter()
            .filter(|f| f.deleted_at.is_none())
            .map(|f| f.id)
            .collect();
        // A marker on a model that is still active is a leftover of an interrupted block.
        self.db.with_conn(|c| {
            c.execute(
                "DELETE FROM folder_trash_origin WHERE file_id IN (SELECT id FROM files WHERE deleted_at IS NULL)",
                [],
            )?;
            Ok(())
        })?;
        Ok(FolderTrashSnapshot { folder_id, ids, offset: 0 })
    }

    /// Moves the next block of models to the trash. The marker is written before the move so
    /// a crash can never leave a trashed model without it; markers of models that stayed are
    /// removed again. After the last block the folder rows that no longer hold a model go.
    pub fn folder_trash_block(
        &mut self,
        snapshot: &mut FolderTrashSnapshot,
        allowed: impl FnMut() -> bool,
    ) -> Result<FolderTrashBlock> {
        self.ready()?;
        let ids = snapshot.next_block_ids().to_vec();
        let mut results = Vec::new();
        if !ids.is_empty() {
            self.db.with_conn(|c| {
                for id in &ids {
                    c.execute(
                        "INSERT OR IGNORE INTO folder_trash_origin (file_id) SELECT id FROM files WHERE id=?1 AND deleted_at IS NULL",
                        [id],
                    )?;
                }
                Ok(())
            })?;
            let outcome = self.delete_files(&ids, allowed);
            // While the journal is blocked an entry may still be rolled forward by recovery,
            // so its marker must stay; leftovers on models that remain are cleared by the
            // next snapshot.
            if outcome.is_ok() && !self.journal.is_blocked() {
                self.db.with_conn(|c| {
                    for id in &ids {
                        c.execute(
                            "DELETE FROM folder_trash_origin WHERE file_id=?1 AND file_id IN (SELECT id FROM files WHERE deleted_at IS NULL)",
                            [id],
                        )?;
                    }
                    Ok(())
                })?;
            }
            results = outcome?;
            // Only untouched ids are retried by the next block; attempted failures are final.
            snapshot.offset += results
                .iter()
                .take_while(|r| r.kind != Some(Error::NotStarted))
                .count();
        }
        let mut block = FolderTrashBlock {
            results,
            remaining: snapshot.remaining(),
            removed_folder_count: None,
            kept_folder_count: None,
            folder_error: None,
        };
        if block.remaining == 0 {
            self.remove_emptied_folders(snapshot.folder_id, &mut block)?;
        }
        Ok(block)
    }

    /// Drops the catalog rows of every folder of the tree that holds no active model, keeping
    /// those with a leftover model and their ancestors, like the desktop does.
    fn remove_emptied_folders(&mut self, root: i64, block: &mut FolderTrashBlock) -> Result<()> {
        let tree = match self.db.with_conn(|c| removal_tree(c, root)) {
            Ok(tree) => tree,
            Err(Error::NotFound) => {
                block.removed_folder_count = Some(0);
                block.kept_folder_count = Some(0);
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let in_tree: HashSet<i64> = tree.folders.iter().map(|f| f.id).collect();
        let mut kept: HashSet<i64> = HashSet::new();
        for file in tree.files.iter().filter(|f| f.deleted_at.is_none()) {
            let mut current = file.folder_id;
            while let Some(id) = current {
                if !kept.insert(id) {
                    break;
                }
                current = tree
                    .folders
                    .iter()
                    .find(|f| f.id == id)
                    .and_then(|f| f.parent_id)
                    .filter(|p| in_tree.contains(p));
            }
        }
        let mut removed = 0;
        for folder in &tree.folders {
            let top = !kept.contains(&folder.id)
                && folder
                    .parent_id
                    .is_none_or(|p| !in_tree.contains(&p) || kept.contains(&p));
            if !top {
                continue;
            }
            match self.remove_folder_from_catalog(folder.id) {
                Ok(count) => removed += count.folder_count,
                Err(Error::RestartRequired) => return Err(Error::RestartRequired),
                Err(e) => {
                    block.folder_error = Some(e);
                    break;
                }
            }
        }
        block.removed_folder_count = Some(removed);
        block.kept_folder_count = Some(kept.len());
        Ok(())
    }
}
