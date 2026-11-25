//! File-system boundary of the desktop file actions (issue #20, package P0a).
//!
//! The flows `move_file_core`, `rename_file_with_conn`, `rename::*`, the trash of a model
//! and the companion carry/trash code do their disk access through the small [`Fs`] trait.
//! Production code always passes [`RealFs`], which delegates 1:1 to `std::fs` (same calls,
//! same errors). Tests pass [`FaultFs`] to fail the n-th call of one operation with a chosen
//! `io::ErrorKind`, without needing Windows or macOS. See
//! `docs/plans/issue20-p0a-fs-grenzen.md` for what is and is not injected.
use std::ffi::OsString;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

/// Only the operations the flows really use.
pub(crate) trait Fs {
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata>;
    fn metadata(&self, path: &Path) -> io::Result<Metadata>;
    /// File names of a directory; an error of any entry fails the whole listing.
    fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<OsString>>;
    /// Only used by the non-Unix collision check.
    #[allow(dead_code)]
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    /// Copy half of a copy-based move: creates `to` (never replaces), copies content,
    /// permissions and time; the source stays. A partial target is removed on failure.
    fn copy_new(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Same as `Path::is_dir` (follows links, any error is `false`).
    fn is_dir(&self, path: &Path) -> bool {
        self.metadata(path).map(|m| m.is_dir()).unwrap_or(false)
    }
    /// Same as `Path::is_file`.
    fn is_file(&self, path: &Path) -> bool {
        self.metadata(path).map(|m| m.is_file()).unwrap_or(false)
    }
    /// Same as `Path::exists`.
    fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }
}

/// The real file system.
pub(crate) struct RealFs;

impl Fs for RealFs {
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        std::fs::symlink_metadata(path)
    }
    fn metadata(&self, path: &Path) -> io::Result<Metadata> {
        std::fs::metadata(path)
    }
    fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<OsString>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            names.push(entry?.file_name());
        }
        Ok(names)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }
    fn copy_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        katalog_core::import::placement::copy_file_new(from, to)
    }
}

