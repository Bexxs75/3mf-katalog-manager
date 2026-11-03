//! Adapter that lets `Directory` (descriptor-relative Linux primitives) and `std::fs::File` act as
//! the file backend of `fs_portable::backend`. Only forwards to the methods that exist; no caller
//! of `fsops` goes through it yet.
use super::{cstring, cvt, Directory};
use crate::fs_portable::backend::{DirHandle, DirMode, EntryKind, EntryMeta, FileMode, Name, OpenFile};
use crate::fs_portable::capabilities::{FileKey, Level};
use std::{
    fs::File,
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, PermissionsExt},
    },
    time::Duration,
};

fn key_of(meta: &std::fs::Metadata) -> FileKey {
    FileKey { volume: meta.dev(), index: u128::from(meta.ino()) }
}
fn owned_by_me_with_mode(meta: &std::fs::Metadata, mask: u32, mode: u32) -> Level {
    // SAFETY: geteuid has no preconditions and cannot fail.
    if meta.uid() == unsafe { libc::geteuid() } && meta.mode() & mask == mode {
        Level::Yes
    } else {
        Level::No
    }
}

impl OpenFile for File {
    fn key(&self) -> io::Result<FileKey> {
        Ok(key_of(&self.metadata()?))
    }
    fn nlink(&self) -> io::Result<Option<u64>> {
        Ok(Some(self.metadata()?.nlink()))
    }
    fn len(&self) -> io::Result<u64> {
        Ok(self.metadata()?.len())
    }
    fn set_mode(&self, mode: FileMode) -> io::Result<()> {
        let bits = match mode {
            FileMode::Private => 0o600,
            FileMode::Final => 0o644,
        };
        self.set_permissions(std::fs::Permissions::from_mode(bits))
    }
    fn sync(&self) -> io::Result<()> {
        self.sync_all()
    }
    fn owned_private(&self) -> io::Result<Level> {
        Ok(owned_by_me_with_mode(&self.metadata()?, 0o777, 0o600))
    }
    fn modified_age(&self) -> io::Result<Duration> {
        self.metadata()?
            .modified()?
            .elapsed()
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))
    }
}

impl DirHandle for Directory {
    type File = File;
    fn try_clone(&self) -> io::Result<Self> {
        Directory::try_clone(self)
    }
    fn key(&self) -> io::Result<FileKey> {
        Ok(key_of(&self.0.metadata()?))
    }
    fn owned_private(&self) -> io::Result<Level> {
        Ok(owned_by_me_with_mode(&self.0.metadata()?, 0o7777, 0o700))
    }
    fn open_dir(&self, name: &Name) -> io::Result<Self> {
        Directory::open_dir(self, name.as_str())
    }
    fn parent(&self) -> io::Result<Self> {
        self.parent_handle()
    }
    #[allow(clippy::unnecessary_cast)] // Unix stat field widths vary.
    fn entry(&self, name: &Name) -> io::Result<EntryMeta> {
        let name = cstring(name.as_str())?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: live descriptor, valid C string, the buffer is only read after success.
        cvt(unsafe {
            libc::fstatat(self.0.as_raw_fd(), name.as_ptr(), stat.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW)
        })?;
        let stat = unsafe { stat.assume_init() };
        let kind = match stat.st_mode & libc::S_IFMT {
            libc::S_IFREG => EntryKind::File,
            libc::S_IFDIR => EntryKind::Dir,
            _ => EntryKind::Other,
        };
        Ok(EntryMeta {
            kind,
            key: FileKey { volume: stat.st_dev as u64, index: u128::from(stat.st_ino as u64) },
            len: stat.st_size as u64,
            nlink: Some(stat.st_nlink as u64),
        })
    }
    fn open_regular(&self, name: &Name) -> io::Result<File> {
        self.read(name.as_str())
    }
    fn create_new(&self, name: &Name) -> io::Result<File> {
        self.create(name.as_str())
    }
    fn mkdir(&self, name: &Name, mode: DirMode) -> io::Result<()> {
        let bits = match mode {
            DirMode::Private => 0o700,
            DirMode::Normal => 0o755,
        };
        Directory::mkdir(self, name.as_str(), bits)
    }
    fn rmdir(&self, name: &Name) -> io::Result<()> {
        Directory::rmdir(self, name.as_str())
    }
    fn unlink(&self, name: &Name) -> io::Result<()> {
        Directory::unlink(self, name.as_str())
    }
    fn rename_noreplace(&self, from: &Name, to_dir: &Self, to: &Name) -> io::Result<()> {
        Directory::rename_noreplace(self, from.as_str(), to_dir, to.as_str())
    }
    fn replace_journal(&self, from: &Name, to: &Name) -> io::Result<()> {
        self.rename(from.as_str(), to.as_str())
    }
    fn sync(&self) -> io::Result<()> {
        Directory::sync(self)
    }
    fn names_exact(&self) -> io::Result<Vec<String>> {
        self.names_checked()
    }
    fn names_lenient(&self) -> io::Result<Vec<String>> {
        self.names()
    }
    fn free_bytes(&self) -> io::Result<u64> {
        self.available()
    }
    fn mount(&self, name: Option<&Name>) -> io::Result<Option<u64>> {
        match name {
            None => self.mount_id(),
            Some(name) => self.entry_mount_id(name.as_str()),
        }
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod backend_tests;
