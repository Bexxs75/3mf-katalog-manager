use super::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    collections::HashSet,
    sync::Mutex,
    time::{Duration, Instant},
};
/// Implementations acquire/release the DB mutex for each callback, never around the operation.
pub trait FsDatabase {
    fn with_conn<T>(&mut self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T>;
}
impl FsDatabase for Connection {
    fn with_conn<T>(&mut self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        f(self)
    }
}
impl FsDatabase for &Mutex<Connection> {
    fn with_conn<T>(&mut self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        f(&mut *self.lock().map_err(|_| Error::Internal)?)
    }
}
/// B implements this using UploadStore. Both methods must release its lock before returning.
pub trait UploadLedger {
    fn detach(&mut self, file_ids: &[i64]) -> Result<()>;
    /// B resets imported entries to uploaded, idempotently, after DB-only removal.
    fn reset_imported(&mut self, file_ids: &[i64]) -> Result<()> {
        if file_ids.is_empty() {
            Ok(())
        } else {
            Err(Error::Unsupported)
        }
    }
    /// Identity from a journal entry, if tracked (including legacy entries resolved by B).
    fn identity(&mut self, _file_id: i64) -> Result<Option<Identity>> {
        Ok(None)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeleteOutcome {
    pub kept_on_disk: bool,
}
#[derive(Debug, Serialize)]
pub struct ItemResult {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Error>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept_on_disk: Option<bool>,
}
#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub detach_ids: Vec<i64>,
    pub manual: Vec<String>,
    pub reset_ids: Vec<i64>,
}
/// Immutable ID snapshot; B stores this behind an opaque session-bound token.
pub struct TrashSnapshot {
    ids: Vec<i64>,
    offset: usize,
    cutoff: Option<chrono::DateTime<chrono::FixedOffset>>,
}
impl TrashSnapshot {
    pub fn remaining(&self) -> usize {
        self.ids.len() - self.offset
    }
}
#[derive(Debug, Serialize)]
pub struct TrashSummary {
    pub count: usize,
    pub bytes: u64,
    pub max_deleted_at: Option<String>,
}
pub struct FileOps<D, L> {
    pub context: FsContext,
    pub journal: Journal,
    pub db: D,
    pub ledger: L,
}
pub(super) fn read_state(c: &Connection, id: i64) -> Result<Option<FileState>> {
    Ok(c.query_row(
        "SELECT id,path,name,folder_id,deleted_at,trash_path FROM files WHERE id=?1",
        [id],
        |r| {
            Ok(FileState {
                id: r.get(0)?,
                path: r.get(1)?,
                name: r.get(2)?,
                folder_id: r.get(3)?,
                deleted_at: r.get(4)?,
                trash_path: r.get(5)?,
            })
        },
    )
    .optional()?)
}
fn available(c: &Connection, s: &FileState) -> Result<()> {
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM files WHERE path=?1 AND id<>?2)",
        params![s.path, s.id],
        |r| r.get::<_, bool>(0),
    )? {
        Err(Error::Conflict)
    } else {
        Ok(())
    }
}
fn commit(c: &mut Connection, intent: &Intent) -> Result<()> {
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if read_state(&tx, intent.before.id)?.as_ref() != Some(&intent.before) {
        return Err(Error::Changed);
    }
    if let Some(s) = &intent.after {
        available(&tx, s)?;
        tx.execute(
            "UPDATE files SET path=?1,name=?2,folder_id=?3,deleted_at=?4,trash_path=?5 WHERE id=?6",
            params![
                s.path,
                s.name,
                s.folder_id,
                s.deleted_at,
                s.trash_path,
                s.id
            ],
        )?;
    } else {
        crate::db::delete_file(&tx, intent.before.id).map_err(|_| Error::Internal)?;
    }
    tx.commit()?;
    Ok(())
}
impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    pub fn new(context: FsContext, journal: Journal, db: D, ledger: L) -> Self {
        Self {
            context,
            journal,
            db,
            ledger,
        }
    }
    pub(super) fn ready(&self) -> Result<()> {
        if self.journal.is_blocked() {
            Err(Error::RestartRequired)
        } else {
            Ok(())
        }
    }
    pub fn state(&mut self, id: i64) -> Result<FileState> {
        self.db
            .with_conn(|c| read_state(c, id)?.ok_or(Error::NotFound))
    }
    fn active(&mut self, id: i64) -> Result<FileState> {
        self.ready()?;
        let s = self.state(id)?;
        if s.deleted_at.is_some() {
            Err(Error::Conflict)
        } else {
            Ok(s)
        }
    }
    fn trashed(&mut self, id: i64) -> Result<FileState> {
        self.ready()?;
        let s = self.state(id)?;
        if s.deleted_at.is_none() {
            Err(Error::Conflict)
        } else {
            Ok(s)
        }
    }
    fn check_source(&mut self, r: &Resolved, id: i64) -> Result<FileIdentity> {
        let identity = FileIdentity::read(r)?;
        if identity.dev != self.context.roots[r.root].identity.dev {
            return Err(Error::CrossDevice);
        }
        if self
            .ledger
            .identity(id)?
            .is_some_and(|i| i.dev != identity.dev || i.ino != identity.ino)
        {
            return Err(Error::Changed);
        }
        Ok(identity)
    }
    fn finish(&mut self, intent: &Intent) -> Result<()> {
        self.journal.phase(&intent.op_id, Phase::Committed)?;
        if self.ledger.detach(&[intent.before.id]).is_err() {
            return Err(self.journal.block(&intent.op_id));
        }
        self.journal.remove(&intent.op_id)
    }
    fn db_only(&mut self, before: FileState, after: Option<FileState>) -> Result<()> {
        let intent = Intent {
            op_id: journal::op_id()?,
            operation: Operation::DbOnly,
            phase: Phase::Prepared,
            root_path: String::new(),
            root_identity: Identity { dev: 0, ino: 0 },
            source: None,
            target: None,
            temporary: None,
            identity: None,
            before,
            after,
        };
        self.journal.add(intent.clone())?;
        if let Err(e) = self.db.with_conn(|c| commit(c, &intent)) {
            self.journal.remove(&intent.op_id)?;
            return Err(e);
        }
        self.finish(&intent)
    }
    fn transfer(
        &mut self,
        before: FileState,
        mut after: FileState,
        source: Resolved,
        mut target: Resolved,
        operation: Operation,
    ) -> Result<()> {
        if source.root != target.root {
            return Err(Error::CrossDevice);
        }
        self.db.with_conn(|c| available(c, &after))?;
        let identity = self.check_source(&source, before.id)?;
        let op_id = journal::op_id()?;
        if operation == Operation::Trash {
            let name = trash_name(before.id, &op_id, &source.name)?;
            target = self.context.resolve_relative(
                source.root,
                &format!("{TRASH}/{name}"),
                Access::Trash,
            )?;
            after.trash_path = Some(self.context.absolute(&target));
        }
        let case_only = operation == Operation::Rename
            && crate::upload_fs::nfc(&source.name.to_lowercase())
                == crate::upload_fs::nfc(&target.name.to_lowercase());
        let temporary = case_only.then(|| {
            let p = std::path::Path::new(&source.relative).parent().unwrap();
            p.join(format!(".fsops-{op_id}.tmp"))
                .to_str()
                .unwrap()
                .to_owned()
        });
        let root = &self.context.roots[source.root];
        let intent = Intent {
            op_id,
            operation,
            phase: Phase::Prepared,
            root_path: root.path.to_str().unwrap().to_owned(),
            root_identity: root.identity,
            source: Some(source.relative.clone()),
            target: Some(target.relative.clone()),
            temporary,
            identity: Some(identity),
            before,
            after: Some(after),
        };
        let temp = intent
            .temporary
            .as_ref()
            .map(|r| {
                self.context
                    .resolve_relative(source.root, r, Access::RenameTarget)
            })
            .transpose()?;
        self.journal.add(intent.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
        if let Some(t) = &temp {
            if let Err(e) = rename_checked(&source, t, identity) {
                return self.transfer_failed(&intent, e);
            }
            self.journal.phase(&intent.op_id, Phase::TmpRenamed)?;
        }
        let actual = temp.as_ref().unwrap_or(&source);
        if let Err(e) = rename_checked(actual, &target, identity) {
            if temp.is_some() && rename_checked(actual, &source, identity).is_err() {
                return Err(self.journal.block(&intent.op_id));
            }
            return self.transfer_failed(&intent, e);
        }
        self.journal.phase(&intent.op_id, Phase::Renamed)?;
        if let Err(e) = self.db.with_conn(|c| commit(c, &intent)) {
            let rollback = if let Some(temp) = &temp {
                rename_checked(&target, temp, identity)
                    .and_then(|()| rename_checked(temp, &source, identity))
            } else {
                rename_checked(&target, &source, identity)
            };
            if rollback.is_err() {
                return Err(self.journal.block(&intent.op_id));
            }
            self.journal.remove(&intent.op_id)?;
            return Err(e);
        }
        self.finish(&intent)
    }
    fn transfer_failed(&mut self, i: &Intent, e: Error) -> Result<()> {
        if matches!(e, Error::RestartRequired) {
            return Err(self.journal.block(&i.op_id));
        }
        self.journal.remove(&i.op_id)?;
        Err(if i.operation == Operation::Trash && e == Error::Conflict {
            Error::TrashConflict
        } else {
            e
        })
    }
    pub fn rename_file(&mut self, id: i64, base: &str) -> Result<()> {
        let before = self.active(id)?;
        let source = self.context.resolve(&before.path, Access::Source)?;
        let name = rename_name(&source.name, base)?;
        if name == source.name {
            return Ok(());
        }
        let path = std::path::Path::new(&before.path)
            .with_file_name(&name)
            .to_str()
            .unwrap()
            .to_owned();
        let target = self.context.resolve(&path, Access::RenameTarget)?;
        let mut after = before.clone();
        after.path = path;
        after.name = name;
        self.transfer(before, after, source, target, Operation::Rename)
    }
    pub fn move_file_to_folder(&mut self, id: i64, folder_id: Option<i64>) -> Result<()> {
        let before = self.active(id)?;
        if folder_id == before.folder_id {
            return Ok(());
        }
        let mut after = before.clone();
        after.folder_id = folder_id;
        let Some(folder) = folder_id else {
            return self.db_only(before, Some(after));
        };
        let folder_path: String = self.db.with_conn(|c| {
            c.query_row("SELECT path FROM folders WHERE id=?1", [folder], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or(Error::NotFound)
        })?;
        let source = self.context.resolve(&before.path, Access::Source)?;
        after.path = std::path::Path::new(&folder_path)
            .join(&source.name)
            .to_str()
            .ok_or(Error::PathUnresolvable)?
            .to_owned();
        let target = self.context.resolve(&after.path, Access::MoveTarget)?;
        if before.path == after.path {
            return self.db_only(before, Some(after));
        }
        self.transfer(before, after, source, target, Operation::Move)
    }
    pub fn delete_file(&mut self, id: i64) -> Result<()> {
        let before = self.active(id)?;
        let mut after = before.clone();
        after.deleted_at = Some(chrono::Utc::now().to_rfc3339());
        after.trash_path = None;
        let source = match self.context.resolve(&before.path, Access::Source) {
            Ok(r) => r,
            Err(Error::NotFound) => return self.db_only(before, Some(after)),
            Err(e) => return Err(e),
        };
        match self.check_source(&source, id) {
            Err(Error::NotFound) => return self.db_only(before, Some(after)),
            Err(e) => return Err(e),
            Ok(_) => (),
        }
        // transfer derives the final trash name and intent from one operation ID.
        let target = self.context.resolve_relative(
            source.root,
            &format!("{TRASH}/pending"),
            Access::Trash,
        )?;
        self.transfer(before, after, source, target, Operation::Trash)
    }
    fn trash_source(&self, s: &FileState) -> Result<Resolved> {
        let (root, _) = self
            .context
            .locate(&s.path)
            .map_err(|_| Error::TrashForeign)?;
        let path = s.trash_path.as_ref().ok_or(Error::TrashForeign)?;
        let source = self
            .context
            .resolve(path, Access::Trash)
            .map_err(|e| match e {
                Error::Internal => Error::Internal,
                Error::NotFound => Error::TrashMissing,
                _ => Error::TrashForeign,
            })?;
        if source.root != root || !source.name.starts_with(&format!("{}-", s.id)) {
            return Err(Error::TrashForeign);
        }
        match FileIdentity::read(&source) {
            Ok(i) if i.dev == self.context.roots[root].identity.dev => Ok(source),
            Err(Error::NotFound) => Err(Error::TrashMissing),
            Err(e @ (Error::Internal | Error::Changed)) => Err(e),
            _ => Err(Error::TrashForeign),
        }
    }
    pub fn restore_file(&mut self, id: i64) -> Result<()> {
        let before = self.trashed(id)?;
        let mut after = before.clone();
        after.deleted_at = None;
        after.trash_path = None;
        if before.trash_path.is_none() {
            return self.db_only(before, Some(after));
        }
        let source = self.trash_source(&before)?;
        let target = self
            .context
            .resolve(&before.path, Access::RestoreTarget)
            .map_err(|e| {
                if e == Error::NotFound {
                    Error::PathUnresolvable
                } else {
                    e
                }
            })?;
        self.transfer(before, after, source, target, Operation::Restore)
    }
    pub fn delete_file_permanently(&mut self, id: i64) -> Result<DeleteOutcome> {
        self.delete_permanently_before(id, None)
    }
    fn delete_permanently_before(
        &mut self,
        id: i64,
        cutoff: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> Result<DeleteOutcome> {
        let before = self.trashed(id)?;
        if let Some(cutoff) = cutoff {
            let deleted = chrono::DateTime::parse_from_rfc3339(
                before.deleted_at.as_deref().ok_or(Error::Changed)?,
            )
            .map_err(|_| Error::Changed)?;
            if deleted > cutoff {
                return Err(Error::Changed);
            }
        }
        if before.trash_path.is_none() {
            self.db_only(before, None)?;
            return Ok(DeleteOutcome {
                kept_on_disk: false,
            });
        }
        let source = match self.trash_source(&before) {
            Ok(r) => r,
            Err(e @ (Error::TrashForeign | Error::TrashMissing)) => {
                self.db_only(before, None)?;
                return Ok(DeleteOutcome {
                    kept_on_disk: e == Error::TrashForeign,
                });
            }
            Err(e) => return Err(e),
        };
        let identity = self.check_source(&source, id)?;
        let root = &self.context.roots[source.root];
        let intent = Intent {
            op_id: journal::op_id()?,
            operation: Operation::Unlink,
            phase: Phase::Prepared,
            root_path: root.path.to_str().unwrap().to_owned(),
            root_identity: root.identity,
            source: Some(source.relative.clone()),
            target: None,
            temporary: None,
            identity: Some(identity),
            before,
            after: None,
        };
        self.journal.add(intent.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
        if FileIdentity::read(&source) != Ok(identity) {
            self.journal.remove(&intent.op_id)?;
            return Err(Error::Changed);
        }
        if let Err(e) = source.parent.unlink(&source.name) {
            self.journal.remove(&intent.op_id)?;
            return Err(e.into());
        }
        if source.parent.sync().is_err() {
            return Err(self.journal.block(&intent.op_id));
        }
        self.journal.phase(&intent.op_id, Phase::Unlinked)?;
        if self.db.with_conn(|c| commit(c, &intent)).is_err() {
            return Err(self.journal.block(&intent.op_id));
        }
        self.finish(&intent)?;
        Ok(DeleteOutcome {
            kept_on_disk: false,
        })
    }
    fn batch(
        &mut self,
        ids: &[i64],
        mut allowed: impl FnMut() -> bool,
        mut action: impl FnMut(&mut Self, i64) -> Result<Option<bool>>,
    ) -> Result<Vec<ItemResult>> {
        self.ready()?;
        if ids.len() > 200 || ids.iter().any(|i| *i <= 0) {
            return Err(Error::InvalidArguments);
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut seen = HashSet::new();
        let mut stopped = false;
        let mut out = Vec::new();
        for &id in ids {
            if !seen.insert(id) {
                continue;
            }
            stopped |= Instant::now() >= deadline || !allowed() || self.journal.is_blocked();
            let r = if stopped {
                Err(Error::NotStarted)
            } else {
                action(self, id)
            };
            out.push(match r {
                Ok(kept) => ItemResult {
                    id: id.to_string(),
                    ok: true,
                    kind: None,
                    kept_on_disk: kept,
                },
                Err(e) => ItemResult {
                    id: id.to_string(),
                    ok: false,
                    kind: Some(e),
                    kept_on_disk: None,
                },
            });
        }
        Ok(out)
    }
    pub fn delete_files(
        &mut self,
        ids: &[i64],
        allowed: impl FnMut() -> bool,
    ) -> Result<Vec<ItemResult>> {
        self.batch(ids, allowed, |s, id| s.delete_file(id).map(|_| None))
    }
    pub fn move_files_to_folder(
        &mut self,
        ids: &[i64],
        folder: Option<i64>,
        allowed: impl FnMut() -> bool,
    ) -> Result<Vec<ItemResult>> {
        self.batch(ids, allowed, |s, id| {
            s.move_file_to_folder(id, folder).map(|_| None)
        })
    }
    pub fn empty_trash_summary(&mut self) -> Result<TrashSummary> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT deleted_at,file_size_bytes FROM files WHERE deleted_at IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            let mut count = 0;
            let mut bytes = 0u64;
            let mut max = None;
            for row in rows {
                let (time, size) = row?;
                let time =
                    chrono::DateTime::parse_from_rfc3339(&time).map_err(|_| Error::Changed)?;
                count += 1;
                bytes = bytes.saturating_add(size.max(0) as u64);
                max = Some(
                    max.map_or(time, |old: chrono::DateTime<chrono::FixedOffset>| {
                        old.max(time)
                    }),
                );
            }
            Ok(TrashSummary {
                count,
                bytes,
                max_deleted_at: max.map(|t| t.to_rfc3339()),
            })
        })
    }
    pub fn empty_trash_snapshot(
        &mut self,
        expected_count: usize,
        max_deleted_at: Option<&str>,
    ) -> Result<TrashSnapshot> {
        self.ready()?;
        let cutoff = max_deleted_at
            .map(chrono::DateTime::parse_from_rfc3339)
            .transpose()
            .map_err(|_| Error::InvalidArguments)?;
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT id,deleted_at FROM files WHERE deleted_at IS NOT NULL ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            let mut ids = Vec::new();
            for row in rows {
                let (id, time) = row?;
                let time =
                    chrono::DateTime::parse_from_rfc3339(&time).map_err(|_| Error::Changed)?;
                if cutoff.is_some_and(|t| time <= t) {
                    ids.push(id);
                }
            }
            if ids.len() != expected_count {
                return Err(Error::Conflict);
            }
            Ok(TrashSnapshot {
                ids,
                offset: 0,
                cutoff,
            })
        })
    }
    pub fn empty_trash(
        &mut self,
        snapshot: &mut TrashSnapshot,
        allowed: impl FnMut() -> bool,
    ) -> Result<Vec<ItemResult>> {
        let end = (snapshot.offset + 200).min(snapshot.ids.len());
        let result = self.batch(&snapshot.ids[snapshot.offset..end], allowed, |s, id| {
            s.delete_permanently_before(id, snapshot.cutoff)
                .map(|r| Some(r.kept_on_disk))
                .map_err(|e| {
                    if e == Error::Conflict {
                        Error::Changed
                    } else {
                        e
                    }
                })
        })?;
        // Only untouched IDs may be retried in a subsequent client-driven block.
        // Attempted failures are terminal: never retry an operation with possible side effects.
        snapshot.offset += result
            .iter()
            .take_while(|r| r.kind != Some(Error::NotStarted))
            .count();
        Ok(result)
    }
}
// File handles pin parents, but a local writer may concurrently relocate an ancestor.
// File operations accept this residual risk; directory operations recheck parent chains.
fn rename_checked(source: &Resolved, target: &Resolved, identity: FileIdentity) -> Result<()> {
    if FileIdentity::read(source)? != identity {
        return Err(Error::Changed);
    }
    source
        .parent
        .rename_noreplace(&source.name, &target.parent, &target.name)?;
    let actual = target
        .parent
        .entry_identity(&target.name)
        .map_err(|_| Error::RestartRequired)?;
    if actual != (identity.dev, identity.ino) {
        let _ = target
            .parent
            .rename_noreplace(&target.name, &source.parent, &source.name);
        return Err(Error::RestartRequired);
    }
    if source.parent.sync().is_err() || target.parent.sync().is_err() {
        return Err(Error::RestartRequired);
    }
    Ok(())
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    /// Always call in write mode when a journal exists, even with fileops disabled.
    /// Returns detach IDs without touching UploadStore. Call finish_recovery only after
    /// the upload journal has been loaded without reconciliation, then reconcile it.
    pub fn recover(&mut self) -> Result<RecoveryReport> {
        let mut report = RecoveryReport::default();
        for intent in self.journal.intents.clone() {
            if intent.phase == Phase::Manual {
                report.manual.push(intent.op_id);
                continue;
            }
            match self.recover_one(&intent) {
                Ok(true) => report.detach_ids.push(intent.before.id),
                Ok(false) => (),
                Err(_) => {
                    self.journal.block(&intent.op_id);
                    report.manual.push(intent.op_id);
                }
            }
        }
        self.recover_folders(&mut report);
        self.journal.recovery_complete();
        Ok(report)
    }
    pub fn finish_recovery(&mut self) -> Result<()> {
        self.finish_folder_recovery()?;
        for intent in self.journal.intents.clone() {
            if intent.phase == Phase::Committed {
                if self.ledger.detach(&[intent.before.id]).is_err() {
                    return Err(self.journal.block(&intent.op_id));
                }
                self.journal.remove(&intent.op_id)?;
            }
        }
        self.journal.recovery_complete();
        self.ready()
    }
    fn recover_one(&mut self, i: &Intent) -> Result<bool> {
        let current = self.db.with_conn(|c| read_state(c, i.before.id))?;
        let db_before = current.as_ref() == Some(&i.before);
        let db_after = current == i.after;
        if i.after.as_ref().is_some_and(|a| a.id != i.before.id) {
            return Err(Error::Changed);
        }
        if i.operation == Operation::DbOnly {
            if i.source.is_some() || i.target.is_some() || i.identity.is_some() {
                return Err(Error::Changed);
            }
            if db_after {
                self.journal.phase(&i.op_id, Phase::Committed)?;
                return Ok(true);
            }
            if db_before {
                self.journal.remove(&i.op_id)?;
                return Ok(false);
            }
            return Err(Error::Changed);
        }
        let root = self
            .context
            .roots
            .iter()
            .position(|r| r.path.to_str() == Some(&i.root_path) && r.identity == i.root_identity)
            .ok_or(Error::Changed)?;
        let source = self.context.resolve_relative(
            root,
            i.source.as_deref().ok_or(Error::Changed)?,
            if matches!(i.operation, Operation::Restore | Operation::Unlink) {
                Access::Trash
            } else {
                Access::Source
            },
        )?;
        let identity = i.identity.ok_or(Error::Changed)?;
        let s = observe(&source, identity)?;
        let target = i
            .target
            .as_ref()
            .map(|r| {
                self.context.resolve_relative(
                    root,
                    r,
                    match i.operation {
                        Operation::Trash => Access::Trash,
                        Operation::Restore => Access::RestoreTarget,
                        Operation::Rename => Access::RenameTarget,
                        _ => Access::MoveTarget,
                    },
                )
            })
            .transpose();
        let temp = i
            .temporary
            .as_ref()
            .map(|r| self.context.resolve_relative(root, r, Access::RenameTarget))
            .transpose();
        if db_before && s == Observed::Matches {
            let matches = |r: &Result<Option<Resolved>>| {
                r.as_ref()
                    .ok()
                    .and_then(|r| r.as_ref())
                    .is_some_and(|r| observe(r, identity) == Ok(Observed::Matches))
            };
            if !matches(&target) && !matches(&temp) {
                self.journal.remove(&i.op_id)?;
                return Ok(false);
            }
        }
        let target = target?;
        let temp = temp?;
        if identity.dev != i.root_identity.dev {
            return Err(Error::Changed);
        }
        let expected_source = if matches!(i.operation, Operation::Restore | Operation::Unlink) {
            i.before.trash_path.as_ref().ok_or(Error::Changed)?
        } else {
            &i.before.path
        };
        if self.context.absolute(&source) != *expected_source {
            return Err(Error::Changed);
        }
        if let Some(t) = &target {
            let after = i.after.as_ref().ok_or(Error::Changed)?;
            let expected_target = if i.operation == Operation::Trash {
                after.trash_path.as_ref().ok_or(Error::Changed)?
            } else {
                &after.path
            };
            if self.context.absolute(t) != *expected_target {
                return Err(Error::Changed);
            }
        }
        if let Some(t) = &temp {
            if i.operation != Operation::Rename
                || std::path::Path::new(&t.relative).parent()
                    != std::path::Path::new(&source.relative).parent()
                || t.name != format!(".fsops-{}.tmp", i.op_id)
            {
                return Err(Error::Changed);
            }
        }
        let t = target
            .as_ref()
            .map(|r| observe(r, identity))
            .transpose()?
            .unwrap_or(Observed::Absent);
        let tmp = temp
            .as_ref()
            .map(|r| observe(r, identity))
            .transpose()?
            .unwrap_or(Observed::Absent);
        // Foreign temporary entries are not ours to delete. Preserve the intent for inspection.
        if tmp == Observed::Foreign {
            return Err(Error::Changed);
        }
        let decision = recovery_decision(RecoveryFacts {
            trusted: true,
            db_before,
            db_after,
            source_matches: s == Observed::Matches,
            source_absent: s == Observed::Absent,
            target_matches: t == Observed::Matches,
            temporary_matches: tmp == Observed::Matches,
            unlink: i.operation == Operation::Unlink,
            db_only: false,
        });
        match decision {
            RecoveryDecision::Discard => {
                self.journal.remove(&i.op_id)?;
                Ok(false)
            }
            RecoveryDecision::Committed => {
                source.parent.sync()?;
                if let Some(t) = target {
                    t.parent.sync()?;
                }
                self.journal.phase(&i.op_id, Phase::Committed)?;
                Ok(true)
            }
            RecoveryDecision::Finish | RecoveryDecision::FinishUnlink => {
                source.parent.sync()?;
                if let Some(t) = target {
                    t.parent.sync()?;
                }
                self.db.with_conn(|c| commit(c, i))?;
                self.journal.phase(&i.op_id, Phase::Committed)?;
                Ok(true)
            }
            RecoveryDecision::FinishTemporary => {
                rename_checked(
                    temp.as_ref().unwrap(),
                    target.as_ref().ok_or(Error::Changed)?,
                    identity,
                )?;
                self.db.with_conn(|c| commit(c, i))?;
                self.journal.phase(&i.op_id, Phase::Committed)?;
                Ok(true)
            }
            RecoveryDecision::RollbackTemporary => {
                rename_checked(temp.as_ref().unwrap(), &source, identity)?;
                self.journal.remove(&i.op_id)?;
                Ok(false)
            }
            RecoveryDecision::Manual => Err(Error::Changed),
        }
    }
}
#[derive(PartialEq, Eq)]
enum Observed {
    Absent,
    Matches,
    Foreign,
}
fn observe(r: &Resolved, identity: FileIdentity) -> Result<Observed> {
    // Exact spelling matters on case-insensitive filesystems.
    if !r.parent.names_checked()?.iter().any(|n| n == &r.name) {
        return Ok(Observed::Absent);
    }
    match FileIdentity::read(r) {
        Ok(actual) if actual == identity => Ok(Observed::Matches),
        Ok(_) => Ok(Observed::Foreign),
        Err(Error::NotFound) => Ok(Observed::Absent),
        Err(Error::PathUnresolvable) => Ok(Observed::Foreign),
        Err(e) => Err(e),
    }
}
