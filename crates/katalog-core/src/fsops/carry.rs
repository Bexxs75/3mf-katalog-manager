//! Companion files travelling with their model on rename, move, trash and restore.
//!
//! Every companion is its own journalled step: the files move first, the model
//! last, then one database commit rewrites `files` and `companion_files`
//! together. So "model at its destination" always implies "all companions at
//! theirs", and the model's rename is the point after which recovery goes
//! forward instead of back. The `_extras` folder itself is never renamed:
//! `Foo.3mf` and `Foo.stl` share `Foo_extras`, and only the files that belong
//! to the moved model may leave it. A permanent delete is the same sequence with
//! unlinks instead of renames, where recovery can only go forward.

use super::operations::{observe, rename_checked, Observed};
use super::*;
use crate::companions::extras_dir_for;
use rusqlite::{params, Connection};
use std::path::Path;

/// Everything the pre-check learned; nothing is on disk or in the journal yet.
#[derive(Default)]
pub(super) struct CarryPlan {
    pub rows: Vec<CompanionState>,
    pub steps: Vec<CompanionStep>,
    pub extras_dir: Option<ExtrasDir>,
    /// Restore: rows that stay in the trash because their file is not ours to move.
    pub kept_in_trash: usize,
    /// Delete: a trash file that is not a plain file of ours stays where it is.
    pub kept_on_disk: bool,
}

/// How the companions of a journal entry sit on disk right now.
pub(super) struct CarryObservation {
    pub db_before: bool,
    pub db_after: bool,
    pub all_source: bool,
    pub all_target: bool,
    pub mixed: bool,
    pub any: bool,
}

impl CarryObservation {
    /// Entries without companion steps: nothing to look at.
    fn none() -> Self {
        Self {
            db_before: true,
            db_after: true,
            all_source: true,
            all_target: true,
            mixed: false,
            any: false,
        }
    }
}

pub(super) fn companion_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CompanionState> {
    Ok(CompanionState {
        id: r.get(0)?,
        file_id: r.get(1)?,
        name: r.get(2)?,
        path: r.get(3)?,
        deleted_at: r.get(4)?,
        trash_path: r.get(5)?,
    })
}

pub(super) fn read_companion(c: &Connection, id: i64) -> Result<Option<CompanionState>> {
    use rusqlite::OptionalExtension;
    Ok(c.query_row(
        "SELECT id,file_id,name,path,deleted_at,trash_path FROM companion_files WHERE id=?1",
        [id],
        companion_from_row,
    )
    .optional()?)
}

