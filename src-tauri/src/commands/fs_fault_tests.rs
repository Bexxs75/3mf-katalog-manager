//! Failure-in-step-n tests for the desktop file actions (issue #20, P0a). `FaultFs` works on
//! real temp files but fails the n-th call of one operation, so errors that normally need
//! Windows/macOS (locks, permission, case-insensitive names) can be reproduced anywhere.
//! They pin down TODAY's behaviour of the flows; see `docs/plans/issue20-p0a-fs-grenzen.md`.
use super::fs_boundary::{FaultFs, Op};
use super::*;
use std::io::ErrorKind::{PermissionDenied, Unsupported};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

struct Fx {
    base: PathBuf,
    conn: Connection,
    src: PathBuf,
    dst: PathBuf,
    src_id: i64,
    dst_id: i64,
}
impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}
impl Fx {
    fn new(tag: &str) -> Self {
        let base = std::fs::canonicalize(unique_test_dir(&format!("fsfault_{tag}"))).unwrap();
        let (src, dst) = (base.join("src"), base.join("dst"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        let conn = db::connect_in_memory().unwrap();
        let src_id = db::insert_folder_with_parent(&conn, "src", None, &src.to_string_lossy()).unwrap();
        let dst_id = db::insert_folder_with_parent(&conn, "dst", None, &dst.to_string_lossy()).unwrap();
        Self { base, conn, src, dst, src_id, dst_id }
    }
    fn model(&self, name: &str) -> (i64, PathBuf) {
        let p = self.src.join(name);
        std::fs::write(&p, b"model").unwrap();
        (db::test_insert_minimal_file(&self.conn, &p.to_string_lossy(), Some(self.src_id)).unwrap(), p)
    }
    fn companion(&self, model: i64, name: &str) -> db::models::CompanionRecord {
        let source = self.base.join(name);
        std::fs::write(&source, PNG).unwrap();
        companions::add_companion_with_conn(&self.conn, model, &source, &[]).unwrap()
    }
    fn path_of(&self, id: i64) -> String {
        db::get_file(&self.conn, id).unwrap().unwrap().path
    }
    fn names(dir: &Path) -> Vec<String> {
        let mut n: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        n.sort();
        n
    }
}

#[test]
fn realfs_move_calls_link_then_remove() {
    let f = Fx::new("real");
    let (id, old) = f.model("a.3mf");
    let fs = FaultFs::new();
    move_file_to_folder_with_conn_fs(&fs, &f.conn, id, Some(f.dst_id), &[]).unwrap();
    assert!(!old.exists() && f.dst.join("a.3mf").exists());
    assert_eq!(f.path_of(id), f.dst.join("a.3mf").to_string_lossy());
    assert_eq!(fs.count(Op::HardLink), 1);
    assert_eq!(fs.count(Op::RemoveFile), 1);
    assert_eq!(fs.count(Op::CopyNew), 0);
    let ops: Vec<Op> = fs.calls().iter().map(|c| c.0).filter(|o| matches!(o, Op::HardLink | Op::RemoveFile)).collect();
    assert_eq!(ops, vec![Op::HardLink, Op::RemoveFile]);
}

#[test]
fn move_copy_ok_but_removing_the_original_fails() {
    let f = Fx::new("copyrm");
    let (id, old) = f.model("a.3mf");
    // Hard link not possible (other volume) -> copy; then deleting the source is refused.
    let fs = FaultFs::new().fail(Op::HardLink, 1, Unsupported).fail(Op::RemoveFile, 1, PermissionDenied);
    let err = move_file_to_folder_with_conn_fs(&fs, &f.conn, id, Some(f.dst_id), &[]).unwrap_err();
    assert!(err.message.contains("injected RemoveFile"), "{}", err.message);
    assert_eq!(fs.count(Op::CopyNew), 1);
    // The copy is removed again, the original stays, the DB was never touched.
    assert!(old.exists());
    assert!(Fx::names(&f.dst).is_empty());
    assert_eq!(f.path_of(id), old.to_string_lossy());
}

#[test]
fn move_link_ok_but_removing_the_original_fails() {
    let f = Fx::new("linkrm");
    let (id, old) = f.model("a.3mf");
    let fs = FaultFs::new().fail(Op::RemoveFile, 1, PermissionDenied);
    assert!(move_file_to_folder_with_conn_fs(&fs, &f.conn, id, Some(f.dst_id), &[]).is_err());
    assert!(old.exists());
    assert!(Fx::names(&f.dst).is_empty(), "the new link is removed again");
    assert_eq!(f.path_of(id), old.to_string_lossy());
}

#[test]
fn move_companion_fails_and_the_model_move_is_undone() {
    let f = Fx::new("compfail");
    let (id, old) = f.model("Foo.3mf");
    let row = f.companion(id, "pic.png");
    let fs = FaultFs::new()
        .fail_on(Op::HardLink, Some("Foo_extras"), 1, PermissionDenied)
        .fail_on(Op::CopyNew, Some("Foo_extras"), 1, PermissionDenied);
    let err = move_file_to_folder_with_conn_fs(&fs, &f.conn, id, Some(f.dst_id), &[]).unwrap_err();
    assert!(err.message.contains("injected"), "{}", err.message);
    assert!(old.exists(), "model is back");
    assert!(Fx::names(&f.dst).is_empty(), "no leftover folder in the target");
    assert_eq!(f.path_of(id), old.to_string_lossy());
    let after = db::companions::get_companion(&f.conn, row.id).unwrap().unwrap();
    assert_eq!(after.path, row.path);
    assert!(Path::new(&row.path).exists());
}

#[test]
fn move_companion_fails_and_the_rollback_fails_too() {
    let f = Fx::new("rollfail");
    let (id, old) = f.model("Foo.3mf");
    let row = f.companion(id, "pic.png");
    // Link calls: 1 model, 2 companion (fails), 3 rollback of the model (fails); same for copies.
    let fs = FaultFs::new()
        .fail(Op::HardLink, 2, PermissionDenied)
        .fail(Op::CopyNew, 1, PermissionDenied)
        .fail(Op::HardLink, 3, PermissionDenied)
        .fail(Op::CopyNew, 2, PermissionDenied);
    let err = move_file_to_folder_with_conn_fs(&fs, &f.conn, id, Some(f.dst_id), &[]).unwrap_err();
    assert!(err.message.contains("Rollback der Dateiverschiebung fehlgeschlagen"), "{}", err.message);
    // Documented current behaviour: the model stays at the new place while the DB still
    // points at the old path (the message tells the user where the file is).
    assert!(!old.exists() && f.dst.join("Foo.3mf").exists());
    assert_eq!(f.path_of(id), old.to_string_lossy());
    assert!(Path::new(&row.path).exists());
}

#[test]
fn move_onto_a_name_that_differs_only_in_case_is_refused() {
    let f = Fx::new("movecase");
    let (id, old) = f.model("foo.3mf");
    std::fs::write(f.dst.join("FOO.3mf"), b"other").unwrap();
    let fs = FaultFs::new().case_insensitive();
    let err = move_file_to_folder_with_conn_fs(&fs, &f.conn, id, Some(f.dst_id), &[]).unwrap_err();
    assert!(!err.message.is_empty());
    assert!(old.exists());
    assert_eq!(std::fs::read(f.dst.join("FOO.3mf")).unwrap(), b"other");
    assert_eq!(Fx::names(&f.dst), vec!["FOO.3mf"]);
    assert_eq!(f.path_of(id), old.to_string_lossy());
}

#[test]
fn rename_case_only_second_step_fails_and_is_rolled_back() {
    let f = Fx::new("ren2");
    let (id, old) = f.model("A.stl");
    let fs = FaultFs::new().case_insensitive().fail(Op::Rename, 2, PermissionDenied);
    let err = files::rename_file_with_conn_fs(&fs, &f.conn, id, "a.stl".into(), &[]).unwrap_err();
    assert!(err.message.contains("injected Rename #2"), "{}", err.message);
    assert_eq!(fs.count(Op::Rename), 3, "to intermediate, to target (fails), back");
    assert_eq!(Fx::names(&f.src), vec!["A.stl"]);
    assert_eq!(f.path_of(id), old.to_string_lossy());
}

#[test]
fn rename_case_only_second_step_and_rollback_fail() {
    let f = Fx::new("ren3");
    let (id, old) = f.model("A.stl");
    let fs = FaultFs::new()
        .case_insensitive()
        .fail(Op::Rename, 2, PermissionDenied)
        .fail(Op::Rename, 3, PermissionDenied);
    let err = files::rename_file_with_conn_fs(&fs, &f.conn, id, "a.stl".into(), &[]).unwrap_err();
    assert!(err.message.contains("Rollback fehlgeschlagen"), "{}", err.message);
    // Documented current behaviour: the data lies under the intermediate name, DB keeps the old.
    let names = Fx::names(&f.src);
    assert_eq!(names.len(), 1);
    assert!(names[0].contains(".3mf-rename-tmp-"), "{names:?}");
    assert!(err.message.contains(&names[0]));
    assert_eq!(f.path_of(id), old.to_string_lossy());
}

#[test]
fn rename_onto_existing_name_in_other_case_collides() {
    let f = Fx::new("rencoll");
    let (id, old) = f.model("a.3mf");
    std::fs::write(f.src.join("b.3mf"), b"other").unwrap();
    let fs = FaultFs::new().case_insensitive();
    let err = files::rename_file_with_conn_fs(&fs, &f.conn, id, "B.3mf".into(), &[]).unwrap_err();
    assert!(!err.message.is_empty());
    assert_eq!(fs.count(Op::Rename), 0);
    assert_eq!(Fx::names(&f.src), vec!["a.3mf", "b.3mf"]);
    assert_eq!(f.path_of(id), old.to_string_lossy());
}

#[test]
fn rename_collision_check_failing_stops_before_any_change() {
    let f = Fx::new("renperm");
    let (id, old) = f.model("a.3mf");
    let fs = FaultFs::new().fail(Op::SymlinkMetadata, 1, PermissionDenied);
    assert!(files::rename_file_with_conn_fs(&fs, &f.conn, id, "b.3mf".into(), &[]).is_err());
    assert_eq!(fs.count(Op::Rename), 0);
    assert!(old.exists());
    assert_eq!(f.path_of(id), old.to_string_lossy());
}

#[test]
fn trash_metadata_permission_error_changes_nothing() {
    let f = Fx::new("trashmeta");
    let (id, old) = f.model("a.3mf");
    let trash = f.base.join("trash");
    std::fs::create_dir_all(&trash).unwrap();
    let file = db::get_file(&f.conn, id).unwrap().unwrap();
    let fs = FaultFs::new().fail(Op::Metadata, 1, PermissionDenied);
    assert!(trash::delete_file_with_conn_fs(&fs, &f.conn, &file, id, &trash).is_err());
    assert!(old.exists() && Fx::names(&trash).is_empty());
    assert!(db::get_file(&f.conn, id).unwrap().unwrap().deleted_at.is_none());
}

#[test]
fn trash_copy_ok_but_removing_the_original_fails() {
    let f = Fx::new("trashrm");
    let (id, old) = f.model("a.3mf");
    let trash = f.base.join("trash");
    std::fs::create_dir_all(&trash).unwrap();
    let file = db::get_file(&f.conn, id).unwrap().unwrap();
    let fs = FaultFs::new().fail(Op::HardLink, 1, Unsupported).fail(Op::RemoveFile, 1, PermissionDenied);
    assert!(trash::delete_file_with_conn_fs(&fs, &f.conn, &file, id, &trash).is_err());
    assert!(old.exists());
    assert!(Fx::names(&trash).is_empty(), "the copy in the trash is removed again");
    assert!(db::get_file(&f.conn, id).unwrap().unwrap().deleted_at.is_none());
}

#[test]
fn trash_companion_move_fails_model_still_goes_to_trash() {
    let f = Fx::new("trashcomp");
    let (id, old) = f.model("Foo.3mf");
    let row = f.companion(id, "pic.png");
    let trash = f.base.join("trash");
    std::fs::create_dir_all(&trash).unwrap();
    let file = db::get_file(&f.conn, id).unwrap().unwrap();
    let fs = FaultFs::new()
        .fail_on(Op::HardLink, Some("Foo_extras"), 1, PermissionDenied)
        .fail_on(Op::CopyNew, Some("Foo_extras"), 1, PermissionDenied);
    trash::delete_file_with_conn_fs(&fs, &f.conn, &file, id, &trash).unwrap();
    assert!(!old.exists());
    assert!(db::get_file(&f.conn, id).unwrap().unwrap().deleted_at.is_some());
    // The companion stays where it is and active (documented: it never fails the removal).
    assert!(Path::new(&row.path).exists());
    let after = db::companions::get_companion(&f.conn, row.id).unwrap().unwrap();
    assert!(after.deleted_at.is_none());
    assert_eq!(Fx::names(&trash), vec![format!("{id}-Foo.3mf")]);
}

#[test]
fn trash_companion_original_removal_fails_row_and_file_stay_consistent() {
    let f = Fx::new("trashcomp2");
    let (id, _old) = f.model("Foo.3mf");
    let row = f.companion(id, "pic.png");
    let trash = f.base.join("trash");
    std::fs::create_dir_all(&trash).unwrap();
    let file = db::get_file(&f.conn, id).unwrap().unwrap();
    // 1st remove_file = the model's link source, 2nd = the companion's.
    let fs = FaultFs::new().fail(Op::RemoveFile, 2, PermissionDenied);
    trash::delete_file_with_conn_fs(&fs, &f.conn, &file, id, &trash).unwrap();
    assert!(Path::new(&row.path).exists());
    assert!(db::companions::get_companion(&f.conn, row.id).unwrap().unwrap().deleted_at.is_none());
    assert_eq!(Fx::names(&trash).len(), 1, "no half-copied companion in the trash");
}
