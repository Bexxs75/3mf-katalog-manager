//! Reading files whose paths come from untrusted places (catalog backups,
//! imports): only regular files, and never more than a fixed budget. A path
//! like `/dev/zero` would otherwise be read forever, and a FIFO would block
//! already when it is opened.

use std::fs::File;
use std::io::{Error, ErrorKind};
use std::path::Path;

/// Opens `path` for reading if it is a regular file. On Unix the file is
/// opened non-blocking, so a FIFO fails the type check instead of hanging;
/// the check runs on the opened handle, so the path can't be swapped between
/// check and use.
pub fn open_regular(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::new(ErrorKind::InvalidInput, "keine reguläre Datei"));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::unique_test_dir;



    #[test]
    fn rejects_a_directory() {
        let dir = unique_test_dir("safe_file_dir");
        assert!(open_regular(&dir).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_device_without_reading_it() {
        let err = open_regular(Path::new("/dev/zero")).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_fifo_without_blocking() {
        let path = unique_test_dir("safe_file_fifo").join("pipe.stl");
        let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        assert_eq!(open_regular(&path).unwrap_err().kind(), ErrorKind::InvalidInput);
    }
}
