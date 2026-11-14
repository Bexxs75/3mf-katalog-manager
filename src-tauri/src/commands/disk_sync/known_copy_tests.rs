//! Copies the user declined are remembered by the sync: not offered again and not hashed
//! again, until the file or its original changes.
use super::*;
use std::cell::Cell;

const OBJ_A: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
const OBJ_B: &str = "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";

struct World {
    base: PathBuf,
    conn: Connection,
    root: PathBuf,
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}
impl World {
    /// A catalog holding `orig.obj`.
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!(
            "disk-known-copies-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&base).unwrap();
        let root = base.join("models");
        std::fs::create_dir(&root).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &root, &root).unwrap();
        std::fs::write(root.join("orig.obj"), OBJ_A).unwrap();
        let mut w = Self { base, conn, root };
        let preview = scan(&w.conn).unwrap();
        let new_paths = preview.new_folders.iter().chain(&preview.new_files).map(|e| e.path.clone()).collect();
        apply(&mut w.conn, DiskSelection { new_paths, ..Default::default() }).unwrap();
        w
    }
    fn write(&self, name: &str, content: &str) -> String {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }
    /// Scan that counts how many files were hashed.
    fn counted_scan(&self, case_insensitive: bool) -> (DiskScan, usize) {
        let hashed = Cell::new(0usize);
        let scan = scan_with_hasher(&self.conn, &mut read_directory, case_insensitive, &|p: &Path| {
            hashed.set(hashed.get() + 1);
            hash_for_match(p)
        })
        .unwrap();
        (scan, hashed.get())
    }
    /// The user opens the dialog and takes over nothing: every copy is declined.
    fn decline_all_copies(&mut self) -> DiskApplied {
        let preview = scan(&self.conn).unwrap();
        let declined_copies = preview.duplicate_files.iter().map(|d| d.entry.path.clone()).collect();
        apply(&mut self.conn, DiskSelection { declined_copies, ..Default::default() }).unwrap()
    }
    fn known(&self) -> Vec<db::known_copies::KnownCopyRecord> {
        db::known_copies::list_known_copies(&self.conn).unwrap()
    }
    fn offered_paths(scan: &DiskScan) -> Vec<String> {
        let mut paths: Vec<String> =
            scan.new_files.iter().chain(scan.duplicate_files.iter().map(|d| &d.entry)).map(|e| e.relative_path.clone()).collect();
        paths.sort();
        paths
    }
}

#[test]
fn a_declined_copy_is_remembered_and_not_offered_or_hashed_again() {
    let mut w = World::new();
    w.write("copy.obj", OBJ_A);

    let (first, hashed_first) = w.counted_scan(false);
    assert_eq!(first.duplicate_files.len(), 1);
    assert_eq!(first.known_copy_count, 0);
    assert_eq!(hashed_first, 1, "precondition: the copy is hashed while nobody remembers it");

    let applied = w.decline_all_copies();
    assert_eq!((applied.noted_copy_count, applied.known_copy_count), (1, 1));
    assert!(applied.file_ids.is_empty(), "declining imports nothing");
    let rows = w.known();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].path.ends_with("copy.obj"));
    assert_eq!(rows[0].size_bytes, OBJ_A.len() as i64);
    assert_eq!(rows[0].content_hash.len(), 64);

    let (second, hashed_second) = w.counted_scan(false);
    assert!(second.duplicate_files.is_empty() && second.new_files.is_empty(), "{second:?}");
    assert_eq!(second.known_copy_count, 1);
    assert_eq!(hashed_second, 0, "a remembered copy is not hashed again");
}

#[test]
fn other_new_files_are_still_offered_next_to_a_remembered_copy() {
    let mut w = World::new();
    w.write("copy.obj", OBJ_A);
    w.decline_all_copies();
    w.write("fresh.obj", OBJ_B);

    let (scan, hashed) = w.counted_scan(false);

    assert_eq!(World::offered_paths(&scan), vec!["fresh.obj"]);
    assert_eq!(hashed, 1, "only the new file is hashed");
}

