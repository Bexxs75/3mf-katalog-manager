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

fn files(root: &Path, dir: &str, count: usize) {
    for i in 0..count {
        touch(&root.join(dir).join(format!("f{i:04}.stl")));
    }
}
fn leak(hook: impl Fn(&Path) + Sync + 'static) -> &'static (dyn Fn(&Path) + Sync) {
    Box::leak(Box::new(hook))
}

#[test]
fn a_directory_far_beyond_the_entry_limit_stops_at_the_limit() {
    let root = dir("dr-wide");
    files(&root, "", 3000);
    let limits = Limits { max_entries: 10, ..Limits::default() };
    let r = build_report(&Snapshot::default(), std::slice::from_ref(&root), limits);
    assert!(r.truncated_by.contains(&TruncatedBy::Entries));
    assert!(r.scanned_entries <= 10 && r.new_files.len() <= 10);
}

#[test]
fn the_walk_checks_the_clock_itself() {
    let root = dir("dr-walkclock");
    files(&root, "d", 300);
    // The first directory (the root) is read at once, the second one only after the budget is spent.
    let calls: &'static std::sync::atomic::AtomicUsize = Box::leak(Box::new(std::sync::atomic::AtomicUsize::new(0)));
    let hook = leak(move |_| {
        if calls.fetch_add(1, Ordering::SeqCst) == 1 {
            std::thread::sleep(Duration::from_millis(400));
        }
    });
    use std::sync::atomic::Ordering;
    let limits = Limits { max_duration: Duration::from_millis(250), walk_hook: Some(hook), ..Limits::default() };
    let r = build_report(&Snapshot::default(), std::slice::from_ref(&root), limits);
    assert!(r.truncated_by.contains(&TruncatedBy::Time), "{:?}", r.truncated_by);
    assert!(r.scanned_entries > 0 && r.new_files.len() < 300, "walk ran on after the deadline: {}", r.new_files.len());
}

#[test]
fn known_files_never_show_up_as_new_after_a_timeout() {
    let root = dir("dr-known-timeout");
    let mut models = Vec::new();
    for i in 0..5 {
        touch(&root.join(format!("k{i}.3mf")));
        models.push(model(i, &root.join(format!("k{i}.3mf")), true));
    }
    let limits = Limits { max_duration: Duration::ZERO, ..Limits::default() };
    let r = build_report(&Snapshot { models, companions: vec![] }, std::slice::from_ref(&root), limits);
    assert!(r.truncated_by.contains(&TruncatedBy::Time));
    assert!(r.new_files.is_empty(), "{:?}", r.new_files);
}

#[test]
fn companion_folders_and_extras_folders_are_not_scanned_for_new_files() {
    let root = dir("dr-extras");
    touch(&root.join("a.3mf"));
    touch(&root.join("a_extras/part.stl"));
    touch(&root.join("pics/x.3mf"));
    touch(&root.join("pics/p.png"));
    let snapshot = Snapshot { models: vec![model(1, &root.join("a.3mf"), true)], companions: vec![companion(1, 1, &root.join("pics/p.png"))] };
    let r = report(&snapshot, &[&root]);
    assert!(r.new_files.is_empty(), "{:?}", r.new_files);
}

#[test]
fn an_empty_root_without_models_is_fine() {
    let root = dir("dr-empty-ok");
    let r = report(&Snapshot::default(), &[&root]);
    assert_eq!(r.roots[0].status, RootStatus::Ok);
}

#[test]
fn rows_with_dot_dot_leave_no_trace_and_count_as_outside() {
    let root = dir("dr-dots");
    touch(&root.join("keep.stl"));
    let sneaky = format!("{}/../outside/x.3mf", root.display());
    let snapshot = Snapshot {
        models: vec![ModelRow { id: 1, path: sneaky.clone(), active: true }, ModelRow { id: 2, path: format!("{}/./keep.stl", root.display()), active: true }],
        companions: vec![CompanionRow { id: 1, file_id: 1, name: "n".into(), path: sneaky }],
    };
    let r = report(&snapshot, &[&root]);
    assert_eq!((r.models_outside_roots, r.companions_outside_roots), (1, 1));
    assert!(r.missing_models.is_empty() && r.dangling_companions.is_empty());
    assert!(!serde_json::to_string(&r).unwrap().contains("outside"));
}

#[test]
fn rows_below_an_unreachable_root_are_not_counted_as_outside() {
    let gone = dir("dr-unreach2").join("unmounted");
    let snapshot = Snapshot { models: vec![model(1, &gone.join("a.3mf"), true)], companions: vec![companion(1, 1, &gone.join("a_extras/c.png"))] };
    let r = report(&snapshot, &[&gone]);
    assert_eq!(r.roots[0].status, RootStatus::Unreachable);
    assert_eq!((r.models_outside_roots, r.companions_outside_roots), (0, 0));
    assert!(r.missing_models.is_empty() && r.dangling_companions.is_empty());
}

fn models_with_missing(root: &Path, total: i64, missing: i64) -> Snapshot {
    touch(&root.join("stray.txt"));
    let models = (0..total)
        .map(|i| {
            let path = root.join(format!("m{i}.3mf"));
            if i >= missing {
                touch(&path);
            }
            model(i, &path, true)
        })
        .collect();
    Snapshot { models, companions: vec![] }
}

#[test]
fn a_root_missing_most_of_its_models_is_flagged_as_perhaps_partly_mounted() {
    let (a, b, c) = (dir("dr-mm-a"), dir("dr-mm-b"), dir("dr-mm-c"));
    assert!(report(&models_with_missing(&a, 10, 6), &[&a]).roots[0].mostly_missing);
    assert!(!report(&models_with_missing(&b, 10, 5), &[&b]).roots[0].mostly_missing, "exactly half is not more than half");
    assert!(!report(&models_with_missing(&c, 9, 9), &[&c]).roots[0].mostly_missing, "too few models to say");
}

#[test]
fn a_writer_can_commit_while_the_disk_is_scanned() {
    let root = dir("dr-writer");
    touch(&root.join("a.stl"));
    let db_path = root.join("c.db");
    let conn = db::connect(&db_path).unwrap();
    db::test_insert_minimal_file(&conn, root.join("a.stl").to_str().unwrap(), None).unwrap();
    let snap = snapshot(&conn).unwrap();
    // Even the connection that took the snapshot is still open; the walk must not depend on it.
    let committed: &'static std::sync::atomic::AtomicBool = Box::leak(Box::new(std::sync::atomic::AtomicBool::new(false)));
    let path = db_path.clone();
    let hook = leak(move |_| {
        let writer = db::connect(&path).unwrap();
        writer.execute("UPDATE files SET path = path", []).unwrap();
        committed.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let limits = Limits { walk_hook: Some(hook), ..Limits::default() };
    let r = build_report(&snap, std::slice::from_ref(&root), limits);
    assert!(committed.load(std::sync::atomic::Ordering::SeqCst), "the walk never ran the hook");
    assert!(!r.truncated);
}

fn folder_rows(root: &Path, names: &[&str]) -> Vec<(i64, String)> {
    names.iter().enumerate().map(|(i, n)| (i as i64 + 1, root.join(n).to_str().unwrap().to_string())).collect()
}

#[test]
fn folder_check_reports_only_directories_that_are_gone() {
    let root = dir("dr-folders");
    fs::create_dir_all(root.join("here/sub")).unwrap();
    fs::write(root.join("keep.txt"), b"x").unwrap();
    let rows = folder_rows(&root, &["here", "here/sub", "gone", "here/gone/deeper"]);
    let (missing, truncated) = missing_folder_ids(&rows, std::slice::from_ref(&root), Limits::default());
    assert_eq!(missing, HashSet::from([3, 4]));
    assert!(!truncated);
}

#[test]
fn folder_check_skips_the_root_itself_and_paths_outside_every_root() {
    let root = dir("dr-folders-out");
    let other = dir("dr-folders-other");
    fs::write(root.join("keep.txt"), b"x").unwrap();
    let rows = vec![(1, root.to_str().unwrap().to_string()), (2, other.join("nowhere").to_str().unwrap().to_string())];
    let (missing, _) = missing_folder_ids(&rows, std::slice::from_ref(&root), Limits::default());
    assert!(missing.is_empty());
}

#[cfg(unix)]
#[test]
fn folder_check_does_not_follow_links_and_calls_them_unknown() {
    let root = dir("dr-folders-link");
    let outside = dir("dr-folders-link-target");
    fs::create_dir_all(outside.join("inside")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    fs::write(root.join("keep.txt"), b"x").unwrap();
    // The first folder exists behind the link, the second does not; neither may be judged.
    let rows = folder_rows(&root, &["link/inside", "link/absent"]);
    let (missing, _) = missing_folder_ids(&rows, std::slice::from_ref(&root), Limits::default());
    assert!(missing.is_empty());
}

#[test]
fn folder_check_trusts_neither_an_empty_nor_an_unreachable_root() {
    let empty = dir("dr-folders-empty");
    let rows = folder_rows(&empty, &["a"]);
    assert!(missing_folder_ids(&rows, std::slice::from_ref(&empty), Limits::default()).0.is_empty());
    let absent = empty.join("not-mounted");
    let rows = folder_rows(&absent, &["a"]);
    assert!(missing_folder_ids(&rows, std::slice::from_ref(&absent), Limits::default()).0.is_empty());
}

#[test]
fn folder_check_stops_at_its_entry_limit_and_says_so() {
    let root = dir("dr-folders-limit");
    fs::write(root.join("keep.txt"), b"x").unwrap();
    let rows = folder_rows(&root, &["a", "b", "c"]);
    let limits = Limits { max_entries: 2, ..Limits::default() };
    let (missing, truncated) = missing_folder_ids(&rows, std::slice::from_ref(&root), limits);
    assert_eq!(missing, HashSet::from([1, 2]));
    assert!(truncated);
}

#[test]
fn excluded_folders_are_not_walked_and_ordered_roots_match_the_report_indexes() {
    let root = dir("dr-excluded");
    touch(&root.join("uploads/a.3mf"));
    touch(&root.join("uploads/deep/b.3mf"));
    touch(&root.join("ok/c.3mf"));
    let snapshot = Snapshot::default();
    let configured = vec![root.clone()];
    let plain = build_report(&snapshot, &configured, Limits::default());
    assert_eq!(plain.new_files.len(), 3);
    let excluded = [root.join("uploads"), root.join("does-not-exist")];
    let r = build_report_with(&snapshot, &configured, ReportOptions { limits: Limits::default(), excluded: &excluded });
    let new: Vec<_> = r.new_files.iter().map(|f| f.relative_path.as_str()).collect();
    assert_eq!(new, vec!["ok/c.3mf"]);
    // Without exclusions the new entry point reports exactly what the old one did.
    assert_eq!(build_report_with(&snapshot, &configured, ReportOptions { limits: Limits::default(), excluded: &[] }), plain);

    // A root inside an excluded folder is not walked at all.
    let inner = build_report_with(&snapshot, &[root.join("uploads")], ReportOptions { limits: Limits::default(), excluded: &[root.join("uploads")] });
    assert!(inner.new_files.is_empty());

    // Index order: shallow roots first, the same for the report and for `ordered_roots`.
    let deep = dir("dr-order").join("x/y");
    fs::create_dir_all(&deep).unwrap();
    touch(&deep.join("deep.3mf"));
    let shallow = dir("dr-order-shallow");
    touch(&shallow.join("s.3mf"));
    let configured = vec![deep.clone(), shallow.clone()];
    let ordered = ordered_roots(&configured);
    assert_eq!(ordered, vec![shallow.clone(), deep.clone()]);
    let r = build_report(&snapshot, &configured, Limits::default());
    let by_index: Vec<_> = r.new_files.iter().map(|f| (f.root, f.relative_path.clone())).collect();
    assert_eq!(by_index, vec![(0, "s.3mf".to_owned()), (1, "deep.3mf".to_owned())]);
}

#[cfg(unix)]
mod unclear_and_folders {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn unclear(r: &DiskReport) -> Vec<(usize, &str, UnclearReason)> {
        r.unclear.iter().map(|u| (u.root, u.relative_path.as_str(), u.reason)).collect()
    }

    #[test]
    fn a_deleted_subfolder_with_several_models_becomes_one_missing_folder() {
        let root = dir("dr-gone-folder");
        touch(&root.join("keep/here.3mf"));
        let snapshot = Snapshot {
            models: vec![
                model(1, &root.join("old/a.3mf"), true),
                model(2, &root.join("old/deeper/b.3mf"), true),
                model(3, &root.join("keep/lost.3mf"), true),
                model(4, &root.join("keep/here.3mf"), true),
            ],
            companions: vec![],
        };
        let r = report(&snapshot, &[&root]);
        assert_eq!(r.missing_models.len(), 3);
        assert_eq!(r.missing_folders, vec![MissingFolder { root: 0, relative_path: "old".into(), models: vec!["1".into(), "2".into()] }]);
        assert!(r.unclear.is_empty());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["missingFolders"][0]["relativePath"], "old");
    }

    #[test]
    fn a_file_or_a_link_on_the_way_is_unclear_never_missing() {
        let root = dir("dr-unclear-way");
        let outside = dir("dr-unclear-way-out");
        touch(&root.join("plain"));
        touch(&outside.join("m.3mf"));
        symlink(&outside, root.join("link")).unwrap();
        let snapshot = Snapshot {
            models: vec![model(1, &root.join("plain/m.3mf"), true), model(2, &root.join("link/m.3mf"), true)],
            companions: vec![companion(9, 1, &root.join("plain/x/pic.png"))],
        };
        let r = report(&snapshot, &[&root]);
        assert!(r.missing_models.is_empty() && r.missing_folders.is_empty() && r.dangling_companions.is_empty());
        assert_eq!(
            unclear(&r),
            vec![(0, "link/m.3mf", UnclearReason::Link), (0, "plain/m.3mf", UnclearReason::Link), (0, "plain/x/pic.png", UnclearReason::Link)]
        );
    }

    #[test]
    fn a_folder_without_permission_is_unclear_with_reason_read() {
        let root = dir("dr-chmod");
        touch(&root.join("locked/m.3mf"));
        touch(&root.join("other.3mf"));
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let denied = fs::read_dir(root.join("locked")).is_err();
        let snapshot = Snapshot { models: vec![model(1, &root.join("locked/m.3mf"), true)], companions: vec![] };
        let r = report(&snapshot, &[&root]);
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
        if denied {
            assert!(r.missing_models.is_empty());
            assert_eq!(unclear(&r), vec![(0, "locked/m.3mf", UnclearReason::Read)]);
        }
    }

    #[test]
    fn rows_below_an_unreachable_or_empty_root_are_unclear_unreachable() {
        let gone = dir("dr-root-gone").join("not-there");
        let empty = dir("dr-root-empty");
        let snapshot = Snapshot {
            models: vec![model(1, &gone.join("a.3mf"), true), model(2, &empty.join("b.3mf"), true), model(3, &empty.join("t.3mf"), false)],
            companions: vec![],
        };
        let r = report(&snapshot, &[&gone, &empty]);
        assert!(r.missing_models.is_empty());
        assert_eq!(r.unclear.len(), 2, "trashed rows are not listed: {:?}", r.unclear);
        assert!(r.unclear.iter().all(|u| u.reason == UnclearReason::Unreachable));
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains(empty.to_str().unwrap()) && !json.contains(gone.to_str().unwrap()));
    }

    #[test]
    fn nothing_new_is_serialized_when_there_is_nothing_to_say() {
        let root = dir("dr-quiet");
        touch(&root.join("a.3mf"));
        let snapshot = Snapshot { models: vec![model(1, &root.join("a.3mf"), true)], companions: vec![] };
        let json = serde_json::to_value(report(&snapshot, &[&root])).unwrap();
        assert!(json.get("unclear").is_none() && json.get("missingFolders").is_none());
    }
}
