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
        let name = cstring(name)?;
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        let file = unsafe { File::from_raw_fd(cvt(fd)?) };
        if !file.metadata()?.is_file() {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(file)
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
