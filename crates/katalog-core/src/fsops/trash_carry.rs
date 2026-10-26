//! Companion files and the web trash: they go to the trash with their model, come
//! back next to it on a restore, and are unlinked when the model is deleted for good.
//!
//! Trash entries are flat (`companion-<id>-<op>-<name>`) because the trash allows
//! exactly one level. Only files that sit in that trash under the name of their own
//! row are ever touched; a row that points anywhere else (the desktop's trash, a
//! restored backup) is left alone and, for a permanent delete, blocks the model so
//! that no file is orphaned.

use super::carry::{path_held, read_companion_rows, CarryPlan, Probe};
use super::*;
use crate::companions::{extras_dir_for, kind_for_extension, lowercase_extension};
use std::path::Path;

/// What a row's `trash_path` turned out to be.
pub(super) enum TrashFile {
    /// A plain file of ours in the web trash.
    Present(String, FileIdentity),
    /// Our place in the web trash, but nothing there any more.
    Gone,
    /// Inside the web trash under the right name, but not a plain file we may move or delete.
    Unsafe,
    /// Not in this root's web trash, or under another row's name.
    Outside,
}

/// The most attempts at a free name; the desktop uses the same bound.
const NAME_ATTEMPTS: u32 = 999;

/// `name`, then `stem (2).ext`, `stem (3).ext`, ... like the desktop's restore.
fn numbered(name: &str, attempt: u32) -> String {
    if attempt == 1 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem} ({attempt}).{ext}"),
        None => format!("{name} ({attempt})"),
    }
}

impl<D: FsDatabase, L: UploadLedger> FileOps<D, L> {
    /// Root-relative form of `path`, which must lie in the same root as the model.
    pub(super) fn relative_in(&self, root: usize, path: &str) -> Result<String> {
        let (r, relative) = self.context.locate(path)?;
        if r != root {
            return Err(Error::CrossDevice);
        }
        Ok(relative)
    }

    pub(super) fn classify_trash_file(&self, root: usize, row: &CompanionState, trash_path: &str) -> Result<TrashFile> {
        let r = match self.context.resolve(trash_path, Access::Trash) {
            Ok(r) => r,
            Err(Error::Internal) => return Err(Error::Internal),
            Err(_) => return Ok(TrashFile::Outside),
        };
        if r.root != root || !r.name.starts_with(&companion_trash_prefix(row.id)) {
            return Ok(TrashFile::Outside);
        }
        match FileIdentity::read(&r) {
            Ok(i) if i.dev == self.context.roots[root].identity.dev => Ok(TrashFile::Present(r.relative, i)),
            Err(Error::NotFound) => Ok(TrashFile::Gone),
            Err(e @ (Error::Internal | Error::Changed)) => Err(e),
            _ => Ok(TrashFile::Unsafe),
        }
    }

    /// Read-only pre-check for sending a model's companions to the trash. The
    /// trash names are added by `transfer`, which owns the operation id.
    pub(super) fn plan_trash(&mut self, before: &FileState, deleted_at: &str) -> Result<CarryPlan> {
        let id = before.id;
        let rows = self.db.with_conn(|c| read_companion_rows(c, id))?;
        if rows.is_empty() {
            return Ok(CarryPlan::default());
        }
        let old_dir = extras_dir_for(Path::new(&before.path)).ok_or(Error::PathUnresolvable)?;
        let active: Vec<&CompanionState> = rows.iter().filter(|r| r.deleted_at.is_none()).collect();
        // Rows removed on their own are not ours to move; an active row somewhere else
        // cannot go along, and leaving it behind would strand its file.
        let travelling: Vec<&CompanionState> = active
            .iter()
            .copied()
            .filter(|r| Path::new(&r.path).parent() == Some(old_dir.as_path()))
            .collect();
        let mut facts = CompanionFacts {
            op: CompanionOp::Trash,
            rows_total: rows.len(),
            rows_to_carry: travelling.len(),
            old_dir: DirState::Absent,
            new_dir: DirState::Absent,
            unsafe_files: 0,
            names_taken: 0,
            rows_unhandled: active.len() - travelling.len(),
        };
        if travelling.is_empty() || facts.rows_unhandled > 0 {
            companion_block(facts).into_result()?;
            return Ok(CarryPlan {
                rows,
                ..CarryPlan::default()
            });
        }
        let (root, _) = self.context.locate(&before.path)?;
        let root_dev = self.context.roots[root].identity.dev;
        let old_str = old_dir.to_str().ok_or(Error::PathUnresolvable)?;
        let old = match self.probe_dir(old_str, None) {
            Err(Error::Protected) => Probe {
                state: DirState::Unsafe,
                identity: None,
                handle: None,
            },
            other => other?,
        };
        facts.old_dir = old.state;
        let present = self.locate_present(&travelling, &old, root_dev, &mut facts.unsafe_files)?;
        companion_block(facts).into_result()?;
        let mut steps = Vec::new();
        for (row, found) in travelling.iter().zip(&present) {
            let source = match found {
                Some(_) => Some(self.relative_in(root, &row.path)?),
                None => None,
            };
            steps.push(CompanionStep {
                before: (*row).clone(),
                after: Some(CompanionState {
                    deleted_at: Some(deleted_at.to_owned()),
                    trash_path: None,
                    ..(*row).clone()
                }),
                source,
                target: None,
                identity: *found,
            });
        }
        Ok(CarryPlan {
            rows,
            steps,
            ..CarryPlan::default()
        })
    }

