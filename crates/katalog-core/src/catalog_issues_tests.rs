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
