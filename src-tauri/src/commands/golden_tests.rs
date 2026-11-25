//! Golden scenarios of today's desktop behaviour (P0). Later packages add their own here.
use super::golden_harness::run_golden;
use super::*;

fn add_file(w: &World, rel: &str, folder: i64) -> i64 {
    let p = w.file(rel, &format!("content of {rel}"));
    db::test_insert_minimal_file(&w.conn, &p.to_string_lossy(), Some(folder)).unwrap()
}
use super::golden_harness::World;

#[test]
fn golden_rename_file() {
    run_golden("rename_file", false, |w| {
        let dir = w.dir("cat");
        let f = db::insert_folder_with_parent(&w.conn, "cat", None, &dir.to_string_lossy()).unwrap();
        let a = add_file(w, "cat/a.3mf", f);
        add_file(w, "cat/taken.3mf", f);
        let r = rename_file_with_conn(&w.conn, a, "b.3mf".into(), &[]);
        w.step("rename a -> b", r);
        let r = rename_file_with_conn(&w.conn, a, "taken.3mf".into(), &[]);
        w.step("rename b -> taken (collision)", r);
        let r = rename_file_with_conn(&w.conn, a, "B.3mf".into(), &[]);
        w.step("rename b -> B (case only)", r);
        let r = rename_file_with_conn(&w.conn, a, "bad/name.3mf".into(), &[]);
        w.step("rename with slash", r);
        let r = rename_file_with_conn(&w.conn, 999, "x.3mf".into(), &[]);
        w.step("rename unknown id", r);
    });
}

#[test]
fn golden_move_file() {
    run_golden("move_file", false, |w| {
        let src = db::insert_folder_with_parent(&w.conn, "src", None, &w.dir("src").to_string_lossy()).unwrap();
        let dst = db::insert_folder_with_parent(&w.conn, "dst", None, &w.dir("dst").to_string_lossy()).unwrap();
        let a = add_file(w, "src/a.3mf", src);
        let b = add_file(w, "src/b.3mf", src);
        add_file(w, "dst/b.3mf", dst);
        let r = move_file_to_folder_with_conn(&w.conn, a, Some(dst), &[]);
        w.step("move a to dst", r);
        let r = move_file_to_folder_with_conn(&w.conn, b, Some(dst), &[]);
        w.step("move b to dst (name taken)", r);
        let r = move_file_to_folder_with_conn(&w.conn, a, Some(12345), &[]);
        w.step("move to unknown folder", r);
    });
}

#[test]
fn golden_create_folder() {
    run_golden("create_folder", false, |w| {
        let base = w.dir("base");
        let root = db::insert_folder_with_parent(&w.conn, "base", None, &base.to_string_lossy()).unwrap();
        let r = create_folder_with_conn(&w.conn, Some(root), "child".into(), &base.join("child"), &[]).map(|d| d.name);
        w.step("create child", r);
        let r = create_folder_with_conn(&w.conn, Some(root), "child".into(), &base.join("child"), &[]).map(|d| d.name);
        w.step("create child again (exists)", r);
        let r = create_folder_with_conn(&w.conn, None, "viaparent".into(), &base.join("viaparent"), &[]).map(|d| d.parent_id.is_some());
        w.step("create without parent id (parent derived)", r);
    });
}

#[test]
fn golden_remove_folder_from_catalog() {
    run_golden("remove_folder_catalog", false, |w| {
        let top = db::insert_folder_with_parent(&w.conn, "top", None, &w.dir("top").to_string_lossy()).unwrap();
        let sub = db::insert_folder_with_parent(&w.conn, "sub", Some(top), &w.dir("top/sub").to_string_lossy()).unwrap();
        add_file(w, "top/a.3mf", top);
        add_file(w, "top/sub/b.3mf", sub);
        let r = remove_folder_with_conn(&w.conn, "abc").map(|c| serde_json::to_value(&c).unwrap());
        w.step("invalid id", r);
        let r = remove_folder_with_conn(&w.conn, &top.to_string()).map(|c| serde_json::to_value(&c).unwrap());
        w.step("remove top (files stay on disk)", r);
    });
}

#[test]
fn golden_trash_file_with_companion() {
    run_golden("trash_file_companion", false, |w| {
        let f = db::insert_folder_with_parent(&w.conn, "cat", None, &w.dir("cat").to_string_lossy()).unwrap();
        let id = add_file(w, "cat/m.3mf", f);
        let companion = w.file("cat/m/photo.png", "png");
        db::companions::insert_companion(&w.conn, id, "photo.png", &companion.to_string_lossy(), "image", Some(3), "2026-01-01T00:00:00Z").unwrap();
        let trash = w.dir("trash");
        let record = db::get_file(&w.conn, id).unwrap().unwrap();
        let r = trash::delete_file_with_conn(&w.conn, &record, id, &trash);
        w.step("delete model with companion", r);
        let record = db::get_file(&w.conn, id).unwrap().unwrap();
        let r = trash::delete_file_with_conn(&w.conn, &record, id, &trash);
        w.step("delete again (already trashed)", r);
    });
}
