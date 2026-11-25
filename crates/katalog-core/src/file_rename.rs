//! "Rename file" of the desktop catalog (issue #20, package P4): the checks, the rename on disk
//! (never replacing a file, a case-only change goes through an intermediate name), the companion
//! files that follow the model, the catalog row, and both rollback branches. The desktop command
//! keeps the lock and the connection; this module has no `fsops` / `upload_fs` dependency, so it
//! builds on Windows and macOS.
//!
//! Every disk access goes through [`FileRenameFs`], which the desktop implements on top of its
//! file-system boundary (`Fs`, P0a); the companion files use [`CompanionFs`] (P3). Tests use a
//! fake that fails in step n. Whether two spellings of a name are one folder (macOS, Windows) is
//! the `case_insensitive` parameter, so Linux tests cover both branches.
//!
//! [`rename_reversible`] is the flow the date rename shares: it renames, carries the companions
//! and calls the host's commit, and undoes everything when a step fails.
use std::ffi::OsString;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use unicode_normalization::UnicodeNormalization;

use crate::companion_actions::{self, CompanionFs};
use crate::db;
use crate::error_code::ErrorCode;
use crate::import::file_names::{validate_name, NameKind};
use crate::import::placement::reject_if_sensitive_path;
use crate::import::{ImportError, ImportResult};

/// The disk operations the rename uses.
pub trait FileRenameFs {
    /// Same as `std::fs::symlink_metadata`.
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata>;
    /// File names of a directory; an error of any entry fails the whole listing.
    fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<OsString>>;
    /// Same as `std::fs::canonicalize` (only the non-Unix collision check uses it).
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    /// Same as `std::fs::rename` (may replace on Linux/macOS; callers check first).
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// A move that never replaces its target (hard link first, copy as fallback), see
    /// `import::placement::move_file`.
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()>;
}

/// `std::fs`, 1:1.
pub struct StdFileRenameFs;

impl FileRenameFs for StdFileRenameFs {
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        std::fs::symlink_metadata(path)
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
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        crate::import::placement::move_file(from, to)
    }
}

pub fn normalized_lowercase(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase().nfc().collect()
}

/// Same folder, different spelling of the name only in letter case (after Unicode normalisation).
pub fn is_case_only(old: &Path, new: &Path) -> bool {
    old.parent() == new.parent()
        && old.file_name() != new.file_name()
        && match (old.file_name().and_then(|s| s.to_str()), new.file_name().and_then(|s| s.to_str())) {
            (Some(old), Some(new)) => normalized_lowercase(old) == normalized_lowercase(new),
            _ => false,
        }
}

/// Whether `new` is taken by an entry that is not `old` itself (a case-only rename on a
/// case-insensitive volume finds `old` under the new spelling, which is no collision).
pub fn target_collides(fs: &dyn FileRenameFs, old: &Path, new: &Path) -> io::Result<bool> {
    let new_metadata = match fs.symlink_metadata(new) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
        Ok(metadata) => metadata,
    };
    if !is_case_only(old, new) {
        return Ok(true);
    }
    // Stored spelling can differ from both paths after an external rename.
    // Exactly one matching entry rules out distinct hard links and symlinks.
    let expected = normalized_lowercase(old.file_name().unwrap().to_str().unwrap());
    let mut matches = 0;
    for entry_name in fs.read_dir_names(old.parent().ok_or_else(|| io::Error::other("missing parent"))?)? {
        if entry_name.to_str().is_some_and(|name| normalized_lowercase(name) == expected) {
            matches += 1;
        }
    }
    if matches != 1 {
        return Ok(true);
    }
    let old_metadata = fs.symlink_metadata(old)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(old_metadata.dev() != new_metadata.dev() || old_metadata.ino() != new_metadata.ino())
    }
    #[cfg(not(unix))]
    {
        // Canonical paths provide a stable fallback without unstable Windows
        // metadata APIs; metadata above also checks that both entries exist.
        Ok(old_metadata.file_type() != new_metadata.file_type() || fs.canonicalize(old)? != fs.canonicalize(new)?)
    }
}

