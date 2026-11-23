//! Restore parent directories under the same journal as the file and its companions.
use super::*;
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

impl FsContext {
    /// Inspect every existing level without following links; record only the missing suffix.
    fn missing_restore_parents(&self, root: usize, target: &str) -> Result<Vec<ExtrasDir>> {
        let relative = RelPath::parse(target)?;
        let parts: Vec<_> = relative.as_str().split('/').collect();
        let mut missing = false;
        let mut directories = Vec::new();
        for end in 1..parts.len() {
            let path = parts[..end].join("/");
            if !missing {
                // Resolving a child applies the restore rule to an existing parent;
                // resolving the directory itself would reject a catalog base or upload root.
                match self.resolve_relative(
                    root,
                    &format!("{path}/.restore-probe"),
                    Access::RestoreTarget,
                ) {
                    Ok(_) => (),
                    Err(Error::NotFound) => {
                        let r = self.resolve_relative(root, &path, Access::RestoreTarget)?;
                        self.directory_chain(root, &r.parent, None)?;
                        missing = true;
                    }
                    Err(e) => return Err(e),
                }
            }
            if missing {
                directories.push(ExtrasDir {
                    relative: path,
                    identity: None,
                });
            }
        }
        Ok(directories)
    }
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    pub(super) fn restore_with_parents(&mut self, before: FileState) -> Result<()> {
        // Reject desktop and foreign trash before creating anything at the destination.
        let source = self.trash_source(&before)?;
        let identity = self.check_source(&source, before.id)?;
        let (root, target) = self.context.locate(&before.path)?;
        if root != source.root {
            return Err(Error::CrossDevice);
        }
        let directories = self.context.missing_restore_parents(root, &target)?;
        if directories.is_empty() {
            let target = self
                .context
                .resolve_relative(root, &target, Access::RestoreTarget)?;
            let carry = self.plan_restore(&before)?;
            if carry.steps.iter().any(|s| s.source.is_some())
                && !matches!(target.parent.entry_identity(&target.name), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
            {
                return Err(Error::Conflict);
            }
            self.outcome = CarryOutcome {
                moved: carry.steps.len(),
                kept_in_trash: carry.kept_in_trash,
            };
            let mut after = before.clone();
            after.deleted_at = None;
            after.trash_path = None;
            return self.transfer(before, after, source, target, Operation::Restore, carry);
        }
        let rows = self
            .db
            .with_conn(|c| super::carry::read_companion_rows(c, before.id))?;
        let mut after = before.clone();
        after.deleted_at = None;
        after.trash_path = None;
        let mut intent = Intent {
            op_id: journal::op_id()?,
            operation: Operation::Restore,
            phase: Phase::Prepared,
            root_path: self.context.roots[root].native.as_str().to_owned(),
            root_identity: self.context.roots[root].identity,
            source: Some(source.relative.clone()),
            target: Some(target.clone()),
            temporary: None,
            identity: Some(identity),
            before,
            after: Some(after),
            companions: Vec::new(),
            extras_dir: None,
            companion_rows: rows,
            companion_add: None,
            restore: Some(RestoreParents { directories }),
        };
        self.journal.add(intent.clone())?;
        if let Err(e) = self.make_restore_parents(&mut intent, root) {
            if e == Error::RestartRequired {
                return Err(e);
            }
            return self.transfer_failed(&intent, root, e);
        }
        let planned = (|| {
            let target = self
                .context
                .resolve_relative(root, &target, Access::RestoreTarget)?;
            match target.parent.entry_identity(&target.name) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
                Ok(_) => return Err(Error::Conflict),
            }
            let carry = self.plan_restore(&intent.before)?;
            if carry.rows != intent.companion_rows {
                return Err(Error::CompanionsChanged);
            }
            Ok((target, carry))
        })();
        let (target, carry) = match planned {
            Ok(plan) => plan,
            Err(e) => return self.transfer_failed(&intent, root, e),
        };
        self.outcome = CarryOutcome {
            moved: carry.steps.len(),
            kept_in_trash: carry.kept_in_trash,
        };
        intent.companions = carry.steps;
        intent.extras_dir = carry.extras_dir;
        self.save_restore_intent(&intent)?;
        self.execute_transfer(source, target, None, intent)
    }

    fn save_restore_intent(&mut self, intent: &Intent) -> Result<()> {
        *self
            .journal
            .intents
            .iter_mut()
            .find(|i| i.op_id == intent.op_id)
            .ok_or(Error::Internal)? = intent.clone();
        if self.journal.check_capacity().is_err() {
            return Err(self.journal.block(&intent.op_id));
        }
        self.journal.persist()
    }