    /// Gives every file that really moves its place in the trash and records it in
    /// the row's new state.
    pub(super) fn name_trash_steps(&self, steps: &mut [CompanionStep], root: usize, op_id: &str) -> Result<()> {
        for step in steps.iter_mut().filter(|s| s.source.is_some()) {
            let name = companion_trash_name(step.before.id, op_id, &step.before.name)?;
            let relative = format!("{TRASH}/{name}");
            let place = self.context.resolve_relative(root, &relative, Access::Trash)?;
            if let Some(after) = &mut step.after {
                after.trash_path = Some(self.context.absolute(&place));
            }
            step.target = Some(relative);
        }
        Ok(())
    }

    /// Read-only pre-check for a restore: which companions of this deletion come back,
    /// under which free names, and which stay in the trash.
    pub(super) fn plan_restore(&mut self, before: &FileState) -> Result<CarryPlan> {
        let id = before.id;
        let rows = self.db.with_conn(|c| read_companion_rows(c, id))?;
        if rows.is_empty() {
            return Ok(CarryPlan::default());
        }
        // Only what went to the trash together with the model; rows removed separately stay put.
        let mine: Vec<&CompanionState> = rows
            .iter()
            .filter(|r| r.deleted_at.is_some() && r.deleted_at == before.deleted_at)
            .collect();
        let mut facts = CompanionFacts::rows_only(CompanionOp::Restore, rows.len());
        if mine.is_empty() {
            return Ok(CarryPlan {
                rows,
                ..CarryPlan::default()
            });
        }
        let new_dir = extras_dir_for(Path::new(&before.path)).ok_or(Error::PathUnresolvable)?;
        let new_str = new_dir.to_str().ok_or(Error::PathUnresolvable)?;
        let (root, _) = self.context.locate(&before.path)?;
        let mut kept = 0usize;
        // The row, and the trash file to move for it (None when the file is gone).
        let mut wanted: Vec<(&CompanionState, Option<(String, FileIdentity)>)> = Vec::new();
        for row in mine {
            // The stored name decides the new file name, so it has to be harmless.
            let harmless = validate_name(&row.name).is_ok_and(|n| n == row.name)
                && lowercase_extension(&row.name).and_then(|e| kind_for_extension(&e)).is_some();
            if !harmless {
                kept += 1;
                continue;
            }
            let file = match row.trash_path.as_deref() {
                None => None,
                Some(path) => match self.classify_trash_file(root, row, path)? {
                    TrashFile::Present(relative, identity) => Some((relative, identity)),
                    TrashFile::Gone => None,
                    TrashFile::Unsafe | TrashFile::Outside => {
                        kept += 1;
                        continue;
                    }
                },
            };
            wanted.push((row, file));
        }
        facts.rows_to_carry = wanted.len();
        if wanted.is_empty() {
            return Ok(CarryPlan {
                rows,
                kept_in_trash: kept,
                ..CarryPlan::default()
            });
        }
        let new = match self.probe_dir(new_str, None) {
            // The upload area is never written to.
            Err(Error::Protected) => return Err(Error::HasCompanions),
            other => other?,
        };
        facts.new_dir = new.state;
        companion_block(facts).into_result()?;

        let mut taken = std::collections::HashSet::new();
        let mut steps = Vec::new();
        for (row, file) in &wanted {
            let (name, path) = self.free_name(&new, &new_dir, row, &taken)?;
            taken.insert(path.clone());
            let (source, target) = match file {
                Some((relative, _)) => (Some(relative.clone()), Some(self.relative_in(root, &path)?)),
                None => (None, None),
            };
            steps.push(CompanionStep {
                before: (*row).clone(),
                after: Some(CompanionState {
                    name,
                    path,
                    deleted_at: None,
                    trash_path: None,
                    ..(*row).clone()
                }),
                source,
                target,
                identity: file.as_ref().map(|(_, i)| *i),
            });
        }
        let extras_dir = if steps.iter().any(|s| s.source.is_some()) && new.state == DirState::Absent {
            Some(ExtrasDir {
                relative: self.relative_in(root, new_str)?,
                identity: None,
            })
        } else {
            None
        };
        Ok(CarryPlan {
            rows,
            steps,
            extras_dir,
            kept_in_trash: kept,
            ..CarryPlan::default()
        })
    }

