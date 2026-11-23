//! Source folders: what may be added, what a scan reports, what a decline remembers, and
//! that import selection requires a catalog folder.
use super::*;

struct Fixture {
    base: PathBuf,
    catalog: PathBuf,
    db: Mutex<Connection>,
    sensitive_dirs: Vec<PathBuf>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "sources-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        let catalog = base.join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::insert_folder_with_parent(&conn, "catalog", None, &catalog.to_string_lossy()).unwrap();
        db::printer_link::set_setting(
            &conn,
            crate::import::file_names::SETTING_CATALOG_BASE_DIR,
            &catalog.to_string_lossy(),
        )
        .unwrap();
        Self {
            base,
            catalog,
            db: Mutex::new(conn),
            sensitive_dirs: Vec::new(),
        }
    }
    fn state(&self) -> SourceState<'_> {
        SourceState {
            db: &self.db,
            sensitive_dirs: &self.sensitive_dirs,
            fs: &StdSourceFs,
        }
    }

    fn dir(&self, relative: &str) -> PathBuf {
        let path = self.base.join(relative);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap()
    }
    fn add(&self, dir: &Path) -> CmdResult<SourceFolderDto> {
        add_source_folder_with_conn(&StdSourceFs, &self.conn(), dir, &self.sensitive_dirs)
    }
    fn scan(&self) -> Vec<SourceScanDto> {
        scan_with_state(&self.state(), MAX_SCAN_ENTRIES).unwrap()
    }
    fn scanned(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .scan()
            .into_iter()
            .flat_map(|s| s.files)
            .map(|f| f.relative_path)
            .collect();
        names.sort();
        names
    }
}

fn stl(seed: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 80];
    bytes.extend_from_slice(&1u32.to_le_bytes());
    let s = seed as f32;
    for value in [
        0.0f32,
        0.0,
        1.0,
        0.0,
        0.0,
        s,
        10.0,
        0.0,
        s,
        0.0,
        10.0,
        s + 1.0,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn write(path: &Path, bytes: &[u8]) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    path.to_path_buf()
}

// ---- adding and removing ----

