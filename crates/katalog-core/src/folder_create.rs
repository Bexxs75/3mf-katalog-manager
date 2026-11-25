//! "Create folder" of the desktop catalog (issue #20, package P2): name check, parent lookup,
//! the folder on disk and the `folders` row. The desktop command keeps the dialog and the
//! lock; this module has no `fsops`/`upload_fs` dependency, so it builds on Windows and macOS.
//!
//! All disk access goes through [`CreateFolderFs`], which the desktop implements on top of
//! its file-system boundary (`Fs`, P0a). Tests use a fake that fails in step n.
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::{self, models::FolderRecord};
use crate::error_code::ErrorCode;
use crate::import::file_names::{validate_name, NameKind};
use crate::import::placement::{comparison_path, reject_if_sensitive_path};
use crate::import::{ImportError as CmdError, ImportResult as CmdResult};
use crate::read::FolderDto;

/// The three disk operations "create folder" uses, in the order it uses them.
pub trait CreateFolderFs {
    /// Resolves a path (used for the catalog parent of a picked folder).
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    /// Same as `Path::exists`.
    fn exists(&self, path: &Path) -> bool;
    /// Same as `std::fs::create_dir` (not `create_dir_all`).
    fn create_dir(&self, path: &Path) -> io::Result<()>;
}

/// `std::fs`, 1:1.
pub struct StdCreateFolderFs;

impl CreateFolderFs for StdCreateFolderFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        path.canonicalize()
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }
}

/// `name` ends up in `join`/`with_file_name` and must be a single harmless path
/// component: "../../etc/x" or "/etc/x" would otherwise create outside the
/// catalog, just by typing text (CWE-22).
pub fn validate_folder_name(name: &str) -> CmdResult<()> {
    validate_name(name, true, NameKind::Folder)
}

/// Target path of a new folder below the catalog folder `parent_id`.
pub fn new_dir_below_parent(conn: &Connection, parent_id: i64, name: &str) -> CmdResult<PathBuf> {
    let folders = db::list_folders(conn).map_err(|e| CmdError::from(e.to_string()))?;
    let parent_folder = folders
        .iter()
        .find(|f| f.id == parent_id)
        .ok_or_else(|| CmdError::from("folder not found"))?;
    Ok(PathBuf::from(&parent_folder.path).join(name))
}

// Pure component comparison; callers resolve existing filesystem paths first.
pub fn find_catalog_parent(path: &Path, folders: &[FolderRecord]) -> Option<i64> {
    let path = comparison_path(path);
    folders
        .iter()
        .filter_map(|folder| {
            let parent = comparison_path(Path::new(&folder.path));
            (!parent.as_os_str().is_empty() && path.starts_with(&parent))
                .then_some((parent.components().count(), folder.id))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, id)| id)
}

