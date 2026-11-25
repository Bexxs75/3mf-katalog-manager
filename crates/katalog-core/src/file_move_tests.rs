//! Tests of the move flow on a fake file system that fails in step n (issue #20, P5).
use super::*;
use crate::unique_test_dir;
use std::cell::{Cell, RefCell};
use std::io::ErrorKind::{AlreadyExists, PermissionDenied};

/// Real files in a temp dir; the n-th call of one step fails with the given kind.
struct FaultMoveFs {
    fail: Vec<(&'static str, usize, io::ErrorKind)>,
    seen: RefCell<Vec<(&'static str, usize)>>,
    calls: RefCell<Vec<String>>,
    lies_about_dir: Cell<bool>,
}

impl FaultMoveFs {
    fn new(fail: &[(&'static str, usize, io::ErrorKind)]) -> Self {
        Self { fail: fail.to_vec(), seen: RefCell::new(Vec::new()), calls: RefCell::new(Vec::new()), lies_about_dir: Cell::new(false) }
    }
    fn step(&self, op: &'static str, path: &Path) -> io::Result<()> {
        self.calls.borrow_mut().push(format!("{op} {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
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
        match self.fail.iter().find(|f| f.0 == op && f.1 == n) {
            Some(f) => Err(f.2.into()),
            None => Ok(()),
        }
    }
}

impl CompanionFs for FaultMoveFs {
    fn symlink_metadata(&self, path: &Path) -> io::Result<std::fs::Metadata> {
        StdCompanionFs.symlink_metadata(path)
    }
    fn exists(&self, path: &Path) -> bool {
        StdCompanionFs.exists(path)
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        StdCompanionFs.canonicalize(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.step("create_dir", path)?;
        StdCompanionFs.create_dir(path)
    }
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        self.step("remove_dir", path)?;
        StdCompanionFs.remove_dir(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        StdCompanionFs.remove_file(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.step("rename", from)?;
        StdCompanionFs.rename(from, to)
    }
    fn create_new(&self, path: &Path) -> io::Result<std::fs::File> {
        StdCompanionFs.create_new(path)
    }
    fn move_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.step("move_file", from)?;
        StdCompanionFs.move_file(from, to)
    }
}

impl FileMoveFs for FaultMoveFs {
    fn is_dir(&self, path: &Path) -> bool {
        self.step("is_dir", path).is_ok() && !self.lies_about_dir.get() && path.is_dir()
    }
    fn is_file(&self, path: &Path) -> bool {
        self.step("is_file", path).is_ok() && path.is_file()
    }
}

struct World {
    base: PathBuf,
    conn: Connection,
    src: PathBuf,
    dst: PathBuf,
    src_id: i64,
    dst_id: i64,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

impl World {
    fn new(tag: &str) -> Self {
        let base = std::fs::canonicalize(unique_test_dir(&format!("file_move_{tag}"))).unwrap();
        let (src, dst) = (base.join("src"), base.join("dst"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        let conn = db::connect_in_memory().unwrap();
        let src_id = db::insert_folder_with_parent(&conn, "src", None, &src.to_string_lossy()).unwrap();
        let dst_id = db::insert_folder_with_parent(&conn, "dst", None, &dst.to_string_lossy()).unwrap();
        Self { base, conn, src, dst, src_id, dst_id }
    }
    fn model(&self, name: &str) -> (i64, PathBuf) {
        let path = self.src.join(name);
        std::fs::write(&path, b"model").unwrap();
        (db::test_insert_minimal_file(&self.conn, &path.to_string_lossy(), Some(self.src_id)).unwrap(), path)
    }
    /// A companion row with a real file in the model's `_extras` folder.
    fn companion(&self, model: i64, model_name: &str) -> PathBuf {
        let dir = self.src.join(format!("{}_extras", model_name.trim_end_matches(".3mf")));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("pic.png");
        std::fs::write(&file, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
        db::companions::insert_companion(&self.conn, model, "pic.png", &file.to_string_lossy(), "image", None, "2026-01-01T00:00:00Z").unwrap();
        file
    }
    fn path_of(&self, id: i64) -> String {
        db::get_file(&self.conn, id).unwrap().unwrap().path
    }
    fn go(&self, fs: &dyn FileMoveFs, id: i64) -> Result<MoveOutcome, MoveRefusal> {
        move_file(fs, &self.conn, id, Some(self.dst_id), &[], false)
    }
}

#[test]
fn a_move_changes_disk_and_catalog() {
    let w = World::new("ok");
    let (id, old) = w.model("a.3mf");
    let outcome = w.go(&StdFileMoveFs, id).unwrap();
    assert_eq!(outcome, MoveOutcome::Moved { from_folder_id: Some(w.src_id), from_path: old.to_string_lossy().into_owned() });
    assert!(!old.exists() && w.dst.join("a.3mf").exists());
    assert_eq!(w.path_of(id), w.dst.join("a.3mf").to_string_lossy());
    assert_eq!(w.go(&StdFileMoveFs, id).unwrap(), MoveOutcome::AlreadyThere);
}

#[test]
fn refusals_carry_their_stable_code() {
    let w = World::new("codes");
    let (id, old) = w.model("a.3mf");
    assert_eq!(w.go(&StdFileMoveFs, 999).unwrap_err().code, "not_found");
    assert_eq!(move_file(&StdFileMoveFs, &w.conn, id, Some(4242), &[], false).unwrap_err().code, "folder_not_found");
    let r = move_file(&StdFileMoveFs, &w.conn, id, Some(w.dst_id), std::slice::from_ref(&w.dst), false).unwrap_err();
    assert_eq!(r.code, "sensitive_path");
    // The two lookups that tell an out-of-date catalog from a fault.
    let fs = FaultMoveFs::new(&[]);
    fs.lies_about_dir.set(true);
    assert_eq!(w.go(&fs, id).unwrap_err().code, "target_missing");
    let fs = FaultMoveFs::new(&[("is_file", 1, PermissionDenied)]);
    let r = w.go(&fs, id).unwrap_err();
    assert_eq!(r.code, "source_missing");
    assert!(r.error.expected);
    assert_eq!(r.error.params, vec![old.display().to_string()]);
    assert!(old.exists(), "nothing was touched");
}

#[test]
fn a_clash_on_the_link_is_a_name_exists_refusal_and_changes_nothing() {
    let w = World::new("clash");
    let (id, old) = w.model("a.3mf");
    let fs = FaultMoveFs::new(&[("move_file", 1, AlreadyExists)]);
    assert_eq!(w.go(&fs, id).unwrap_err().code, "name_exists");
    let fs = FaultMoveFs::new(&[("move_file", 1, PermissionDenied)]);
    assert_eq!(w.go(&fs, id).unwrap_err().code, "error");
    assert!(old.exists());
    assert_eq!(w.path_of(id), old.to_string_lossy());
}

#[test]
fn a_companion_that_cannot_follow_sends_the_model_back() {
    let w = World::new("comp");
    let (id, old) = w.model("Foo.3mf");
    let pic = w.companion(id, "Foo.3mf");
    // move_file calls: 1 model, 2 companion (fails), 3 the model back.
    let fs = FaultMoveFs::new(&[("move_file", 2, PermissionDenied)]);
    let r = w.go(&fs, id).unwrap_err();
    assert_eq!(r.code, "error");
    assert!(r.origin.is_none(), "the returned error is the origin itself");
    assert!(old.exists() && !w.dst.join("Foo.3mf").exists());
    assert_eq!(w.path_of(id), old.to_string_lossy());
    assert!(pic.exists());
}

#[test]
fn a_companion_fault_and_a_failing_move_back_name_the_new_place() {
    let w = World::new("comp2");
    let (id, old) = w.model("Foo.3mf");
    w.companion(id, "Foo.3mf");
    let fs = FaultMoveFs::new(&[("move_file", 2, PermissionDenied), ("move_file", 3, PermissionDenied)]);
    let r = w.go(&fs, id).unwrap_err();
    assert_eq!(r.code, "error");
    assert!(r.error.message.contains("UND Rollback der Dateiverschiebung fehlgeschlagen"), "{}", r.error.message);
    assert!(r.error.message.contains(&w.dst.join("Foo.3mf").display().to_string()));
    // The documented state: the model stays at the new place, the catalog at the old one.
    assert!(!old.exists() && w.dst.join("Foo.3mf").exists());
    assert_eq!(w.path_of(id), old.to_string_lossy());
}

fn block_update(w: &World, id: i64) {
    w.conn
        .execute_batch(&format!("CREATE TRIGGER block_move BEFORE UPDATE OF path ON files WHEN NEW.id = {id} BEGIN SELECT RAISE(ABORT, 'blocked'); END;"))
        .unwrap();
}

#[test]
fn a_failing_catalog_update_puts_model_and_companions_back() {
    let w = World::new("db");
    let (id, old) = w.model("Foo.3mf");
    let pic = w.companion(id, "Foo.3mf");
    block_update(&w, id);
    let r = w.go(&FaultMoveFs::new(&[]), id).unwrap_err();
    assert_eq!(r.code, "error");
    assert!(old.exists() && pic.exists());
    assert!(!w.dst.join("Foo.3mf").exists() && !w.dst.join("Foo_extras").exists());
    assert_eq!(w.path_of(id), old.to_string_lossy());
}

#[test]
fn a_failing_catalog_update_and_a_failing_move_back_say_where_the_file_is() {
    let w = World::new("db2");
    let (id, old) = w.model("a.3mf");
    block_update(&w, id);
    let fs = FaultMoveFs::new(&[("move_file", 2, PermissionDenied)]);
    let r = w.go(&fs, id).unwrap_err();
    assert!(r.error.message.contains("DB-Update fehlgeschlagen"), "{}", r.error.message);
    assert!(r.error.message.contains("UND Rollback der Dateiverschiebung fehlgeschlagen"));
    assert!(w.dst.join("a.3mf").exists() && !old.exists());
}

#[test]
fn a_path_owned_by_another_row_is_a_name_clash_after_the_move_back() {
    let w = World::new("owned");
    let (id, old) = w.model("a.3mf");
    db::test_insert_minimal_file(&w.conn, &w.dst.join("a.3mf").to_string_lossy(), Some(w.dst_id)).unwrap();
    let r = w.go(&FaultMoveFs::new(&[]), id).unwrap_err();
    assert_eq!(r.code, "name_exists");
    assert!(old.exists() && !w.dst.join("a.3mf").exists());
}

fn batch(w: &World, fs: &dyn FileMoveFs, ids: &[String], folder: i64) -> Result<Vec<MoveFileResult>, CmdError> {
    move_files(fs, || Ok::<_, CmdError>(&w.conn), ids, folder, &[], false)
}

#[test]
fn the_batch_reports_each_file_and_keeps_going() {
    let w = World::new("batch");
    let (ok, _) = w.model("ok.3mf");
    let (dup, _) = w.model("dup.3mf");
    std::fs::write(w.dst.join("dup.3mf"), b"other").unwrap();
    let ids: Vec<String> = [ok.to_string(), dup.to_string(), "9999".into(), "x".into(), ok.to_string()].into();
    let results = batch(&w, &StdFileMoveFs, &ids, w.dst_id).unwrap();
    let summary: Vec<_> = results.iter().map(|r| (r.status, r.reason.as_deref().unwrap_or("-"))).collect();
    assert_eq!(
        summary,
        vec![("moved", "-"), ("skipped", "name_exists"), ("failed", "not_found"), ("failed", "invalid_id"), ("skipped", "already_there")]
    );
    assert_eq!(results[0].from_folder_id.as_deref(), Some(w.src_id.to_string().as_str()));
    assert!(results[1].from_path.is_none());
}

#[test]
fn the_batch_limit_is_5000_and_a_bad_target_fails_the_whole_call() {
    let w = World::new("limit");
    assert_eq!(MAX_BATCH_MOVE_FILES, 5000);
    let at_limit: Vec<String> = vec!["x".into(); MAX_BATCH_MOVE_FILES];
    assert_eq!(batch(&w, &StdFileMoveFs, &at_limit, w.dst_id).unwrap().len(), MAX_BATCH_MOVE_FILES);
    let over: Vec<String> = vec!["x".into(); MAX_BATCH_MOVE_FILES + 1];
    let err = move_files(&StdFileMoveFs, || -> Result<&Connection, CmdError> { unreachable!("refused before the first lock") }, &over, 1, &[], false).unwrap_err();
    assert_eq!(err.message, ErrorCode::MoveTooManyFiles.as_str());
    assert_eq!(err.params, vec!["5001".to_string(), "5000".to_string()]);
    assert_eq!(batch(&w, &StdFileMoveFs, &[], 4242).unwrap_err().message, "folder not found");
    let fs = FaultMoveFs::new(&[("is_dir", 1, PermissionDenied)]);
    assert_eq!(batch(&w, &fs, &[], w.dst_id).unwrap_err().message, ErrorCode::TargetFolderMissingOnDisk.as_str());
}

#[test]
fn the_batch_stops_when_the_lock_fails_and_keeps_what_was_moved() {
    let w = World::new("lock");
    let (a, _) = w.model("a.3mf");
    let (b, _) = w.model("b.3mf");
    let left = Cell::new(2);
    let lock = || {
        left.set(left.get() - 1);
        if left.get() >= 0 { Ok(&w.conn) } else { Err(CmdError::from("locked")) }
    };
    let err = move_files(&StdFileMoveFs, lock, &[a.to_string(), b.to_string()], w.dst_id, &[], false).unwrap_err();
    assert_eq!(err.message, "locked");
    assert!(w.dst.join("a.3mf").exists() && w.src.join("b.3mf").exists());
}

#[test]
fn the_original_companion_error_survives_a_failing_move_back() {
    let w = World::new("origin");
    let (id, _) = w.model("Foo.3mf");
    w.companion(id, "Foo.3mf");
    // The companion's name is taken in the new folder (coded error with a parameter), then
    // the move back of the model fails too.
    let fs = FaultMoveFs::new(&[("move_file", 2, AlreadyExists), ("move_file", 3, PermissionDenied)]);
    let r = w.go(&fs, id).unwrap_err();
    assert!(r.error.message.contains("UND Rollback"), "{}", r.error.message);
    let origin = r.origin.expect("origin kept for the log");
    assert!(origin.expected);
    assert_eq!(origin.message, ErrorCode::CompanionNameTaken.as_str());
    assert_eq!(origin.params, vec!["pic.png".to_string()]);
    // The same clash with a working move back returns the origin itself, once.
    let w = World::new("origin2");
    let (id, _) = w.model("Foo.3mf");
    w.companion(id, "Foo.3mf");
    let r = w.go(&FaultMoveFs::new(&[("move_file", 2, AlreadyExists)]), id).unwrap_err();
    assert!(r.origin.is_none());
    assert_eq!(r.error.message, ErrorCode::CompanionNameTaken.as_str());
    assert_eq!(r.error.params, vec!["pic.png".to_string()]);
}