#[test]
fn a_plain_folder_is_remembered_by_its_resolved_path() {
    let f = Fixture::new();
    let downloads = f.dir("downloads");
    let dto = f.add(&downloads).unwrap();
    assert_eq!(dto.path, downloads.to_string_lossy());
    assert!(dto.reachable);
    assert_eq!(store::list_source_folders(&f.conn()).unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn a_link_to_a_folder_is_stored_resolved_and_counts_as_the_same_folder() {
    let f = Fixture::new();
    let real = f.dir("real");
    let link = f.base.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert_eq!(f.add(&link).unwrap().path, real.to_string_lossy());
    assert!(
        f.add(&real).is_err(),
        "the same folder under another name is a duplicate"
    );
}

#[test]
fn unfit_folders_are_refused() {
    let f = Fixture::new();
    let file = write(&f.base.join("a-file.txt"), b"x");
    assert!(f.add(&f.base.join("missing")).is_err(), "missing");
    assert!(f.add(&file).is_err(), "a file");
    assert!(f.add(Path::new("relative/dir")).is_err(), "relative");
    let dotdot = f.dir("one").join("..").join("one");
    assert!(f.add(&dotdot).is_err(), "with ..");
    assert!(
        f.add(&f.dir("ok")).is_ok(),
        "counter-check: a normal folder is fine"
    );
}

#[test]
fn the_catalog_folder_and_everything_around_it_is_refused() {
    let f = Fixture::new();
    let inside = f.dir("catalog/sub");
    assert!(f.add(&f.catalog).is_err(), "the catalog folder itself");
    assert!(f.add(&inside).is_err(), "a folder inside it");
    assert!(f.add(&f.base).is_err(), "a folder that contains it");
    assert!(f.add(&f.dir("elsewhere")).is_ok(), "counter-check");
}

#[test]
fn a_catalog_tree_kept_in_place_is_refused_too() {
    let f = Fixture::new();
    let in_place = f.dir("in-place");
    db::insert_folder_with_parent(&f.conn(), "in-place", None, &in_place.to_string_lossy())
        .unwrap();
    assert!(f.add(&in_place).is_err());
    assert!(f.add(&in_place.join("deeper")).is_err());
}

#[test]
fn protected_system_folders_are_refused() {
    let mut f = Fixture::new();
    let secret = f.dir("home/.ssh");
    let home = secret.parent().unwrap().to_path_buf();
    f.sensitive_dirs = vec![secret.clone()];
    assert!(f.add(&secret).is_err(), "a protected folder");
    assert!(f.add(&secret.join("nested")).is_err() || !secret.join("nested").exists());
    assert!(
        f.add(&home).is_err(),
        "a folder that contains a protected one"
    );
    assert!(f.add(&f.dir("fine")).is_ok(), "counter-check");
}

#[test]
fn source_folders_may_not_overlap() {
    let f = Fixture::new();
    let outer = f.dir("outer");
    let inner = f.dir("outer/inner");
    f.add(&outer).unwrap();
    assert!(f.add(&outer).is_err(), "duplicate");
    assert!(f.add(&inner).is_err(), "child of a source folder");
    let f2 = Fixture::new();
    f2.add(&f2.dir("outer/inner")).unwrap();
    assert!(
        f2.add(&f2.dir("outer")).is_err(),
        "parent of a source folder"
    );
}

#[test]
fn removing_forgets_the_folder_and_its_declined_files_but_never_touches_files() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let file = write(&dir.join("a.stl"), &stl(1));
    let id: i64 = f.add(&dir).unwrap().id.parse().unwrap();
    reject_with_state(
        &f.state(),
        &[file.to_string_lossy().into_owned()],
        MAX_SCAN_ENTRIES,
    )
    .unwrap();
    assert_eq!(store::count_rejected_sources(&f.conn()).unwrap(), 1);
    remove_source_folder_with_conn(&f.conn(), id).unwrap();
    assert!(store::list_source_folders(&f.conn()).unwrap().is_empty());
    assert_eq!(store::count_rejected_sources(&f.conn()).unwrap(), 0);
    assert!(file.is_file(), "the files stay");
    assert!(
        remove_source_folder_with_conn(&f.conn(), id).is_err(),
        "not twice"
    );
}

#[test]
fn the_start_check_setting_is_off_by_default() {
    let f = Fixture::new();
    assert!(!store::source_check_on_start(&f.conn()).unwrap());
    store::set_source_check_on_start(&f.conn(), true).unwrap();
    assert!(store::source_check_on_start(&f.conn()).unwrap());
    store::set_source_check_on_start(&f.conn(), false).unwrap();
    assert!(!store::source_check_on_start(&f.conn()).unwrap());
}

// ---- scanning ----

#[test]
fn a_scan_lists_new_model_files_only() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    write(&dir.join("a.stl"), &stl(1));
    write(&dir.join("sub/b.3mf"), b"x");
    write(&dir.join("notes.txt"), b"x");
    write(&dir.join("photo.png"), b"x");
    write(&dir.join(".hidden.stl"), &stl(2));
    write(&dir.join("download.stl.part"), &stl(3));
    write(&dir.join("~$temp.stl"), &stl(4));
    f.add(&dir).unwrap();
    assert_eq!(f.scanned(), vec!["a.stl", "sub/b.3mf"]);
    let scan = &f.scan()[0];
    assert!(!scan.unreachable && !scan.truncated);
    assert_eq!(scan.files[0].size_bytes, stl(1).len() as i64);
}

#[test]
fn a_scan_reads_only_and_changes_nothing() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    write(&dir.join("a.stl"), &stl(1));
    f.add(&dir).unwrap();
    let before = std::fs::read(dir.join("a.stl")).unwrap();
    let modified = std::fs::metadata(dir.join("a.stl"))
        .unwrap()
        .modified()
        .unwrap();
    f.scan();
    f.scan();
    assert_eq!(std::fs::read(dir.join("a.stl")).unwrap(), before);
    assert_eq!(
        std::fs::metadata(dir.join("a.stl"))
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    assert!(
        db::list_files(&f.conn()).unwrap().is_empty(),
        "nothing entered the catalog"
    );
}

