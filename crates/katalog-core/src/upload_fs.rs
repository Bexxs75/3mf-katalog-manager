//! Descriptor-relative filesystem operations for the web upload boundary.
use std::{
    ffi::{CStr, CString},
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path},
};
use unicode_normalization::UnicodeNormalization;

pub fn nfc(value: &str) -> String {
    value.nfc().collect()
}
pub struct Directory(File);
fn cstring(name: &str) -> io::Result<CString> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    CString::new(name).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}
fn cvt(result: i32) -> io::Result<i32> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}
impl Directory {
    pub fn open(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut file = File::open("/")?;
        for part in path.components() {
            match part {
                Component::RootDir => continue,
                Component::Normal(name) => {
                    let name = CString::new(name.as_bytes())
                        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
                    // SAFETY: valid descriptor and NUL-terminated component; ownership transfers once.
                    let fd = unsafe {
                        libc::openat(
                            file.as_raw_fd(),
                            name.as_ptr(),
                            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                        )
                    };
                    file = unsafe { File::from_raw_fd(cvt(fd)?) };
                }
                _ => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
            }
        }
        Ok(Self(file))
    }
    pub fn create(&self, name: &str) -> io::Result<File> {
        let name = cstring(name)?;
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        Ok(unsafe { File::from_raw_fd(cvt(fd)?) })
    }
    pub fn read(&self, name: &str) -> io::Result<File> {
        #[cfg(all(test, target_os = "linux"))]
        if let Some(errno) = FSOPS_READ_ERROR.with(|h| h.take()) {
            return Err(io::Error::from_raw_os_error(errno));
        }
        let name = cstring(name)?;
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC,
            )
        };
        // O_NONBLOCK keeps a FIFO from blocking the open; the type check follows at once.
        let file = unsafe { File::from_raw_fd(cvt(fd)?) };
        if !file.metadata()?.is_file() {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(file)
    }
    /// Like `read`, but for journals that drive recovery decisions: the file must
    /// be owned by the current user, mode 0600, not hard-linked and at most
    /// `max_len` bytes. A violation is reported as `InvalidData`.
    pub fn read_private(&self, name: &str, max_len: u64) -> io::Result<File> {
        use std::os::unix::fs::MetadataExt;
        let file = self.read(name)?;
        let meta = file.metadata()?;
        // SAFETY: geteuid has no preconditions and cannot fail.
        if meta.nlink() != 1
            || meta.uid() != unsafe { libc::geteuid() }
            || meta.mode() & 0o777 != 0o600
            || meta.len() > max_len
        {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(file)
    }
    /// Inspect the directory entry itself, including a replacement symlink.
    #[allow(clippy::unnecessary_cast)] // Unix stat field widths vary.
    pub fn entry_identity(&self, name: &str) -> io::Result<(u64, u64)> {
        #[cfg(all(test, target_os = "linux"))]
        if let Some(errno) = FSOPS_ENTRY_ERROR.with(|h| h.take()) {
            return Err(io::Error::from_raw_os_error(errno));
        }
        let name = cstring(name)?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        cvt(unsafe {
            libc::fstatat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;
        let stat = unsafe { stat.assume_init() };
        Ok((stat.st_dev as u64, stat.st_ino as u64))
    }
    pub fn checked(&self, name: &str, dev: u64, ino: u64) -> io::Result<File> {
        let file = self.read(name)?;
        let m = file.metadata()?;
        if m.dev() != dev || m.ino() != ino || m.nlink() != 1 {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(file)
    }
    pub fn link(&self, source: &str, target: &str) -> io::Result<()> {
        let (source, target) = (cstring(source)?, cstring(target)?);
        cvt(unsafe {
            libc::linkat(
                self.0.as_raw_fd(),
                source.as_ptr(),
                self.0.as_raw_fd(),
                target.as_ptr(),
                0,
            )
        })?;
        Ok(())
    }
    /// Hard-links `source` of another directory into this one under `target`.
    /// Fails with `AlreadyExists` instead of replacing; never follows a symlink
    /// at `source`.
    pub fn link_from(&self, source_dir: &Directory, source: &str, target: &str) -> io::Result<()> {
        let (source, target) = (cstring(source)?, cstring(target)?);
        cvt(unsafe {
            libc::linkat(
                source_dir.0.as_raw_fd(),
                source.as_ptr(),
                self.0.as_raw_fd(),
                target.as_ptr(),
                0,
            )
        })?;
        Ok(())
    }
    pub fn unlink(&self, name: &str) -> io::Result<()> {
        let name = cstring(name)?;
        cvt(unsafe { libc::unlinkat(self.0.as_raw_fd(), name.as_ptr(), 0) })?;
        Ok(())
    }
    pub fn rename(&self, source: &str, target: &str) -> io::Result<()> {
        let (source, target) = (cstring(source)?, cstring(target)?);
        cvt(unsafe {
            libc::renameat(
                self.0.as_raw_fd(),
                source.as_ptr(),
                self.0.as_raw_fd(),
                target.as_ptr(),
            )
        })?;
        Ok(())
    }
    pub fn sync(&self) -> io::Result<()> {
        self.0.sync_all()
    }
    pub fn device(&self) -> io::Result<u64> {
        Ok(self.0.metadata()?.dev())
    }
    #[allow(clippy::unnecessary_cast)] // statvfs field widths differ across Unix platforms.
    pub fn available(&self) -> io::Result<u64> {
        let mut data = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        cvt(unsafe { libc::fstatvfs(self.0.as_raw_fd(), data.as_mut_ptr()) })?;
        let data = unsafe { data.assume_init() };
        Ok((data.f_bavail as u64).saturating_mul(data.f_frsize as u64))
    }
    pub fn names(&self) -> io::Result<Vec<String>> {
        let dot = c".";
        let fd = cvt(unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                dot.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        })?;
        let dir = unsafe { libc::fdopendir(fd) };
        if dir.is_null() {
            unsafe {
                libc::close(fd);
            }
            return Err(io::Error::last_os_error());
        }
        let mut names = Vec::new();
        loop {
            let entry = unsafe { libc::readdir(dir) };
            if entry.is_null() {
                break;
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if let Ok(name) = name.to_str() {
                names.push(name.to_owned());
            }
        }
        unsafe {
            libc::closedir(dir);
        }
        Ok(names)
    }
}

// Additive Linux primitives for web file operations. Existing upload semantics stay unchanged.
#[cfg(target_os = "linux")]
impl Directory {
    pub fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.0.metadata()
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(self.0.try_clone()?))
    }
    pub fn open_dir(&self, name: &str) -> io::Result<Self> {
        let name = cstring(name)?;
        self.open_dir_cstr(&name)
    }
    fn open_dir_cstr(&self, name: &CStr) -> io::Result<Self> {
        // SAFETY: borrowed live descriptor, valid C string, ownership transferred once.
        let fd = cvt(unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        Ok(Self(unsafe { File::from_raw_fd(fd) }))
    }
    /// Only for building protected ancestor identities, never for resolving user paths.
    pub fn parent_handle(&self) -> io::Result<Self> {
        self.open_dir_cstr(c"..")
    }
    pub fn mkdir(&self, name: &str, mode: u32) -> io::Result<()> {
        let name = cstring(name)?;
        cvt(unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), mode as libc::mode_t) })?;
        Ok(())
    }
    /// Never recursive; the kernel rejects a nonempty directory.
    pub fn rmdir(&self, name: &str) -> io::Result<()> {
        let name = cstring(name)?;
        cvt(unsafe { libc::unlinkat(self.0.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) })?;
        Ok(())
    }
    pub fn rename_noreplace(
        &self,
        source: &str,
        target_dir: &Self,
        target: &str,
    ) -> io::Result<()> {
        #[cfg(test)]
        if let Some(errno) = FSOPS_RENAME_ERROR.with(|h| h.take()) {
            return Err(io::Error::from_raw_os_error(errno));
        }
        #[cfg(test)]
        if FSOPS_CASE_INSENSITIVE.with(|h| h.get())
            && source != target
            && source.to_lowercase() == target.to_lowercase()
        {
            return Err(io::Error::from_raw_os_error(libc::EEXIST));
        }
        let (source, target) = (cstring(source)?, cstring(target)?);
        cvt(unsafe {
            libc::renameat2(
                self.0.as_raw_fd(),
                source.as_ptr(),
                target_dir.0.as_raw_fd(),
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        })?;
        #[cfg(test)]
        FSOPS_AFTER_RENAME_ERROR.with(|h| {
            if let Some(errno) = h.take() {
                FSOPS_ENTRY_ERROR.with(|e| e.set(Some(errno)));
            }
        });
        Ok(())
    }
    /// None on kernels/filesystems without mount IDs; callers must compare st_dev too.
    pub fn mount_id(&self) -> io::Result<Option<u64>> {
        self.statx_mount(c"")
    }
    pub fn entry_mount_id(&self, name: &str) -> io::Result<Option<u64>> {
        #[cfg(test)]
        if let Some(id) = FSOPS_ENTRY_MOUNT.with(|h| h.get()) {
            return Ok(Some(id));
        }
        self.statx_mount(&cstring(name)?)
    }
    fn statx_mount(&self, name: &CStr) -> io::Result<Option<u64>> {
        let mut stat = std::mem::MaybeUninit::<libc::statx>::zeroed();
        let result = unsafe {
            libc::statx(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                libc::STATX_MNT_ID,
                stat.as_mut_ptr(),
            )
        };
        if result < 0 {
            let e = io::Error::last_os_error();
            if matches!(
                e.raw_os_error(),
                Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP)
            ) {
                return Ok(None);
            }
            return Err(e);
        }
        let stat = unsafe { stat.assume_init() };
        Ok((stat.stx_mask & libc::STATX_MNT_ID != 0).then_some(stat.stx_mnt_id))
    }
}

