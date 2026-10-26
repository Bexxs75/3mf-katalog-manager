use super::carry::{apply_steps, read_companion_rows, CarryPlan};
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
    fn reset_imported(&mut self, file_ids: &[i64]) -> Result<()>;
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
    /// Companion files that went to the trash, came back, or were removed with the model.
    #[serde(skip_serializing_if = "is_zero")]
    pub companions_moved: usize,
    /// Companion files a restore left in the trash because they are not ours to move.
    #[serde(skip_serializing_if = "is_zero")]
    pub companions_kept_in_trash: usize,
}
fn is_zero(n: &usize) -> bool {
    *n == 0
}
/// What the last file operation did with companions; read it after the call returned `Ok`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarryOutcome {
    pub moved: usize,
    pub kept_in_trash: usize,
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
    /// Companion files removed on their own; they follow the models, block by block.
    loose: Vec<i64>,
    loose_offset: usize,
}
impl TrashSnapshot {
    pub fn ids(&self) -> &[i64] { &self.ids }
    /// Model ids of the next block; empty once every model is done.
    pub fn next_block_ids(&self) -> &[i64] {
        let end = self.offset.saturating_add(200).min(self.ids.len());
        self.ids.get(self.offset..end).unwrap_or(&[])
    }
    /// Companion row ids of the next block; only offered after the last model block,
    /// so one block never mixes the two kinds.
    pub fn next_loose_block_ids(&self) -> &[i64] {
        if self.offset < self.ids.len() {
            return &[];
        }
        let end = self.loose_offset.saturating_add(200).min(self.loose.len());
        self.loose.get(self.loose_offset..end).unwrap_or(&[])
    }
    pub fn loose_ids(&self) -> &[i64] {
        &self.loose
    }
    pub fn loose_count(&self) -> usize {
        self.loose.len()
    }
    pub fn remaining(&self) -> usize {
        self.ids.len().saturating_sub(self.offset) + self.loose.len().saturating_sub(self.loose_offset)
    }
}
#[derive(Debug, Serialize)]
pub struct TrashSummary {
    pub count: usize,
    pub blocked_by_companions: usize,
    /// Companion files in the web trash that go with the counted entries.
    pub companion_count: usize,
    /// Companion files removed on their own that emptying the trash deletes as well.
    pub loose_companion_count: usize,
    pub loose_companion_bytes: u64,
    pub bytes: u64,
    pub max_deleted_at: Option<String>,
}
pub struct FileOps<D, L> {
    pub context: FsContext,
    pub journal: Journal,
    pub db: D,
    pub ledger: L,
    pub outcome: CarryOutcome,
}
pub(super) fn read_state(c: &Connection, id: i64) -> Result<Option<FileState>> {
    #[cfg(test)]
    super::tests::file_state_read();
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
/// Operations that move, trash, restore or delete a model must see the companion rows
/// exactly as the journal remembers them; a database-only move leaves `_extras` alone.
fn touches_companions(intent: &Intent) -> bool {
    match intent.operation {
        Operation::DbOnly => intent
            .after
            .as_ref()
            .is_none_or(|a| a.deleted_at != intent.before.deleted_at),
        _ => true,
    }
}
fn commit(c: &mut Connection, intent: &Intent) -> Result<()> {
    if intent.operation.is_companion_edit() {
        return super::companion_edit::commit_companion_edit(c, intent);
    }
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if read_state(&tx, intent.before.id)?.as_ref() != Some(&intent.before) {
        return Err(Error::Changed);
    }
    // The companions moved on disk under the picture taken before the journal entry.
    // A row that appeared, vanished or changed since then would be stranded or
    // overwritten, so the whole operation is undone instead. Recovery runs the same
    // check: it is cheap and the disk side is already settled by then.
    if touches_companions(intent) && read_companion_rows(&tx, intent.before.id)? != intent.companion_rows {
        return Err(Error::CompanionsChanged);
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
        apply_steps(&tx, &intent.companions)?;
    } else {
        // Explicit, so the rows do not depend on a connection having foreign keys on.
        tx.execute("DELETE FROM companion_files WHERE file_id=?1", [intent.before.id])?;
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
            outcome: CarryOutcome::default(),
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
    /// True when a permanent delete of this trashed model would be refused because a
    /// companion row cannot be handled here; the web UI shows such entries as desktop-only.
    pub fn blocked_by_companions(&mut self, id: i64) -> Result<bool> {
        let s = self.state(id)?;
        if s.deleted_at.is_none() {
            return Ok(false);
        }
        match self.plan_purge(&s) {
            Err(Error::HasCompanions) => Ok(true),
            Err(e) => Err(e),
            Ok(_) => Ok(false),
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
        self.journal.remove(&intent.op_id)?;
        self.cleanup_old_extras(intent);
        Ok(())
    }
    /// Changes the database only. Companion files that still exist on disk cannot be
    /// handled without the model's own move, so such a plan is refused.
    fn db_only(&mut self, before: FileState, after: Option<FileState>, carry: CarryPlan) -> Result<()> {
        if carry.steps.iter().any(|s| s.source.is_some()) {
            return Err(Error::HasCompanions);
        }
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
            companions: carry.steps,
            extras_dir: None,
            companion_rows: carry.rows,
            companion_add: None,
        };
        self.journal.add(intent.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
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
        mut carry: CarryPlan,
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
            self.name_trash_steps(&mut carry.steps, source.root, &op_id)?;
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
        let mut intent = Intent {
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
            companions: carry.steps,
            extras_dir: carry.extras_dir,
            companion_rows: carry.rows,
            companion_add: None,
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
        // Companions first, so that "model at its destination" implies "companions at theirs".
        if !intent.companions.is_empty() || intent.extras_dir.is_some() {
            self.carry_forward(&mut intent, source.root)?;
        }
        if let Some(t) = &temp {
            if let Err(e) = rename_checked(&source, t, identity) {
                return self.transfer_failed(&intent, source.root, e);
            }
            self.journal.phase(&intent.op_id, Phase::TmpRenamed)?;
        }
        let actual = temp.as_ref().unwrap_or(&source);
        if let Err(e) = rename_checked(actual, &target, identity) {
            if temp.is_some() && rename_checked(actual, &source, identity).is_err() {
                return Err(self.journal.block(&intent.op_id));
            }
            return self.transfer_failed(&intent, source.root, e);
        }
        self.journal.phase(&intent.op_id, Phase::Renamed)?;
        #[cfg(test)]
        if super::tests::at_point("before_commit", 0) {
            return Err(self.journal.simulate_crash());
        }
        if let Err(e) = self.db.with_conn(|c| commit(c, &intent)) {
            let rollback = if let Some(temp) = &temp {
                rename_checked(&target, temp, identity)
                    .and_then(|()| rename_checked(temp, &source, identity))
            } else {
                rename_checked(&target, &source, identity)
            };
            if rollback.is_err() || self.rollback_carry(&intent, source.root).is_err() {
                return Err(self.journal.block(&intent.op_id));
            }
            self.journal.remove(&intent.op_id)?;
            return Err(e);
        }
        self.finish(&intent)
    }
    fn transfer_failed(&mut self, i: &Intent, root: usize, e: Error) -> Result<()> {
        if matches!(e, Error::RestartRequired) {
            return Err(self.journal.block(&i.op_id));
        }
        if self.rollback_carry(i, root).is_err() {
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
        let carry = self.plan_carry(&before, &path, CompanionOp::Rename)?;
        let mut after = before.clone();
        after.path = path;
        after.name = name;
        self.transfer(before, after, source, target, Operation::Rename, carry)
    }
    pub fn move_file_to_folder(&mut self, id: i64, folder_id: Option<i64>) -> Result<()> {
        let before = self.active(id)?;
        if folder_id == before.folder_id {
            return Ok(());
        }
        let mut after = before.clone();
        after.folder_id = folder_id;
        let Some(folder) = folder_id else {
            return self.db_only(before, Some(after), CarryPlan::default());
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
            return self.db_only(before, Some(after), CarryPlan::default());
        }
        // Only a real disk move takes companions along; database-only changes leave `_extras` where it is.
        let carry = self.plan_carry(&before, &after.path, CompanionOp::Move)?;
        self.transfer(before, after, source, target, Operation::Move, carry)
    }
    pub fn delete_file(&mut self, id: i64) -> Result<()> {
        let before = self.active(id)?;
        self.outcome = CarryOutcome::default();
        let deleted_at = chrono::Utc::now().to_rfc3339();
        let carry = self.plan_trash(&before, &deleted_at)?;
        let mut after = before.clone();
        after.deleted_at = Some(deleted_at);
        after.trash_path = None;
        let source = match self.context.resolve(&before.path, Access::Source) {
            Ok(r) => r,
            Err(Error::NotFound) => return self.db_only(before, Some(after), carry),
            Err(e) => return Err(e),
        };
        match self.check_source(&source, id) {
            Err(Error::NotFound) => return self.db_only(before, Some(after), carry),
            Err(e) => return Err(e),
            Ok(_) => (),
        }
        // transfer derives the final trash name and intent from one operation ID.
        let target = self.context.resolve_relative(
            source.root,
            &format!("{TRASH}/pending"),
            Access::Trash,
        )?;
        self.outcome.moved = carry.steps.len();
        self.transfer(before, after, source, target, Operation::Trash, carry)
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
        self.outcome = CarryOutcome::default();
        let carry = self.plan_restore(&before)?;
        self.outcome = CarryOutcome {
            moved: carry.steps.len(),
            kept_in_trash: carry.kept_in_trash,
        };
        let mut after = before.clone();
        after.deleted_at = None;
        after.trash_path = None;
        if before.trash_path.is_none() {
            return self.db_only(before, Some(after), carry);
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
        // A taken model path is the common refusal; find it before any companion moves.
        if carry.steps.iter().any(|s| s.source.is_some())
            && !matches!(target.parent.entry_identity(&target.name), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        {
            return Err(Error::Conflict);
        }
        self.transfer(before, after, source, target, Operation::Restore, carry)
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
        self.outcome = CarryOutcome::default();
        let carry = self.plan_purge(&before)?;
        if let Some(cutoff) = cutoff {
            let deleted = chrono::DateTime::parse_from_rfc3339(
                before.deleted_at.as_deref().ok_or(Error::Changed)?,
            )
            .map_err(|_| Error::Changed)?;
            if deleted > cutoff {
                return Err(Error::Changed);
            }
        }
        let kept_by_plan = carry.kept_on_disk;
        if before.trash_path.is_none() {
            self.db_only(before, None, carry)?;
            return Ok(DeleteOutcome {
                kept_on_disk: kept_by_plan,
            });
        }
        let (source, identity, model_present) = match self.trash_source(&before) {
            Ok(r) => {
                let identity = self.check_source(&r, id)?;
                (r, identity, true)
            }
            // The model's file is already gone but companion files are not: the journalled
            // delete still has to remove them. Recovery sees the missing model as "unlinked".
            Err(Error::TrashMissing) if !carry.steps.is_empty() => {
                let r = self.missing_trash_place(&before)?;
                let dev = self.context.roots[r.root].identity.dev;
                (r, FileIdentity { dev, ino: 0, size: 0 }, false)
            }
            Err(e @ (Error::TrashForeign | Error::TrashMissing)) => {
                self.db_only(before, None, carry)?;
                return Ok(DeleteOutcome {
                    kept_on_disk: kept_by_plan || e == Error::TrashForeign,
                });
            }
            Err(e) => return Err(e),
        };
        self.outcome.moved = carry.steps.len();
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
            companions: carry.steps,
            extras_dir: None,
            companion_rows: carry.rows,
            companion_add: None,
        };
        self.journal.add(intent.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
        // Once a file is gone nothing can be undone, so the rows are looked at again
        // right before the first unlink: a row that appeared meanwhile has a file that
        // this delete does not know about.
        // The model row and the companion rows are read in one transaction, so both
        // are judged against the same moment.
        let checked = self.db.with_conn(|c| {
            let tx = c.transaction()?;
            if read_state(&tx, id)?.as_ref() != Some(&intent.before) {
                return Err(Error::Changed);
            }
            if read_companion_rows(&tx, id)? != intent.companion_rows {
                return Err(Error::CompanionsChanged);
            }
            Ok(())
        });
        if let Err(e) = checked {
            self.journal.remove(&intent.op_id)?;
            return Err(e);
        }
        // Companions first, the model last. From the first unlink on a failure leaves
        // the entry for recovery to finish.
        let mut gone = 0usize;
        for (k, step) in intent.companions.iter().enumerate() {
            let (Some(relative), Some(file)) = (&step.source, step.identity) else {
                continue;
            };
            if let Err(e) = self.unlink_companion(source.root, relative, file) {
                if gone == 0 {
                    self.journal.remove(&intent.op_id)?;
                    return Err(e);
                }
                return Err(self.journal.block(&intent.op_id));
            }
            gone += 1;
            #[cfg(test)]
            if super::tests::at_point("after_unlink", k) {
                return Err(self.journal.simulate_crash());
            }
            let _ = k;
        }
        if gone > 0 {
            self.journal.phase(&intent.op_id, Phase::CompanionsMoved)?;
        }
        if model_present && FileIdentity::read(&source) != Ok(identity) {
            if gone == 0 {
                self.journal.remove(&intent.op_id)?;
                return Err(Error::Changed);
            }
            return Err(self.journal.block(&intent.op_id));
        }
        if model_present {
            if let Err(e) = source.parent.unlink(&source.name) {
                if gone == 0 {
                    self.journal.remove(&intent.op_id)?;
                    return Err(e.into());
                }
                return Err(self.journal.block(&intent.op_id));
            }
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
            kept_on_disk: kept_by_plan,
        })
    }
    /// Where a trashed model's file would be, for a model whose file is gone.
    fn missing_trash_place(&self, s: &FileState) -> Result<Resolved> {
        let (root, _) = self.context.locate(&s.path).map_err(|_| Error::TrashForeign)?;
        let path = s.trash_path.as_ref().ok_or(Error::TrashForeign)?;
        let place = self.context.resolve(path, Access::Trash).map_err(|_| Error::TrashForeign)?;
        if place.root != root || !place.name.starts_with(&format!("{}-", s.id)) {
            return Err(Error::TrashForeign);
        }
        Ok(place)
    }
    /// Removes one companion from the trash, but only the very inode the journal names.
    pub(super) fn unlink_companion(&mut self, root: usize, relative: &str, identity: FileIdentity) -> Result<()> {
        let r = self.context.resolve_relative(root, relative, Access::Trash)?;
        if FileIdentity::read(&r) != Ok(identity) {
            return Err(Error::Changed);
        }
        r.parent.unlink(&r.name)?;
        Ok(())
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
            self.outcome = CarryOutcome::default();
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
                    companions_moved: self.outcome.moved,
                    companions_kept_in_trash: self.outcome.kept_in_trash,
                },
                Err(e) => ItemResult {
                    id: id.to_string(),
                    ok: false,
                    kind: Some(e),
                    kept_on_disk: None,
                    companions_moved: 0,
                    companions_kept_in_trash: 0,
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
    fn trash_roots(&self) -> Vec<std::path::PathBuf> {
        self.context.roots.iter().map(|r| r.path.clone()).collect()
    }
    pub fn empty_trash_summary(&mut self) -> Result<TrashSummary> {
        let roots = self.trash_roots();
        self.db.with_conn(|c| trash_summary(c, &roots))
    }
    pub fn empty_trash_snapshot(
        &mut self,
        expected_count: usize,
        max_deleted_at: Option<&str>,
    ) -> Result<TrashSnapshot> {
        self.empty_trash_snapshot_with(expected_count, 0, max_deleted_at)
    }
    /// `expected_loose` is the number of individually removed companion files the
    /// caller was shown; the snapshot refuses a different number.
    pub fn empty_trash_snapshot_with(
        &mut self,
        expected_count: usize,
        expected_loose: usize,
        max_deleted_at: Option<&str>,
    ) -> Result<TrashSnapshot> {
        self.ready()?;
        let cutoff = max_deleted_at
            .map(chrono::DateTime::parse_from_rfc3339)
            .transpose()
            .map_err(|_| Error::InvalidArguments)?;
        let roots = self.trash_roots();
        self.db.with_conn(|c| {
            let mut ids = Vec::new();
            for entry in trash_entries(c, &roots)? {
                let time = chrono::DateTime::parse_from_rfc3339(&entry.deleted_at)
                    .map_err(|_| Error::Changed)?;
                // Entries a permanent delete would refuse are not offered for emptying.
                if !entry.blocked && cutoff.is_some_and(|t| time <= t) {
                    ids.push(entry.id);
                }
            }
            if ids.len() != expected_count {
                return Err(Error::Conflict);
            }
            let mut loose = Vec::new();
            for entry in loose_companions(c, &roots)? {
                let time = chrono::DateTime::parse_from_rfc3339(&entry.deleted_at)
                    .map_err(|_| Error::Changed)?;
                if cutoff.is_some_and(|t| time <= t) {
                    loose.push(entry.id);
                }
            }
            if loose.len() != expected_loose {
                return Err(Error::Conflict);
            }
            Ok(TrashSnapshot {
                ids,
                offset: 0,
                cutoff,
                loose,
                loose_offset: 0,
            })
        })
    }
    pub fn empty_trash(
        &mut self,
        snapshot: &mut TrashSnapshot,
        allowed: impl FnMut() -> bool,
    ) -> Result<Vec<ItemResult>> {
        if snapshot.offset >= snapshot.ids.len() && snapshot.loose_offset < snapshot.loose.len() {
            let cutoff = snapshot.cutoff;
            let mut result = self.batch(snapshot.next_loose_block_ids(), allowed, |s, id| {
                s.purge_loose_companion(id, cutoff).map(|()| None)
            })?;
            snapshot.loose_offset += result
                .iter()
                .take_while(|r| r.kind != Some(Error::NotStarted))
                .count();
            // Row ids of companion files are not model ids; a prefix keeps clients from mixing them up.
            for r in &mut result {
                r.id = format!("c{}", r.id);
            }
            return Ok(result);
        }
        let result = self.batch(snapshot.next_block_ids(), allowed, |s, id| {
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
pub(super) fn rename_checked(source: &Resolved, target: &Resolved, identity: FileIdentity) -> Result<()> {
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
                self.cleanup_old_extras(&intent);
            }
        }
        self.journal.recovery_complete();
        self.ready()
    }
    fn recover_one(&mut self, i: &Intent) -> Result<bool> {
        if i.operation.is_companion_edit() {
            // The model stays as it is, so nothing is detached from the upload ledger.
            self.recover_companion_edit(i)?;
            return Ok(false);
        }
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
            // The companion rows change in the same transaction, so they are either all
            // as before or all as after; anything else is not ours to settle.
            let rows = self.observe_carry(i, 0)?;
            if db_after && rows.db_after {
                self.journal.phase(&i.op_id, Phase::Committed)?;
                return Ok(true);
            }
            if db_before && rows.db_before {
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
        let carried = self.observe_carry(i, root)?;
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
        if db_before && s == Observed::Matches && carried.all_source {
            let matches = |r: &Result<Option<Resolved>>| {
                r.as_ref()
                    .ok()
                    .and_then(|r| r.as_ref())
                    .is_some_and(|r| observe(r, identity) == Ok(Observed::Matches))
            };
            if !matches(&target) && !matches(&temp) {
                self.remove_created_extras(i, root);
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
            companions_db_before: carried.db_before,
            companions_db_after: carried.db_after,
            companions_all_source: carried.all_source,
            companions_all_target: carried.all_target,
            companions_mixed: carried.mixed,
            companions_any: carried.any,
        });
        match decision {
            RecoveryDecision::Discard => {
                self.remove_created_extras(i, root);
                self.journal.remove(&i.op_id)?;
                Ok(false)
            }
            RecoveryDecision::RollbackCompanions => {
                self.rollback_carry(i, root)?;
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
                if decision == RecoveryDecision::FinishUnlink && i.operation == Operation::Unlink {
                    self.finish_unlink(i, root, &source, identity)?;
                }
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
impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    /// Removes what is left of an interrupted permanent delete: every companion still
    /// in the trash under its journalled identity, then the model. Anything else at
    /// those places is not ours and stops recovery.
    fn finish_unlink(&mut self, i: &Intent, root: usize, model: &Resolved, identity: FileIdentity) -> Result<()> {
        for (step_index, step) in i.companions.iter().enumerate() {
            let (Some(relative), Some(file)) = (&step.source, step.identity) else {
                continue;
            };
            match self.observe_rel(root, relative, Access::Trash, file)? {
                Observed::Absent => (),
                Observed::Matches => self.unlink_companion(root, relative, file)?,
                Observed::Foreign => return Err(Error::Changed),
            }
            #[cfg(test)]
            if super::tests::at_point("recovery_unlink", step_index) {
                return Err(Error::RestartRequired);
            }
            let _ = step_index;
        }
        if observe(model, identity)? == Observed::Matches {
            model.parent.unlink(&model.name)?;
        }
        Ok(())
    }
}
#[derive(PartialEq, Eq)]
pub(super) enum Observed {
    Absent,
    Matches,
    Foreign,
}
pub(super) fn observe(r: &Resolved, identity: FileIdentity) -> Result<Observed> {
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

#[cfg(test)]
mod snapshot_accessor_tests {
    use super::*;
    #[test]
    fn snapshot_blocks_come_only_from_its_own_ids_and_never_underflow() {
        let mut s = TrashSnapshot { ids: (0..450).collect(), offset: 0, cutoff: None, loose: Vec::new(), loose_offset: 0 };
        assert_eq!(s.next_block_ids().len(), 200);
        assert_eq!(s.remaining(), 450);
        s.offset = 400;
        assert_eq!(s.next_block_ids(), &s.ids()[400..450]);
        // An inconsistent offset must degrade to "nothing left", not panic.
        s.offset = 900;
        assert!(s.next_block_ids().is_empty());
        assert_eq!(s.remaining(), 0);
    }
}
