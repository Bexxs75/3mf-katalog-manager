//! Web-only file operations. Callers hold the instance fileops gate through completion.
//! Database access is scoped to short callbacks; filesystem I/O never holds a DB lock.
use crate::upload_fs::Directory;
use serde::{Deserialize, Serialize};
mod context;
mod names;
pub use context::*;
pub use names::*;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    InvalidName,
    InvalidArguments,
    NotFound,
    Conflict,
    Changed,
    Cycle,
    Inconsistent,
    CrossDevice,
    Busy,
    Unsupported,
    TrashForeign,
    TrashMissing,
    TrashConflict,
    PathUnresolvable,
    Protected,
    HasCompanions,
    RestartRequired,
    NotStarted,
    Internal,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_value(self).unwrap().as_str().unwrap()
        )
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::InvalidData {
            return Self::PathUnresolvable;
        }
        match e.raw_os_error() {
            Some(libc::EXDEV) => Self::CrossDevice,
            Some(libc::EBUSY) => Self::Busy,
            Some(libc::EINVAL | libc::ENOSYS | libc::EOPNOTSUPP) => Self::Unsupported,
            Some(libc::EEXIST | libc::ENOTEMPTY) => Self::Conflict,
            Some(libc::ENOENT) => Self::NotFound,
            Some(libc::ELOOP | libc::ENOTDIR) => Self::PathUnresolvable,
            _ => Self::Internal,
        }
    }
}
impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::Utf8Error(_, _) => Self::PathUnresolvable,
            _ => Self::Internal,
        }
    }
}
mod journal;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod companion_gap_tests;
pub use journal::*;
mod operations;
pub use operations::*;

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
