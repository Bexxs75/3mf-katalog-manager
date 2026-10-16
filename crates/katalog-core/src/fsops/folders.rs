use super::*;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{collections::HashSet, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderState {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub parent_id: Option<i64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderTree {
    folders: Vec<FolderState>,
    files: Vec<FileState>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderOperation {
    Create,
    Relocate,
    Remove,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderIntent {
    pub op_id: String,
    pub phase: Phase,
    pub operation: FolderOperation,
    root_path: String,
    root_identity: Identity,
    source: Option<String>,
    target: Option<String>,
    identity: Option<Identity>,
    before: FolderTree,
    after: FolderTree,
    parent: Option<FolderState>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRemovalSummary {
    pub name: String,
    pub subfolder_count: usize,
    pub model_count: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRemovalCount {
    pub model_count: usize,
    pub folder_count: usize,
}
fn folder(c: &Connection, id: i64) -> Result<Option<FolderState>> {
    Ok(c.query_row(
        "SELECT id,name,path,parent_id FROM folders WHERE id=?1",
        [id],
        |r| {
            Ok(FolderState {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                parent_id: r.get(3)?,
            })
        },
    )
    .optional()?)
}
fn valid_path(path: &str) -> Result<&Path> {
    if !path.starts_with('/')
        || path[1..].split('/').any(|p| matches!(p, "" | "." | ".."))
        || path.contains(['\\', '\0'])
    {
        return Err(Error::Inconsistent);
    }
    Ok(Path::new(path))
}
fn under(path: &str, base: &str) -> bool {
    Path::new(path).starts_with(base)
}
fn rewrite(path: &str, old: &str, new: &str) -> Result<String> {
    let suffix = valid_path(path)?
        .strip_prefix(valid_path(old)?)
        .map_err(|_| Error::Inconsistent)?;
    if suffix.as_os_str().is_empty() {
        return Ok(new.to_owned());
    }
    Ok(Path::new(new)
        .join(suffix)
        .to_str()
        .ok_or(Error::Inconsistent)?
        .to_owned())
}
fn tree(c: &Connection, id: i64) -> Result<FolderTree> {
    let root = folder(c, id)?.ok_or(Error::NotFound)?;
    valid_path(&root.path)?;
    let mut stmt = c.prepare("WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id=?1 UNION SELECT f.id FROM folders f JOIN subtree s ON f.parent_id=s.id) SELECT id FROM subtree ORDER BY id")?;
    let ids: Vec<i64> = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut folders = Vec::new();
    for id in &ids {
        folders.push(folder(c, *id)?.ok_or(Error::Changed)?);
    }
    for f in &folders {
        valid_path(&f.path)?;
        if !under(&f.path, &root.path) || (f.id != id && f.path == root.path) {
            return Err(Error::Inconsistent);
        }
        if f.id != id {
            let parent = folders
                .iter()
                .find(|p| Some(p.id) == f.parent_id)
                .ok_or(Error::Inconsistent)?;
            if Path::new(&f.path).parent() != Some(Path::new(&parent.path)) {
                return Err(Error::Inconsistent);
            }
        }
    }
    let mut stmt = c.prepare("SELECT id,path FROM folders ORDER BY id")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (fid, path) = row?;
        if under(&path, &root.path) && !ids.contains(&fid) {
            return Err(Error::Inconsistent);
        }
    }
    let mut stmt = c.prepare("SELECT id FROM files ORDER BY id")?;
    let mut files = Vec::new();
    for fid in stmt.query_map([], |r| r.get::<_, i64>(0))? {
        let f = operations::read_state(c, fid?)?.ok_or(Error::Changed)?;
        let assigned = f.folder_id.is_some_and(|id| ids.contains(&id));
        if assigned || under(&f.path, &root.path) {
            valid_path(&f.path)?;
            if !under(&f.path, &root.path) || f.path == root.path {
                return Err(Error::Inconsistent);
            }
            if let Some(parent_id) = f.folder_id {
                let parent = folders
                    .iter()
                    .find(|p| p.id == parent_id)
                    .ok_or(Error::Inconsistent)?;
                if Path::new(&f.path).parent() != Some(Path::new(&parent.path)) {
                    return Err(Error::Inconsistent);
                }
            }
            files.push(f);
        }
    }
    Ok(FolderTree { folders, files })
}
fn anchor(i: &FolderIntent) -> Result<&FolderState> {
    if i.operation == FolderOperation::Create {
        i.after.folders.first().ok_or(Error::Changed)
    } else {
        let source = i.source_absolute().ok_or(Error::Changed)?;
        i.before
            .folders
            .iter()
            .find(|f| f.path == source)
            .ok_or(Error::Changed)
    }
}
impl FolderIntent {
    fn source_absolute(&self) -> Option<String> {
        self.source.as_ref().map(|s| {
            Path::new(&self.root_path)
                .join(s)
                .to_string_lossy()
                .into_owned()
        })
    }
    fn validate(&self) -> Result<()> {
        let root = anchor(self)?;
        for tree in [&self.before, &self.after] {
            let folder_ids: HashSet<_> = tree.folders.iter().map(|f| f.id).collect();
            let file_ids: HashSet<_> = tree.files.iter().map(|f| f.id).collect();
            if folder_ids.len() != tree.folders.len() || file_ids.len() != tree.files.len() {
                return Err(Error::Changed);
            }
        }
        match self.operation {
            FolderOperation::Create => {
                if self.before != FolderTree::default()
                    || self.after.folders.len() != 1
                    || !self.after.files.is_empty()
                    || self.source.is_some()
                {
                    return Err(Error::Changed);
                }
                let parent = self.parent.as_ref().ok_or(Error::Changed)?;
                if root.parent_id != Some(parent.id)
                    || root.path
                        != Path::new(&parent.path)
                            .join(validate_name(&root.name)?)
                            .to_string_lossy()
                {
                    return Err(Error::Changed);
                }
                let target = self.target.as_ref().ok_or(Error::Changed)?;
                if root.path != Path::new(&self.root_path).join(target).to_string_lossy() {
                    return Err(Error::Changed);
                }
            }
            FolderOperation::Relocate => {
                let target = self.target.as_ref().ok_or(Error::Changed)?;
                let path = Path::new(&self.root_path).join(target);
                let path = path.to_str().ok_or(Error::Changed)?;
                let name = Path::new(path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or(Error::Changed)?;
                validate_name(name)?;
                let expected_parent = self
                    .parent
                    .as_ref()
                    .map(|p| p.path.as_str())
                    .or_else(|| Path::new(&root.path).parent().and_then(|p| p.to_str()))
                    .ok_or(Error::Changed)?;
                if Path::new(path).parent() != Some(Path::new(expected_parent)) {
                    return Err(Error::Changed);
                }
                let mut expected = self.before.clone();
                for f in &mut expected.folders {
                    f.path = rewrite(&f.path, &root.path, path)?;
                    if f.id == root.id {
                        f.name = name.into();
                        if let Some(parent) = &self.parent {
                            f.parent_id = Some(parent.id);
                        }
                    }
                }
                for f in &mut expected.files {
                    f.path = rewrite(&f.path, &root.path, path)?;
                }
                if expected != self.after || self.identity.is_none() {
                    return Err(Error::Changed);
                }
            }
            FolderOperation::Remove => {
                let mut expected = FolderTree::default();
                for f in &self.before.files {
                    if f.deleted_at.is_some() {
                        let mut f = f.clone();
                        f.folder_id = None;
                        expected.files.push(f);
                    }
                }
                if expected != self.after || self.target.is_some() {
                    return Err(Error::Changed);
                }
            }
        }
        if self
            .identity
            .is_some_and(|id| id.dev != self.root_identity.dev)
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
    fn ids(&self) -> Vec<i64> {
        self.before
            .files
            .iter()
            .filter(|f| self.operation != FolderOperation::Remove || f.deleted_at.is_none())
            .map(|f| f.id)
            .collect()
    }
}
fn matches(c: &Connection, i: &FolderIntent, after: bool) -> Result<bool> {
    let expected = if after { &i.after } else { &i.before };
    if let Some(p) = &i.parent {
        if folder(c, p.id)?.as_ref() != Some(p) {
            return Ok(false);
        }
    }
    let folder_ids: HashSet<_> = i
        .before
        .folders
        .iter()
        .chain(&i.after.folders)
        .map(|f| f.id)
        .collect();
    for id in folder_ids {
        if folder(c, id)?.as_ref() != expected.folders.iter().find(|f| f.id == id) {
            return Ok(false);
        }
    }
    let file_ids: HashSet<_> = i
        .before
        .files
        .iter()
        .chain(&i.after.files)
        .map(|f| f.id)
        .collect();
    for id in file_ids {
        if operations::read_state(c, id)?.as_ref() != expected.files.iter().find(|f| f.id == id) {
            return Ok(false);
        }
    }
    if i.operation == FolderOperation::Relocate
        || (i.operation == FolderOperation::Remove && !after)
    {
        return Ok((if i.operation == FolderOperation::Remove {
            removal_tree(c, anchor(i)?.id)
        } else {
            tree(c, anchor(i)?.id)
        })
        .is_ok_and(|t| t == *expected));
    }
    Ok(true)
}
fn available(c: &Connection, i: &FolderIntent) -> Result<()> {
    for f in &i.after.folders {
        if c.query_row(
            "SELECT EXISTS(SELECT 1 FROM folders WHERE path=?1 AND id<>?2)",
            params![f.path, f.id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Error::Conflict);
        }
    }
    for f in &i.after.files {
        if c.query_row(
            "SELECT EXISTS(SELECT 1 FROM files WHERE path=?1 AND id<>?2)",
            params![f.path, f.id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Error::Conflict);
        }
    }
    Ok(())
}
fn commit(c: &mut Connection, i: &FolderIntent) -> Result<()> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !matches(&tx, i, false)? {
        return Err(Error::Changed);
    }
    available(&tx, i)?;
    match i.operation {
        FolderOperation::Create => {
            let f = &i.after.folders[0];
            tx.execute(
                "INSERT INTO folders(id,name,path,parent_id) VALUES(?1,?2,?3,?4)",
                params![f.id, f.name, f.path, f.parent_id],
            )?;
        }
        FolderOperation::Relocate => {
            for f in &i.after.folders {
                tx.execute(
                    "UPDATE folders SET name=?1,path=?2,parent_id=?3 WHERE id=?4",
                    params![f.name, f.path, f.parent_id, f.id],
                )?;
            }
            for f in &i.after.files {
                tx.execute(
                    "UPDATE files SET path=?1 WHERE id=?2",
                    params![f.path, f.id],
                )?;
            }
        }
        FolderOperation::Remove => {
            for f in &i.before.files {
                if f.deleted_at.is_none() {
                    tx.execute("DELETE FROM files WHERE id=?1", [f.id])?;
                }
            }
            // Direct deletion deliberately retains unused tags, matching desktop catalog_removal.
            tx.execute("DELETE FROM folders WHERE id=?1", [anchor(i)?.id])?;
        }
    }
    tx.commit()?;
    Ok(())
}
impl Journal {
    fn add_folder(&mut self, i: FolderIntent) -> Result<()> {
        if self.is_blocked() {
            return Err(Error::RestartRequired);
        }
        let phase = i.phase;
        self.folders.push(i);
        if let Err(e) = self.check_capacity() {
            self.folders.pop();
            return Err(e);
        }
        self.persist()?;
        self.checkpoint(phase)
    }
    fn folder_phase(&mut self, id: &str, phase: Phase) -> Result<()> {
        self.folders
            .iter_mut()
            .find(|i| i.op_id == id)
            .ok_or(Error::Internal)?
            .phase = phase;
        self.persist()?;
        self.checkpoint(phase)
    }
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    fn folder_state(&mut self, id: i64) -> Result<FolderState> {
        self.ready()?;
        self.db.with_conn(|c| folder(c, id)?.ok_or(Error::NotFound))
    }
    fn folder_guard(&mut self, r: &Resolved) -> Result<Identity> {
        let id = self.context.directory_identity(r)?;
        let base: Option<String> = self.db.with_conn(|c| {
            Ok(c.query_row(
                "SELECT value FROM app_settings WHERE key='catalog_base_dir'",
                [],
                |r| r.get(0),
            )
            .optional()?)
        })?;
        if let Some(base) = base {
            self.context.protects_catalog(id, &base)?;
        }
        Ok(id)
    }
    fn folder_finish(&mut self, i: &FolderIntent) -> Result<()> {
        self.journal.folder_phase(&i.op_id, Phase::Committed)?;
        let ids = i.ids();
        let result = if i.operation == FolderOperation::Remove {
            self.ledger.reset_imported(&ids)
        } else {
            self.ledger.detach(&ids)
        };
        if result.is_err() {
            return Err(self.journal.block(&i.op_id));
        }
        self.journal.remove(&i.op_id)
    }
    pub fn create_folder(&mut self, parent_id: Option<i64>, name: &str) -> Result<i64> {
        self.ready()?;
        let name = validate_name(name)?;
        let parent = self.folder_state(parent_id.ok_or(Error::InvalidArguments)?)?;
        let path = Path::new(&parent.path)
            .join(&name)
            .to_str()
            .ok_or(Error::PathUnresolvable)?
            .to_owned();
        let target = self.context.resolve(&path, Access::Directory)?;
        self.context
            .directory_chain(target.root, &target.parent, None)?;
        let id=self.db.with_conn(|c|Ok(c.query_row("SELECT MAX(COALESCE((SELECT MAX(id) FROM folders),0),COALESCE((SELECT seq FROM sqlite_sequence WHERE name='folders'),0))+1",[],|r|r.get::<_,i64>(0))?))?;
        let r = &self.context.roots[target.root];
        let mut i = FolderIntent {
            op_id: journal::op_id()?,
            phase: Phase::Prepared,
            operation: FolderOperation::Create,
            root_path: r.path.to_str().unwrap().into(),
            root_identity: r.identity,
            source: None,
            target: Some(target.relative.clone()),
            identity: None,
            before: FolderTree::default(),
            after: FolderTree {
                folders: vec![FolderState {
                    id,
                    name,
                    path,
                    parent_id,
                }],
                files: vec![],
            },
            parent: Some(parent),
        };
        self.db.with_conn(|c| available(c, &i))?;
        self.journal.add_folder(i.clone())?;
        if let Err(e) = target.parent.mkdir(&target.name, 0o755) {
            self.journal.remove(&i.op_id)?;
            return Err(e.into());
        }
        // A crash before recording the new inode is ambiguous: recovery never rmdirs an unknown inode.
        let identity = match self.context.directory_identity(&target) {
            Ok(id) => id,
            Err(_) => return Err(self.journal.block(&i.op_id)),
        };
        i.identity = Some(identity);
        self.journal
            .folders
            .iter_mut()
            .find(|j| j.op_id == i.op_id)
            .unwrap()
            .identity = Some(identity);
        if target.parent.sync().is_err() {
            return Err(self.journal.block(&i.op_id));
        }
        self.journal.folder_phase(&i.op_id, Phase::Renamed)?;
        #[cfg(test)]
        super::tests::before_mutation();
        if let Err(e) = self.db.with_conn(|c| commit(c, &i)) {
            if self.remove_created(&target, identity).is_err() {
                return Err(self.journal.block(&i.op_id));
            }
            self.journal.remove(&i.op_id)?;
            return Err(e);
        }
        self.folder_finish(&i)?;
        Ok(id)
    }
    fn remove_created(&self, r: &Resolved, id: Identity) -> Result<()> {
        self.context.directory_chain(r.root, &r.parent, None)?;
        if self.context.directory_identity(r)? != id {
            return Err(Error::Changed);
        }
        r.parent.rmdir(&r.name)?;
        r.parent.sync()?;
        Ok(())
    }
    pub fn rename_folder(&mut self, id: i64, name: &str) -> Result<()> {
        let name = validate_name(name)?;
        let before = self.folder_state(id)?;
        let source = self.context.resolve(&before.path, Access::Directory)?;
        self.folder_guard(&source)?;
        if source.name == name {
            return Ok(());
        }
        let path = Path::new(&before.path)
            .with_file_name(&name)
            .to_str()
            .ok_or(Error::PathUnresolvable)?
            .to_owned();
        self.relocate_folder(before, path, name, None)
    }
    pub fn move_folder(&mut self, id: i64, parent_id: Option<i64>) -> Result<()> {
        let before = self.folder_state(id)?;
        let pid = parent_id.ok_or(Error::Conflict)?;
        if pid == id {
            return Err(Error::Cycle);
        }
        let parent = self.folder_state(pid)?;
        let cycle=self.db.with_conn(|c|Ok(c.query_row("WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id=?1 UNION SELECT f.id FROM folders f JOIN subtree s ON f.parent_id=s.id) SELECT EXISTS(SELECT 1 FROM subtree WHERE id=?2)",params![id,pid],|r|r.get::<_,bool>(0))?))?;
        if cycle {
            return Err(Error::Cycle);
        }
        if before.parent_id == Some(pid) {
            return Err(Error::Conflict);
        }
        let name = Path::new(&before.path)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(Error::PathUnresolvable)?
            .to_owned();
        let path = Path::new(&parent.path)
            .join(&name)
            .to_str()
            .ok_or(Error::PathUnresolvable)?
            .to_owned();
        self.relocate_folder(before, path, name, Some(parent))
    }
    fn relocate_folder(
        &mut self,
        before: FolderState,
        path: String,
        name: String,
        parent: Option<FolderState>,
    ) -> Result<()> {
        let source = self.context.resolve(&before.path, Access::Directory)?;
        let identity = self.folder_guard(&source)?;
        let target = self.context.resolve(&path, Access::Directory)?;
        if source.root != target.root {
            return Err(Error::CrossDevice);
        }
        self.context
            .directory_chain(target.root, &target.parent, Some(identity))?;
        let old = self.db.with_conn(|c| tree(c, before.id))?;
        let mut after = old.clone();
        for f in &mut after.folders {
            f.path = rewrite(&f.path, &before.path, &path)?;
            if f.id == before.id {
                f.name = name.clone();
                if let Some(p) = &parent {
                    f.parent_id = Some(p.id);
                }
            }
        }
        for f in &mut after.files {
            f.path = rewrite(&f.path, &before.path, &path)?;
        }
        let r = &self.context.roots[source.root];
        let i = FolderIntent {
            op_id: journal::op_id()?,
            phase: Phase::Prepared,
            operation: FolderOperation::Relocate,
            root_path: r.path.to_str().unwrap().into(),
            root_identity: r.identity,
            source: Some(source.relative.clone()),
            target: Some(target.relative.clone()),
            identity: Some(identity),
            before: old,
            after,
            parent,
        };
        self.db.with_conn(|c| available(c, &i))?;
        self.journal.add_folder(i.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
        if let Err(e) = self.rename_directory(&source, &target, identity) {
            if e == Error::RestartRequired {
                return Err(self.journal.block(&i.op_id));
            }
            self.journal.remove(&i.op_id)?;
            return Err(e);
        }
        self.journal.folder_phase(&i.op_id, Phase::Renamed)?;
        if let Err(e) = self.db.with_conn(|c| commit(c, &i)) {
            if self.rename_directory(&target, &source, identity).is_err() {
                return Err(self.journal.block(&i.op_id));
            }
            self.journal.remove(&i.op_id)?;
            return Err(e);
        }
        self.folder_finish(&i)
    }
    fn rename_directory(&self, source: &Resolved, target: &Resolved, id: Identity) -> Result<()> {
        if self.context.directory_identity(source)? != id {
            return Err(Error::Changed);
        }
        self.context
            .directory_chain(source.root, &source.parent, None)?;
        self.context
            .directory_chain(target.root, &target.parent, Some(id))?;
        source
            .parent
            .rename_noreplace(&source.name, &target.parent, &target.name)?;
        #[cfg(test)]
        super::tests::after_directory_rename();
        let post = (|| -> Result<()> {
            if self.context.directory_identity(target)? != id {
                return Err(Error::Changed);
            }
            self.context
                .directory_chain(source.root, &source.parent, None)?;
            self.context
                .directory_chain(target.root, &target.parent, None)?;
            source.parent.sync()?;
            target.parent.sync()?;
            Ok(())
        })();
        if post.is_err() {
            // Only roll back our own inode, never a replacement supplied by another writer.
            if target.parent.entry_identity(&target.name).ok() == Some((id.dev, id.ino)) {
                let _ = target
                    .parent
                    .rename_noreplace(&target.name, &source.parent, &source.name);
                let _ = source.parent.sync();
                let _ = target.parent.sync();
            }
            return Err(Error::RestartRequired);
        }
        Ok(())
    }
    pub fn folder_removal_summary(&mut self, id: i64) -> Result<FolderRemovalSummary> {
        let root = self.folder_state(id)?;
        self.db.with_conn(|c| {
            let t = removal_tree(c, id)?;
            Ok(FolderRemovalSummary {
                name: root.name,
                subfolder_count: t.folders.len() - 1,
                model_count: t.files.iter().filter(|f| f.deleted_at.is_none()).count(),
            })
        })
    }
    pub fn remove_folder_from_catalog(&mut self, id: i64) -> Result<FolderRemovalCount> {
        let f = self.folder_state(id)?;
        let before = self.db.with_conn(|c| removal_tree(c, id))?;
        let (root, relative) = self.context.locate(&f.path)?;
        // DB-only removal must also work for stale disk paths. Check every registered
        // descendant independently, so inconsistent links cannot bypass protection.
        let mut identity = None;
        for entry in &before.folders {
            match self.context.resolve(&entry.path, Access::Directory) {
                Ok(resolved) => match self.context.directory_identity(&resolved) {
                    Ok(found) => {
                        let base: Option<String> = self.db.with_conn(|c| {
                            Ok(c.query_row(
                                "SELECT value FROM app_settings WHERE key='catalog_base_dir'",
                                [],
                                |r| r.get(0),
                            )
                            .optional()?)
                        })?;
                        if let Some(base) = base {
                            self.context.protects_catalog(found, &base)?;
                        }
                        if entry.id == id {
                            identity = Some(found);
                        }
                    }
                    Err(Error::NotFound) => (),
                    Err(e) => return Err(e),
                },
                Err(Error::NotFound) => (),
                Err(e) => return Err(e),
            }
        }
        let contains_base=self.db.with_conn(|c|Ok(c.query_row("WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id=?1 UNION SELECT f.id FROM folders f JOIN subtree s ON f.parent_id=s.id) SELECT EXISTS(SELECT 1 FROM folders WHERE id IN (SELECT id FROM subtree) AND path=(SELECT value FROM app_settings WHERE key='catalog_base_dir'))",[id],|r|r.get::<_,bool>(0))?))?;
        if contains_base {
            return Err(Error::Protected);
        }
        let model_count = before
            .files
            .iter()
            .filter(|f| f.deleted_at.is_none())
            .count();
        let folder_count = before.folders.len();
        let mut after = FolderTree::default();
        for f in &before.files {
            if f.deleted_at.is_some() {
                let mut f = f.clone();
                f.folder_id = None;
                after.files.push(f);
            }
        }
        let r = &self.context.roots[root];
        let i = FolderIntent {
            op_id: journal::op_id()?,
            phase: Phase::Prepared,
            operation: FolderOperation::Remove,
            root_path: r.path.to_str().unwrap().into(),
            root_identity: r.identity,
            source: Some(relative),
            target: None,
            identity,
            before,
            after,
            parent: None,
        };
        self.journal.add_folder(i.clone())?;
        if let Err(e) = self.db.with_conn(|c| commit(c, &i)) {
            self.journal.remove(&i.op_id)?;
            return Err(e);
        }
        self.folder_finish(&i)?;
        Ok(FolderRemovalCount {
            model_count,
            folder_count,
        })
    }
    pub(super) fn recover_folders(&mut self, report: &mut RecoveryReport) {
        for i in self.journal.folders.clone() {
            if i.phase == Phase::Manual {
                report.manual.push(i.op_id);
                continue;
            }
            match self.recover_folder(&i) {
                Ok(true) => {
                    if i.operation == FolderOperation::Remove {
                        report.reset_ids.extend(i.ids())
                    } else {
                        report.detach_ids.extend(i.ids())
                    }
                }
                Ok(false) => (),
                Err(_) => {
                    self.journal.block(&i.op_id);
                    report.manual.push(i.op_id);
                }
            }
        }
    }
    pub(super) fn finish_folder_recovery(&mut self) -> Result<()> {
        for i in self.journal.folders.clone() {
            if i.phase == Phase::Committed {
                self.folder_finish(&i)?;
            }
        }
        Ok(())
    }
    fn recover_folder(&mut self, i: &FolderIntent) -> Result<bool> {
        i.validate()?;
        if i.after.folders.is_empty() && i.before.folders.is_empty() {
            return Err(Error::Changed);
        }
        let root = self
            .context
            .roots
            .iter()
            .position(|r| r.path.to_str() == Some(&i.root_path) && r.identity == i.root_identity)
            .ok_or(Error::Changed)?;
        let before = self.db.with_conn(|c| matches(c, i, false))?;
        let after = self.db.with_conn(|c| matches(c, i, true))?;
        if i.operation == FolderOperation::Remove {
            if after {
                self.journal.folder_phase(&i.op_id, Phase::Committed)?;
                return Ok(true);
            }
            if before {
                self.journal.remove(&i.op_id)?;
                return Ok(false);
            }
            return Err(Error::Changed);
        }
        let target = self.context.resolve_relative(
            root,
            i.target.as_deref().ok_or(Error::Changed)?,
            Access::Directory,
        );
        if i.operation == FolderOperation::Create {
            if !i.before.folders.is_empty() || i.after.folders.len() != 1 || i.source.is_some() {
                return Err(Error::Changed);
            }
            let target = target?;
            if self.context.absolute(&target) != i.after.folders[0].path {
                return Err(Error::Changed);
            }
            let observed = observe_dir(&self.context, &target, i.identity)?;
            if after && observed == DirObserved::Matches {
                target.parent.sync()?;
                self.journal.folder_phase(&i.op_id, Phase::Committed)?;
                return Ok(true);
            }
            if before && observed == DirObserved::Absent {
                self.journal.remove(&i.op_id)?;
                return Ok(false);
            }
            if before && observed == DirObserved::Matches {
                self.remove_created(&target, i.identity.ok_or(Error::Changed)?)?;
                self.journal.remove(&i.op_id)?;
                return Ok(false);
            }
            return Err(Error::Changed);
        }
        let source = self.context.resolve_relative(
            root,
            i.source.as_deref().ok_or(Error::Changed)?,
            Access::Directory,
        )?;
        let identity = i.identity.ok_or(Error::Changed)?;
        if identity.dev != i.root_identity.dev {
            return Err(Error::Changed);
        }
        let s = observe_dir(&self.context, &source, Some(identity))?;
        let t = target
            .as_ref()
            .ok()
            .map(|r| observe_dir(&self.context, r, Some(identity)))
            .transpose()?
            .unwrap_or(DirObserved::Foreign);
        if before && s == DirObserved::Matches && t != DirObserved::Matches {
            self.journal.remove(&i.op_id)?;
            return Ok(false);
        }
        if s == DirObserved::Absent && t == DirObserved::Matches {
            let target = target?;
            source.parent.sync()?;
            target.parent.sync()?;
            if before {
                self.db.with_conn(|c| commit(c, i))?;
            } else if !after {
                return Err(Error::Changed);
            }
            self.journal.folder_phase(&i.op_id, Phase::Committed)?;
            return Ok(true);
        }
        Err(Error::Changed)
    }
}
fn removal_tree(c: &Connection, id: i64) -> Result<FolderTree> {
    folder(c, id)?.ok_or(Error::NotFound)?;
    let mut stmt=c.prepare("WITH RECURSIVE subtree(id) AS (SELECT id FROM folders WHERE id=?1 UNION SELECT f.id FROM folders f JOIN subtree s ON f.parent_id=s.id) SELECT id FROM subtree ORDER BY id")?;
    let ids: Vec<i64> = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut result = FolderTree::default();
    for id in &ids {
        result.folders.push(folder(c, *id)?.ok_or(Error::Changed)?);
    }
    let mut stmt = c.prepare("SELECT id FROM files ORDER BY id")?;
    for id in stmt.query_map([], |r| r.get::<_, i64>(0))? {
        let f = operations::read_state(c, id?)?.ok_or(Error::Changed)?;
        if f.folder_id.is_some_and(|id| ids.contains(&id)) {
            result.files.push(f);
        }
    }
    Ok(result)
}
#[derive(PartialEq, Eq)]
enum DirObserved {
    Absent,
    Matches,
    Foreign,
}
fn observe_dir(ctx: &FsContext, r: &Resolved, identity: Option<Identity>) -> Result<DirObserved> {
    if !r.parent.names_checked()?.iter().any(|n| n == &r.name) {
        return Ok(DirObserved::Absent);
    }
    match ctx.directory_identity(r) {
        Ok(id) if Some(id) == identity => Ok(DirObserved::Matches),
        Ok(_) | Err(Error::PathUnresolvable) => Ok(DirObserved::Foreign),
        Err(Error::NotFound) => Ok(DirObserved::Absent),
        Err(e) => Err(e),
    }
}
pub fn fs_create_folder<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    parent_id: Option<i64>,
    name: &str,
) -> Result<i64> {
    ops.create_folder(parent_id, name)
}
pub fn fs_rename_folder<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    id: i64,
    name: &str,
) -> Result<()> {
    ops.rename_folder(id, name)
}
pub fn fs_move_folder<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    id: i64,
    parent_id: Option<i64>,
) -> Result<()> {
    ops.move_folder(id, parent_id)
}
pub fn fs_remove_folder_from_catalog<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    id: i64,
) -> Result<FolderRemovalCount> {
    ops.remove_folder_from_catalog(id)
}
pub fn fs_folder_removal_summary<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    id: i64,
) -> Result<FolderRemovalSummary> {
    ops.folder_removal_summary(id)
}