/// A sibling name for the first step of a case-only rename: visible (no leading dot), keeps the
/// extension of a file, fits the 255 byte name limit.
pub fn intermediate_name(fs: &dyn FileRenameFs, old: &Path, counter: u64) -> io::Result<String> {
    let name = old.file_name().and_then(|s| s.to_str()).ok_or_else(|| io::Error::other("invalid source name"))?;
    let is_directory = fs.symlink_metadata(old)?.is_dir();
    let (stem, extension) = if is_directory {
        (name, String::new())
    } else {
        (
            old.file_stem().and_then(|s| s.to_str()).unwrap_or(name),
            old.extension().and_then(|s| s.to_str()).map(|ext| format!(".{ext}")).unwrap_or_default(),
        )
    };
    let suffix = format!(".3mf-rename-tmp-{}-{counter}{extension}", std::process::id());
    let budget = 255usize
        .checked_sub(suffix.len())
        .filter(|budget| *budget > 0)
        .ok_or_else(|| io::Error::other("extension too long for intermediate name"))?;
    let stem = stem.trim_start_matches('.');
    let mut end = stem.len().min(budget);
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = if end == 0 { "_" } else { &stem[..end] };
    Ok(format!("{prefix}{suffix}"))
}

/// Renames a single file inside its directory without ever replacing an existing
/// entry: the collision check alone leaves a window in which another program could
/// create the target, and `std::fs::rename` would then overwrite it on Linux and
/// macOS. `move_file` links first, which fails with `AlreadyExists` instead.
/// Case-only renames and symlinks keep `rename_on_disk`: a case-only target is the
/// file itself on a case-insensitive volume (a link would be refused), and a link
/// to a symlink would not move the symlink.
pub fn rename_file_on_disk(fs: &dyn FileRenameFs, old: &Path, new: &Path) -> io::Result<()> {
    if target_collides(fs, old, new)? {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "rename target exists"));
    }
    let is_symlink = fs.symlink_metadata(old).is_ok_and(|m| m.file_type().is_symlink());
    if is_case_only(old, new) || is_symlink {
        return rename_on_disk(fs, old, new);
    }
    crate::import::rename::race_point("before-file-rename", new);
    fs.move_file(old, new)
}

/// Renames a file or folder in its directory (the target must not exist, except as the same
/// entry under another spelling).
pub fn rename_on_disk(fs: &dyn FileRenameFs, old: &Path, new: &Path) -> io::Result<()> {
    rename_with(fs, old, new, |from, to| fs.rename(from, to))
}

/// `rename_on_disk` with the rename step given by the caller (tests count and fail steps).
pub fn rename_with(fs: &dyn FileRenameFs, old: &Path, new: &Path, mut rename: impl FnMut(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    if target_collides(fs, old, new)? {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "rename target exists"));
    }
    if !is_case_only(old, new) {
        return rename(old, new);
    }
    // A free destination permits one atomic rename, even for case-only changes.
    match fs.symlink_metadata(new) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return rename(old, new),
        Err(error) => return Err(error),
        Ok(_) => {}
    }
    rename_via_intermediate(fs, old, new, rename)
}

/// The two-step rename of a case-only change on a case-insensitive volume: old -> intermediate
/// -> new. A failing second step puts the file back; if that fails too, the error names the
/// intermediate file so nothing is lost.
pub fn rename_via_intermediate(fs: &dyn FileRenameFs, old: &Path, new: &Path, mut rename: impl FnMut(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    let parent = old.parent().ok_or_else(|| io::Error::other("missing parent"))?;
    // A sibling keeps both rename steps on the same filesystem. Check with
    // symlink_metadata so dangling links cannot be used as intermediate names.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let intermediate = loop {
        let counter = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate = parent.join(intermediate_name(fs, old, counter)?);
        match fs.symlink_metadata(&candidate) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => break candidate,
            Err(error) => return Err(error),
            Ok(_) => continue,
        }
    };
    rename(old, &intermediate)?;
    // The source no longer aliases the destination: every existing target
    // now belongs to somebody else, including a dangling symlink.
    let second = match fs.symlink_metadata(new) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => rename(&intermediate, new),
        Err(error) => Err(error),
        Ok(_) => Err(io::Error::new(io::ErrorKind::AlreadyExists, "rename target exists")),
    };
    if let Err(error) = second {
        let rollback = match fs.symlink_metadata(old) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => rename(&intermediate, old),
            Err(e) => Err(e),
            Ok(_) => Err(io::Error::new(io::ErrorKind::AlreadyExists, "rollback target exists")),
        };
        if let Err(rollback) = rollback {
            return Err(io::Error::other(format!(
                "Umbenennung fehlgeschlagen ({error}); Rollback fehlgeschlagen ({rollback}); Daten liegen unter {}",
                intermediate.display()
            )));
        }
        return Err(error);
    }
    Ok(())
}

