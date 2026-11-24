use super::*;
use crate::db::models::{FileType, NewFile};
use std::fs;

fn record(id: i64, hash: Option<&str>, imported_at: &str, path: &str) -> FileRecord {
    let mut conn = db::connect_in_memory().unwrap();
    let mut file = new_file(path, hash, imported_at);
    file.name = format!("file-{id}.3mf");
    let row = db::insert_file(&mut conn, &file).unwrap();
    let mut rec = db::list_files(&conn).unwrap().into_iter().find(|f| f.id == row).unwrap();
    rec.id = id;
    rec
}

fn new_file(path: &str, hash: Option<&str>, imported_at: &str) -> NewFile {
    NewFile {
        name: "m.3mf".into(),
        path: path.into(),
        file_type: FileType::ThreeMf,
        folder_id: None,
        origin: "local".into(),
        cloud_id: None,
        sync_status: "local-only".into(),
        file_size_bytes: 1,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        thumbnail_png: None,
        imported_at: imported_at.into(),
        file_modified_at: None,
        materials: Vec::new(),
        metadata: Default::default(),
        tags: Vec::new(),
        print_status: "not_printed".into(),
        last_viewed_at: None,
        creator: None,
        content_hash: hash.map(str::to_string),
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        license: None,
        source: Default::default(),
        queue_position: None,
        favorite: false,
        plate_count: None,
        plate_dimensions_json: None,
        slice_info_json: None,
        print_info: None,
    }
}

fn ids(groups: &[Vec<FileRecord>]) -> Vec<Vec<i64>> {
    groups.iter().map(|g| g.iter().map(|f| f.id).collect()).collect()
}

#[test]
fn identical_hashes_form_a_group_oldest_first() {
    let a = record(1, Some("h"), "2026-01-02", "/x/a.3mf");
    let b = record(2, Some("h"), "2026-01-01", "/x/b.3mf");
    assert_eq!(ids(&group_duplicates(vec![a, b], &HashSet::new())), vec![vec![2, 1]]);
}

#[test]
fn an_orphaned_member_leaves_no_group() {
    let a = record(1, Some("h"), "2026-01-01", "/x/a.3mf");
    let b = record(2, Some("h"), "2026-01-02", "/x/b.3mf");
    assert!(group_duplicates(vec![a, b], &HashSet::from([1])).is_empty());
}

#[test]
fn files_without_a_hash_and_single_hashes_are_ignored() {
    let a = record(1, None, "2026-01-01", "/x/a.3mf");
    let b = record(2, None, "2026-01-02", "/x/b.3mf");
    let c = record(3, Some("only"), "2026-01-03", "/x/c.3mf");
    assert!(group_duplicates(vec![a, b, c], &HashSet::new()).is_empty());
}

#[test]
fn groups_are_sorted_by_their_oldest_member() {
    let files = vec![
        record(1, Some("late"), "2026-03-01", "/x/1"),
        record(2, Some("late"), "2026-03-02", "/x/2"),
        record(3, Some("early"), "2026-01-01", "/x/3"),
        record(4, Some("early"), "2026-01-02", "/x/4"),
    ];
    assert_eq!(ids(&group_duplicates(files, &HashSet::new())), vec![vec![3, 4], vec![1, 2]]);
}

fn catalog_with(dir: &Path, names: &[(&str, bool, Option<&str>)]) -> Connection {
    let mut conn = db::connect_in_memory().unwrap();
    for (i, (name, exists, hash)) in names.iter().enumerate() {
        let path = dir.join(name);
        if *exists {
            fs::write(&path, b"x").unwrap();
        }
        let day = format!("2026-01-{:02}", i + 1);
        db::insert_file(&mut conn, &new_file(path.to_str().unwrap(), *hash, &day)).unwrap();
    }
    conn
}

#[test]
fn scan_reports_missing_files_and_duplicates_among_present_ones() {
    let dir = crate::unique_test_dir("ci-scan");
    let conn = catalog_with(&dir, &[("a.3mf", true, Some("h")), ("b.3mf", true, Some("h")), ("gone.3mf", false, Some("h"))]);
    let issues = scan(&conn, &StdMissingProbe, IssueLimits::UNLIMITED).unwrap();
    assert_eq!(issues.orphaned.len(), 1);
    assert_eq!(issues.duplicate_groups.len(), 1);
    assert_eq!(issues.duplicate_groups[0].len(), 2);
    assert!(!issues.orphaned.contains(&issues.duplicate_groups[0][0]));
    assert!(!issues.truncated);
}

