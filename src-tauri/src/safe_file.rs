//! Reading files whose paths come from untrusted places (catalog backups,
//! imports): only regular files, and never more than a fixed budget. A path
//! like `/dev/zero` would otherwise be read forever, and a FIFO would block
//! already when it is opened.

use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::path::Path;

/// Largest model file (STL/OBJ) that is read into memory. Real models stay far
/// below; parsing needs roughly another 1.5x of the file size.
pub const MAX_MODEL_FILE_BYTES: u64 = 1024 * 1024 * 1024;

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

/// Reads a regular file completely, but at most `max` bytes; a larger file is
/// an error instead of a huge allocation.
pub fn read_bounded(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let file = open_regular(path)?;
    if file.metadata()?.len() > max {
        return Err(Error::new(ErrorKind::InvalidData, "Datei ist zu groß"));
    }
    let mut bytes = Vec::new();
    // The size can still grow after the check; `take` keeps the read finite.
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(Error::new(ErrorKind::InvalidData, "Datei ist zu groß"));
    }
    Ok(bytes)
}

/// Creates `path` exclusively (never follows a symlink, never replaces a file)
/// and, on Unix, readable only by the owner from the first moment on - a
/// chmod afterwards leaves a window in which others can open the file.
pub fn create_new_private(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Creates a new private file `<prefix>.<pid>-<nanos>-<n><suffix>` in `dir`.
/// A fixed name would collide with a leftover or a second run in the same
/// process (`create_new` then fails with "File exists"); a name taken by
/// someone else (e.g. a planted symlink) is skipped, never opened.
pub fn create_private_temp(dir: &Path, prefix: &str, suffix: &str) -> std::io::Result<(std::path::PathBuf, File)> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut last_err = None;
    for attempt in 0..16 {
        let candidate = dir.join(format!("{prefix}.{}-{nanos}-{attempt}{suffix}", std::process::id()));
        match create_new_private(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::other("no temp file could be created")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::unique_test_dir;

    #[test]
    fn reads_a_regular_file() {
        let path = unique_test_dir("safe_file_regular").join("a.stl");
        std::fs::write(&path, b"solid").unwrap();
        assert_eq!(read_bounded(&path, 100).unwrap(), b"solid");
    }

    #[test]
    fn rejects_a_file_over_the_limit() {
        let path = unique_test_dir("safe_file_big").join("a.stl");
        std::fs::write(&path, vec![0u8; 101]).unwrap();
        assert_eq!(read_bounded(&path, 100).unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_a_directory() {
        let dir = unique_test_dir("safe_file_dir");
        assert!(open_regular(&dir).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_device_without_reading_it() {
        let err = read_bounded(Path::new("/dev/zero"), 100).unwrap_err();
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

    /// Every parser entry that takes a path must reject special files at once.
    #[cfg(unix)]
    #[test]
    fn model_parsers_reject_devices_and_fifos_without_hanging() {
        let fifo = unique_test_dir("parser_fifo").join("pipe.stl");
        let c_path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for path in [Path::new("/dev/zero"), fifo.as_path()] {
                let results = [
                    crate::stl::parse_stl_file(path).is_err(),
                    crate::obj::parse_obj_file(path).is_err(),
                    crate::threemf::parse_3mf_file(path).is_err(),
                    crate::threemf::extract_render_meshes_from_path(path).is_err(),
                ];
                let _ = tx.send(results);
            }
        });
        for _ in 0..2 {
            let results = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("parsers must not hang on /dev/zero or a FIFO");
            assert_eq!(results, [true; 4]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_is_0600_from_the_start_and_never_replaces_anything() {
        use std::os::unix::fs::PermissionsExt;
        let dir = unique_test_dir("private_new");
        let path = dir.join("a.db");
        let file = create_new_private(&path).unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(create_new_private(&path).unwrap_err().kind(), ErrorKind::AlreadyExists);

        let target = dir.join("victim.txt");
        std::fs::write(&target, b"KEEP").unwrap();
        let link = dir.join("link.db");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(create_new_private(&link).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"KEEP");
    }
}
