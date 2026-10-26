use super::*;
use crate::db;
use std::fs;

fn dir(name: &str) -> PathBuf {
    crate::unique_test_dir(name).canonicalize().unwrap()
}
fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"x").unwrap();
}
fn model(id: i64, path: &Path, active: bool) -> ModelRow {
    ModelRow { id, path: path.to_str().unwrap().into(), active }
}
fn companion(id: i64, file_id: i64, path: &Path) -> CompanionRow {
    CompanionRow { id, file_id, name: path.file_name().unwrap().to_str().unwrap().into(), path: path.to_str().unwrap().into() }
}
fn report(snapshot: &Snapshot, roots: &[&Path]) -> DiskReport {
    build_report(snapshot, &roots.iter().map(|p| p.to_path_buf()).collect::<Vec<_>>(), Limits::default())
}

#[test]
fn reports_missing_models_new_files_and_dangling_companions_relative_to_the_root() {
    let root = dir("dr-basic");
    touch(&root.join("a/present.3mf"));
    touch(&root.join("a/new.stl"));
    touch(&root.join("b/deep/also_new.obj"));
    touch(&root.join("a/notes.txt"));
    touch(&root.join("a/present_extras/pic.png"));
    touch(&root.join(".hidden/ignored.stl"));
    let snapshot = Snapshot {
        models: vec![model(1, &root.join("a/present.3mf"), true), model(2, &root.join("a/gone.3mf"), true), model(3, &root.join("a/trashed.3mf"), false)],
        companions: vec![
            companion(10, 1, &root.join("a/present_extras/pic.png")),
            companion(11, 1, &root.join("a/present_extras/lost.pdf")),
        ],
    };
    let r = report(&snapshot, &[&root]);
    assert_eq!(r.missing_models, vec![MissingModel { id: "2".into(), root: 0, relative_path: "a/gone.3mf".into() }]);
    let new: Vec<_> = r.new_files.iter().map(|f| f.relative_path.as_str()).collect();
    assert_eq!(new, vec!["a/new.stl", "b/deep/also_new.obj"]);
    assert_eq!(r.dangling_companions.len(), 1);
    assert_eq!(r.dangling_companions[0].relative_path, "a/present_extras/lost.pdf");
    assert_eq!(r.dangling_companions[0].reason, DanglingReason::Missing);
    assert!(!r.truncated);
    assert_eq!(r.roots[0].status, RootStatus::Ok);
    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains(root.to_str().unwrap()), "no absolute path may appear: {json}");
}

#[test]
fn a_trashed_model_still_counts_as_known_so_its_path_is_not_new() {
    let root = dir("dr-trash");
    touch(&root.join("t.stl"));
    let snapshot = Snapshot { models: vec![model(1, &root.join("t.stl"), false)], companions: vec![] };
    let r = report(&snapshot, &[&root]);
    assert!(r.new_files.is_empty());
    assert!(r.missing_models.is_empty());
}

#[cfg(unix)]
#[test]
fn links_are_not_followed_and_never_called_missing() {
    use std::os::unix::fs::symlink;
    let root = dir("dr-links");
    let outside = dir("dr-links-outside");
    touch(&outside.join("secret.stl"));
    touch(&outside.join("real.3mf"));
    symlink(&outside, root.join("linked_dir")).unwrap();
    symlink(outside.join("secret.stl"), root.join("linked_file.stl")).unwrap();
    touch(&root.join("keep.stl"));
    let snapshot = Snapshot {
        models: vec![model(1, &root.join("linked_dir/real.3mf"), true), model(2, &root.join("linked_file.stl"), true), model(3, &root.join("keep.stl"), true)],
        companions: vec![companion(5, 3, &root.join("linked_file.stl"))],
    };
    let r = report(&snapshot, &[&root]);
    assert!(r.new_files.is_empty(), "{:?}", r.new_files);
    assert!(r.missing_models.is_empty(), "{:?}", r.missing_models);
    // A final link is something other than a file; a link on the way is not judged.
    assert_eq!(r.dangling_companions.len(), 1);
    assert_eq!(r.dangling_companions[0].reason, DanglingReason::NotFile);
    assert!(!serde_json::to_string(&r).unwrap().contains("secret"));
}