#[test]
fn files_already_in_the_catalog_are_left_out() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let known = write(&dir.join("known.stl"), &stl(1));
    write(&dir.join("fresh.stl"), &stl(2));
    f.add(&dir).unwrap();
    assert_eq!(
        f.scanned(),
        vec!["fresh.stl", "known.stl"],
        "before: both are new"
    );
    db::test_insert_minimal_file(&f.conn(), &known.to_string_lossy(), None).unwrap();
    assert_eq!(f.scanned(), vec!["fresh.stl"]);
}

#[test]
fn only_three_levels_below_the_folder_are_looked_at() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    write(&dir.join("l0.stl"), &stl(1));
    write(&dir.join("a/l1.stl"), &stl(2));
    write(&dir.join("a/b/l2.stl"), &stl(3));
    write(&dir.join("a/b/c/l3.stl"), &stl(4));
    write(&dir.join("a/b/c/d/l4.stl"), &stl(5));
    f.add(&dir).unwrap();
    assert_eq!(
        f.scanned(),
        vec!["a/b/c/l3.stl", "a/b/l2.stl", "a/l1.stl", "l0.stl"]
    );
}

#[test]
fn the_entry_limit_stops_the_scan_and_says_so() {
    let f = Fixture::new();
    let dir = f.dir("limit");
    for n in 0..20 {
        write(&dir.join(format!("m{n:02}.stl")), &stl(n));
    }
    let walked = walk_source_folder(&StdSourceFs, &dir, MAX_SCAN_DEPTH, 8);
    assert!(walked.truncated);
    assert_eq!(walked.files.len(), 8);
    let all = walk_source_folder(&StdSourceFs, &dir, MAX_SCAN_DEPTH, 1000);
    assert!(
        !all.truncated,
        "counter-check: under the limit nothing is cut"
    );
    assert_eq!(all.files.len(), 20);
}

#[cfg(unix)]
#[test]
fn links_are_never_followed_and_are_counted() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let outside = f.dir("outside");
    write(&outside.join("secret.stl"), &stl(1));
    write(&dir.join("real.stl"), &stl(2));
    std::os::unix::fs::symlink(outside.join("secret.stl"), dir.join("link.stl")).unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("linked-dir")).unwrap();
    f.add(&dir).unwrap();
    assert_eq!(f.scanned(), vec!["real.stl"]);
    assert_eq!(
        f.scan()[0].skipped,
        1,
        "the model-looking link is counted; the folder link is simply not entered"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_folders_are_skipped_and_counted() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return; // root reads everything
    }
    let f = Fixture::new();
    let dir = f.dir("downloads");
    write(&dir.join("ok.stl"), &stl(1));
    let locked = f.dir("downloads/locked");
    write(&locked.join("hidden.stl"), &stl(2));
    f.add(&dir).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let scan = f.scan();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(scan[0].files.len(), 1);
    assert_eq!(scan[0].skipped, 1);
}

#[test]
fn a_folder_that_vanished_is_reported_unreachable() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    write(&dir.join("a.stl"), &stl(1));
    let dto = f.add(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let scan = f.scan();
    assert!(scan[0].unreachable && scan[0].files.is_empty());
    assert_eq!(scan[0].folder_id, dto.id);
}

#[cfg(unix)]
#[test]
fn a_folder_replaced_by_a_link_is_not_scanned() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    f.add(&dir).unwrap();
    let elsewhere = f.dir("elsewhere");
    write(&elsewhere.join("other.stl"), &stl(1));
    std::fs::remove_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &dir).unwrap();
    let scan = f.scan();
    assert!(
        scan[0].unreachable && scan[0].files.is_empty(),
        "it now leads somewhere else"
    );
}

