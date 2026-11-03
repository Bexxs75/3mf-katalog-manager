//! Adapter that lets `Directory` (descriptor-relative Linux primitives) and `std::fs::File` act as
//! the file backend of `fs_portable::backend`. Only forwards to the methods that exist; no caller
//! of `fsops` goes through it yet.
use super::{cstring, cvt, Directory};
use crate::fs_portable::backend::{
    AliasMap, DirHandle, DirMode, EntryKind, EntryMeta, FileBackend, FileMode, MountAlias, Name, OpenFile,
};
use crate::fs_portable::capabilities::{
    Capabilities, FileKey, IdentityKind, Level, NlinkKind, Normalization,
};
use crate::fs_portable::path::{NativePath, Style};
use std::{
    io::{Read, Write},
    path::Path,
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

/// The Linux backend: every directory is a descriptor, so the folder a root names is pinned once
/// it is open (ancestors are the remaining risk, as for the server today).
pub struct HandleBackend;

// File systems whose (device, inode) identity is trusted once the probe saw it stay put across a
// rename. Identity over restarts, remounts and time cannot be proven by a probe, so everything
// outside this list reports `Unknown` instead of an assumption.
const LOCAL_FS_TYPES: [u64; 5] = [
    0xEF53,      // ext2, ext3, ext4
    0x5846_5342, // xfs
    0x9123_683E, // btrfs
    0x0102_1994, // tmpfs
    0xF2F5_2010, // f2fs
];
// File systems without hard links, where a link count of 1 is simply true.
const NO_HARDLINK_FS_TYPES: [u64; 2] = [
    0x4D44,      // FAT (msdos, vfat)
    0x2011_BAB0, // exFAT
];

fn fs_type(dir: &Directory) -> io::Result<u64> {
    let mut buf = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: live descriptor, the buffer is only read after success.
    cvt(unsafe { libc::fstatfs(dir.0.as_raw_fd(), buf.as_mut_ptr()) })?;
    let buf = unsafe { buf.assume_init() };
    #[allow(clippy::unnecessary_cast)] // f_type is a signed word of varying width.
    Ok((buf.f_type as u64) & 0xFFFF_FFFF)
}
fn is_unsupported(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::EINVAL | libc::ENOSYS | libc::EOPNOTSUPP | libc::EPERM)
    )
}

/// Everything the probe creates is removed again, also when a step fails halfway.
fn probe_files(root: &Directory, trash: &Directory, id: &str) -> io::Result<Capabilities> {
    let name = Name::new(&format!(".fsops-probe-{id}"))?;
    let link = Name::new(&format!(".fsops-probe-{id}-link"))?;
    let target = Name::new(&format!(".fsops-target-{id}"))?;
    let result = probe_steps(root, trash, &name, &link, &target);
    for (dir, leftover) in [(root, &name), (root, &link), (trash, &target), (trash, &name)] {
        let _ = DirHandle::unlink(dir, leftover);
    }
    // Only the result of the steps counts; a directory sync that fails here was judged above.
    let _ = DirHandle::sync(root).and_then(|()| DirHandle::sync(trash));
    result
}