#[test]
fn a_remembered_copy_is_offered_again_when_its_original_is_gone() {
    // Gone from the catalog (moved to the trash) and changed in the catalog are the same case.
    for how in ["trashed", "other content"] {
        let mut w = World::new();
        w.write("copy.obj", OBJ_A);
        w.decline_all_copies();
        match how {
            "trashed" => w.conn.execute("UPDATE files SET deleted_at = '2026-11-01T10:00:00Z'", []).unwrap(),
            _ => w.conn.execute("UPDATE files SET content_hash = ?1", ["f".repeat(64)]).unwrap(),
        };

        let (scan, hashed) = w.counted_scan(false);

        assert!(w.known().is_empty(), "{how}: the row is forgotten");
        // With no cataloged content left there is nothing to compare with, so nothing is read at all.
        assert_eq!(hashed, if how == "trashed" { 0 } else { 1 }, "{how}: the file is judged again");
        assert_eq!(scan.known_copy_count, 0, "{how}");
        // Without the original it is just a new model; with a changed original, no match either.
        assert_eq!(scan.new_files.len(), 1, "{how}");
        assert!(scan.duplicate_files.is_empty(), "{how}");
    }
}

#[test]
fn a_remembered_copy_whose_original_is_missing_on_disk_is_offered_again() {
    let mut w = World::new();
    w.write("copy.obj", OBJ_A);
    w.decline_all_copies();
    std::fs::remove_file(w.root.join("orig.obj")).unwrap();

    let (scan, _) = w.counted_scan(false);

    assert!(w.known().is_empty());
    assert_eq!(scan.known_copy_count, 0);
    // The copy now looks like the moved original.
    assert_eq!(scan.renamed_files.len(), 1, "{scan:?}");
}

#[test]
fn a_remembered_copy_that_changed_its_size_is_judged_again() {
    let mut w = World::new();
    let copy = w.write("copy.obj", OBJ_A);
    w.decline_all_copies();
    std::fs::write(&copy, format!("{OBJ_A}# edited\n")).unwrap();

    let (scan, hashed) = w.counted_scan(false);

    assert!(w.known().is_empty(), "the row is forgotten");
    assert_eq!(hashed, 1);
    assert_eq!(World::offered_paths(&scan), vec!["copy.obj"]);
    assert!(scan.duplicate_files.is_empty(), "the content differs now, so it is a new file, not a copy");
}

#[test]
fn a_vanished_copy_is_forgotten_on_the_next_scan() {
    let mut w = World::new();
    let copy = w.write("copy.obj", OBJ_A);
    w.decline_all_copies();
    std::fs::remove_file(copy).unwrap();

    let (scan, _) = w.counted_scan(false);

    assert!(w.known().is_empty());
    assert_eq!(scan.known_copy_count, 0);
}

#[test]
fn a_copy_that_was_ticked_is_imported_and_not_remembered() {
    let mut w = World::new();
    let copy = w.write("copy.obj", OBJ_A);
    let other = w.write("other.obj", OBJ_A);
    let preview = scan(&w.conn).unwrap();
    // The webview names both as declined, but one is also chosen: the choice wins.
    let selection = DiskSelection {
        new_paths: vec![copy.clone()],
        declined_copies: preview.duplicate_files.iter().map(|d| d.entry.path.clone()).collect(),
        ..Default::default()
    };

    let applied = apply(&mut w.conn, selection).unwrap();

    assert_eq!(applied.file_ids.len(), 1);
    assert_eq!(applied.noted_copy_count, 1);
    let rows = w.known();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, other);
}

#[test]
fn only_paths_the_scan_lists_as_copies_are_remembered() {
    let mut w = World::new();
    w.write("fresh.obj", OBJ_B);
    let outside = w.base.join("elsewhere.obj");
    std::fs::write(&outside, OBJ_A).unwrap();
    let selection = DiskSelection {
        declined_copies: vec![
            outside.to_string_lossy().into_owned(),
            w.root.join("orig.obj").to_string_lossy().into_owned(),
            w.root.join("fresh.obj").to_string_lossy().into_owned(),
            "/nowhere/at/all.obj".into(),
        ],
        ..Default::default()
    };

    let applied = apply(&mut w.conn, selection).unwrap();

    assert_eq!((applied.noted_copy_count, applied.known_copy_count), (0, 0));
    assert!(w.known().is_empty(), "outside the roots, already cataloged and merely new files are not remembered");
}