#[test]
fn a_folder_that_became_protected_is_not_scanned() {
    let mut f = Fixture::new();
    let dir = f.dir("downloads");
    write(&dir.join("a.stl"), &stl(1));
    f.add(&dir).unwrap();
    assert_eq!(f.scanned().len(), 1);
    f.sensitive_dirs = vec![dir.clone()];
    assert!(f.scan()[0].unreachable);
}

// ---- duplicates ----

#[test]
fn a_new_file_with_the_content_of_a_catalog_model_is_marked_as_its_duplicate() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let known_path = write(&f.catalog.join("Known.stl"), &stl(1));
    let id = db::test_insert_minimal_file(&f.conn(), &known_path.to_string_lossy(), None).unwrap();
    let hash = files::compute_content_hash(&known_path).unwrap();
    f.conn()
        .execute(
            "UPDATE files SET content_hash = ?1, file_size_bytes = ?2 WHERE id = ?3",
            rusqlite::params![hash, stl(1).len() as i64, id],
        )
        .unwrap();
    write(&dir.join("copy.stl"), &stl(1));
    let mut other = stl(2);
    assert_eq!(other.len(), stl(1).len());
    other[100] ^= 1;
    write(&dir.join("different.stl"), &other);
    f.add(&dir).unwrap();
    let scan = f.scan();
    let by_name = |name: &str| {
        scan[0]
            .files
            .iter()
            .find(|file| file.relative_path == name)
            .unwrap()
    };
    assert_eq!(by_name("copy.stl").duplicate_of_id, Some(id.to_string()));
    assert_eq!(
        by_name("different.stl").duplicate_of_id,
        None,
        "same size, other content"
    );
    f.conn()
        .execute("UPDATE files SET content_hash = NULL", [])
        .unwrap();
    assert_eq!(
        f.scan()[0]
            .files
            .iter()
            .find(|file| file.relative_path == "copy.stl")
            .unwrap()
            .duplicate_of_id,
        None,
        "no stored hash, no claim"
    );
}

// ---- declining ----

#[test]
fn a_declined_file_is_hidden_until_its_size_changes_or_the_list_is_reset() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let file = write(&dir.join("a.stl"), &stl(1));
    f.add(&dir).unwrap();
    let path = file.to_string_lossy().into_owned();
    assert_eq!(
        reject_with_state(&f.state(), std::slice::from_ref(&path), MAX_SCAN_ENTRIES).unwrap(),
        1
    );
    let rows = store::list_rejected_sources(&f.conn()).unwrap();
    assert_eq!(
        (rows[0].path.as_str(), rows[0].size_bytes),
        (path.as_str(), stl(1).len() as i64),
        "path and size are remembered"
    );
    assert!(f.scanned().is_empty(), "hidden");

    write(&file, &[stl(1), vec![0u8; 50]].concat());
    assert_eq!(
        f.scanned(),
        vec!["a.stl"],
        "a changed file is offered again"
    );
    std::fs::write(&file, stl(1)).unwrap();
    assert!(f.scanned().is_empty(), "same size again: still declined");

    assert_eq!(store::clear_rejected_sources(&f.conn()).unwrap(), 1);
    assert_eq!(store::count_rejected_sources(&f.conn()).unwrap(), 0);
    assert_eq!(
        f.scanned(),
        vec!["a.stl"],
        "after the reset it is offered again"
    );
}

