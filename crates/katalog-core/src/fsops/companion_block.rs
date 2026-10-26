//! Which operations may carry a model's companion files, and which cases stay
//! refused with `HasCompanions`. The decision is a pure function of facts gathered from
//! the database and the disk, so every combination can be tested without a
//! filesystem.

use super::*;

/// The operation that is about to touch a model that has companion rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompanionOp {
    Rename,
    Move,
    Trash,
    Restore,
    DeletePermanently,
}

/// State of a folder as the pre-check found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirState {
    /// Nothing at that name.
    Absent,
    /// A plain folder on the root's device, reachable without links.
    Plain,
    /// A link, a file, another device or a protected place.
    Unsafe,
    /// The same inode as the source folder, e.g. a case-only rename on a
    /// case-insensitive filesystem.
    SameAsSource,
}

#[derive(Clone, Copy, Debug)]
pub struct CompanionFacts {
    pub op: CompanionOp,
    /// All rows of the model, trashed ones included.
    pub rows_total: usize,
    /// Active rows that sit in the model's own `_extras` folder; only these travel.
    pub rows_to_carry: usize,
    /// The model's current `_extras` folder.
    pub old_dir: DirState,
    /// The `_extras` folder at the destination.
    pub new_dir: DirState,
    /// Files to carry that are links, have several names, are no regular file,
    /// lie on another device or whose row is malformed.
    pub unsafe_files: usize,
    /// Files to carry whose name is already taken at the destination.
    pub names_taken: usize,
    /// Rows the operation can neither carry nor leave alone without stranding a file:
    /// an active row outside the model's folder when the model goes to the trash, or
    /// a row of the desktop trash or an active row when the model is deleted for good.
    pub rows_unhandled: usize,
}