#[test]
fn only_the_copies_the_user_declined_are_remembered() {
    // Two copies are listed; the request names one of them. The other must stay offered
    // (it may be a copy the user ticked, or one that appeared after the dialog opened).
    let mut w = World::new();
    w.write("copy_a.obj", OBJ_A);
    w.write("copy_b.obj", OBJ_A);
    let declined = w.root.join("copy_a.obj").to_string_lossy().into_owned();

    let applied = apply(&mut w.conn, DiskSelection { declined_copies: vec![declined], ..Default::default() }).unwrap();

    assert_eq!((applied.noted_copy_count, applied.known_copy_count), (1, 1));
    let rows = w.known();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].path.ends_with("copy_a.obj"));
    let (scan, _) = w.counted_scan(false);
    assert_eq!(World::offered_paths(&scan), vec!["copy_b.obj"]);
    assert_eq!(scan.known_copy_count, 1);
}

#[test]
fn resetting_forgets_everything_and_the_copy_is_offered_again() {
    let mut w = World::new();
    w.write("copy.obj", OBJ_A);
    w.write("sub/copy2.obj", OBJ_A);
    w.decline_all_copies();
    assert_eq!(w.known().len(), 2);
    let listed = known_copies::list_known_copies_with_conn(&w.conn).unwrap();
    assert_eq!(listed.total, 2);
    let relative: Vec<&str> = listed.items.iter().map(|i| i.relative_path.as_str()).collect();
    assert!(relative.contains(&"copy.obj") && relative.contains(&"sub/copy2.obj".replace('/', std::path::MAIN_SEPARATOR_STR).as_str()), "{relative:?}");
    assert!(listed.items.iter().all(|i| i.copy_of_name.as_deref() == Some("orig.obj")), "{listed:?}");

    assert_eq!(db::known_copies::clear_known_copies(&w.conn).unwrap(), 2);

    let (scan, hashed) = w.counted_scan(false);
    assert_eq!(scan.duplicate_files.len(), 2);
    assert_eq!(hashed, 2);
    assert_eq!(known_copies::list_known_copies_with_conn(&w.conn).unwrap().total, 0);
}

#[test]
fn a_folder_that_only_holds_remembered_copies_is_not_offered_as_new() {
    let mut w = World::new();
    w.write("sub/copy.obj", OBJ_A);
    let first = scan(&w.conn).unwrap();
    assert_eq!(first.new_folders.len(), 1, "precondition: the folder is offered while the copy is open");
    w.decline_all_copies();

    let (scan, _) = w.counted_scan(false);

    assert!(scan.new_folders.is_empty(), "{scan:?}");
    assert_eq!(scan.known_copy_count, 1);
}

#[test]
fn a_remembered_copy_matches_whatever_the_case_of_its_path_where_paths_are_folded() {
    let mut w = World::new();
    w.write("Copy.obj", OBJ_A);
    let preview = scan_with_path_mode(&w.conn, &mut read_directory, true).unwrap();
    let selection = DiskSelection {
        declined_copies: preview.duplicate_files.iter().map(|d| d.entry.path.clone()).collect(),
        ..Default::default()
    };
    apply_with_path_mode(&mut w.conn, selection, read_model_with_hash, true).unwrap();
    assert_eq!(w.known().len(), 1);
    std::fs::rename(w.root.join("Copy.obj"), w.root.join("COPY.obj")).unwrap();

    let (folded, hashed) = w.counted_scan(true);
    assert_eq!(folded.known_copy_count, 1, "the other spelling is the same file there");
    assert_eq!(hashed, 0);

    let (exact, _) = w.counted_scan(false);
    assert_eq!(exact.known_copy_count, 0, "where case matters, COPY.obj is another file");
    assert!(w.known().is_empty(), "so the row for Copy.obj is stale");
}

#[test]
fn declining_the_same_copy_again_keeps_one_row() {
    let mut w = World::new();
    w.write("copy.obj", OBJ_A);
    // A scan that does not know the row, as after a reset in another window, declines it twice.
    let preview = scan(&w.conn).unwrap();
    let declined: Vec<String> = preview.duplicate_files.iter().map(|d| d.entry.path.clone()).collect();
    for _ in 0..2 {
        apply(&mut w.conn, DiskSelection { declined_copies: declined.clone(), ..Default::default() }).unwrap();
    }
    assert_eq!(w.known().len(), 1);
}

#[test]
fn an_unreachable_root_forgets_nothing() {
    let mut w = World::new();
    let copy = w.write("copy.obj", OBJ_A);
    w.decline_all_copies();
    std::fs::remove_file(copy).unwrap();
    std::fs::rename(&w.root, w.base.join("models-away")).unwrap();

    let result = scan(&w.conn).unwrap();

    assert!(!result.unreachable_roots.is_empty());
    assert_eq!(w.known().len(), 1, "an unplugged drive must not look like deleted copies");
}

