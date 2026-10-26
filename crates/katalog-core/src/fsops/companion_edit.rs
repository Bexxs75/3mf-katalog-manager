//! A single companion file on its own: remove it to the web trash, bring it back, and
//! (when the trash is emptied) delete it for good. The model is never touched.
//!
//! Each of the three is one journal entry (format version 3) with exactly one
//! `CompanionStep`. The shape is the same as for a model: the file moves first, then
//! one database commit rewrites the row, so before the commit recovery goes back and
//! after it everything is final. Only a permanent delete has a point of no return, the
//! unlink, after which recovery goes forward.
//!
//! A removed file whose bytes are already gone from disk keeps its row, marked as
//! removed without a trash path, so that "undo" stays possible and emptying the trash
//! removes the row.

use super::carry::{read_companion, read_companion_rows, Probe};
use super::trash_carry::TrashFile;
use super::*;
use crate::companions::{extras_dir_for, kind_for_extension, lowercase_extension, validate_stored_fields, MAX_COMPANIONS_PER_MODEL};
use crate::companion_read::same_dir;
use rusqlite::{Connection, TransactionBehavior};
use std::path::Path;

/// What recovery does with an interrupted single-companion entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompanionEditDecision {
    /// Nothing happened yet (or only a folder this entry made): forget the entry.
    Discard,
    /// The file already moved but the row did not: put it back.
    Rollback,
    /// The row has its new state and the file is where it belongs.
    Committed,
    /// A permanent delete already unlinked the file: remove the row.
    FinishPurge,
    Manual,
}

#[derive(Clone, Copy, Debug)]
pub struct CompanionEditFacts {
    pub purge: bool,
    /// The row is exactly as the journal remembers it from before.
    pub db_before: bool,
    /// The row has its new state (purge: the row is gone).
    pub db_after: bool,
    /// A file travels (or is unlinked) at all.
    pub file_any: bool,
    /// The file is where it started (purge: not unlinked yet).
    pub file_at_source: bool,
    /// The file is where it ends (purge: already unlinked).
    pub file_at_target: bool,
}

pub const COMPANION_EDIT_TABLE: &[(CompanionEditDecision, &str)] = &[
    (CompanionEditDecision::Manual, "row neither before nor after, or the file is somewhere unexpected"),
    (CompanionEditDecision::Committed, "row after and the file at its destination"),
    (CompanionEditDecision::Discard, "row before and the file still at its source (or no file)"),
    (CompanionEditDecision::Rollback, "row before and the file already at its destination"),
    (CompanionEditDecision::FinishPurge, "permanent delete: row before and the file already unlinked"),
];

pub fn companion_edit_decision(f: CompanionEditFacts) -> CompanionEditDecision {
    use CompanionEditDecision::*;
    if f.db_after {
        // A purge deletes the row last, so a missing row means the file went first.
        return if f.purge || !f.file_any || f.file_at_target { Committed } else { Manual };
    }
    if !f.db_before {
        return Manual;
    }
    if !f.file_any || f.file_at_source {
        return Discard;
    }
    if f.file_at_target {
        return if f.purge { FinishPurge } else { Rollback };
    }
    Manual
}

/// The commit of a single-companion entry, inside one immediate transaction.
pub(super) fn commit_companion_edit(c: &mut Connection, i: &Intent) -> Result<()> {
    if i.operation == Operation::CompanionAdd {
        return super::companion_add::commit_companion_add(c, i).map(|_| ());
    }
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let step = i.companions.first().ok_or(Error::Internal)?;
    if i.operation == Operation::CompanionPurge {
        // The file is already gone; only the row is left to remove, and only if it is still
        // the very row that was judged (a restore or a rename in between must not lose it).
        let b = &step.before;
        let n = tx.execute(
            "DELETE FROM companion_files WHERE id=?1 AND file_id=?2 AND deleted_at IS ?3 AND trash_path IS ?4",
            rusqlite::params![b.id, b.file_id, b.deleted_at, b.trash_path],
        )?;
        if n != 1 {
            return Err(Error::Changed);
        }
        tx.commit()?;
        return Ok(());
    }
    if super::operations::read_state(&tx, i.before.id)?.as_ref() != Some(&i.before) {
        return Err(Error::Changed);
    }
    if read_companion_rows(&tx, i.before.id)? != i.companion_rows {
        return Err(Error::CompanionsChanged);
    }
    if i.operation == Operation::CompanionRestore {
        let active: i64 = tx.query_row(
            "SELECT COUNT(*) FROM companion_files WHERE file_id=?1 AND deleted_at IS NULL",
            [i.before.id],
            |r| r.get(0),
        )?;
        if active >= MAX_COMPANIONS_PER_MODEL {
            return Err(Error::CompanionLimit);
        }
    }
    super::carry::apply_steps(&tx, &i.companions)?;
    tx.commit()?;
    Ok(())
}