    /// The first name that is free on disk, in the table (the path is unique, and a
    /// row's own old path counts as free for itself) and among the names chosen so far.
    pub(super) fn free_name(
        &mut self,
        dir: &Probe,
        new_dir: &Path,
        row: &CompanionState,
        taken: &std::collections::HashSet<String>,
    ) -> Result<(String, String)> {
        for attempt in 1..=NAME_ATTEMPTS {
            let name = numbered(&row.name, attempt);
            if validate_name(&name).map_or(true, |n| n != name) {
                continue;
            }
            let path = new_dir.join(&name).to_str().ok_or(Error::PathUnresolvable)?.to_owned();
            let on_disk = dir
                .handle
                .as_ref()
                .is_some_and(|h| !matches!(h.entry_identity(&name), Err(e) if e.kind() == std::io::ErrorKind::NotFound));
            if on_disk || taken.contains(&path) || self.db.with_conn(|c| path_held(c, &path, row.id))? {
                continue;
            }
            return Ok((name, path));
        }
        Err(Error::CompanionsConflict)
    }

    /// Read-only pre-check for deleting a trashed model for good. Rows without a
    /// file simply disappear with the model; a row that would leave a file behind
    /// outside the web trash blocks it.
    pub(super) fn plan_purge(&mut self, before: &FileState) -> Result<CarryPlan> {
        let id = before.id;
        let rows = self.db.with_conn(|c| read_companion_rows(c, id))?;
        if rows.is_empty() {
            return Ok(CarryPlan::default());
        }
        let root = self.context.locate(&before.path).ok().map(|(r, _)| r);
        let mut facts = CompanionFacts::rows_only(CompanionOp::DeletePermanently, rows.len());
        let mut steps = Vec::new();
        let mut kept_on_disk = false;
        for row in &rows {
            // An active row still has its file in the catalog, whatever its trash_path says.
            if row.deleted_at.is_none() {
                facts.rows_unhandled += 1;
                continue;
            }
            // A trashed row without a trash path has nothing left to remove.
            let Some(path) = row.trash_path.as_deref() else { continue };
            let found = match root {
                Some(root) => self.classify_trash_file(root, row, path)?,
                None => TrashFile::Outside,
            };
            match found {
                TrashFile::Present(relative, identity) => steps.push(CompanionStep {
                    before: row.clone(),
                    after: None,
                    source: Some(relative),
                    target: None,
                    identity: Some(identity),
                }),
                TrashFile::Gone => (),
                TrashFile::Unsafe => kept_on_disk = true,
                TrashFile::Outside => facts.rows_unhandled += 1,
            }
        }
        facts.rows_to_carry = steps.len();
        companion_block(facts).into_result()?;
        Ok(CarryPlan {
            rows,
            steps,
            kept_on_disk,
            ..CarryPlan::default()
        })
    }
}