#[test]
fn only_files_of_a_fresh_scan_can_be_declined_or_imported() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let listed = write(&dir.join("a.stl"), &stl(1));
    let unlisted = write(&f.dir("elsewhere").join("b.stl"), &stl(2));
    let known = write(&dir.join("known.stl"), &stl(3));
    db::test_insert_minimal_file(&f.conn(), &known.to_string_lossy(), None).unwrap();
    f.add(&dir).unwrap();
    let text = |p: &Path| p.to_string_lossy().into_owned();

    assert!(
        reject_with_state(&f.state(), &[text(&unlisted)], MAX_SCAN_ENTRIES).is_err(),
        "outside every source folder"
    );
    assert!(
        reject_with_state(&f.state(), &[text(&known)], MAX_SCAN_ENTRIES).is_err(),
        "already in the catalog"
    );
    assert!(
        reject_with_state(
            &f.state(),
            &[text(&listed), "/etc/passwd".into()],
            MAX_SCAN_ENTRIES
        )
        .is_err(),
        "one bad path refuses the whole call"
    );
    assert_eq!(
        store::count_rejected_sources(&f.conn()).unwrap(),
        0,
        "nothing was stored"
    );
    assert!(accepted_import_paths(&f.state(), &[text(&unlisted)], MAX_SCAN_ENTRIES).is_err());
    assert!(
        accepted_import_paths(&f.state(), &[], MAX_SCAN_ENTRIES).is_err(),
        "nothing chosen"
    );
    assert_eq!(
        accepted_import_paths(
            &f.state(),
            &[text(&listed), text(&listed)],
            MAX_SCAN_ENTRIES
        )
        .unwrap(),
        vec![listed.clone()],
        "counter-check, duplicates collapse"
    );
    assert!(unlisted.exists() && known.exists());
}

#[test]
fn an_import_selection_needs_a_catalog_folder() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let file = write(&dir.join("a.stl"), &stl(1));
    f.add(&dir).unwrap();
    f.conn()
        .execute(
            "DELETE FROM app_settings WHERE key = ?1",
            [crate::import::file_names::SETTING_CATALOG_BASE_DIR],
        )
        .unwrap();
    assert!(accepted_import_paths(
        &f.state(),
        &[file.to_string_lossy().into_owned()],
        MAX_SCAN_ENTRIES
    )
    .is_err());
    assert!(file.is_file());
}

fn accepted_import_paths(
    state: &SourceState<'_>,
    requested: &[String],
    max_entries: usize,
) -> CmdResult<Vec<PathBuf>> {
    accepted_import_selection(state, requested, max_entries).map(|(paths, _)| paths)
}

/// No corresponding files exist: every read must go through the supplied adapter.
struct VirtualFs<'a> {
    root: PathBuf,
    db: &'a Mutex<Connection>,
}
impl SourceFs for VirtualFs<'_> {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        Ok(path.to_path_buf())
    }
    fn stat(&self, path: &Path, _: bool) -> io::Result<SourceMetadata> {
        if path == self.root {
            Ok(SourceMetadata {
                kind: SourceKind::Directory,
                len: 0,
            })
        } else if path == self.root.join("copy.stl") {
            Ok(SourceMetadata {
                kind: SourceKind::File,
                len: 3,
            })
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }
    fn list(&self, path: &Path) -> io::Result<SourceEntries<'_>> {
        assert!(
            self.db.try_lock().is_ok(),
            "the scan must release the database lock"
        );
        assert_eq!(path, self.root);
        Ok(Box::new(std::iter::once(Ok(SourceEntry {
            path: self.root.join("copy.stl"),
            kind: Ok(SourceKind::File),
        }))))
    }
    fn content_hash(&self, path: &Path) -> CmdResult<String> {
        assert!(self.db.try_lock().is_ok());
        assert_eq!(path, self.root.join("copy.stl"));
        files::compute_reader_hash(&b"abc"[..])
    }
}