/// The row, its model and everything the three operations need to know first.
struct Subject {
    row: CompanionState,
    model: FileState,
    rows: Vec<CompanionState>,
    root: usize,
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    fn load_subject(&mut self, companion_id: i64) -> Result<Subject> {
        self.ready()?;
        if companion_id <= 0 {
            return Err(Error::InvalidArguments);
        }
        let record = self
            .db
            .with_conn(|c| crate::db::companions::get_companion(c, companion_id).map_err(|_| Error::Internal))?
            .ok_or(Error::NotFound)?;
        // A restored backup could hold any path or name; such a row is never touched.
        validate_stored_fields(&record.name, &record.path, &record.kind).map_err(|_| Error::Inconsistent)?;
        let row = self.db.with_conn(|c| read_companion(c, companion_id))?.ok_or(Error::NotFound)?;
        let model = self.state(row.file_id)?;
        let expected = extras_dir_for(Path::new(&model.path)).ok_or(Error::PathUnresolvable)?;
        // A removed row keeps the place it came from; the model may have been renamed or moved
        // since, so only an active row has to sit in the model's current companion folder.
        if row.deleted_at.is_none() && !Path::new(&row.path).parent().is_some_and(|d| same_dir(d, &expected)) {
            return Err(Error::Inconsistent);
        }
        let (root, _) = self.context.locate(&model.path)?;
        let rows = self.db.with_conn(|c| read_companion_rows(c, model.id))?;
        Ok(Subject { row, model, rows, root })
    }

    fn companion_intent(&self, op_id: String, operation: Operation, s: &Subject, step: CompanionStep, extras_dir: Option<ExtrasDir>) -> Intent {
        let root = &self.context.roots[s.root];
        Intent {
            op_id,
            operation,
            phase: Phase::Prepared,
            root_path: root.path.to_str().unwrap().to_owned(),
            root_identity: root.identity,
            source: None,
            target: None,
            temporary: None,
            identity: None,
            before: s.model.clone(),
            after: Some(s.model.clone()),
            companions: vec![step],
            extras_dir,
            companion_rows: s.rows.clone(),
            companion_add: None,
        }
    }

    /// The row and its model as the three operations will judge them; lets a caller
    /// refuse (and spend no budget) before it starts one.
    pub fn companion_subject(&mut self, companion_id: i64) -> Result<(CompanionState, FileState)> {
        let s = self.load_subject(companion_id)?;
        Ok((s.row, s.model))
    }

    /// Sends one active companion file to the web trash.
    pub fn trash_companion(&mut self, companion_id: i64) -> Result<()> {
        let s = self.load_subject(companion_id)?;
        if s.row.deleted_at.is_some() || s.model.deleted_at.is_some() {
            return Err(Error::Conflict);
        }
        let extras = extras_dir_for(Path::new(&s.model.path)).ok_or(Error::PathUnresolvable)?;
        let extras = extras.to_str().ok_or(Error::PathUnresolvable)?.to_owned();
        let root_dev = self.context.roots[s.root].identity.dev;
        let probe: Probe = self.probe_dir(&extras, None)?;
        let mut unsafe_files = 0;
        let present = self.locate_present(&[&s.row], &probe, root_dev, &mut unsafe_files)?;
        companion_block(CompanionFacts {
            op: CompanionOp::Trash,
            rows_total: s.rows.len(),
            rows_to_carry: 1,
            old_dir: probe.state,
            new_dir: DirState::Absent,
            unsafe_files,
            names_taken: 0,
            rows_unhandled: 0,
        })
        .into_result()?;
        let found = present[0];
        let source = match found {
            Some(_) => Some(self.relative_in(s.root, &s.row.path)?),
            None => None,
        };
        let op_id = journal::op_id()?;
        let mut steps = vec![CompanionStep {
            before: s.row.clone(),
            after: Some(CompanionState {
                deleted_at: Some(chrono::Utc::now().to_rfc3339()),
                trash_path: None,
                ..s.row.clone()
            }),
            source,
            target: None,
            identity: found,
        }];
        self.name_trash_steps(&mut steps, s.root, &op_id)?;
        let step = steps.remove(0);
        self.run_edit(op_id, Operation::CompanionTrash, &s, step, None)
    }