/// Creation after path selection, without UI state so disk/DB outcomes are testable.
/// Fails if the target already exists. The name must have passed [`validate_folder_name`].
pub fn create_folder(
    conn: &Connection,
    fs: &dyn CreateFolderFs,
    parent: Option<i64>,
    name: String,
    new_dir: &Path,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<FolderDto> {
    reject_if_sensitive_path(new_dir, sensitive_dirs)?;
    let parent = match parent {
        Some(id) => Some(id),
        None => {
            let mut folders = db::list_folders(conn).map_err(|e| CmdError::from(e.to_string()))?;
            // Canonical spelling respects the actual filesystem's case rules,
            // including case-insensitive macOS volumes and symlinked selections.
            for folder in &mut folders {
                if let Ok(path) = fs.canonicalize(Path::new(&folder.path)) {
                    folder.path = path.to_string_lossy().into_owned();
                }
            }
            let base = new_dir.parent().ok_or_else(|| CmdError::from("folder has no parent path"))?;
            let base = fs.canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
            find_catalog_parent(&base, &folders)
        }
    };
    if fs.exists(new_dir) {
        return Err(map_create_folder_error(io::ErrorKind::AlreadyExists.into(), new_dir));
    }
    fs.create_dir(new_dir).map_err(|e| map_create_folder_error(e, new_dir))?;

    let new_id = db::insert_folder_with_parent(conn, &name, parent, &new_dir.to_string_lossy())
        .map_err(|e| CmdError::from(e.to_string()))?;
    Ok(FolderDto {
        id: new_id.to_string(),
        name,
        path: new_dir.to_string_lossy().to_string(),
        parent_id: parent.map(|p| p.to_string()),
        count: 0,
        missing: false,
        move_target: true,
    })
}

/// An existing target is the coded `folderExists`; every other disk error stays plain text.
pub fn map_create_folder_error(error: io::Error, path: &Path) -> CmdError {
    if error.kind() == io::ErrorKind::AlreadyExists {
        CmdError::coded_with(ErrorCode::FolderExists, [path.display()])
    } else {
        error.to_string().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{native_test_path, unique_test_dir};
    use std::cell::{Cell, RefCell};

    /// Real files in a temp dir; the n-th call of one step fails with the given kind.
    struct FaultCreateFs {
        fail: Option<(&'static str, usize, io::ErrorKind)>,
        seen: Cell<usize>,
        calls: RefCell<Vec<&'static str>>,
        /// Pretends the target is already there (a case-insensitive volume answers so
        /// for a sibling that differs only in case).
        exists_override: bool,
    }

    impl FaultCreateFs {
        fn new(fail: Option<(&'static str, usize, io::ErrorKind)>) -> Self {
            Self { fail, seen: Cell::new(0), calls: RefCell::new(Vec::new()), exists_override: false }
        }
        fn step(&self, op: &'static str) -> io::Result<()> {
            self.calls.borrow_mut().push(op);
            if let Some((fail_op, nth, kind)) = self.fail {
                if fail_op == op {
                    self.seen.set(self.seen.get() + 1);
                    if self.seen.get() == nth {
                        return Err(kind.into());
                    }
                }
            }
            Ok(())
        }
    }

    impl CreateFolderFs for FaultCreateFs {
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            self.step("canonicalize")?;
            path.canonicalize()
        }
        fn exists(&self, path: &Path) -> bool {
            self.calls.borrow_mut().push("exists");
            self.exists_override || path.exists()
        }
        fn create_dir(&self, path: &Path) -> io::Result<()> {
            self.step("create_dir")?;
            std::fs::create_dir(path)
        }
    }

    fn rows(conn: &Connection) -> Vec<(i64, String, Option<i64>, String)> {
        db::list_folders(conn).unwrap().into_iter().map(|f| (f.id, f.name, f.parent_id, f.path)).collect()
    }

    #[test]
    fn mkdir_failure_leaves_no_row_and_maps_like_the_desktop() {
        for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::NotFound, io::ErrorKind::Other] {
            let conn = db::connect_in_memory().unwrap();
            let base = unique_test_dir("p2_core_mkdir_fail");
            let dir = base.join("child");
            let fs = FaultCreateFs::new(Some(("create_dir", 1, kind)));
            let err = create_folder(&conn, &fs, None, "child".into(), &dir, &[]).unwrap_err();
            assert!(!err.expected, "{kind:?}: {err:?}");
            assert_eq!(err.message, io::Error::from(kind).to_string());
            assert!(!dir.exists());
            assert!(rows(&conn).is_empty());
            std::fs::remove_dir_all(base).unwrap();
        }
    }

    #[test]
    fn mkdir_already_exists_is_the_coded_folder_exists_without_a_row() {
        let conn = db::connect_in_memory().unwrap();
        let base = unique_test_dir("p2_core_mkdir_exists");
        let dir = base.join("child");
        // The check before mkdir misses it (race): mkdir itself reports AlreadyExists.
        let fs = FaultCreateFs::new(Some(("create_dir", 1, io::ErrorKind::AlreadyExists)));
        let err = create_folder(&conn, &fs, None, "child".into(), &dir, &[]).unwrap_err();
        assert_eq!((err.expected, err.message.as_str(), err.params.clone()), (true, "folderExists", vec![dir.display().to_string()]));
        assert!(rows(&conn).is_empty());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn case_collision_on_a_case_insensitive_volume_is_folder_exists_and_skips_mkdir() {
        let conn = db::connect_in_memory().unwrap();
        let base = unique_test_dir("p2_core_case");
        std::fs::create_dir(base.join("Neu")).unwrap();
        let dir = base.join("neu");
        let mut fs = FaultCreateFs::new(None);
        fs.exists_override = true;
        let err = create_folder(&conn, &fs, None, "neu".into(), &dir, &[]).unwrap_err();
        assert_eq!((err.expected, err.message.as_str(), err.params.clone()), (true, "folderExists", vec![dir.display().to_string()]));
        assert!(!fs.calls.borrow().contains(&"create_dir"));
        assert!(rows(&conn).is_empty());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn canonicalize_failure_falls_back_to_the_given_spelling() {
        let conn = db::connect_in_memory().unwrap();
        let base = unique_test_dir("p2_core_canon");
        let root = db::insert_folder_with_parent(&conn, "base", None, &base.to_string_lossy()).unwrap();
        let dir = base.join("child");
        // Every canonicalize call fails (the first is the catalog folder, the second the base).
        struct NoCanon(FaultCreateFs);
        impl CreateFolderFs for NoCanon {
            fn canonicalize(&self, _: &Path) -> io::Result<PathBuf> {
                Err(io::ErrorKind::PermissionDenied.into())
            }
            fn exists(&self, p: &Path) -> bool {
                self.0.exists(p)
            }
            fn create_dir(&self, p: &Path) -> io::Result<()> {
                self.0.create_dir(p)
            }
        }
        let dto = create_folder(&conn, &NoCanon(FaultCreateFs::new(None)), None, "child".into(), &dir, &[]).unwrap();
        assert_eq!(dto.parent_id, Some(root.to_string()));
        assert!(dir.is_dir());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn missing_parent_on_disk_is_the_plain_io_error() {
        let conn = db::connect_in_memory().unwrap();
        let base = unique_test_dir("p2_core_noparent");
        let dir = base.join("gone").join("child");
        let err = create_folder(&conn, &StdCreateFolderFs, None, "child".into(), &dir, &[]).unwrap_err();
        assert!(!err.expected, "{err:?}");
        assert_eq!(err.message, std::fs::create_dir(&dir).unwrap_err().to_string());
        assert!(rows(&conn).is_empty());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn success_creates_dir_and_row_below_the_catalog_parent() {
        let conn = db::connect_in_memory().unwrap();
        let base = unique_test_dir("p2_core_ok");
        let parent = db::insert_folder_with_parent(&conn, "base", None, &base.to_string_lossy()).unwrap();
        let dir = base.join("Neu");
        let fs = FaultCreateFs::new(None);
        let dto = create_folder(&conn, &fs, Some(parent), "Neu".into(), &dir, &[]).unwrap();
        assert!(dir.is_dir());
        assert_eq!((dto.name.as_str(), dto.count, dto.missing, dto.move_target), ("Neu", 0, false, true));
        assert_eq!(dto.parent_id, Some(parent.to_string()));
        assert_eq!(*fs.calls.borrow(), vec!["exists", "create_dir"]);
        assert_eq!(rows(&conn).len(), 2);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn parent_lookup_and_longest_catalog_parent() {
        let conn = db::connect_in_memory().unwrap();
        let base = unique_test_dir("p2_core_lookup");
        let parent = db::insert_folder_with_parent(&conn, "base", None, &base.to_string_lossy()).unwrap();
        assert_eq!(new_dir_below_parent(&conn, parent, "n").unwrap(), base.join("n"));
        let err = new_dir_below_parent(&conn, parent + 99, "n").unwrap_err();
        assert_eq!((err.message.as_str(), err.expected), ("folder not found", false));
        std::fs::remove_dir_all(base).unwrap();

        let rec = |id, path: &str| FolderRecord { id, name: String::new(), parent_id: None, path: native_test_path(path) };
        let folders = [rec(1, "/m"), rec(2, "/m/sub"), rec(3, "/m-other")];
        let probe = |p: &str| find_catalog_parent(Path::new(&native_test_path(p)), &folders);
        assert_eq!(probe("/m/sub/x"), Some(2));
        assert_eq!(probe("/m/x"), Some(1));
        assert_eq!(probe("/n/x"), None);
    }

    #[test]
    fn name_rules_are_the_folder_codes() {
        let cases: &[(&str, ErrorCode)] = &[
            ("", ErrorCode::FolderNameEmpty),
            ("a/b", ErrorCode::FolderNameForbiddenChar),
            ("..", ErrorCode::FolderNameNotAllowed),
            ("a.", ErrorCode::FolderNameEndsWithDotOrSpace),
            ("a ", ErrorCode::FolderNameEndsWithDotOrSpace),
            ("CON", ErrorCode::FolderNameWindowsReserved),
            ("lpt9.log", ErrorCode::FolderNameWindowsReserved),
        ];
        for (name, code) in cases {
            assert_eq!(validate_folder_name(name).unwrap_err().message, code.as_str(), "{name:?}");
        }
        assert!(validate_folder_name("Projekt 2026").is_ok());
    }
}
