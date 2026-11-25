use super::*;
use crate::unique_test_dir;
use std::cell::{Cell, RefCell};
use std::io::ErrorKind::PermissionDenied;

/// Real files in a temp dir; the n-th call of one step fails with the given kind, every call
/// is logged, and `is_dir` can be forced to `false`.
struct FakeFs {
    fail: Option<(&'static str, usize, io::ErrorKind)>,
    seen: Cell<usize>,
    calls: RefCell<Vec<&'static str>>,
    no_dirs: bool,
}

impl FakeFs {
    fn new(fail: Option<(&'static str, usize, io::ErrorKind)>) -> Self {
        Self { fail, seen: Cell::new(0), calls: RefCell::new(Vec::new()), no_dirs: false }
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
    fn count(&self, op: &str) -> usize {
        self.calls.borrow().iter().filter(|c| **c == op).count()
    }
}

impl FileRenameFs for FakeFs {
    fn symlink_metadata(&self, path: &Path) -> io::Result<std::fs::Metadata> {
        self.step("symlink_metadata")?;
        std::fs::symlink_metadata(path)
    }
    fn read_dir_names(&self, dir: &Path) -> io::Result<Vec<std::ffi::OsString>> {
        StdFolderMoveFs.read_dir_names(dir)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.step("canonicalize")?;
        std::fs::canonicalize(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.step("rename")?;
        std::fs::rename(from, to)
    }
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        StdFolderMoveFs.move_file(from, to)
    }
}

impl FolderMoveFs for FakeFs {
    fn is_dir(&self, path: &Path) -> bool {
        self.calls.borrow_mut().push("is_dir");
        !self.no_dirs && path.is_dir()
    }
    fn exists(&self, path: &Path) -> bool {
        self.calls.borrow_mut().push("exists");
        path.exists()
    }
}

trait Coded {
    fn is_coded(&self, code: ErrorCode) -> bool;
}

impl Coded for CmdError {
    fn is_coded(&self, code: ErrorCode) -> bool {
        self.expected && self.message == code.as_str()
    }
}

struct World {
    conn: Connection,
    dir: PathBuf,
    alt: i64,
    sub: i64,
    ziel: i64,
    file: i64,
}

/// `Alt` (with `sub`, a model and a trashed companion) and `Ziel` below a temp directory.
fn world(name: &str) -> World {
    let dir = unique_test_dir(name).canonicalize().unwrap();
    let conn = db::connect_in_memory().unwrap();
    std::fs::create_dir_all(dir.join("Alt/sub")).unwrap();
    std::fs::create_dir_all(dir.join("Ziel")).unwrap();
    let alt = db::insert_folder_with_parent(&conn, "Alt", None, &dir.join("Alt").to_string_lossy()).unwrap();
    let sub = db::insert_folder_with_parent(&conn, "sub", Some(alt), &dir.join("Alt/sub").to_string_lossy()).unwrap();
    let ziel = db::insert_folder_with_parent(&conn, "Ziel", None, &dir.join("Ziel").to_string_lossy()).unwrap();
    let model = dir.join("Alt/sub/a.3mf");
    std::fs::write(&model, b"model").unwrap();
    let file = db::test_insert_minimal_file(&conn, &model.to_string_lossy(), Some(sub)).unwrap();
    conn.execute(
        "INSERT INTO companion_files (file_id, name, path, kind, added_at, deleted_at, trash_path) VALUES (?1, 'g.png', ?2, 'image', 't', 't2', '/trash/companion-1-g.png')",
        rusqlite::params![file, dir.join("Alt/sub/a_extras/g.png").to_string_lossy()],
    )
    .unwrap();
    World { conn, dir, alt, sub, ziel, file }
}

impl World {
    fn path_of(&self, id: i64) -> String {
        db::list_folders(&self.conn).unwrap().into_iter().find(|f| f.id == id).unwrap().path
    }
    fn file_path(&self) -> String {
        db::get_file(&self.conn, self.file).unwrap().unwrap().path
    }
    fn companion(&self) -> (String, String) {
        self.conn
            .query_row("SELECT path, trash_path FROM companion_files", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
    }
    fn block_updates(&self) {
        self.conn
            .execute_batch("CREATE TRIGGER block BEFORE UPDATE OF path ON companion_files BEGIN SELECT RAISE(ABORT, 'blocked'); END;")
            .unwrap();
    }
    fn finish(self) {
        let _ = std::fs::remove_dir_all(self.dir);
    }
}

#[test]
fn rename_carries_every_path_below_it_but_not_the_trash_path() {
    let w = world("p6_core_rename");
    rename_folder(&w.conn, &StdFolderMoveFs, w.alt, "Neu".into(), &[]).unwrap();
    assert_eq!(w.path_of(w.alt), w.dir.join("Neu").to_string_lossy());
    assert_eq!(w.path_of(w.sub), w.dir.join("Neu/sub").to_string_lossy());
    assert_eq!(w.file_path(), w.dir.join("Neu/sub/a.3mf").to_string_lossy());
    // The trashed companion's restore path follows, the place it lies in does not.
    assert_eq!(w.companion(), (w.dir.join("Neu/sub/a_extras/g.png").to_string_lossy().into_owned(), "/trash/companion-1-g.png".into()));
    assert!(w.dir.join("Neu/sub/a.3mf").is_file());
    w.finish();
}

#[test]
fn rename_to_the_same_name_touches_nothing() {
    let w = world("p6_core_rename_same");
    let fs = FakeFs::new(None);
    rename_folder(&w.conn, &fs, w.alt, "Alt".into(), &[]).unwrap();
    assert_eq!(fs.count("rename"), 0);
    w.finish();
}

#[test]
fn rename_refuses_a_taken_name_before_any_rename() {
    let w = world("p6_core_rename_taken");
    let fs = FakeFs::new(None);
    let err = rename_folder(&w.conn, &fs, w.alt, "Ziel".into(), &[]).unwrap_err();
    assert!(err.expected && err.message == "renameTargetFolderExists", "{err:?}");
    assert_eq!(err.params, vec![w.dir.join("Ziel").display().to_string()]);
    assert_eq!(fs.count("rename"), 0);
    w.finish();
}

#[test]
fn rename_failure_on_disk_leaves_the_catalog_alone() {
    let w = world("p6_core_rename_disk");
    let fs = FakeFs::new(Some(("rename", 1, PermissionDenied)));
    let err = rename_folder(&w.conn, &fs, w.alt, "Neu".into(), &[]).unwrap_err();
    assert_eq!(err.message, io::Error::from(PermissionDenied).to_string());
    assert!(!err.expected);
    assert_eq!(w.path_of(w.alt), w.dir.join("Alt").to_string_lossy());
    w.finish();
}

#[test]
fn rename_puts_the_folder_back_when_the_catalog_update_fails() {
    let w = world("p6_core_rename_db");
    w.block_updates();
    let err = rename_folder(&w.conn, &StdFolderMoveFs, w.alt, "Neu".into(), &[]).unwrap_err();
    assert_eq!(err.message, "database error: blocked");
    assert!(w.dir.join("Alt/sub/a.3mf").is_file());
    assert_eq!(w.path_of(w.alt), w.dir.join("Alt").to_string_lossy());
    assert_eq!(w.file_path(), w.dir.join("Alt/sub/a.3mf").to_string_lossy());
    w.finish();
}

#[test]
fn rename_names_both_errors_when_the_roll_back_fails_too() {
    let w = world("p6_core_rename_both");
    w.block_updates();
    let fs = FakeFs::new(Some(("rename", 2, PermissionDenied)));
    let err = rename_folder(&w.conn, &fs, w.alt, "Neu".into(), &[]).unwrap_err();
    let new = w.dir.join("Neu");
    assert_eq!(
        err.message,
        format!(
            "DB-Update fehlgeschlagen (database error: blocked) UND Rollback der Ordner-Umbenennung fehlgeschlagen ({}) - Ordner heisst jetzt {}, DB verweist teilweise noch auf {}",
            io::Error::from(PermissionDenied),
            new.display(),
            w.dir.join("Alt").display()
        )
    );
    assert!(!err.expected);
    assert!(new.join("sub/a.3mf").is_file());
    w.finish();
}

#[test]
fn move_changes_parent_and_paths() {
    let w = world("p6_core_move");
    move_folder(&w.conn, &StdFolderMoveFs, w.alt, Some(w.ziel), &[]).unwrap();
    assert_eq!(w.path_of(w.alt), w.dir.join("Ziel/Alt").to_string_lossy());
    assert_eq!(w.file_path(), w.dir.join("Ziel/Alt/sub/a.3mf").to_string_lossy());
    let parent = db::list_folders(&w.conn).unwrap().into_iter().find(|f| f.id == w.alt).unwrap().parent_id;
    assert_eq!(parent, Some(w.ziel));
    assert!(w.dir.join("Ziel/Alt/sub/a.3mf").is_file());
    w.finish();
}

#[test]
fn move_refuses_cycles_by_id_and_a_missing_target() {
    let w = world("p6_core_move_cycles");
    let fs = FakeFs::new(None);
    let refusal = |target| move_folder(&w.conn, &fs, w.alt, target, &[]).unwrap_err();
    assert!(refusal(Some(w.alt)).is_coded(ErrorCode::FolderMoveIntoSelf));
    assert!(refusal(Some(w.sub)).is_coded(ErrorCode::FolderMoveIntoDescendant));
    assert_eq!(refusal(None).message, "Ein Ordner kann nicht ohne Zielordner verschoben werden");
    assert_eq!(refusal(Some(999)).message, "target folder not found");
    assert_eq!(fs.count("rename"), 0);
    w.finish();
}

#[test]
fn move_checks_the_real_locations_and_the_disk_before_renaming() {
    let w = world("p6_core_move_real");
    // `Ziel` lies inside `Alt` on disk although the catalog says they are siblings.
    std::fs::create_dir_all(w.dir.join("Alt/inner")).unwrap();
    let inner = db::insert_folder_with_parent(&w.conn, "inner", None, &w.dir.join("Alt/inner").to_string_lossy()).unwrap();
    let fs = FakeFs::new(None);
    let err = move_folder(&w.conn, &fs, w.alt, Some(inner), &[]).unwrap_err();
    assert!(err.is_coded(ErrorCode::FolderMoveIntoSelfOrDescendant), "{err:?}");
    let mut missing = FakeFs::new(None);
    missing.no_dirs = true;
    let err = move_folder(&w.conn, &missing, w.alt, Some(w.ziel), &[]).unwrap_err();
    assert!(err.is_coded(ErrorCode::FolderMissingOnDisk), "{err:?}");
    assert_eq!(err.params, vec![w.dir.join("Alt").display().to_string()]);
    assert_eq!(fs.count("rename") + missing.count("rename"), 0);
    w.finish();
}

#[test]
fn move_skips_the_real_location_check_when_a_path_cannot_be_resolved() {
    let w = world("p6_core_move_resolve");
    let fs = FakeFs::new(Some(("canonicalize", 1, PermissionDenied)));
    move_folder(&w.conn, &fs, w.alt, Some(w.ziel), &[]).unwrap();
    assert_eq!(fs.count("rename"), 1);
    w.finish();
}

#[test]
fn move_puts_the_folder_back_and_names_both_errors_when_that_fails() {
    let w = world("p6_core_move_db");
    w.block_updates();
    let err = move_folder(&w.conn, &StdFolderMoveFs, w.alt, Some(w.ziel), &[]).unwrap_err();
    assert_eq!(err.message, "database error: blocked");
    assert!(w.dir.join("Alt/sub/a.3mf").is_file());
    assert_eq!(w.path_of(w.alt), w.dir.join("Alt").to_string_lossy());
    let fs = FakeFs::new(Some(("rename", 2, PermissionDenied)));
    let err = move_folder(&w.conn, &fs, w.alt, Some(w.ziel), &[]).unwrap_err();
    assert!(
        err.message.starts_with("DB-Update fehlgeschlagen (database error: blocked) UND Rollback des Ordner-Moves fehlgeschlagen ("),
        "{}",
        err.message
    );
    assert!(w.dir.join("Ziel/Alt/sub/a.3mf").is_file());
    w.finish();
}

#[test]
fn managed_location_rejects_protected_paths_and_their_ancestors() {
    let w = world("p6_core_managed");
    let secret = w.dir.join("Ziel");
    let expanded = expand_sensitive_dirs(std::slice::from_ref(&secret));
    assert!(check_managed_location(&secret.join("x"), &expanded).is_err());
    assert!(check_managed_location(&w.dir, &expanded).unwrap_err().message.contains("enthaelt ein geschuetztes Verzeichnis"));
    assert!(check_managed_location(&w.dir.join("Alt"), &expanded).is_ok());
    w.finish();
}

#[test]
fn is_descendant_walks_the_parent_chain() {
    let w = world("p6_core_descendant");
    let folders = db::list_folders(&w.conn).unwrap();
    assert!(is_descendant(&folders, w.sub, w.alt));
    assert!(!is_descendant(&folders, w.alt, w.sub));
    assert!(!is_descendant(&folders, w.ziel, w.alt));
    w.finish();
}
