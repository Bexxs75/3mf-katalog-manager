//! The desktop trash has a separate unlink-only capability, never a file root.
use super::carry::read_companion_rows;
use super::operations::{commit, read_state};
use super::*;
use std::path::{Path, PathBuf};

pub struct DesktopTrashPurge {
    path: PathBuf,
    dir: Directory,
    identity: Identity,
}
impl DesktopTrashPurge {
    /// Missing trash is normal on catalogs that have only used the web client.
    pub fn open(db_dir: &Path) -> Result<Option<Self>> {
        let base = std::fs::canonicalize(db_dir)?;
        let parent = Directory::open(&base)?;
        let dir = match parent.open_dir("trash") {
            Ok(dir) => dir,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if dir.device()? != parent.device()? || dir.mount_id()? != parent.mount_id()? {
            return Err(Error::CrossDevice);
        }
        let identity = Identity::of(&dir)?;
        Ok(Some(Self {
            path: base.join("trash"),
            dir,
            identity,
        }))
    }

    fn name<'a>(&self, path: &'a str, id: i64, companion: bool) -> Result<&'a str> {
        NativePath::parse(path, Style::Posix)?;
        let p = Path::new(path);
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(Error::TrashForeign)?;
        if p.parent() != Some(self.path.as_path())
            || !crate::fs_portable::names::desktop_trash_name_matches(name, id, companion)
        {
            return Err(Error::TrashForeign);
        }
        Ok(name)
    }

    fn resolve(&self, path: &str, id: i64, companion: bool) -> Result<Resolved> {
        let name = self.name(path, id, companion)?.to_owned();
        // A replaced directory must not redirect an old capability to another tree.
        if Identity::of(&Directory::open(&self.path)?)? != self.identity {
            return Err(Error::Changed);
        }
        Ok(Resolved {
            root: 0,
            relative: name.clone(),
            parent: self.dir.try_clone()?,
            name,
        })
    }
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    pub(super) fn desktop_matches(&self, path: Option<&str>, id: i64, companion: bool) -> bool {
        self.context
            .desktop_trash
            .as_ref()
            .zip(path)
            .is_some_and(|(cap, path)| cap.name(path, id, companion).is_ok())
    }

    pub(super) fn has_desktop_purge(&mut self, model: &FileState) -> Result<bool> {
        if self.desktop_matches(model.trash_path.as_deref(), model.id, false) {
            return Ok(true);
        }
        let rows = self.db.with_conn(|c| read_companion_rows(c, model.id))?;
        Ok(rows
            .iter()
            .any(|r| self.desktop_matches(r.trash_path.as_deref(), r.id, true)))
    }

    fn purge_place(
        &self,
        model: &FileState,
        path: &str,
        id: i64,
        companion: bool,
    ) -> Result<Resolved> {
        if let Some(cap) = &self.context.desktop_trash {
            if cap.name(path, id, companion).is_ok() {
                return cap.resolve(path, id, companion);
            }
        }
        let (root, _) = self.context.locate(&model.path)?;
        let r = self.context.resolve(path, Access::Trash)?;
        let prefix = if companion {
            companion_trash_prefix(id)
        } else {
            format!("{id}-")
        };
        if r.root != root || !r.name.starts_with(&prefix) {
            return Err(Error::TrashForeign);
        }
        Ok(r)
    }

    fn purge_identity(
        &self,
        model: &FileState,
        path: &str,
        id: i64,
        companion: bool,
    ) -> Result<Option<FileIdentity>> {
        let r = self.purge_place(model, path, id, companion)?;
        match FileIdentity::read(&r) {
            Ok(identity) if identity.dev == r.parent.device()? => Ok(Some(identity)),
            Ok(_) => Err(Error::CrossDevice),
            Err(Error::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(super) fn purge_desktop(
        &mut self,
        before: FileState,
        loose: Option<i64>,
        cutoff: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> Result<DeleteOutcome> {
        let rows = self.db.with_conn(|c| read_companion_rows(c, before.id))?;
        let stamp = if let Some(id) = loose {
            if before.deleted_at.is_some() {
                return Err(Error::Changed);
            }
            rows.iter()
                .find(|r| r.id == id)
                .and_then(|r| r.deleted_at.as_deref())
        } else {
            before.deleted_at.as_deref()
        }
        .ok_or(Error::Changed)?;
        let deleted = chrono::DateTime::parse_from_rfc3339(stamp).map_err(|_| Error::Changed)?;
        if cutoff.is_some_and(|c| deleted > c) {
            return Err(Error::Changed);
        }
        let mut steps = Vec::new();
        for row in rows.iter().filter(|r| loose.is_none_or(|id| r.id == id)) {
            if row.deleted_at.is_none() {
                return Err(Error::HasCompanions);
            }
            let identity = row
                .trash_path
                .as_deref()
                .map(|p| self.purge_identity(&before, p, row.id, true))
                .transpose()?
                .flatten();
            steps.push(CompanionStep {
                before: row.clone(),
                after: None,
                source: row.trash_path.clone(),
                target: None,
                identity,
            });
        }
        let source = if loose.is_none() {
            before.trash_path.clone()
        } else {
            None
        };
        let identity = source
            .as_deref()
            .map(|p| self.purge_identity(&before, p, before.id, false))
            .transpose()?
            .flatten();
        let cap = self
            .context
            .desktop_trash
            .as_ref()
            .ok_or(Error::TrashForeign)?;
        let intent = Intent {
            op_id: journal::op_id()?,
            operation: Operation::PurgeDesktopTrash,
            phase: Phase::Prepared,
            root_path: cap.path.to_str().ok_or(Error::PathUnresolvable)?.to_owned(),
            root_identity: cap.identity,
            source,
            target: None,
            temporary: None,
            identity,
            after: loose.map(|_| before.clone()),
            before,
            companions: steps,
            companion_rows: rows,
            extras_dir: None,
            companion_add: None,
        };
        self.journal.add(intent.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
        if let Err(e) = self.run_desktop_purge(&intent) {
            return Err(if e == Error::RestartRequired {
                e
            } else {
                self.journal.block(&intent.op_id)
            });
        }
        if loose.is_some() {
            self.journal.phase(&intent.op_id, Phase::Committed)?;
            self.journal.remove(&intent.op_id)?;
        } else {
            self.finish(&intent)?;
        }
        Ok(DeleteOutcome {
            kept_on_disk: false,
        })
    }

    fn validate_desktop_intent(&self, i: &Intent) -> Result<()> {
        let cap = self.context.desktop_trash.as_ref().ok_or(Error::Changed)?;
        let loose = i.after.is_some();
        if i.root_path != cap.path.to_str().ok_or(Error::Changed)?
            || i.root_identity != cap.identity
            || i.target.is_some()
            || i.temporary.is_some()
            || i.extras_dir.is_some()
            || i.companion_add.is_some()
            || (loose
                && (i.after.as_ref() != Some(&i.before)
                    || i.before.deleted_at.is_some()
                    || i.source.is_some()
                    || i.identity.is_some()
                    || i.companions.len() != 1))
            || (!loose && (i.before.deleted_at.is_none() || i.source != i.before.trash_path))
        {
            return Err(Error::Changed);
        }
        let expected: Vec<_> = i
            .companion_rows
            .iter()
            .filter(|r| !loose || i.companions[0].before.id == r.id)
            .collect();
        if expected.len() != i.companions.len() {
            return Err(Error::Changed);
        }
        for (row, step) in expected.into_iter().zip(&i.companions) {
            if row != &step.before
                || row.deleted_at.is_none()
                || step.after.is_some()
                || step.target.is_some()
                || step.source != row.trash_path
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }

    fn run_desktop_purge(&mut self, i: &Intent) -> Result<()> {
        self.validate_desktop_intent(i)?;
        self.db.with_conn(|c| {
            let tx = c.transaction()?;
            if read_state(&tx, i.before.id)?.as_ref() != Some(&i.before)
                || read_companion_rows(&tx, i.before.id)? != i.companion_rows
            {
                return Err(Error::Changed);
            }
            Ok(())
        })?;
        let mut files: Vec<_> = i
            .companions
            .iter()
            .map(|s| (s.source.as_deref(), s.before.id, true, s.identity))
            .collect();
        files.push((i.source.as_deref(), i.before.id, false, i.identity));
        // Preflight the complete set before the first irreversible effect.
        for &(path, id, companion, expected) in &files {
            if let Some(path) = path {
                let observed = self.purge_identity(&i.before, path, id, companion)?;
                if observed.is_some() && observed != expected {
                    return Err(Error::Changed);
                }
            } else if expected.is_some() {
                return Err(Error::Changed);
            }
        }
        for (k, (path, id, companion, expected)) in files.into_iter().enumerate() {
            let Some(path) = path else { continue };
            #[cfg(test)]
            if super::tests::at_point("before_unlink", k) {
                return Err(self.journal.simulate_crash());
            }
            let place = self.purge_place(&i.before, path, id, companion)?;
            match FileIdentity::read(&place) {
                Ok(found) if Some(found) == expected => place.parent.unlink(&place.name)?,
                Err(Error::NotFound) => (),
                _ => return Err(Error::Changed),
            }
            place.parent.sync()?;
            #[cfg(test)]
            if super::tests::at_point("after_unlink", k) {
                return Err(self.journal.simulate_crash());
            }
            let _ = k;
        }
        self.journal.phase(&i.op_id, Phase::Unlinked)?;
        if i.after.is_none() {
            self.db.with_conn(|c| commit(c, i))?;
        } else {
            self.db.with_conn(|c| {
                let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                if read_state(&tx, i.before.id)?.as_ref() != Some(&i.before)
                    || read_companion_rows(&tx, i.before.id)? != i.companion_rows
                {
                    return Err(Error::Changed);
                }
                tx.execute(
                    "DELETE FROM companion_files WHERE id=?1 AND deleted_at IS NOT NULL",
                    [i.companions[0].before.id],
                )?;
                tx.commit()?;
                Ok(())
            })?;
        }
        Ok(())
    }

    pub(super) fn recover_desktop_purge(&mut self, i: &Intent) -> Result<bool> {
        self.validate_desktop_intent(i)?;
        let (state, rows) = self.db.with_conn(|c| {
            Ok((
                read_state(c, i.before.id)?,
                read_companion_rows(c, i.before.id)?,
            ))
        })?;
        let finished = if i.after.is_none() {
            state.is_none() && rows.is_empty()
        } else {
            state.as_ref() == Some(&i.before)
                && rows
                    == i.companion_rows
                        .iter()
                        .filter(|r| r.id != i.companions[0].before.id)
                        .cloned()
                        .collect::<Vec<_>>()
        };
        if !finished {
            self.run_desktop_purge(i)?;
        }
        self.journal.phase(&i.op_id, Phase::Committed)?;
        if i.after.is_some() {
            self.journal.remove(&i.op_id)?;
        }
        Ok(i.after.is_none())
    }
}