#[test]
fn limits_cut_the_lists_and_set_truncated() {
    let dir = crate::unique_test_dir("ci-limits");
    let conn = catalog_with(&dir, &[("g1", false, None), ("g2", false, None), ("g3", false, None)]);
    let limits = IssueLimits { max_orphaned: 2, ..IssueLimits::UNLIMITED };
    let issues = scan(&conn, &StdMissingProbe, limits).unwrap();
    assert_eq!(issues.orphaned.len(), 2);
    assert!(issues.truncated);

    let conn = catalog_with(&dir, &[("p1", true, Some("a")), ("p2", true, Some("a")), ("p3", true, Some("b")), ("p4", true, Some("b"))]);
    let limits = IssueLimits { max_groups: 1, ..IssueLimits::UNLIMITED };
    let issues = scan(&conn, &StdMissingProbe, limits).unwrap();
    assert_eq!(issues.duplicate_groups.len(), 1);
    assert!(issues.truncated);

    let limits = IssueLimits { max_duration: Duration::ZERO, ..IssueLimits::UNLIMITED };
    assert!(scan(&conn, &StdMissingProbe, limits).unwrap().truncated);
}

#[test]
fn a_probe_error_is_unclear_and_never_missing() {
    struct Denied;
    impl MissingProbe for Denied {
        fn is_missing(&self, _: &Path) -> std::io::Result<bool> {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        }
    }
    let dir = crate::unique_test_dir("ci-denied");
    let conn = catalog_with(&dir, &[("gone.3mf", false, None)]);
    assert!(scan(&conn, &Denied, IssueLimits::UNLIMITED).unwrap().orphaned.is_empty());
}

#[cfg(unix)]
#[test]
fn std_probe_distinguishes_not_found_from_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = crate::unique_test_dir("ci-eacces");
    let locked = dir.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("m.3mf"), b"x").unwrap();
    assert!(StdMissingProbe.is_missing(&dir.join("nope.3mf")).unwrap());
    assert!(!StdMissingProbe.is_missing(&locked.join("m.3mf")).unwrap());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    // Root ignores directory modes; only assert when the directory really is closed.
    let result = StdMissingProbe.is_missing(&locked.join("m.3mf"));
    if fs::read_dir(&locked).is_err() {
        assert!(result.is_err(), "EACCES must be unclear, not missing");
    }
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn roots_probe_only_decides_below_a_root() {
    let root = crate::unique_test_dir("ci-roots").canonicalize().unwrap();
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/m.3mf"), b"x").unwrap();
    let roots = vec![root.clone()];
    let probe = RootsMissingProbe::new(&roots);
    assert!(!probe.is_missing(&root.join("a/m.3mf")).unwrap());
    assert!(probe.is_missing(&root.join("a/gone.3mf")).unwrap());
    assert!(probe.is_missing(&root.join("nodir/gone.3mf")).unwrap());
    assert!(probe.is_missing(Path::new("/definitely/elsewhere/m.3mf")).is_err());
    assert!(probe.is_missing(&root).is_err());
    assert!(probe.is_missing(&root.join("a/../a/m.3mf")).is_err());
}

#[cfg(unix)]
#[test]
fn roots_probe_treats_links_as_unclear() {
    let root = crate::unique_test_dir("ci-links").canonicalize().unwrap();
    fs::create_dir_all(root.join("real")).unwrap();
    fs::write(root.join("real/m.3mf"), b"x").unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();
    std::os::unix::fs::symlink(root.join("real/m.3mf"), root.join("file-link.3mf")).unwrap();
    let roots = vec![root.clone()];
    let probe = RootsMissingProbe::new(&roots);
    assert!(probe.is_missing(&root.join("link/m.3mf")).is_err());
    assert!(probe.is_missing(&root.join("file-link.3mf")).is_err());
}

/// Rows with explicit import days (earlier day = older), listed by name.
fn catalog_dated(dir: &Path, rows: &[(&str, bool, Option<&str>, &str)]) -> Connection {
    let mut conn = db::connect_in_memory().unwrap();
    for (name, exists, hash, day) in rows {
        let path = dir.join(name);
        if *exists {
            fs::write(&path, b"x").unwrap();
        }
        let mut file = new_file(path.to_str().unwrap(), *hash, day);
        file.name = name.to_string();
        db::insert_file(&mut conn, &file).unwrap();
    }
    conn
}

#[test]
fn the_orphan_limit_overflow_row_never_anchors_a_duplicate_group() {
    let dir = crate::unique_test_dir("ci-overflow");
    // "a" lives (newer), "b" is missing (fills the limit), "c" is missing and OLDER with the same hash.
    let conn = catalog_dated(
        &dir,
        &[("a-alive.3mf", true, Some("h"), "2026-02-01"), ("b-gone.3mf", false, None, "2026-01-05"), ("c-gone.3mf", false, Some("h"), "2026-01-01")],
    );
    let limits = IssueLimits { max_orphaned: 1, ..IssueLimits::UNLIMITED };
    let issues = scan(&conn, &StdMissingProbe, limits).unwrap();
    assert!(issues.truncated);
    assert_eq!(issues.orphaned.len(), 1);
    assert!(issues.duplicate_groups.is_empty(), "the unchecked overflow row must not become the anchor: {issues:?}");
    // Unbounded, the missing copy is an orphan and leaves no group.
    let full = scan(&conn, &StdMissingProbe, IssueLimits::UNLIMITED).unwrap();
    assert_eq!(full.orphaned.len(), 2);
    assert!(full.duplicate_groups.is_empty());
}