#[test]
fn rows_outside_every_root_are_counted_but_never_listed() {
    let root = dir("dr-out");
    touch(&root.join("x.stl"));
    let other = dir("dr-out-other");
    let snapshot = Snapshot {
        models: vec![model(1, &root.join("x.stl"), true), model(2, &other.join("secret/gone.3mf"), true), model(3, &other.join("y.3mf"), false)],
        companions: vec![companion(1, 1, &other.join("secret/c.png"))],
    };
    let r = report(&snapshot, &[&root]);
    assert_eq!((r.models_outside_roots, r.companions_outside_roots), (1, 1));
    assert!(r.missing_models.is_empty() && r.dangling_companions.is_empty());
    assert!(!serde_json::to_string(&r).unwrap().contains("secret"));
}

#[test]
fn an_empty_root_that_should_hold_models_is_not_trusted() {
    let root = dir("dr-empty");
    let snapshot = Snapshot { models: vec![model(1, &root.join("a.3mf"), true)], companions: vec![companion(1, 1, &root.join("a_extras/c.png"))] };
    let r = report(&snapshot, &[&root]);
    assert_eq!(r.roots[0].status, RootStatus::Empty);
    assert!(r.missing_models.is_empty() && r.dangling_companions.is_empty() && r.new_files.is_empty());
}

#[test]
fn an_unreachable_root_is_flagged_and_skipped() {
    let root = dir("dr-unreach").join("nope");
    let r = report(&Snapshot::default(), &[&root]);
    assert_eq!(r.roots[0].status, RootStatus::Unreachable);
}

#[test]
fn nested_roots_do_not_duplicate_entries() {
    let outer = dir("dr-nest");
    touch(&outer.join("inner/n.stl"));
    let inner = outer.join("inner");
    let r = report(&Snapshot::default(), &[&inner, &outer]);
    assert_eq!(r.new_files.len(), 1);
    assert_eq!(r.new_files[0].relative_path, "inner/n.stl");
}

#[test]
fn item_limit_truncates_each_list_and_says_so() {
    let root = dir("dr-items");
    for i in 0..5 {
        touch(&root.join(format!("n{i}.stl")));
    }
    let mut models = vec![model(99, &root.join("n0.stl"), true)];
    models.extend((0..5).map(|i| model(i, &root.join(format!("gone{i}.3mf")), true)));
    let limits = Limits { max_items: 2, ..Limits::default() };
    let r = build_report(&Snapshot { models, companions: vec![] }, std::slice::from_ref(&root), limits);
    assert_eq!(r.missing_models.len(), 2);
    assert_eq!(r.new_files.len(), 2);
    assert!(r.truncated && r.truncated_by.contains(&TruncatedBy::Items));
}

#[test]
fn entry_limit_stops_the_walk() {
    let root = dir("dr-entries");
    for i in 0..20 {
        touch(&root.join(format!("n{i:02}.stl")));
    }
    let limits = Limits { max_entries: 5, ..Limits::default() };
    let r = build_report(&Snapshot::default(), std::slice::from_ref(&root), limits);
    assert!(r.truncated && r.truncated_by.contains(&TruncatedBy::Entries));
    assert!(r.scanned_entries <= 5 && r.new_files.len() <= 5);
}

#[test]
fn time_limit_ends_the_work_with_a_truncated_report() {
    let root = dir("dr-time");
    touch(&root.join("a.stl"));
    let snapshot = Snapshot { models: vec![model(1, &root.join("g.3mf"), true)], companions: vec![] };
    let limits = Limits { max_duration: Duration::ZERO, ..Limits::default() };
    let r = build_report(&snapshot, std::slice::from_ref(&root), limits);
    assert!(r.truncated && r.truncated_by.contains(&TruncatedBy::Time));
}

#[test]
fn snapshot_reads_active_companions_of_active_models_only() {
    let conn = db::connect_in_memory().unwrap();
    let live = db::test_insert_minimal_file(&conn, "/m/a.3mf", None).unwrap();
    let trashed = db::test_insert_minimal_file(&conn, "/m/b.3mf", None).unwrap();
    db::companions::insert_companion(&conn, live, "p.png", "/m/a_extras/p.png", "image", Some(1), "now").unwrap();
    let gone = db::companions::insert_companion(&conn, live, "q.png", "/m/a_extras/q.png", "image", Some(1), "now").unwrap();
    db::companions::soft_delete_companion(&conn, gone, None, "now").unwrap();
    db::companions::insert_companion(&conn, trashed, "r.png", "/m/b_extras/r.png", "image", Some(1), "now").unwrap();
    db::soft_delete_file(&conn, trashed, None, "now").unwrap();
    let s = snapshot(&conn).unwrap();
    assert_eq!(s.models.len(), 2);
    assert!(s.models.iter().any(|m| m.id == trashed && !m.active));
    assert_eq!(s.companions.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["p.png"]);
    assert!(conn.is_autocommit());
}