    /// Brings one removed companion file back next to its model, under a free name.
    pub fn restore_companion(&mut self, companion_id: i64) -> Result<()> {
        let s = self.load_subject(companion_id)?;
        if s.row.deleted_at.is_none() || s.model.deleted_at.is_some() {
            return Err(Error::Conflict);
        }
        if s.rows.iter().filter(|r| r.deleted_at.is_none()).count() as i64 >= MAX_COMPANIONS_PER_MODEL {
            return Err(Error::CompanionLimit);
        }
        // The stored name decides the new file name, so it has to be harmless.
        let harmless = validate_name(&s.row.name).is_ok_and(|n| n == s.row.name)
            && lowercase_extension(&s.row.name).and_then(|e| kind_for_extension(&e)).is_some();
        if !harmless {
            return Err(Error::InvalidName);
        }
        let file = match s.row.trash_path.as_deref() {
            None => None,
            Some(path) => match self.classify_trash_file(s.root, &s.row, path)? {
                TrashFile::Present(relative, identity) => Some((relative, identity)),
                TrashFile::Gone => None,
                // The desktop's trash (or anything else) is not ours to move from.
                TrashFile::Unsafe | TrashFile::Outside => return Err(Error::TrashForeign),
            },
        };
        let new_dir = extras_dir_for(Path::new(&s.model.path)).ok_or(Error::PathUnresolvable)?;
        let new_str = new_dir.to_str().ok_or(Error::PathUnresolvable)?.to_owned();
        let new = self.probe_dir(&new_str, None)?;
        companion_block(CompanionFacts {
            op: CompanionOp::Restore,
            rows_total: s.rows.len(),
            rows_to_carry: 1,
            old_dir: DirState::Absent,
            new_dir: new.state,
            unsafe_files: 0,
            names_taken: 0,
            rows_unhandled: 0,
        })
        .into_result()?;
        let (name, path) = self.free_name(&new, &new_dir, &s.row, &std::collections::HashSet::new())?;
        let (source, target) = match &file {
            Some((relative, _)) => (Some(relative.clone()), Some(self.relative_in(s.root, &path)?)),
            None => (None, None),
        };
        let extras_dir = if file.is_some() && new.state == DirState::Absent {
            Some(ExtrasDir { relative: self.relative_in(s.root, &new_str)?, identity: None })
        } else {
            None
        };
        let step = CompanionStep {
            before: s.row.clone(),
            after: Some(CompanionState { name, path, deleted_at: None, trash_path: None, ..s.row.clone() }),
            source,
            target,
            identity: file.map(|(_, i)| i),
        };
        self.run_edit(journal::op_id()?, Operation::CompanionRestore, &s, step, extras_dir)
    }