// Measurements, not checks. Run by hand with
//   cargo test -p mf-katalog-manager --lib --release -- --ignored --nocapture measure_
// and set TMPDIR to a folder on the disk the numbers should describe.
fn env_number(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
#[ignore]
fn measure_hashing_with_and_without_remembered_copies() {
    let copies = env_number("KC_COPIES", 200);
    let megabytes = env_number("KC_MEGABYTES", 20);
    let mut w = World::new();
    // One cataloged original of the size under test, then the copies as real files.
    let body: Vec<u8> = (0..megabytes * 1024 * 1024).map(|i| (i % 251) as u8).collect();
    let original = w.root.join("big-original.stl");
    std::fs::write(&original, &body).unwrap();
    let hash = files::compute_content_hash(&original).unwrap();
    let folder_id: i64 = w.conn.query_row("SELECT id FROM folders LIMIT 1", [], |r| r.get(0)).unwrap();
    let id = db::test_insert_minimal_file(&w.conn, original.to_str().unwrap(), Some(folder_id)).unwrap();
    w.conn.execute("UPDATE files SET content_hash = ?1 WHERE id = ?2", rusqlite::params![hash, id]).unwrap();
    for i in 0..copies {
        std::fs::write(w.root.join(format!("copy-{i:03}.stl")), &body).unwrap();
    }
    let timed = |w: &World| {
        let start = std::time::Instant::now();
        let (scan, hashed) = w.counted_scan(false);
        (start.elapsed(), scan, hashed)
    };
    let (open_time, open_scan, open_hashed) = timed(&w);
    assert_eq!(open_scan.duplicate_files.len(), copies);
    let declined: Vec<String> = open_scan.duplicate_files.iter().map(|d| d.entry.path.clone()).collect();
    apply(&mut w.conn, DiskSelection { declined_copies: declined, ..Default::default() }).unwrap();
    let (kept_time, kept_scan, kept_hashed) = timed(&w);
    eprintln!("MEASURE copies={copies} size={megabytes} MiB total={} MiB", copies * megabytes);
    eprintln!("MEASURE not remembered: {open_time:?}, files hashed {open_hashed}, offered {}", open_scan.duplicate_files.len());
    eprintln!("MEASURE remembered:     {kept_time:?}, files hashed {kept_hashed}, offered {}, skipped {}", kept_scan.duplicate_files.len(), kept_scan.known_copy_count);
    assert_eq!(kept_hashed, 0);
}

#[test]
#[ignore]
fn measure_scan_of_a_settled_catalog_without_remembered_copies() {
    let files_count = env_number("KC_FILES", 5000);
    let mut w = World::new();
    for i in 0..files_count {
        let ext = ["obj", "obj", "obj"][i % 3];
        w.write(&format!("dir{}/model-{i:05}.{ext}", i % 50), &format!("v 0 0 0\nv {i} 0 0\nv 0 1 0\nf 1 2 3\n"));
    }
    let preview = scan(&w.conn).unwrap();
    let new_paths = preview.new_folders.iter().chain(&preview.new_files).map(|e| e.path.clone()).collect();
    let start = std::time::Instant::now();
    apply(&mut w.conn, DiskSelection { new_paths, ..Default::default() }).unwrap();
    eprintln!("MEASURE cataloguing {files_count} files took {:?}", start.elapsed());
    let mut scans = Vec::new();
    let mut lookups = Vec::new();
    for _ in 0..7 {
        let start = std::time::Instant::now();
        let result = scan(&w.conn).unwrap();
        scans.push(start.elapsed());
        assert!(result.new_files.is_empty());
        // The extra work of this feature on a catalog without remembered copies: the table is empty.
        let mut again = DiskScan::default();
        let start = std::time::Instant::now();
        skip_known_copies(&w.conn, &mut again, false).unwrap();
        lookups.push(start.elapsed());
    }
    scans.sort();
    lookups.sort();
    eprintln!("MEASURE scan of {files_count} unchanged files, median of 7: {:?} (min {:?}, max {:?})", scans[3], scans[0], scans[6]);
    eprintln!("MEASURE the added table check, median of 7: {:?} (max {:?}) = {:.4} % of the scan", lookups[3], lookups[6], lookups[3].as_secs_f64() / scans[3].as_secs_f64() * 100.0);
}