/// `commands::move_file` (core: `katalog_core::import::placement::move_file`) on top of
/// [`Fs`]: hard link first and remove the source, copy as fallback; never replaces the
/// target. Same steps as the core function, so the two must stay in sync.
pub(crate) fn move_file_with(fs: &dyn Fs, from: &Path, to: &Path) -> io::Result<()> {
    let is_symlink = fs.symlink_metadata(from).map(|m| m.file_type().is_symlink()).unwrap_or(false);
    if !is_symlink {
        match fs.hard_link(from, to) {
            Ok(()) => {
                if let Err(remove_err) = fs.remove_file(from) {
                    let _ = fs.remove_file(to);
                    return Err(remove_err);
                }
                return Ok(());
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(e),
            Err(_) => {}
        }
    }
    fs.copy_new(from, to)?;
    if let Err(remove_err) = fs.remove_file(from) {
        let _ = fs.remove_file(to);
        return Err(remove_err);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) use fault::{FaultFs, Op};

#[cfg(test)]
mod fault {
    use super::*;
    use std::cell::RefCell;

    /// The operations of [`Fs`], for faults and the call log.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Op {
        SymlinkMetadata,
        Metadata,
        ReadDir,
        Canonicalize,
        Rename,
        HardLink,
        RemoveFile,
        RemoveDir,
        CreateDir,
        CopyNew,
    }

    struct Fault {
        op: Op,
        /// Only calls whose (first) path contains this text count; `None` counts all.
        path_contains: Option<String>,
        /// 1-based number of the matching call that fails.
        nth: usize,
        kind: io::ErrorKind,
        seen: usize,
    }

    /// Real files on disk (use a temp dir), but the n-th call of an operation can be made
    /// to fail, every call is logged, and a case-insensitive volume can be simulated.
    #[derive(Default)]
    pub(crate) struct FaultFs {
        faults: RefCell<Vec<Fault>>,
        calls: RefCell<Vec<(Op, PathBuf, Option<PathBuf>)>>,
        case_insensitive: bool,
    }

    impl FaultFs {
        pub(crate) fn new() -> Self {
            Self::default()
        }
        /// Simulates macOS/Windows name lookup: a missing name that matches an existing
        /// sibling in lower case is reported as existing (metadata of that sibling), and
        /// links/copies onto such a name fail with `AlreadyExists`.
        pub(crate) fn case_insensitive(mut self) -> Self {
            self.case_insensitive = true;
            self
        }
        /// The `nth` (1-based) call of `op` fails with `kind`.
        pub(crate) fn fail(self, op: Op, nth: usize, kind: io::ErrorKind) -> Self {
            self.fail_on(op, None, nth, kind)
        }
        /// Like `fail`, but only calls whose first path contains `needle` are counted.
        pub(crate) fn fail_on(self, op: Op, needle: Option<&str>, nth: usize, kind: io::ErrorKind) -> Self {
            self.faults.borrow_mut().push(Fault {
                op,
                path_contains: needle.map(str::to_owned),
                nth,
                kind,
                seen: 0,
            });
            self
        }
        pub(crate) fn calls(&self) -> Vec<(Op, PathBuf, Option<PathBuf>)> {
            self.calls.borrow().clone()
        }
        pub(crate) fn count(&self, op: Op) -> usize {
            self.calls.borrow().iter().filter(|c| c.0 == op).count()
        }

        fn enter(&self, op: Op, path: &Path, second: Option<&Path>) -> io::Result<()> {
            self.calls.borrow_mut().push((op, path.to_path_buf(), second.map(Path::to_path_buf)));
            let text = path.to_string_lossy();
            // Every matching fault counts the call, even when an earlier one already fails it.
            let mut result = Ok(());
            for fault in self.faults.borrow_mut().iter_mut() {
                if fault.op != op || fault.path_contains.as_deref().is_some_and(|n| !text.contains(n)) {
                    continue;
                }
                fault.seen += 1;
                if fault.seen == fault.nth && result.is_ok() {
                    result = Err(io::Error::new(fault.kind, format!("injected {:?} #{} on {text}", fault.op, fault.nth)));
                }
            }
            result
        }

        /// A sibling that differs from `path` only in case (simulated insensitive volume).
        fn case_twin(&self, path: &Path) -> Option<PathBuf> {
            if !self.case_insensitive {
                return None;
            }
            let (dir, name) = (path.parent()?, path.file_name()?.to_str()?.to_lowercase());
            std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_lowercase() == name)
            })
        }
    }

    impl Fs for FaultFs {
        fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
            self.enter(Op::SymlinkMetadata, path, None)?;
            match std::fs::symlink_metadata(path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => match self.case_twin(path) {
                    Some(twin) => std::fs::symlink_metadata(twin),
                    None => Err(e),
                },
                other => other,
            }
        }
        fn metadata(&self, path: &Path) -> io::Result<Metadata> {
            self.enter(Op::Metadata, path, None)?;
            match std::fs::metadata(path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => match self.case_twin(path) {
                    Some(twin) => std::fs::metadata(twin),
                    None => Err(e),
                },
                other => other,
            }
        }
        fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<OsString>> {
            self.enter(Op::ReadDir, dir, None)?;
            RealFs.read_dir_names(dir)
        }
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            self.enter(Op::Canonicalize, path, None)?;
            std::fs::canonicalize(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.enter(Op::Rename, from, Some(to))?;
            std::fs::rename(from, to)
        }
        fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.enter(Op::HardLink, from, Some(to))?;
            // A simulated insensitive volume refuses a link onto the same name in other case.
            if self.case_twin(to).is_some_and(|twin| twin != from) {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            std::fs::hard_link(from, to)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.enter(Op::RemoveFile, path, None)?;
            std::fs::remove_file(path)
        }
        fn remove_dir(&self, path: &Path) -> io::Result<()> {
            self.enter(Op::RemoveDir, path, None)?;
            std::fs::remove_dir(path)
        }
        fn create_dir(&self, path: &Path) -> io::Result<()> {
            self.enter(Op::CreateDir, path, None)?;
            std::fs::create_dir(path)
        }
        fn copy_new(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.enter(Op::CopyNew, from, Some(to))?;
            if self.case_twin(to).is_some() {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            RealFs.copy_new(from, to)
        }
    }
}