#[test]
fn after_a_timeout_unchecked_rows_never_form_duplicate_groups() {
    struct Slow(std::cell::Cell<usize>);
    impl MissingProbe for Slow {
        fn is_missing(&self, path: &Path) -> std::io::Result<bool> {
            self.0.set(self.0.get() + 1);
            std::thread::sleep(Duration::from_millis(40));
            Ok(!path.exists())
        }
    }
    let dir = crate::unique_test_dir("ci-timeout");
    let conn = catalog_dated(&dir, &[("a-alive.3mf", true, Some("h"), "2026-02-01"), ("b-gone.3mf", false, Some("h"), "2026-01-01")]);
    let slow = Slow(Default::default());
    let limits = IssueLimits { max_duration: Duration::from_millis(10), ..IssueLimits::UNLIMITED };
    let issues = scan(&conn, &slow, limits).unwrap();
    assert_eq!(slow.0.get(), 1, "only the first row was looked at");
    assert!(issues.truncated);
    assert!(issues.duplicate_groups.is_empty(), "the unchecked older missing copy must not anchor a group: {issues:?}");
}

#[test]
fn light_scan_matches_the_record_scan() {
    let dir = crate::unique_test_dir("ci-parity");
    let conn = catalog_dated(
        &dir,
        &[
            ("a.3mf", true, Some("h"), "2026-01-03"),
            ("b.3mf", true, Some("h"), "2026-01-01"),
            ("c.3mf", false, Some("h"), "2026-01-02"),
            ("d.3mf", true, Some("k"), "2026-01-05"),
            ("e.3mf", true, Some("k"), "2026-01-04"),
        ],
    );
    let files = db::list_files(&conn).unwrap();
    assert_eq!(scan(&conn, &StdMissingProbe, IssueLimits::UNLIMITED).unwrap(), scan_records(&files, &StdMissingProbe, IssueLimits::UNLIMITED));
}

#[cfg(unix)]
mod roots_probe_races {
    use super::*;
    use std::os::unix::fs::symlink;

    fn tree(name: &str) -> (PathBuf, PathBuf) {
        let base = crate::unique_test_dir(name).canonicalize().unwrap();
        let root = base.join("root");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/m.3mf"), b"x").unwrap();
        let outside = base.join("outside");
        fs::create_dir_all(outside.join("b")).unwrap(); // no m.3mf here
        (root, outside)
    }

    #[test]
    fn an_intermediate_swapped_for_a_link_between_two_components_is_unclear() {
        let (root, outside) = tree("ci-race-mid");
        let roots = vec![root.clone()];
        let probe = RootsMissingProbe::new(&roots);
        let target = root.join("a/b/gone.3mf");
        let mut swapped = false;
        let result = probe.probe(&target, &mut |index| {
            // `a` is open by now; before `b` is looked at, `b` becomes a link to a folder that
            // lacks the file. Following it would report the model as missing.
            if index == 1 {
                fs::rename(root.join("a/b"), root.join("a/b.old")).unwrap();
                symlink(outside.join("b"), root.join("a/b")).unwrap();
                swapped = true;
            }
        });
        assert!(swapped);
        assert!(result.is_err(), "a link met on the way is unclear, got {result:?}");
    }

    #[test]
    fn a_root_replaced_by_a_link_is_unclear() {
        let (root, _) = tree("ci-race-root");
        let roots = vec![root.clone()];
        let probe = RootsMissingProbe::new(&roots);
        assert!(!probe.is_missing(&root.join("a/b/m.3mf")).unwrap());
        let moved = root.with_file_name("root.old");
        fs::rename(&root, &moved).unwrap();
        symlink(&moved, &root).unwrap();
        assert!(probe.is_missing(&root.join("a/b/gone.3mf")).is_err());
        assert!(probe.is_missing(&root.join("a/b/m.3mf")).is_err());
    }

    #[test]
    fn a_root_replaced_by_another_folder_is_unclear() {
        let (root, _) = tree("ci-race-root2");
        let roots = vec![root.clone()];
        let probe = RootsMissingProbe::new(&roots);
        fs::rename(&root, root.with_file_name("root.old")).unwrap();
        fs::create_dir_all(root.join("a")).unwrap();
        assert!(probe.is_missing(&root.join("a/gone.3mf")).is_err());
    }

    #[test]
    fn a_deleted_subfolder_is_missing_but_a_file_or_link_on_the_way_is_unclear() {
        let (root, outside) = tree("ci-parents");
        fs::write(root.join("a/file"), b"x").unwrap();
        symlink(outside.join("b"), root.join("a/link")).unwrap();
        let roots = vec![root.clone()];
        let probe = RootsMissingProbe::new(&roots);
        assert!(probe.is_missing(&root.join("a/removed/deeper/m.3mf")).unwrap());
        assert!(probe.is_missing(&root.join("a/file/m.3mf")).is_err(), "ENOTDIR is unclear");
        assert!(probe.is_missing(&root.join("a/link/m.3mf")).is_err());
        assert!(probe.is_missing(&root.join("a/link")).is_err());
    }
}