pub(super) fn read_companion_rows(c: &Connection, file_id: i64) -> Result<Vec<CompanionState>> {
    let mut stmt = c.prepare(
        "SELECT id,file_id,name,path,deleted_at,trash_path FROM companion_files WHERE file_id=?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([file_id], |r| {
        Ok(CompanionState {
            id: r.get(0)?,
            file_id: r.get(1)?,
            name: r.get(2)?,
            path: r.get(3)?,
            deleted_at: r.get(4)?,
            trash_path: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// True when another row already holds `path`; the column is unique.
pub(super) fn path_held(c: &Connection, path: &str, own: i64) -> Result<bool> {
    Ok(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM companion_files WHERE path=?1 AND id<>?2)",
        params![path, own],
        |r| r.get(0),
    )?)
}

/// Writes the new state of every carried row; called inside the commit transaction.
pub(super) fn apply_steps(c: &Connection, steps: &[CompanionStep]) -> Result<()> {
    for step in steps {
        let Some(a) = &step.after else { continue };
        if path_held(c, &a.path, a.id)? {
            return Err(Error::CompanionsConflict);
        }
        c.execute(
            "UPDATE companion_files SET name=?1,path=?2,deleted_at=?3,trash_path=?4 WHERE id=?5",
            params![a.name, a.path, a.deleted_at, a.trash_path, a.id],
        )?;
    }
    Ok(())
}

pub(super) struct Probe {
    pub state: DirState,
    pub identity: Option<Identity>,
    pub handle: Option<Directory>,
}

/// Where each side of a companion step lives: renames and moves stay in the
/// catalog, the trash takes files from it, a restore brings them back.
pub(super) fn step_access(op: Operation) -> (Access, Access) {
    match op {
        Operation::Trash | Operation::CompanionTrash => (Access::Source, Access::Trash),
        Operation::Restore | Operation::CompanionRestore => (Access::Trash, Access::MoveTarget),
        Operation::Unlink | Operation::CompanionPurge => (Access::Trash, Access::Trash),
        _ => (Access::Source, Access::MoveTarget),
    }
}

/// The absolute source and target a step must have for this operation, so a
/// journal that names other places is never followed.
fn step_places(op: Operation, step: &CompanionStep) -> Result<(&str, Option<&str>)> {
    let after = step.after.as_ref();
    match op {
        Operation::Trash | Operation::CompanionTrash => Ok((&step.before.path, Some(after.ok_or(Error::Changed)?.trash_path.as_deref().ok_or(Error::Changed)?))),
        Operation::Restore | Operation::CompanionRestore => Ok((
            step.before.trash_path.as_deref().ok_or(Error::Changed)?,
            Some(&after.ok_or(Error::Changed)?.path),
        )),
        Operation::Unlink | Operation::CompanionPurge => Ok((step.before.trash_path.as_deref().ok_or(Error::Changed)?, None)),
        _ => Ok((&step.before.path, Some(&after.ok_or(Error::Changed)?.path))),
    }
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    /// Looks at a companion folder through the pinned handles. A protected place is
    /// reported as `Protected`; links, files and other devices as `Unsafe`.
    pub(super) fn probe_dir(&self, path: &str, source: Option<Identity>) -> Result<Probe> {
        let unsafe_dir = Probe {
            state: DirState::Unsafe,
            identity: None,
            handle: None,
        };
        let r = match self.context.resolve(path, Access::Directory) {
            Ok(r) => r,
            Err(e @ (Error::Internal | Error::Protected)) => return Err(e),
            Err(_) => return Ok(unsafe_dir),
        };
        match r.parent.entry_identity(&r.name) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Probe {
                    state: DirState::Absent,
                    identity: None,
                    handle: None,
                })
            }
            Err(_) => return Ok(unsafe_dir),
            Ok(_) => (),
        }
        let id = match self.context.directory_identity(&r) {
            Ok(id) => id,
            Err(Error::Internal) => return Err(Error::Internal),
            Err(_) => return Ok(unsafe_dir),
        };
        let Ok(handle) = r.parent.open_dir(&r.name) else {
            return Ok(unsafe_dir);
        };
        Ok(Probe {
            state: if source == Some(id) {
                DirState::SameAsSource
            } else {
                DirState::Plain
            },
            identity: Some(id),
            handle: Some(handle),
        })
    }

    /// For each row: the identity of its file when it is a plain file inside `dir`,
    /// `None` when it is gone, and a count of the ones that cannot be trusted.
    pub(super) fn locate_present(
        &self,
        rows: &[&CompanionState],
        dir: &Probe,
        root_dev: u64,
        unsafe_files: &mut usize,
    ) -> Result<Vec<Option<FileIdentity>>> {
        let mut present = Vec::new();
        for row in rows {
            let mut found = None;
            if dir.state == DirState::Plain {
                match self.context.resolve(&row.path, Access::Source) {
                    Ok(r) => {
                        if Some(Identity::of(&r.parent)?) != dir.identity
                            || Path::new(&row.path).file_name().and_then(|n| n.to_str()) != Some(&row.name)
                            || r.name != row.name
                        {
                            *unsafe_files += 1;
                        } else {
                            match FileIdentity::read(&r) {
                                Ok(fi) if fi.dev == root_dev => found = Some(fi),
                                Ok(_) => *unsafe_files += 1,
                                Err(Error::NotFound) => (),
                                Err(Error::Internal) => return Err(Error::Internal),
                                Err(_) => *unsafe_files += 1,
                            }
                        }
                    }
                    Err(Error::NotFound) => (),
                    Err(Error::Internal) => return Err(Error::Internal),
                    Err(_) => *unsafe_files += 1,
                }
            }
            present.push(found);
        }
        Ok(present)
    }

    /// Read-only pre-check for a rename or move of `before` to `new_model_path`. It
    /// decides whether the companions travel, are refused, or are not affected, and
    /// returns the steps to journal. Nothing is changed.
    pub(super) fn plan_carry(
        &mut self,
        before: &FileState,
        new_model_path: &str,
        op: CompanionOp,
    ) -> Result<CarryPlan> {
        let id = before.id;
        let rows = self.db.with_conn(|c| read_companion_rows(c, id))?;
        if rows.is_empty() {
            return Ok(CarryPlan::default());
        }
        let (Some(old_dir), Some(new_dir)) = (
            extras_dir_for(Path::new(&before.path)),
            extras_dir_for(Path::new(new_model_path)),
        ) else {
            return Err(Error::PathUnresolvable);
        };
        // Only rows that really sit in the model's own folder travel; a row pointing
        // anywhere else stays exactly as it is, so a move never reaches a foreign file.
        let travelling: Vec<&CompanionState> = if old_dir == new_dir {
            Vec::new()
        } else {
            rows.iter()
                .filter(|r| r.deleted_at.is_none() && Path::new(&r.path).parent() == Some(old_dir.as_path()))
                .collect()
        };
        let mut facts = CompanionFacts {
            op,
            rows_total: rows.len(),
            rows_to_carry: travelling.len(),
            old_dir: DirState::Absent,
            new_dir: DirState::Absent,
            unsafe_files: 0,
            names_taken: 0,
            rows_unhandled: 0,
        };
        if travelling.is_empty() {
            companion_block(facts).into_result()?;
            return Ok(CarryPlan {
                rows,
                ..CarryPlan::default()
            });
        }
        let (root, _) = self.context.locate(&before.path)?;
        let root_dev = self.context.roots[root].identity.dev;
        let old_str = old_dir.to_str().ok_or(Error::PathUnresolvable)?;
        let new_str = new_dir.to_str().ok_or(Error::PathUnresolvable)?;
        let old = match self.probe_dir(old_str, None) {
            Err(Error::Protected) => Probe {
                state: DirState::Unsafe,
                identity: None,
                handle: None,
            },
            other => other?,
        };
        facts.old_dir = old.state;
        // Lets a test swap the folder between its check and the per-file checks.
        #[cfg(test)]
        let _ = super::tests::at_point("plan_files", 0);

        // Where each file is: present (identity known), gone from disk, or untrustworthy.
        let present = self.locate_present(&travelling, &old, root_dev, &mut facts.unsafe_files)?;
        let any_present = present.iter().any(Option::is_some);
        let new_state;
        {
            let new = match self.probe_dir(new_str, old.identity) {
                // A protected destination is a place we never write companions to.
                Err(Error::Protected) => return Err(Error::HasCompanions),
                other => other?,
            };
            new_state = new.state;
            facts.new_dir = new.state;
            if let Some(handle) = &new.handle {
                // Also for rows whose file is gone: a loose file already at that name would
                // wrongly become the model's companion.
                for row in &travelling {
                    if !matches!(handle.entry_identity(&row.name), Err(e) if e.kind() == std::io::ErrorKind::NotFound) {
                        facts.names_taken += 1;
                    }
                }
            }
        }
        // The row's path is unique even for files that are gone from disk.
        for row in &travelling {
            let target = new_dir.join(&row.name);
            let target = target.to_str().ok_or(Error::PathUnresolvable)?;
            if self.db.with_conn(|c| path_held(c, target, row.id))? {
                facts.names_taken += 1;
            }
        }
        companion_block(facts).into_result()?;

        let relative = |path: &str| -> Result<String> {
            let (r, rel) = self.context.locate(path)?;
            if r != root {
                return Err(Error::CrossDevice);
            }
            Ok(rel)
        };
        let mut steps = Vec::new();
        for (row, found) in travelling.iter().zip(&present) {
            let target_path = new_dir
                .join(&row.name)
                .to_str()
                .ok_or(Error::PathUnresolvable)?
                .to_owned();
            let (source, target) = match found {
                Some(_) => (Some(relative(&row.path)?), Some(relative(&target_path)?)),
                None => (None, None),
            };
            steps.push(CompanionStep {
                before: (*row).clone(),
                after: Some(CompanionState {
                    path: target_path,
                    ..(*row).clone()
                }),
                source,
                target,
                identity: *found,
            });
        }
        let extras_dir = if any_present && new_state == DirState::Absent {
            Some(ExtrasDir {
                relative: relative(new_str)?,
                identity: None,
            })
        } else {
            None
        };
        Ok(CarryPlan {
            rows,
            steps,
            extras_dir,
            ..CarryPlan::default()
        })
    }

    /// Creates the destination folder if needed and moves the companions. On any
    /// failure it has already put everything back (or blocked the journal), so the
    /// caller returns the error without touching the journal.
    pub(super) fn carry_forward(&mut self, i: &mut Intent, root: usize) -> Result<()> {
        if let Some(d) = i.extras_dir.clone() {
            let made = self
                .context
                .resolve_relative(root, &d.relative, Access::Directory)
                .and_then(|r| {
                    r.parent.mkdir(&r.name, 0o755)?;
                    Ok(r)
                });
            let r = match made {
                Ok(r) => r,
                Err(e) => return Err(self.abort_carry(i, root, e)),
            };
            // A crash before the inode is recorded is ambiguous: recovery never removes an unknown inode.
            let identity = match self.context.directory_identity(&r) {
                Ok(id) => id,
                Err(_) => return Err(self.journal.block(&i.op_id)),
            };
            // The caller's copy learns the inode too, so a later rollback can remove the folder.
            if let Some(d) = &mut i.extras_dir {
                d.identity = Some(identity);
            }
            let op_id = i.op_id.clone();
            if let Some(d) = self
                .journal
                .intents
                .iter_mut()
                .find(|j| j.op_id == op_id)
                .and_then(|j| j.extras_dir.as_mut())
            {
                d.identity = Some(identity);
            }
            self.journal.persist()?;
            if r.parent.sync().is_err() {
                return Err(self.journal.block(&i.op_id));
            }
            #[cfg(test)]
            if super::tests::at_point("after_mkdir", 0) {
                return Err(self.journal.simulate_crash());
            }
        }
        let (from, to) = step_access(i.operation);
        for (k, step) in i.companions.iter().enumerate() {
            let (Some(s), Some(t), Some(identity)) = (&step.source, &step.target, step.identity) else {
                continue;
            };
            let moved = self
                .context
                .resolve_relative(root, s, from)
                .and_then(|src| {
                    let tgt = self.context.resolve_relative(root, t, to)?;
                    rename_checked(&src, &tgt, identity)
                });
            if let Err(e) = moved {
                return Err(self.abort_carry(i, root, e));
            }
            #[cfg(test)]
            if super::tests::at_point("after_companion", k) {
                return Err(self.journal.simulate_crash());
            }
            let _ = k;
        }
        self.journal.phase(&i.op_id, Phase::CompanionsMoved)
    }

    fn abort_carry(&mut self, i: &Intent, root: usize, e: Error) -> Error {
        if e == Error::RestartRequired {
            return self.journal.block(&i.op_id);
        }
        if self.rollback_carry(i, root).is_err() {
            return self.journal.block(&i.op_id);
        }
        if let Err(e) = self.journal.remove(&i.op_id) {
            return e;
        }
        if e == Error::Conflict {
            Error::CompanionsConflict
        } else {
            e
        }
    }

    pub(super) fn observe_rel(
        &self,
        root: usize,
        relative: &str,
        access: Access,
        identity: FileIdentity,
    ) -> Result<Observed> {
        match self.context.resolve_relative(root, relative, access) {
            Ok(r) => observe(&r, identity),
            Err(Error::NotFound) => Ok(Observed::Absent),
            Err(e) => Err(e),
        }
    }

    /// Puts every companion that already reached its target back at its source, then
    /// removes the folder this operation created. A file is only moved when its
    /// identity matches the journal; anything else is left for manual review.
    pub(super) fn rollback_carry(&mut self, i: &Intent, root: usize) -> Result<()> {
        let (from, to) = step_access(i.operation);
        for (step_index, step) in i.companions.iter().enumerate().rev() {
            let (Some(s), Some(t), Some(identity)) = (&step.source, &step.target, step.identity) else {
                continue;
            };
            let at_source = self.observe_rel(root, s, from, identity)?;
            let at_target = self.observe_rel(root, t, to, identity)?;
            match (at_source, at_target) {
                // Untouched at its source, whether or not it changed in place meanwhile.
                (Observed::Matches, _) | (Observed::Foreign, Observed::Absent) => (),
                (Observed::Absent, Observed::Matches) => {
                    let src = self.context.resolve_relative(root, s, from)?;
                    let tgt = self.context.resolve_relative(root, t, to)?;
                    rename_checked(&tgt, &src, identity)?;
                    #[cfg(test)]
                    if super::tests::at_point("recovery_rollback", step_index) {
                        return Err(Error::RestartRequired);
                    }
                    let _ = step_index;
                }
                _ => return Err(Error::Changed),
            }
        }
        self.remove_created_extras(i, root);
        Ok(())
    }

    /// Removes the destination folder this operation made, if it is still that very
    /// inode and empty. `rmdir` refuses a non-empty folder, so this cannot lose files.
    pub(super) fn remove_created_extras(&mut self, i: &Intent, root: usize) {
        let Some(ExtrasDir {
            relative,
            identity: Some(identity),
        }) = &i.extras_dir
        else {
            return;
        };
        let Ok(r) = self
            .context
            .resolve_relative(root, relative, Access::Directory)
        else {
            return;
        };
        if self.context.directory_identity(&r) == Ok(*identity) {
            let _ = r.parent.rmdir(&r.name);
            let _ = r.parent.sync();
        }
    }

    /// After a finished rename, move or trash the old companion folder is usually
    /// empty. Removing it is best effort and not journalled: at worst an empty folder
    /// stays. A restore fills the folder instead, so it never gets here.
    pub(super) fn cleanup_old_extras(&mut self, i: &Intent) {
        if !matches!(i.operation, Operation::Rename | Operation::Move | Operation::Trash | Operation::CompanionTrash)
            || !i.companions.iter().any(|s| s.source.is_some())
        {
            return;
        }
        let Some(old) = extras_dir_for(Path::new(&i.before.path)) else {
            return;
        };
        let Some(old) = old.to_str() else { return };
        let Ok(r) = self.context.resolve(old, Access::Directory) else {
            return;
        };
        if self.context.directory_identity(&r).is_ok() {
            let _ = r.parent.rmdir(&r.name);
            let _ = r.parent.sync();
        }
    }

    /// Recovery's picture of the companions: database against the journal, and each
    /// file at its source, its target, or neither. For a delete "at the target" means
    /// "already unlinked".
    pub(super) fn observe_carry(&mut self, i: &Intent, root: usize) -> Result<CarryObservation> {
        let companion_op = matches!(
            i.operation,
            Operation::Rename
                | Operation::Move
                | Operation::Trash
                | Operation::Restore
                | Operation::Unlink
                | Operation::CompanionTrash
                | Operation::CompanionRestore
                | Operation::CompanionPurge
        ) || (i.operation == Operation::DbOnly && i.has_companion_data());
        if !companion_op {
            return Ok(CarryObservation::none());
        }
        let rows = self.db.with_conn(|c| read_companion_rows(c, i.before.id))?;
        let mut obs = CarryObservation::none();
        obs.db_before = rows == i.companion_rows;
        obs.db_after = i
            .companions
            .iter()
            .filter_map(|s| s.after.as_ref())
            .all(|a| rows.iter().find(|r| r.id == a.id) == Some(a));
        let root_path = self.context.roots.get(root).map(|r| r.path.clone());
        let unlink = matches!(i.operation, Operation::Unlink | Operation::CompanionPurge);
        let (from, to) = step_access(i.operation);
        let (mut at_source, mut at_target, mut invalid) = (0usize, 0usize, false);
        let mut seen = std::collections::HashSet::new();
        for step in &i.companions {
            // The same rule as for the model: after a delete the row is gone, otherwise it keeps its id.
            if step.before.file_id != i.before.id
                || step.after.as_ref().is_some_and(|a| a.id != step.before.id)
                || step.after.is_none() != (unlink || i.operation == Operation::DbOnly && i.after.is_none())
                || !seen.insert(step.before.id)
            {
                return Err(Error::Changed);
            }
            let (Some(s), Some(identity)) = (&step.source, step.identity) else {
                continue;
            };
            let (source_abs, target_abs) = step_places(i.operation, step)?;
            let root_path = root_path.as_ref().ok_or(Error::Changed)?;
            if root_path.join(s) != Path::new(source_abs) {
                return Err(Error::Changed);
            }
            if unlink {
                if step.target.is_some() {
                    return Err(Error::Changed);
                }
                obs.any = true;
                match self.observe_rel(root, s, from, identity)? {
                    Observed::Matches => at_source += 1,
                    Observed::Absent => at_target += 1,
                    Observed::Foreign => invalid = true,
                }
                continue;
            }
            let (Some(t), Some(target_abs)) = (&step.target, target_abs) else {
                continue;
            };
            if root_path.join(t) != Path::new(target_abs) {
                return Err(Error::Changed);
            }
            let so = self.observe_rel(root, s, from, identity)?;
            let to = self.observe_rel(root, t, to, identity)?;
            obs.any = true;
            match (so, to) {
                // A source that changed in place but did not move is untouched, as in the rollback.
                (Observed::Matches | Observed::Foreign, Observed::Absent) | (Observed::Matches, Observed::Foreign) => at_source += 1,
                (Observed::Absent, Observed::Matches) => at_target += 1,
                _ => invalid = true,
            }
        }
        if obs.any {
            obs.all_source = !invalid && at_target == 0;
            obs.all_target = !invalid && at_source == 0;
            obs.mixed = !invalid && at_source > 0 && at_target > 0;
        }
        Ok(obs)
    }
}