#[test]
fn virtual_sources_use_the_adapter_for_resolution_listing_metadata_and_hashing() {
    let f = Fixture::new();
    let root = f.base.join("virtual");
    assert!(!root.exists());
    let fs = VirtualFs {
        root: root.clone(),
        db: &f.db,
    };
    let state = SourceState {
        db: &f.db,
        sensitive_dirs: &[],
        fs: &fs,
    };
    {
        let conn = f.conn();
        let id = db::test_insert_minimal_file(
            &conn,
            &f.catalog.join("known.stl").to_string_lossy(),
            None,
        )
        .unwrap();
        conn.execute(
            "UPDATE files SET file_size_bytes = 3, content_hash = ?1 WHERE id = ?2",
            rusqlite::params![files::compute_reader_hash(&b"abc"[..]).unwrap(), id],
        )
        .unwrap();
        add_source_folder_with_conn(&fs, &conn, &root, &[]).unwrap();
    }
    let scans = scan_with_state(&state, MAX_SCAN_ENTRIES).unwrap();
    assert_eq!(scans[0].files.len(), 1);
    assert!(scans[0].files[0].duplicate_of_id.is_some());
    let requested = vec![root.join("copy.stl").to_string_lossy().into_owned()];
    let (paths, roots) = accepted_import_selection(&state, &requested, MAX_SCAN_ENTRIES).unwrap();
    assert_eq!(paths, vec![root.join("copy.stl")]);
    assert_eq!(roots, vec![root.clone(), root.clone()]);
    assert_eq!(
        reject_with_state(&state, &requested, MAX_SCAN_ENTRIES).unwrap(),
        1
    );
    assert!(scan_with_state(&state, MAX_SCAN_ENTRIES).unwrap()[0]
        .files
        .is_empty());
    assert!(!root.exists());
}

#[test]
fn entry_limit_boundary_and_zero_match_desktop_walk() {
    let f = Fixture::new();
    let root = f.dir("limit");
    write(&root.join("a.stl"), b"a");
    write(&root.join("b.stl"), b"b");
    let zero = walk_source_folder(&StdSourceFs, &root, MAX_SCAN_DEPTH, 0);
    assert!(zero.truncated);
    assert!(zero.files.is_empty());
    let exact = walk_source_folder(&StdSourceFs, &root, MAX_SCAN_DEPTH, 2);
    assert!(!exact.truncated);
    assert_eq!(exact.files.len(), 2);
}

#[test]
fn invalid_selections_and_duplicate_folders_keep_existing_error_codes() {
    let f = Fixture::new();
    let root = f.dir("downloads");
    f.add(&root).unwrap();
    assert_eq!(
        f.add(&root).expect_err("duplicate").message,
        ErrorCode::SourceFolderDuplicate.as_str()
    );
    assert_eq!(
        accepted_import_selection(&f.state(), &[], MAX_SCAN_ENTRIES)
            .expect_err("empty selection")
            .message,
        ErrorCode::SourceNoFilesSelected.as_str()
    );
    assert_eq!(
        reject_with_state(&f.state(), &["missing.stl".into()], MAX_SCAN_ENTRIES)
            .expect_err("unlisted file")
            .message,
        ErrorCode::SourceFileNotNew.as_str()
    );
}

struct ChangedEntries { errors: bool }
impl SourceFs for ChangedEntries {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> { Ok(path.into()) }
    fn stat(&self, _: &Path, _: bool) -> io::Result<SourceMetadata> {
        Ok(SourceMetadata { kind: SourceKind::Other, len: 0 })
    }
    fn list(&self, root: &Path) -> io::Result<SourceEntries<'_>> {
        if self.errors {
            Ok(Box::new(std::iter::repeat_with(|| Err(io::ErrorKind::InvalidData.into()))))
        } else {
            Ok(Box::new(std::iter::once(Ok(SourceEntry {
                path: root.join("changed.stl"), kind: Ok(SourceKind::File),
            }))))
        }
    }
    fn content_hash(&self, _: &Path) -> CmdResult<String> { panic!("must not hash") }
}

#[test]
fn unreadable_entries_consume_the_scan_budget() {
    let walked = walk_source_folder(&ChangedEntries { errors: true }, Path::new("/virtual"), 3, 4);
    assert_eq!(walked.skipped, 4);
    assert!(walked.truncated);
}

#[test]
fn a_file_replaced_by_a_special_entry_is_not_offered() {
    let walked = walk_source_folder(&ChangedEntries { errors: false }, Path::new("/virtual"), 3, 4);
    assert!(walked.files.is_empty());
    assert_eq!(walked.skipped, 1);
}