/// Why [`rename_reversible`] stopped. Everything done before is undone when this is returned;
/// the host decides how each kind is worded and logged.
#[derive(Debug)]
pub enum RenameFlowError {
    /// The rename on disk failed (`AlreadyExists`: the target appeared after the check).
    Disk(io::Error),
    /// The companion files could not follow; the rename was put back.
    Companion(ImportError),
    /// A step failed and the rename could not be put back; the text names where the file is.
    Combined(String),
    /// The host's commit failed; the rename and the companions were put back.
    Commit(String),
}

/// Renames the model file `old` -> `new`, carries its companion files, and calls `commit` (the
/// host's catalog update, exactly once, after the files moved). A failing step undoes the earlier
/// ones; a failing undo is reported in [`RenameFlowError::Combined`] with the place of the file.
#[allow(clippy::too_many_arguments)]
pub fn rename_reversible(
    conn: &Connection,
    fs: &dyn FileRenameFs,
    companion_fs: &dyn CompanionFs,
    file_id: i64,
    old: &Path,
    new: &Path,
    case_insensitive: bool,
    commit: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), RenameFlowError> {
    rename_file_on_disk(fs, old, new).map_err(RenameFlowError::Disk)?;
    // The model's companion files follow into the folder named after the new name.
    let carried = match companion_actions::carry(companion_fs, conn, file_id, old, new, case_insensitive) {
        Ok(carried) => carried,
        Err(error) => {
            if let Err(rollback_err) = rename_file_on_disk(fs, new, old) {
                return Err(RenameFlowError::Combined(format!(
                    "Begleitdateien konnten nicht mitgenommen werden ({error}) UND Rollback der Datei-Umbenennung fehlgeschlagen ({rollback_err}) - Datei heisst jetzt {}",
                    new.display()
                )));
            }
            return Err(RenameFlowError::Companion(error));
        }
    };
    if let Err(db_err) = commit() {
        carried.undo(companion_fs, conn);
        // Undo the rename so file system and DB don't drift apart.
        if let Err(rollback_err) = rename_file_on_disk(fs, new, old) {
            return Err(RenameFlowError::Combined(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Datei-Umbenennung fehlgeschlagen ({rollback_err}) - Datei heisst jetzt {}, DB verweist weiter auf {}",
                new.display(),
                old.display()
            )));
        }
        return Err(RenameFlowError::Commit(db_err));
    }
    carried.finish(companion_fs);
    log::debug!(target: "datei", "umbenannt: {} -> {}", old.display(), new.display());
    Ok(())
}

