//! Web-only file operations. Callers hold the instance fileops gate through completion.
//! Database access is scoped to short callbacks; filesystem I/O never holds a DB lock.
use crate::upload_fs::Directory;
use serde::{Deserialize, Serialize};
mod context;
use crate::fs_portable::names;
pub use context::*;
pub use crate::fs_portable::names::*;
pub use crate::fs_portable::recovery::*;
use crate::fs_portable::path::{NativePath, RelPath, Style};
pub use crate::fs_portable::{Error, Result};
mod carry;
mod companion_add;
mod companion_edit;
mod companion_block;
mod trash_carry;
mod trash_view;
mod journal;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod companion_gap_tests;
#[cfg(test)]
mod parity_golden_tests;
#[cfg(test)]
mod parity_error_tests;
#[cfg(test)]
mod companion_carry_tests;
#[cfg(test)]
mod companion_trash_tests;
#[cfg(test)]
mod companion_folder_tests;
#[cfg(test)]
mod companion_edit_tests;
#[cfg(test)]
mod companion_add_tests;
pub use journal::*;
mod operations;
pub use operations::*;
pub use companion_block::*;
pub use companion_add::*;
pub use companion_edit::*;
pub use trash_view::*;

pub fn fs_rename_file<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    id: i64,
    name: &str,
) -> Result<()> {
    ops.rename_file(id, name)
}
pub fn fs_move_files_to_folder<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    ids: &[i64],
    folder: Option<i64>,
    allowed: impl FnMut() -> bool,
) -> Result<Vec<ItemResult>> {
    ops.move_files_to_folder(ids, folder, allowed)
}
pub fn fs_delete_files<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    ids: &[i64],
    allowed: impl FnMut() -> bool,
) -> Result<Vec<ItemResult>> {
    ops.delete_files(ids, allowed)
}
pub fn fs_restore_file<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    id: i64,
) -> Result<()> {
    ops.restore_file(id)
}
pub fn fs_delete_file_permanently<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    id: i64,
) -> Result<DeleteOutcome> {
    ops.delete_file_permanently(id)
}
pub fn fs_empty_trash<D: FsDatabase, L: UploadLedger>(
    ops: &mut FileOps<D, L>,
    snapshot: &mut TrashSnapshot,
    allowed: impl FnMut() -> bool,
) -> Result<Vec<ItemResult>> {
    ops.empty_trash(snapshot, allowed)
}

mod folders;
pub use folders::*;