impl CompanionFacts {
    /// Facts for an operation that does not look at the disk at all.
    pub fn rows_only(op: CompanionOp, rows_total: usize) -> Self {
        Self {
            op,
            rows_total,
            rows_to_carry: 0,
            old_dir: DirState::Absent,
            new_dir: DirState::Absent,
            unsafe_files: 0,
            names_taken: 0,
            rows_unhandled: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No companion is affected; the operation proceeds as for any model.
    Unaffected,
    /// The companions in `_extras` travel with the model.
    Carry,
    /// Refused: this case is only handled on the desktop (`HasCompanions`).
    Blocked,
    /// Refused: the destination cannot take the companions (`CompanionsConflict`).
    Conflict,
}

impl Verdict {
    pub fn into_result(self) -> Result<bool> {
        match self {
            Self::Unaffected => Ok(false),
            Self::Carry => Ok(true),
            Self::Blocked => Err(Error::HasCompanions),
            Self::Conflict => Err(Error::CompanionsConflict),
        }
    }
}

/// A model without rows is never affected. A row that would be stranded blocks the
/// operation before anything moves; unwatched places never become a reason to guess.
pub fn companion_block(f: CompanionFacts) -> Verdict {
    if f.rows_total == 0 {
        return Verdict::Unaffected;
    }
    if f.rows_unhandled > 0 {
        return Verdict::Blocked;
    }
    if f.rows_to_carry == 0 {
        // Rows elsewhere or in the trash are not ours to move and block nothing.
        return Verdict::Unaffected;
    }
    // A folder that cannot be trusted would make the move land on someone else's file.
    if matches!(f.old_dir, DirState::Unsafe | DirState::SameAsSource) {
        return Verdict::Blocked;
    }
    if f.unsafe_files > 0 || f.new_dir == DirState::SameAsSource {
        return Verdict::Blocked;
    }
    if f.new_dir == DirState::Unsafe || f.names_taken > 0 {
        return Verdict::Conflict;
    }
    Verdict::Carry
}

/// What the pre-check of a folder rename or move found among the companion rows of the
/// folder's subtree. The folder moves as one directory entry, so every `_extras` folder
/// inside travels with it; only the stored paths have to follow.
#[derive(Clone, Copy, Debug, Default)]
pub struct FolderCompanionFacts {
    /// Rows of the tree's models and rows whose path lies in the tree.
    pub rows_total: usize,
    /// A path outside the folder, or a row of a model that is not part of the tree.
    pub rows_outside: usize,
    /// A trash path that is not in the web trash of the model's root.
    pub rows_foreign_trash: usize,
    /// An active row whose file is a link, has several names, is no regular file, lies on
    /// another device, or whose path runs through a link.
    pub rows_unsafe: usize,
}

pub fn folder_block(f: FolderCompanionFacts) -> Verdict {
    if f.rows_total == 0 {
        return Verdict::Unaffected;
    }
    if f.rows_outside > 0 || f.rows_foreign_trash > 0 || f.rows_unsafe > 0 {
        return Verdict::Blocked;
    }
    Verdict::Carry
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> CompanionFacts {
        CompanionFacts {
            op: CompanionOp::Rename,
            rows_total: 2,
            rows_to_carry: 2,
            old_dir: DirState::Plain,
            new_dir: DirState::Absent,
            unsafe_files: 0,
            names_taken: 0,
            rows_unhandled: 0,
        }
    }

    #[test]
    fn a_model_without_rows_is_never_affected() {
        for op in [CompanionOp::Rename, CompanionOp::Move, CompanionOp::Trash, CompanionOp::Restore, CompanionOp::DeletePermanently] {
            assert_eq!(companion_block(CompanionFacts::rows_only(op, 0)), Verdict::Unaffected, "{op:?}");
        }
    }

    #[test]
    fn a_stranded_row_blocks_every_operation() {
        for op in [CompanionOp::Rename, CompanionOp::Move, CompanionOp::Trash, CompanionOp::Restore, CompanionOp::DeletePermanently] {
            let f = CompanionFacts { op, rows_unhandled: 1, ..base() };
            assert_eq!(companion_block(f), Verdict::Blocked, "{op:?}");
            // Even when nothing else would travel.
            let f = CompanionFacts { op, rows_unhandled: 1, rows_to_carry: 0, ..base() };
            assert_eq!(companion_block(f), Verdict::Blocked, "{op:?}");
        }
    }

    #[test]
    fn rows_that_are_none_of_our_business_do_not_block_trash_restore_or_delete() {
        for op in [CompanionOp::Trash, CompanionOp::Restore, CompanionOp::DeletePermanently] {
            assert_eq!(companion_block(CompanionFacts::rows_only(op, 3)), Verdict::Unaffected, "{op:?}");
        }
    }

    #[test]
    fn trash_restore_and_delete_carry_like_rename_when_everything_is_plain() {
        for op in [CompanionOp::Trash, CompanionOp::Restore, CompanionOp::DeletePermanently] {
            assert_eq!(companion_block(CompanionFacts { op, ..base() }), Verdict::Carry, "{op:?}");
            // The model's folder, a link or a foreign inode stops trash and restore alike.
            assert_eq!(companion_block(CompanionFacts { op, old_dir: DirState::Unsafe, ..base() }), Verdict::Blocked, "{op:?}");
            assert_eq!(companion_block(CompanionFacts { op, unsafe_files: 1, ..base() }), Verdict::Blocked, "{op:?}");
        }
        assert_eq!(
            companion_block(CompanionFacts { op: CompanionOp::Restore, old_dir: DirState::Absent, new_dir: DirState::Unsafe, ..base() }),
            Verdict::Conflict
        );
    }

    #[test]
    fn rename_and_move_carry_when_everything_is_plain() {
        for op in [CompanionOp::Rename, CompanionOp::Move] {
            for new_dir in [DirState::Absent, DirState::Plain] {
                let f = CompanionFacts { op, new_dir, ..base() };
                assert_eq!(companion_block(f), Verdict::Carry, "{op:?} {new_dir:?}");
            }
        }
    }

    #[test]
    fn rows_that_do_not_travel_block_nothing() {
        let f = CompanionFacts { rows_to_carry: 0, old_dir: DirState::Unsafe, ..base() };
        assert_eq!(companion_block(f), Verdict::Unaffected);
    }

    #[test]
    fn an_untrustworthy_source_folder_or_file_is_the_remaining_block() {
        for old_dir in [DirState::Unsafe, DirState::SameAsSource] {
            assert_eq!(companion_block(CompanionFacts { old_dir, ..base() }), Verdict::Blocked, "{old_dir:?}");
        }
        assert_eq!(companion_block(CompanionFacts { unsafe_files: 1, ..base() }), Verdict::Blocked);
        assert_eq!(companion_block(CompanionFacts { new_dir: DirState::SameAsSource, ..base() }), Verdict::Blocked);
        // A model whose files are all gone still moves: only the rows are rewritten.
        assert_eq!(companion_block(CompanionFacts { old_dir: DirState::Absent, ..base() }), Verdict::Carry);
    }

    #[test]
    fn a_destination_that_cannot_take_the_files_is_a_conflict() {
        assert_eq!(companion_block(CompanionFacts { new_dir: DirState::Unsafe, ..base() }), Verdict::Conflict);
        assert_eq!(companion_block(CompanionFacts { names_taken: 1, new_dir: DirState::Plain, ..base() }), Verdict::Conflict);
        // Unsafe files outrank a conflict: the source side is checked first.
        assert_eq!(companion_block(CompanionFacts { unsafe_files: 1, names_taken: 1, ..base() }), Verdict::Blocked);
    }

    #[test]
    fn verdicts_map_to_the_error_kinds() {
        assert_eq!(Verdict::Unaffected.into_result(), Ok(false));
        assert_eq!(Verdict::Carry.into_result(), Ok(true));
        assert_eq!(Verdict::Blocked.into_result(), Err(Error::HasCompanions));
        assert_eq!(Verdict::Conflict.into_result(), Err(Error::CompanionsConflict));
    }

    #[test]
    fn folder_truth_table() {
        for total in [0usize, 2] {
            for outside in [0usize, 1] {
                for trash in [0usize, 1] {
                    for unsafe_rows in [0usize, 1] {
                        let f = FolderCompanionFacts {
                            rows_total: total,
                            rows_outside: outside,
                            rows_foreign_trash: trash,
                            rows_unsafe: unsafe_rows,
                        };
                        let want = if total == 0 {
                            Verdict::Unaffected
                        } else if outside + trash + unsafe_rows > 0 {
                            Verdict::Blocked
                        } else {
                            Verdict::Carry
                        };
                        assert_eq!(folder_block(f), want, "{f:?}");
                    }
                }
            }
        }
    }
}