    fn make_restore_parents(&mut self, intent: &mut Intent, root: usize) -> Result<()> {
        let count = intent
            .restore
            .as_ref()
            .ok_or(Error::Internal)?
            .directories
            .len();
        for index in 0..count {
            let relative = intent.restore.as_ref().unwrap().directories[index]
                .relative
                .clone();
            let r = self
                .context
                .resolve_relative(root, &relative, Access::RestoreTarget)?;
            self.context.directory_chain(root, &r.parent, None)?;
            // EEXIST is a conflict: a concurrent creator's directory is never ours to remove.
            r.parent.mkdir(&r.name, 0o755)?;
            #[cfg(test)]
            if super::tests::at_point("restore_mkdir_unrecorded", index) {
                return Err(self.journal.simulate_crash());
            }
            let id = match self.context.directory_identity(&r) {
                Ok(id) => id,
                Err(_) => return Err(self.journal.block(&intent.op_id)),
            };
            intent.restore.as_mut().unwrap().directories[index].identity = Some(id);
            self.save_restore_intent(intent)?;
            if r.parent.sync().is_err() {
                return Err(self.journal.block(&intent.op_id));
            }
            #[cfg(test)]
            if super::tests::at_point("restore_after_mkdir", index) {
                return Err(self.journal.simulate_crash());
            }
        }
        Ok(())
    }

    /// Never remove a pre-existing, replaced, nonempty or unrecorded directory.
    pub(super) fn remove_restore_parents(&mut self, i: &Intent, root: usize) {
        let Some(restore) = &i.restore else { return };
        let Some(target) = i.target.as_deref() else {
            return;
        };
        if self.context.roots[root].identity != i.root_identity
            || !self.context.roots[root].is_at(&i.root_path)
        {
            return;
        }
        for d in restore.directories.iter().rev() {
            let Some(identity) = d.identity else { continue };
            if !Path::new(target)
                .parent()
                .is_some_and(|p| p.starts_with(&d.relative))
            {
                continue;
            }
            let Ok(r) = self
                .context
                .resolve_relative(root, &d.relative, Access::Directory)
            else {
                continue;
            };
            if self.context.directory_identity(&r) == Ok(identity) {
                let _ = r.parent.rmdir(&r.name);
                let _ = r.parent.sync();
            }
        }
    }
}

/// Like the desktop restore, prefer an existing folder, then a catalog ancestor,
/// the configured catalog base, or the first directory created by this operation.
fn folder_link(c: &Connection, i: &Intent, create: bool) -> Result<Option<i64>> {
    let after = i.after.as_ref().ok_or(Error::Changed)?;
    let dir = Path::new(&after.path)
        .parent()
        .ok_or(Error::PathUnresolvable)?;
    let folders = crate::db::list_folders(c).map_err(|_| Error::Internal)?;
    if let Some(folder) = folders.iter().find(|f| Path::new(&f.path) == dir) {
        return Ok(Some(folder.id));
    }
    let created = i
        .restore
        .as_ref()
        .and_then(|r| r.directories.first())
        .map(|d| Path::new(&i.root_path).join(&d.relative));
    let from_folder: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM folder_trash_origin WHERE file_id=?1)",
        [after.id],
        |r| r.get(0),
    )?;
    if created.is_none() && !from_folder {
        return Ok(after.folder_id);
    }
    let anchor = folders
        .iter()
        .filter(|f| dir.starts_with(&f.path))
        .max_by_key(|f| Path::new(&f.path).components().count())
        .map(|f| PathBuf::from(&f.path));
    let base = crate::db::printer_link::get_setting(c, "catalog_base_dir")
        .map_err(|_| Error::Internal)?
        .map(PathBuf::from)
        .filter(|p| dir.starts_with(p));
    let Some(anchor) = anchor.or(base).or(created) else {
        return Ok(after.folder_id);
    };
    if !create {
        return Err(Error::Changed);
    }
    crate::db::ensure_folder_path(c, &anchor, dir)
        .map(Some)
        .map_err(|_| Error::Internal)
}

pub(super) fn relink(c: &Connection, i: &Intent) -> Result<()> {
    let folder = folder_link(c, i, true)?;
    c.execute(
        "UPDATE files SET folder_id=?1 WHERE id=?2",
        rusqlite::params![folder, i.before.id],
    )?;
    Ok(())
}

pub(super) fn matches_after(
    c: &Connection,
    i: &Intent,
    current: Option<&FileState>,
) -> Result<bool> {
    if i.restore.is_none() && i.operation != Operation::Restore {
        return Ok(current == i.after.as_ref());
    }
    let Some(mut expected) = i.after.clone() else {
        return Ok(false);
    };
    // The folder id is allocated inside the commit transaction, not guessed in the journal.
    let Some(actual) = current else {
        return Ok(false);
    };
    if actual.deleted_at.is_some() {
        return Ok(false);
    }
    expected.folder_id = folder_link(c, i, false)?;
    let origin: Option<i64> = c
        .query_row(
            "SELECT file_id FROM folder_trash_origin WHERE file_id=?1",
            [expected.id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(actual == &expected && origin.is_none())
}