/// Core logic of `rename_file`: renames a real file on disk (same directory, never replacing a
/// file) and updates `name`/`path` in the catalog; the companion files follow.
pub fn rename_file(
    conn: &Connection,
    fs: &dyn FileRenameFs,
    companion_fs: &dyn CompanionFs,
    id: i64,
    new_name: &str,
    sensitive_dirs: &[PathBuf],
    case_insensitive: bool,
) -> ImportResult<()> {
    validate_name(new_name, true, NameKind::File)?;
    let file = db::get_file(conn, id).map_err(|e| e.to_string())?.ok_or_else(|| "file not found".to_string())?;
    if file.deleted_at.is_some() {
        return Err(ImportError::coded(ErrorCode::FileInTrash));
    }

    let old_path = PathBuf::from(&file.path);
    let new_path = old_path.with_file_name(new_name);
    reject_if_sensitive_path(&old_path, sensitive_dirs)?;
    reject_if_sensitive_path(&new_path, sensitive_dirs)?;

    if new_path == old_path {
        return Ok(());
    }
    if target_collides(fs, &old_path, &new_path).map_err(|e| e.to_string())? {
        return Err(ImportError::coded_with(ErrorCode::FileTargetExists, [new_path.display()]));
    }
    let mut commit = || db::rename_file(conn, id, new_name, &new_path.to_string_lossy()).map_err(|e| e.to_string());
    rename_reversible(conn, fs, companion_fs, id, &old_path, &new_path, case_insensitive, &mut commit).map_err(|error| match error {
        // Created by someone else after the check above; reported like the early collision.
        RenameFlowError::Disk(e) if e.kind() == io::ErrorKind::AlreadyExists => ImportError::coded_with(ErrorCode::FileTargetExists, [new_path.display()]),
        RenameFlowError::Disk(e) => e.to_string().into(),
        RenameFlowError::Companion(error) => error,
        RenameFlowError::Combined(text) | RenameFlowError::Commit(text) => text.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companion_actions::{add_companion, StdCompanionFs};
    use crate::unique_test_dir;
    use std::cell::{Cell, RefCell};
    use std::io::ErrorKind::PermissionDenied;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    /// Real files in a temp dir; the n-th call of one operation fails, and a case-insensitive
    /// volume can be simulated (a missing name that matches a sibling in lower case "exists").
    #[derive(Default)]
    struct FakeFs {
        faults: Vec<(&'static str, usize, io::ErrorKind)>,
        seen: RefCell<Vec<(&'static str, usize)>>,
        calls: RefCell<Vec<String>>,
        insensitive: bool,
        renames: Cell<usize>,
    }

    impl FakeFs {
        fn fail(mut self, op: &'static str, nth: usize) -> Self {
            self.faults.push((op, nth, PermissionDenied));
            self
        }
        fn insensitive(mut self) -> Self {
            self.insensitive = true;
            self
        }
        fn enter(&self, op: &'static str, path: &Path) -> io::Result<()> {
            self.calls.borrow_mut().push(format!("{op} {}", path.display()));
            let mut seen = self.seen.borrow_mut();
            let n = match seen.iter_mut().find(|s| s.0 == op) {
                Some(s) => {
                    s.1 += 1;
                    s.1
                }
                None => {
                    seen.push((op, 1));
                    1
                }
            };
            match self.faults.iter().find(|f| f.0 == op && f.1 == n) {
                Some(f) => Err(io::Error::new(f.2, format!("injected {op} #{n}"))),
                None => Ok(()),
            }
        }
        fn twin(&self, path: &Path) -> Option<PathBuf> {
            let lower = path.file_name()?.to_str()?.to_lowercase();
            std::fs::read_dir(path.parent()?)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| self.insensitive && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_lowercase() == lower))
        }
    }

    impl FileRenameFs for FakeFs {
        fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
            self.enter("symlink_metadata", path)?;
            match std::fs::symlink_metadata(path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => match self.twin(path) {
                    Some(twin) => std::fs::symlink_metadata(twin),
                    None => Err(e),
                },
                other => other,
            }
        }
        fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<OsString>> {
            self.enter("read_dir_names", dir)?;
            StdFileRenameFs.read_dir_names(dir)
        }
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            self.enter("canonicalize", path)?;
            StdFileRenameFs.canonicalize(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.renames.set(self.renames.get() + 1);
            self.enter("rename", from)?;
            StdFileRenameFs.rename(from, to)
        }
        fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.enter("move_file", from)?;
            StdFileRenameFs.move_file(from, to)
        }
    }

    struct Fx {
        base: PathBuf,
        root: PathBuf,
        conn: Connection,
        folder: i64,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    impl Fx {
        fn new(tag: &str) -> Self {
            let base = unique_test_dir(&format!("p4_core_{tag}"));
            let base = base.canonicalize().unwrap_or(base);
            let root = base.join("catalog");
            std::fs::create_dir_all(&root).unwrap();
            let conn = db::connect_in_memory().unwrap();
            let folder = db::insert_folder_with_parent(&conn, "catalog", None, &root.to_string_lossy()).unwrap();
            Self { base, root, conn, folder }
        }
        fn model(&self, name: &str) -> (i64, PathBuf) {
            let path = self.root.join(name);
            std::fs::write(&path, b"model").unwrap();
            (db::test_insert_minimal_file(&self.conn, &path.to_string_lossy(), Some(self.folder)).unwrap(), path)
        }
        fn companion(&self, model: i64, name: &str) {
            let source = self.base.join(name);
            std::fs::write(&source, PNG).unwrap();
            add_companion(&self.conn, &StdCompanionFs, model, &source, &[]).unwrap();
        }
        fn rename(&self, fs: &dyn FileRenameFs, id: i64, name: &str, case_insensitive: bool) -> ImportResult<()> {
            rename_file(&self.conn, fs, &StdCompanionFs, id, name, &[], case_insensitive)
        }
        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.root).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }
        fn row(&self, id: i64) -> (String, String) {
            let f = db::get_file(&self.conn, id).unwrap().unwrap();
            (f.name, f.path)
        }
    }

    #[test]
    fn renames_the_file_and_the_row_and_carries_the_companions() {
        let fx = Fx::new("plain");
        let (id, old) = fx.model("Foo.3mf");
        fx.companion(id, "a.png");
        fx.rename(&StdFileRenameFs, id, "Bar.3mf", false).unwrap();
        assert_eq!(fx.names(), ["Bar.3mf", "Bar_extras"]);
        assert_eq!(fx.row(id), ("Bar.3mf".to_string(), fx.root.join("Bar.3mf").to_string_lossy().into_owned()));
        let rows = db::companions::list_active_companions(&fx.conn, id).unwrap();
        assert_eq!(rows[0].path, fx.root.join("Bar_extras/a.png").to_string_lossy());
        assert!(!old.exists());
    }

    #[test]
    fn refuses_what_the_checks_refuse_and_changes_nothing() {
        let fx = Fx::new("checks");
        let (id, _) = fx.model("a.3mf");
        fx.model("taken.3mf");
        std::fs::create_dir(fx.root.join("adir.3mf")).unwrap();
        let code = |e: ImportError| (e.message, e.params);
        assert_eq!(code(fx.rename(&StdFileRenameFs, id, "a/b.3mf", false).unwrap_err()), ("fileNameForbiddenChar".to_string(), vec!["/".to_string()]));
        assert_eq!(fx.rename(&StdFileRenameFs, 999, "x.3mf", false).unwrap_err().message, "file not found");
        assert_eq!(fx.rename(&StdFileRenameFs, id, "a.3mf", false), Ok(()));
        for name in ["taken.3mf", "adir.3mf"] {
            let e = fx.rename(&StdFileRenameFs, id, name, false).unwrap_err();
            assert_eq!((e.message.as_str(), e.expected), ("fileTargetExists", true));
            assert_eq!(e.params, [fx.root.join(name).display().to_string()]);
        }
        let trashed = db::soft_delete_file(&fx.conn, id, Some("x"), "2026-01-01T00:00:00Z");
        trashed.unwrap();
        assert_eq!(fx.rename(&StdFileRenameFs, id, "c.3mf", false).unwrap_err().message, "fileInTrash");
        assert_eq!(fx.names(), ["a.3mf", "adir.3mf", "taken.3mf"]);
    }

    #[test]
    fn a_target_that_appears_after_the_check_is_reported_as_taken() {
        let fx = Fx::new("race");
        let (id, old) = fx.model("a.3mf");
        // The hard link of the move finds the name taken (the check ran before).
        struct Racing(PathBuf);
        impl FileRenameFs for Racing {
            fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
                StdFileRenameFs.symlink_metadata(path)
            }
            fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<OsString>> {
                StdFileRenameFs.read_dir_names(dir)
            }
            fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
                StdFileRenameFs.canonicalize(path)
            }
            fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
                StdFileRenameFs.rename(from, to)
            }
            fn move_file(&self, _from: &Path, to: &Path) -> io::Result<()> {
                std::fs::write(&self.0, b"someone else").unwrap();
                Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{}", to.display())))
            }
        }
        let e = fx.rename(&Racing(fx.root.join("b.3mf")), id, "b.3mf", false).unwrap_err();
        assert_eq!((e.message.as_str(), e.expected), ("fileTargetExists", true));
        assert!(old.exists());
        assert_eq!(fx.row(id).0, "a.3mf");
    }

    #[test]
    fn a_blocked_companion_folder_puts_the_file_back() {
        let fx = Fx::new("blocked");
        let (id, old) = fx.model("Foo.3mf");
        fx.companion(id, "a.png");
        std::fs::write(fx.root.join("Bar_extras"), b"a file").unwrap();
        let e = fx.rename(&StdFileRenameFs, id, "Bar.3mf", false).unwrap_err();
        assert_eq!(e.message, "companionTargetFolderBlocked");
        assert!(old.exists() && !fx.root.join("Bar.3mf").exists());
        assert_eq!(fx.row(id).0, "Foo.3mf");
    }

    #[test]
    fn a_failing_commit_puts_files_and_companions_back_and_reports_a_failed_way_back() {
        let fx = Fx::new("commit");
        let (id, old) = fx.model("Foo.3mf");
        fx.companion(id, "a.png");
        let new = fx.root.join("Bar.3mf");
        let calls = Cell::new(0);
        let mut commit = || {
            calls.set(calls.get() + 1);
            Err("simulated".to_string())
        };
        let r = rename_reversible(&fx.conn, &StdFileRenameFs, &StdCompanionFs, id, &old, &new, false, &mut commit);
        assert!(matches!(r, Err(RenameFlowError::Commit(ref t)) if t == "simulated"), "{r:?}");
        assert_eq!(calls.get(), 1);
        assert_eq!(fx.names(), ["Foo.3mf", "Foo_extras"]);
        let rows = db::companions::list_active_companions(&fx.conn, id).unwrap();
        assert_eq!(rows[0].path, fx.root.join("Foo_extras/a.png").to_string_lossy());
        // The way back of the file is locked too: the text says where the file is now.
        let fs = FakeFs::default().fail("move_file", 2);
        let mut commit = || Err("simulated".to_string());
        let r = rename_reversible(&fx.conn, &fs, &StdCompanionFs, id, &old, &new, false, &mut commit);
        let Err(RenameFlowError::Combined(text)) = r else { panic!("{r:?}") };
        assert!(text.starts_with("DB-Update fehlgeschlagen (simulated) UND Rollback der Datei-Umbenennung fehlgeschlagen (injected move_file #2)"), "{text}");
        assert!(text.ends_with(&format!("Datei heisst jetzt {}, DB verweist weiter auf {}", new.display(), old.display())), "{text}");
        assert!(new.exists());
    }

    #[test]
    fn a_case_only_rename_goes_through_an_intermediate_name() {
        let fx = Fx::new("case");
        let (id, _) = fx.model("foo.3mf");
        fx.companion(id, "a.png");
        let fs = FakeFs::default().insensitive();
        fx.rename(&fs, id, "Foo.3mf", true).unwrap();
        assert_eq!(fs.renames.get(), 2, "intermediate and target (the companion folder uses the companion fs)");
        assert_eq!(fx.names(), ["Foo.3mf", "Foo_extras"]);
        assert_eq!(fx.row(id).0, "Foo.3mf");
    }

    #[test]
    fn a_locked_second_step_of_a_case_only_rename_restores_the_original() {
        let fx = Fx::new("second");
        let (id, _) = fx.model("two.3mf");
        let fs = FakeFs::default().insensitive().fail("rename", 2);
        let e = fx.rename(&fs, id, "Two.3mf", true).unwrap_err();
        assert!(e.message.starts_with("injected rename #2"), "{}", e.message);
        assert_eq!(fx.names(), ["two.3mf"]);
        assert_eq!(fx.row(id).0, "two.3mf");
        assert_eq!(fs.renames.get(), 3, "intermediate, failed second step, way back");
    }

    #[test]
    fn a_failed_way_back_leaves_the_intermediate_file_and_names_it() {
        let fx = Fx::new("lost");
        let (id, _) = fx.model("three.3mf");
        let fs = FakeFs::default().insensitive().fail("rename", 2).fail("rename", 3);
        let e = fx.rename(&fs, id, "Three.3mf", true).unwrap_err();
        assert!(e.message.starts_with("Umbenennung fehlgeschlagen (injected rename #2); Rollback fehlgeschlagen (injected rename #3); Daten liegen unter "), "{}", e.message);
        let names = fx.names();
        assert_eq!(names.len(), 1);
        assert!(names[0].contains(".3mf-rename-tmp-") && names[0].ends_with(".3mf"), "{names:?}");
        assert!(e.message.ends_with(&fx.root.join(&names[0]).display().to_string()), "{}", e.message);
    }

    #[test]
    fn a_distinct_entry_with_the_other_case_is_a_collision_and_two_spellings_are_not() {
        let fx = Fx::new("distinct");
        let (a, a_path) = fx.model("Other.3mf");
        fx.model("other-name.3mf");
        let fs = FakeFs::default().insensitive();
        assert!(target_collides(&fs, &a_path, &fx.root.join("other-name.3mf")).unwrap());
        // `A.3mf` vs `a.3mf` is the same entry on the simulated volume.
        assert!(!target_collides(&fs, &a_path, &fx.root.join("other.3mf")).unwrap());
        assert!(is_case_only(&a_path, &fx.root.join("other.3mf")));
        assert!(!is_case_only(&a_path, &fx.root.join("other-name.3mf")));
        assert!(fx.rename(&fs, a, "other-name.3mf", true).is_err());
    }

    #[test]
    fn an_unreadable_directory_fails_the_collision_check() {
        let fx = Fx::new("readdir");
        let (id, _) = fx.model("list.3mf");
        let fs = FakeFs::default().insensitive().fail("read_dir_names", 1);
        let e = fx.rename(&fs, id, "List.3mf", true).unwrap_err();
        assert!(e.message.starts_with("injected read_dir_names #1"), "{}", e.message);
        assert_eq!(fx.names(), ["list.3mf"]);
    }
}