#[cfg(all(test, target_os = "linux"))]
thread_local! {
    pub(crate) static FSOPS_READDIR_ERROR: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    pub(crate) static FSOPS_READ_ERROR: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    pub(crate) static FSOPS_CASE_INSENSITIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static FSOPS_AFTER_RENAME_ERROR: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    static FSOPS_ENTRY_ERROR: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    pub(crate) static FSOPS_RENAME_ERROR: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    pub(crate) static FSOPS_ENTRY_MOUNT: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

#[cfg(target_os = "linux")]
impl Directory {
    /// Exact directory enumeration for recovery: an I/O error must never mean absence.
    /// Kept separate from the existing upload API to preserve its behavior.
    pub fn names_checked(&self) -> io::Result<Vec<String>> {
        #[cfg(test)]
        if let Some(errno) = FSOPS_READDIR_ERROR.with(|h| h.take()) {
            return Err(io::Error::from_raw_os_error(errno));
        }
        let fd = cvt(unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        })?;
        let dir = unsafe { libc::fdopendir(fd) };
        if dir.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(error);
        }
        let mut names = Vec::new();
        let result = loop {
            // Linux errno is thread-local. readdir returns NULL for both EOF and error.
            unsafe {
                *libc::__errno_location() = 0;
            }
            let entry = unsafe { libc::readdir(dir) };
            if entry.is_null() {
                let errno = unsafe { *libc::__errno_location() };
                break if errno == 0 {
                    Ok(names)
                } else {
                    Err(io::Error::from_raw_os_error(errno))
                };
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if let Ok(name) = name.to_str() {
                names.push(name.to_owned());
            }
        };
        let close = unsafe { libc::closedir(dir) };
        if result.is_ok() {
            cvt(close)?;
        }
        result
    }
}

#[cfg(all(test, target_os = "linux"))]
mod read_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn read_rejects_fifo_without_blocking_and_read_private_checks_mode_links_and_size() {
        let dir = crate::unique_test_dir("upload_fs_read");
        let fifo = CString::new(dir.join("pipe").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let handle = Directory::open(&dir).unwrap();
        // With no writer a blocking open would hang the test.
        assert_eq!(handle.read("pipe").unwrap_err().kind(), io::ErrorKind::InvalidData);

        std::fs::write(dir.join("journal"), b"{}").unwrap();
        std::fs::set_permissions(dir.join("journal"), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(handle.read_private("journal", 16).is_ok());
        assert_eq!(handle.read_private("journal", 1).unwrap_err().kind(), io::ErrorKind::InvalidData);
        std::fs::set_permissions(dir.join("journal"), std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(handle.read_private("journal", 16).is_err());
        std::fs::set_permissions(dir.join("journal"), std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::hard_link(dir.join("journal"), dir.join("second")).unwrap();
        assert!(handle.read_private("journal", 16).is_err());
    }
}
