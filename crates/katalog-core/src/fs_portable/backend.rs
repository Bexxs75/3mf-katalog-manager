//! Contract of a file backend: the primitives the file operations need, derived from what `fsops`
//! calls today. The traits use only `std`, so every host compiles them; the Linux handle adapter
//! lives next to `upload_fs`. Nothing routes the file operations through these traits yet.
use super::capabilities::{Capabilities, FileKey, Level};
use super::path::NativePath;
use std::io;

/// One path component, already checked: never empty, `.` or `..`, no `/`, no NUL. A backend for a
/// system with other separators may refuse more; it never accepts less.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name(String);
impl Name {
    pub fn new(name: &str) -> io::Result<Self> {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Ok(Self(name.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Other,
}
/// What a directory entry is, without following a symlink at that entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryMeta {
    pub kind: EntryKind,
    pub key: FileKey,
    pub len: u64,
    /// `None` where the system has no link count.
    pub nlink: Option<u64>,
}
/// End mode of a file: private while it is being written, readable once it is final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileMode {
    /// 0600
    Private,
    /// 0644
    Final,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirMode {
    /// 0700
    Private,
    /// 0755
    Normal,
}

/// A mount as the system lists it; only a candidate for an alias, the decision rests on identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountAlias {
    pub mount_id: u64,
    /// Path inside the mounted file system.
    pub root: String,
    /// Where it is mounted.
    pub point: String,
}
/// Mounts whose paths are valid UTF-8.
pub type AliasMap = Vec<MountAlias>;

/// An opened regular file. Identity and link count come from the opened object, never from a
/// second look at the path. `len` can fail, so an `is_empty` beside it would only mislead.
#[allow(clippy::len_without_is_empty)]
pub trait OpenFile: io::Read + io::Write + Send {
    fn key(&self) -> io::Result<FileKey>;
    fn nlink(&self) -> io::Result<Option<u64>>;
    fn len(&self) -> io::Result<u64>;
    fn set_mode(&self, mode: FileMode) -> io::Result<()>;
    fn sync(&self) -> io::Result<()>;
    /// Owned by the current user and readable by nobody else.
    fn owned_private(&self) -> io::Result<Level>;
    fn modified_age(&self) -> io::Result<std::time::Duration>;
}

/// An opened directory; every operation is relative to it.
pub trait DirHandle: Sized + Send {
    type File: OpenFile;
    fn try_clone(&self) -> io::Result<Self>;
    fn key(&self) -> io::Result<FileKey>;
    /// Owned by the current user and closed to everybody else.
    fn owned_private(&self) -> io::Result<Level>;
    /// Never follows a symlink.
    fn open_dir(&self, name: &Name) -> io::Result<Self>;
    /// Only to walk the chain of ancestors, never to resolve a user path.
    fn parent(&self) -> io::Result<Self>;
    /// Looks at the entry itself, also when it is a symlink.
    fn entry(&self, name: &Name) -> io::Result<EntryMeta>;
    /// Opens a regular file for reading; anything else is refused with `InvalidData`.
    fn open_regular(&self, name: &Name) -> io::Result<Self::File>;
    /// Creates a private file; fails with `AlreadyExists` instead of opening a present one.
    fn create_new(&self, name: &Name) -> io::Result<Self::File>;
    fn mkdir(&self, name: &Name, mode: DirMode) -> io::Result<()>;
    /// Never recursive.
    fn rmdir(&self, name: &Name) -> io::Result<()>;
    /// Removes the entry by name; it cannot pin the object that was checked beforehand.
    fn unlink(&self, name: &Name) -> io::Result<()>;
    /// Fails with `AlreadyExists` instead of replacing a present target.
    fn rename_noreplace(&self, from: &Name, to_dir: &Self, to: &Name) -> io::Result<()>;
    /// Replaces `to`: only for the journal file.
    fn replace_journal(&self, from: &Name, to: &Name) -> io::Result<()>;
    fn sync(&self) -> io::Result<()>;
    /// All names; an error is never reported as an empty list. Names that are not UTF-8 are left out.
    fn names_exact(&self) -> io::Result<Vec<String>>;
    /// For clean-up: the same listing, but an error may simply end it early.
    fn names_lenient(&self) -> io::Result<Vec<String>>;
    fn free_bytes(&self) -> io::Result<u64>;
    /// Mount id of the directory itself or of one entry; `None` where the system has none.
    fn mount(&self, name: Option<&Name>) -> io::Result<Option<u64>>;
}

pub trait FileBackend: Send + Sync {
    type Dir: DirHandle;
    fn open_root(&self, root: &NativePath) -> io::Result<Self::Dir>;
    /// Establishes the capabilities of one root by trying them with a probe file; the probe
    /// cleans up after itself.
    fn probe(&self, root: &Self::Dir, trash: &Self::Dir) -> io::Result<Capabilities>;
    fn mount_aliases(&self) -> io::Result<AliasMap>;
    fn random_op_id(&self) -> io::Result<[u8; 16]>;
    fn map_error(&self, e: &io::Error) -> super::Error;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_single_plain_components() {
        for good in ["a", "a b", "..a", "a..", ".hidden", "ü", "a\\b", "a:b"] {
            assert_eq!(Name::new(good).unwrap().as_str(), good);
        }
        for bad in ["", ".", "..", "a/b", "/", "a\0b"] {
            assert_eq!(Name::new(bad).unwrap_err().kind(), io::ErrorKind::InvalidInput, "{bad:?}");
        }
    }
}
