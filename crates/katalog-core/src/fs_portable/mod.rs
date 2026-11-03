//! Platform-independent core of the web file operations: the error type, name rules and the
//! recovery decision table. Compiled on every system; nothing in here touches a descriptor,
//! `libc` or `/proc` (except the Unix-only `From<io::Error>` mapping). `fsops` (Linux only)
//! re-exports everything under the old paths.
use serde::{Deserialize, Serialize};
pub mod names;
pub mod recovery;

use unicode_normalization::UnicodeNormalization;

pub fn nfc(value: &str) -> String {
    value.nfc().collect()
}
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
    /// A companion file's name is taken at the destination, or the destination folder is not a plain folder.
    CompanionsConflict,
    /// The model's companion rows changed while the operation was under way.
    CompanionsChanged,
    /// The model already has the most companion files it may have.
    CompanionLimit,
    /// An uploaded companion file has an extension that is not on the allowlist.
    CompanionType,
    /// An uploaded companion file's content does not match its extension, or is empty.
    CompanionContent,
    /// An uploaded companion file is larger than a companion file may be.
    CompanionTooLarge,
    /// An upload's body stalled for too long.
    UploadTimeout,
    /// An upload's body is shorter or longer than announced, or broke off.
    UploadLength,
    /// Writing the upload would leave too little free space.
    InsufficientStorage,
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
#[cfg(unix)]
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
            // Out of space or quota: the caller can free some, so it is not an internal fault.
            Some(libc::ENOSPC | libc::EDQUOT | libc::EFBIG) => Self::InsufficientStorage,
            _ => Self::Internal,
        }
    }
}
/// Other systems: map by `io::ErrorKind` only. Compiled for portability, no caller yet; the path
/// backend will route this through `FileBackend::map_error` (K1c).
#[cfg(not(unix))]
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind as K;
        match e.kind() {
            K::InvalidData => Self::PathUnresolvable,
            K::AlreadyExists | K::DirectoryNotEmpty => Self::Conflict,
            K::NotFound => Self::NotFound,
            K::Unsupported => Self::Unsupported,
            K::CrossesDevices => Self::CrossDevice,
            K::ResourceBusy => Self::Busy,
            K::StorageFull | K::QuotaExceeded | K::FileTooLarge => Self::InsufficientStorage,
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
#[cfg(test)]
mod tests;