fn probe_steps(
    root: &Directory,
    trash: &Directory,
    name: &Name,
    link: &Name,
    target: &Name,
) -> io::Result<Capabilities> {
    let mut file = root.create_new(name)?;
    file.write_all(b"fileops probe")?;
    OpenFile::sync(&file)?;
    let key = file.key()?;
    // The opened object and the directory entry must name the same thing.
    let mut stable = root.entry(name)?.key == key;

    let dir_fsync = match DirHandle::sync(root).and_then(|()| DirHandle::sync(trash)) {
        Ok(()) => Level::Yes,
        Err(e) if is_unsupported(&e) => Level::No,
        Err(e) => return Err(e),
    };

    // Link count: 1 on a new file, 2 with a second name, 1 again without it.
    let nlink = if file.nlink()? != Some(1) {
        NlinkKind::Unreliable
    } else {
        match root.link(name.as_str(), link.as_str()) {
            Ok(()) => {
                let two = file.nlink()? == Some(2);
                DirHandle::unlink(root, link)?;
                if two && file.nlink()? == Some(1) {
                    NlinkKind::Reliable
                } else {
                    NlinkKind::Unreliable
                }
            }
            Err(e) if is_unsupported(&e) => {
                if NO_HARDLINK_FS_TYPES.contains(&fs_type(root)?) {
                    NlinkKind::NoHardlinks
                } else {
                    NlinkKind::Unreliable
                }
            }
            Err(e) => return Err(e),
        }
    };

    // No-replace: a present target must survive. Only then the rename is moved there and back to
    // see that the identity does not change with it.
    trash.create_new(target)?.sync_all()?;
    let atomic = match DirHandle::rename_noreplace(root, name, trash, target) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Level::Yes,
        Err(e) if is_unsupported(&e) => Level::No,
        Err(e) => return Err(e),
        // The present target was replaced: no-replace is not honoured.
        Ok(()) => Level::No,
    };
    let mut proven = false;
    if atomic == Level::Yes {
        DirHandle::unlink(trash, target)?;
        DirHandle::rename_noreplace(root, name, trash, target)?;
        let _ = DirHandle::sync(trash);
        stable &= trash.entry(target)?.key == key;
        DirHandle::rename_noreplace(trash, target, root, name)?;
        stable &= root.entry(name)?.key == key;
        proven = true;
    }
    let file_identity = if proven && !stable {
        IdentityKind::Unstable
    } else if proven && LOCAL_FS_TYPES.contains(&fs_type(root)?) {
        IdentityKind::DevIno64
    } else {
        IdentityKind::Unknown
    };

    let detects_mount_aliases = match (root.mount(None), trash.mount(None)) {
        (Ok(Some(_)), Ok(Some(_))) => Level::Yes,
        _ => Level::Unknown,
    };
    Ok(Capabilities {
        prevents_symlink_races: Level::Yes,
        atomic_noreplace_rename: atomic,
        dir_fsync,
        file_identity,
        nlink,
        // Whether a folder is case-folding (ext4/f2fs `casefold`) is not probed yet.
        case_insensitive_names: Level::Unknown,
        unicode_normalization: Normalization::None,
        detects_mount_aliases,
        private_files: if trash.owned_private()? == Level::Yes { Level::Yes } else { Level::No },
        // Deleting goes by name; no delete through the checked object exists here.
        unlink_verified_object: Level::No,
    })
}

fn unescape_mount_path(s: &str) -> io::Result<Option<String>> {
    let bad = || io::Error::from(io::ErrorKind::InvalidData);
    let mut bytes = Vec::new();
    let mut input = s.as_bytes().iter().copied();
    while let Some(b) = input.next() {
        if b == b'\\' {
            let mut n = 0u16;
            for _ in 0..3 {
                let digit = input.next().filter(|b| (b'0'..=b'7').contains(b)).ok_or_else(bad)?;
                n = n * 8 + u16::from(digit - b'0');
            }
            bytes.push(u8::try_from(n).map_err(|_| bad())?);
        } else {
            bytes.push(b);
        }
    }
    Ok(String::from_utf8(bytes).ok())
}
/// Reads `/proc/self/mountinfo` text. A mount whose paths are not UTF-8 is left out.
pub(crate) fn parse_mountinfo(text: &str) -> io::Result<AliasMap> {
    let bad = || io::Error::from(io::ErrorKind::InvalidData);
    let mut out = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 6 {
            return Err(bad());
        }
        let (root, point) = (unescape_mount_path(fields[3])?, unescape_mount_path(fields[4])?);
        if let (Some(root), Some(point)) = (root, point) {
            out.push(MountAlias { mount_id: fields[0].parse().map_err(|_| bad())?, root, point });
        }
    }
    Ok(out)
}

impl FileBackend for HandleBackend {
    type Dir = Directory;
    fn open_root(&self, root: &NativePath) -> io::Result<Directory> {
        if root.style() != Style::Posix {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Directory::open(Path::new(root.as_str()))
    }
    fn probe(&self, root: &Directory, trash: &Directory) -> io::Result<Capabilities> {
        let id: String = self.random_op_id()?.iter().map(|b| format!("{b:02x}")).collect();
        probe_files(root, trash, &id)
    }
    fn mount_aliases(&self) -> io::Result<AliasMap> {
        parse_mountinfo(&std::fs::read_to_string("/proc/self/mountinfo")?)
    }
    fn random_op_id(&self) -> io::Result<[u8; 16]> {
        let mut bytes = [0u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    fn map_error(&self, e: &io::Error) -> crate::fs_portable::Error {
        // `From<io::Error>` consumes its argument, so rebuild an equal error from the parts it reads.
        crate::fs_portable::Error::from(match e.raw_os_error() {
            Some(errno) => io::Error::from_raw_os_error(errno),
            None => io::Error::from(e.kind()),
        })
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod backend_tests;