    /// The common run of a trash or restore: journal, move, commit, finish.
    fn run_edit(&mut self, op_id: String, operation: Operation, s: &Subject, step: CompanionStep, extras_dir: Option<ExtrasDir>) -> Result<()> {
        let moves = step.source.is_some();
        let mut intent = self.companion_intent(op_id, operation, s, step, extras_dir);
        self.journal.add(intent.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
        // Creates the folder if needed and renames the file; it undoes itself on failure.
        if moves || intent.extras_dir.is_some() {
            self.carry_forward(&mut intent, s.root)?;
        }
        #[cfg(test)]
        if super::tests::at_point("before_commit", 0) {
            return Err(self.journal.simulate_crash());
        }
        if let Err(e) = self.db.with_conn(|c| commit_companion_edit(c, &intent)) {
            if self.rollback_carry(&intent, s.root).is_err() {
                return Err(self.journal.block(&intent.op_id));
            }
            self.journal.remove(&intent.op_id)?;
            return Err(e);
        }
        self.finish_companion_edit(&intent)
    }

    fn finish_companion_edit(&mut self, i: &Intent) -> Result<()> {
        self.journal.phase(&i.op_id, Phase::Committed)?;
        self.journal.remove(&i.op_id)?;
        self.cleanup_old_extras(i);
        Ok(())
    }

    /// Deletes a removed companion file for good (empty trash). `cutoff` is the
    /// emptying's own cut-off: a file removed later is left alone.
    pub(super) fn purge_loose_companion(&mut self, companion_id: i64, cutoff: Option<chrono::DateTime<chrono::FixedOffset>>) -> Result<()> {
        let s = self.load_subject(companion_id)?;
        // A trashed model owns its companions; only a live model's removed files are loose.
        if s.row.deleted_at.is_none() || s.model.deleted_at.is_some() {
            return Err(Error::Changed);
        }
        if let Some(cutoff) = cutoff {
            let at = chrono::DateTime::parse_from_rfc3339(s.row.deleted_at.as_deref().ok_or(Error::Changed)?)
                .map_err(|_| Error::Changed)?;
            if at > cutoff {
                return Err(Error::Changed);
            }
        }
        let file = match s.row.trash_path.as_deref() {
            None => None,
            Some(path) => match self.classify_trash_file(s.root, &s.row, path)? {
                TrashFile::Present(relative, identity) => Some((relative, identity)),
                TrashFile::Gone => None,
                TrashFile::Unsafe | TrashFile::Outside => return Err(Error::TrashForeign),
            },
        };
        let step = CompanionStep {
            before: s.row.clone(),
            after: None,
            source: file.as_ref().map(|(r, _)| r.clone()),
            target: None,
            identity: file.as_ref().map(|(_, i)| *i),
        };
        let intent = self.companion_intent(journal::op_id()?, Operation::CompanionPurge, &s, step, None);
        self.journal.add(intent.clone())?;
        #[cfg(test)]
        super::tests::before_mutation();
        // The model and its rows are judged once more right before the unlink, which
        // nothing can undo.
        let checked = self.db.with_conn(|c| {
            let tx = c.transaction()?;
            if super::operations::read_state(&tx, intent.before.id)?.as_ref() != Some(&intent.before) {
                return Err(Error::Changed);
            }
            if read_companion_rows(&tx, intent.before.id)? != intent.companion_rows {
                return Err(Error::CompanionsChanged);
            }
            Ok(())
        });
        if let Err(e) = checked {
            self.journal.remove(&intent.op_id)?;
            return Err(e);
        }
        if let (Some(relative), Some((_, identity))) = (&intent.companions[0].source, &file) {
            if let Err(e) = self.unlink_companion(s.root, relative, *identity) {
                self.journal.remove(&intent.op_id)?;
                return Err(e);
            }
            #[cfg(test)]
            if super::tests::at_point("after_unlink", 0) {
                return Err(self.journal.simulate_crash());
            }
            self.journal.phase(&intent.op_id, Phase::CompanionsMoved)?;
            let place = self.context.resolve_relative(s.root, relative, Access::Trash)?;
            if place.parent.sync().is_err() {
                return Err(self.journal.block(&intent.op_id));
            }
            self.journal.phase(&intent.op_id, Phase::Unlinked)?;
        }
        if self.db.with_conn(|c| commit_companion_edit(c, &intent)).is_err() {
            return Err(self.journal.block(&intent.op_id));
        }
        self.finish_companion_edit(&intent)
    }

    /// Settles an interrupted single-companion entry. Errors leave the entry for the
    /// caller to block.
    pub(super) fn recover_companion_edit(&mut self, i: &Intent) -> Result<()> {
        if i.operation == Operation::CompanionAdd {
            return self.recover_companion_add(i);
        }
        let purge = i.operation == Operation::CompanionPurge;
        let root = self
            .context
            .roots
            .iter()
            .position(|r| r.path.to_str() == Some(&i.root_path) && r.identity == i.root_identity)
            .ok_or(Error::Changed)?;
        // The entry has exactly the shape this code writes; anything else was not ours.
        let [step] = i.companions.as_slice() else { return Err(Error::Changed) };
        if i.source.is_some()
            || i.target.is_some()
            || i.temporary.is_some()
            || i.identity.is_some()
            || i.after.as_ref() != Some(&i.before)
            || (i.extras_dir.is_some() && i.operation != Operation::CompanionRestore)
            || step.after.is_none() != purge
            || validate_name(&step.before.name).is_err()
        {
            return Err(Error::Changed);
        }
        // Both ends of the move lie in the model's own companion folder (or the trash).
        let extras = extras_dir_for(Path::new(&i.before.path)).ok_or(Error::Changed)?;
        let inside = |p: &str| Path::new(p).parent().is_some_and(|d| same_dir(d, &extras));
        let places_ok = match i.operation {
            Operation::CompanionTrash => inside(&step.before.path),
            // The old place of a removed row may predate a rename of the model.
            Operation::CompanionRestore => step.after.as_ref().is_some_and(|a| inside(&a.path)),
            _ => true,
        };
        if !places_ok {
            return Err(Error::Changed);
        }
        let obs = self.observe_carry(i, root)?;
        let row = self.db.with_conn(|c| read_companion(c, step.before.id))?;
        let facts = CompanionEditFacts {
            purge,
            db_before: row.as_ref() == Some(&step.before),
            db_after: if purge { row.is_none() } else { row == step.after },
            file_any: obs.any,
            file_at_source: obs.all_source,
            file_at_target: obs.all_target,
        };
        match companion_edit_decision(facts) {
            CompanionEditDecision::Discard => {
                self.remove_created_extras(i, root);
                self.journal.remove(&i.op_id)
            }
            CompanionEditDecision::Rollback => {
                self.rollback_carry(i, root)?;
                self.journal.remove(&i.op_id)
            }
            CompanionEditDecision::Committed => self.finish_companion_edit(i),
            CompanionEditDecision::FinishPurge => {
                self.db.with_conn(|c| commit_companion_edit(c, i))?;
                self.finish_companion_edit(i)
            }
            CompanionEditDecision::Manual => Err(Error::Changed),
        }
    }
}
