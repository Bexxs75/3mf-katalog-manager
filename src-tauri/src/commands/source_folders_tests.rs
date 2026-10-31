//! Source folders: what may be added, what a scan reports, what a decline remembers, and
//! that an import takes the same way as a normal import with a catalog folder.
use super::*;

struct Fixture {
    base: PathBuf,
    catalog: PathBuf,
    state: AppState,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!(
            "sources-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let catalog = base.join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        let conn = db::connect_in_memory().unwrap();
        db::insert_folder_with_parent(&conn, "catalog", None, &catalog.to_string_lossy()).unwrap();
        db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, &catalog.to_string_lossy()).unwrap();
        let state = AppState {
            import_jobs: ImportJobs::default(),
            db: Mutex::new(conn),
            trash_dir: base.join("trash"),
            db_path: base.join("catalog.db"),
            sensitive_dirs: Vec::new(),
        };
        Self { base, catalog, state }
    }
    fn dir(&self, relative: &str) -> PathBuf {
        let path = self.base.join(relative);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.state.db.lock().unwrap()
    }
    fn add(&self, dir: &Path) -> CmdResult<SourceFolderDto> {
        add_source_folder_with_conn(&self.conn(), dir, &self.state.sensitive_dirs)
    }
    fn scan(&self) -> Vec<SourceScanDto> {
        scan_with_state(&self.state, MAX_SCAN_ENTRIES).unwrap()
    }
    fn scanned(&self) -> Vec<String> {
        let mut names: Vec<String> = self.scan().into_iter().flat_map(|s| s.files).map(|f| f.relative_path).collect();
        names.sort();
        names
    }
}

fn stl(seed: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 80];
    bytes.extend_from_slice(&1u32.to_le_bytes());
    let s = seed as f32;
    for value in [0.0f32, 0.0, 1.0, 0.0, 0.0, s, 10.0, 0.0, s, 0.0, 10.0, s + 1.0] {
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
    assert!(f.add(&real).is_err(), "the same folder under another name is a duplicate");
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
    assert!(f.add(&f.dir("ok")).is_ok(), "counter-check: a normal folder is fine");
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
    db::insert_folder_with_parent(&f.conn(), "in-place", None, &in_place.to_string_lossy()).unwrap();
    assert!(f.add(&in_place).is_err());
    assert!(f.add(&in_place.join("deeper")).is_err());
}

#[test]
fn protected_system_folders_are_refused() {
    let mut f = Fixture::new();
    let secret = f.dir("home/.ssh");
    let home = secret.parent().unwrap().to_path_buf();
    f.state.sensitive_dirs = vec![secret.clone()];
    assert!(f.add(&secret).is_err(), "a protected folder");
    assert!(f.add(&secret.join("nested")).is_err() || !secret.join("nested").exists());
    assert!(f.add(&home).is_err(), "a folder that contains a protected one");
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
    assert!(f2.add(&f2.dir("outer")).is_err(), "parent of a source folder");
}

#[test]
fn removing_forgets_the_folder_and_its_declined_files_but_never_touches_files() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let file = write(&dir.join("a.stl"), &stl(1));
    let id: i64 = f.add(&dir).unwrap().id.parse().unwrap();
    reject_with_state(&f.state, &[file.to_string_lossy().into_owned()], MAX_SCAN_ENTRIES).unwrap();
    assert_eq!(store::count_rejected_sources(&f.conn()).unwrap(), 1);
    remove_source_folder_with_conn(&f.conn(), id).unwrap();
    assert!(store::list_source_folders(&f.conn()).unwrap().is_empty());
    assert_eq!(store::count_rejected_sources(&f.conn()).unwrap(), 0);
    assert!(file.is_file(), "the files stay");
    assert!(remove_source_folder_with_conn(&f.conn(), id).is_err(), "not twice");
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
    let modified = std::fs::metadata(dir.join("a.stl")).unwrap().modified().unwrap();
    f.scan();
    f.scan();
    assert_eq!(std::fs::read(dir.join("a.stl")).unwrap(), before);
    assert_eq!(std::fs::metadata(dir.join("a.stl")).unwrap().modified().unwrap(), modified);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    assert!(db::list_files(&f.conn()).unwrap().is_empty(), "nothing entered the catalog");
}

#[test]
fn files_already_in_the_catalog_are_left_out() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let known = write(&dir.join("known.stl"), &stl(1));
    write(&dir.join("fresh.stl"), &stl(2));
    f.add(&dir).unwrap();
    assert_eq!(f.scanned(), vec!["fresh.stl", "known.stl"], "before: both are new");
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
    assert_eq!(f.scanned(), vec!["a/b/c/l3.stl", "a/b/l2.stl", "a/l1.stl", "l0.stl"]);
}

#[test]
fn the_entry_limit_stops_the_scan_and_says_so() {
    let dir = Fixture::new().dir("limit");
    for n in 0..20 {
        write(&dir.join(format!("m{n:02}.stl")), &stl(n));
    }
    let walked = walk_source_folder(&dir, MAX_SCAN_DEPTH, 8);
    assert!(walked.truncated);
    assert_eq!(walked.files.len(), 8);
    let all = walk_source_folder(&dir, MAX_SCAN_DEPTH, 1000);
    assert!(!all.truncated, "counter-check: under the limit nothing is cut");
    assert_eq!(all.files.len(), 20);
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
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
    assert_eq!(f.scan()[0].skipped, 1, "the model-looking link is counted; the folder link is simply not entered");
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
    assert!(scan[0].unreachable && scan[0].files.is_empty(), "it now leads somewhere else");
}

#[test]
fn a_folder_that_became_protected_is_not_scanned() {
    let mut f = Fixture::new();
    let dir = f.dir("downloads");
    write(&dir.join("a.stl"), &stl(1));
    f.add(&dir).unwrap();
    assert_eq!(f.scanned().len(), 1);
    f.state.sensitive_dirs = vec![dir.clone()];
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
    let by_name = |name: &str| scan[0].files.iter().find(|file| file.relative_path == name).unwrap();
    assert_eq!(by_name("copy.stl").duplicate_of_id, Some(id.to_string()));
    assert_eq!(by_name("different.stl").duplicate_of_id, None, "same size, other content");
    f.conn().execute("UPDATE files SET content_hash = NULL", []).unwrap();
    assert_eq!(f.scan()[0].files.iter().find(|file| file.relative_path == "copy.stl").unwrap().duplicate_of_id, None, "no stored hash, no claim");
}

// ---- declining ----

#[test]
fn a_declined_file_is_hidden_until_its_size_changes_or_the_list_is_reset() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let file = write(&dir.join("a.stl"), &stl(1));
    f.add(&dir).unwrap();
    let path = file.to_string_lossy().into_owned();
    assert_eq!(reject_with_state(&f.state, std::slice::from_ref(&path), MAX_SCAN_ENTRIES).unwrap(), 1);
    let rows = store::list_rejected_sources(&f.conn()).unwrap();
    assert_eq!((rows[0].path.as_str(), rows[0].size_bytes), (path.as_str(), stl(1).len() as i64), "path and size are remembered");
    assert!(f.scanned().is_empty(), "hidden");

    write(&file, &[stl(1), vec![0u8; 50]].concat());
    assert_eq!(f.scanned(), vec!["a.stl"], "a changed file is offered again");
    std::fs::write(&file, stl(1)).unwrap();
    assert!(f.scanned().is_empty(), "same size again: still declined");

    assert_eq!(store::clear_rejected_sources(&f.conn()).unwrap(), 1);
    assert_eq!(store::count_rejected_sources(&f.conn()).unwrap(), 0);
    assert_eq!(f.scanned(), vec!["a.stl"], "after the reset it is offered again");
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

    assert!(reject_with_state(&f.state, &[text(&unlisted)], MAX_SCAN_ENTRIES).is_err(), "outside every source folder");
    assert!(reject_with_state(&f.state, &[text(&known)], MAX_SCAN_ENTRIES).is_err(), "already in the catalog");
    assert!(reject_with_state(&f.state, &[text(&listed), "/etc/passwd".into()], MAX_SCAN_ENTRIES).is_err(), "one bad path refuses the whole call");
    assert_eq!(store::count_rejected_sources(&f.conn()).unwrap(), 0, "nothing was stored");
    assert!(accepted_import_paths(&f.state, &[text(&unlisted)], MAX_SCAN_ENTRIES).is_err());
    assert!(accepted_import_paths(&f.state, &[], MAX_SCAN_ENTRIES).is_err(), "nothing chosen");
    assert_eq!(accepted_import_paths(&f.state, &[text(&listed), text(&listed)], MAX_SCAN_ENTRIES).unwrap(), vec![listed.clone()], "counter-check, duplicates collapse");
    assert!(unlisted.exists() && known.exists());
}

#[test]
fn an_import_needs_a_catalog_folder_so_files_are_always_moved() {
    let f = Fixture::new();
    let dir = f.dir("downloads");
    let file = write(&dir.join("a.stl"), &stl(1));
    f.add(&dir).unwrap();
    f.conn().execute("DELETE FROM app_settings WHERE key = ?1", [folders::SETTING_CATALOG_BASE_DIR]).unwrap();
    assert!(accepted_import_paths(&f.state, &[file.to_string_lossy().into_owned()], MAX_SCAN_ENTRIES).is_err());
    assert!(file.is_file());
}

// ---- the import job itself (run with the worker, as the queue does) ----

/// A temporary folder with STL models, for the tests that run the import worker.
struct JobFixture(PathBuf);
impl JobFixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "source_import_job_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn stl(&self, seed: u32) -> PathBuf {
        write(&self.0.join(format!("model_{seed}.stl")), &stl(seed))
    }
}
impl Drop for JobFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn finish_models(service: &ImportJobs, job: &ImportJob, conn: &mut Connection, paths: &[PathBuf]) {
    let error = run_models(service, job, &mut &mut *conn, paths, &[]).err().map(|error| ImportJobError {
        kind: "database".into(),
        message: error.message,
    });
    service.finish(job, error);
}

#[test]
fn source_folder_files_are_moved_into_the_catalog_folder_like_a_normal_import() {
    let fixture = JobFixture::new();
    let catalog = fixture.0.join("catalog");
    std::fs::create_dir(&catalog).unwrap();
    let downloads = fixture.0.join("downloads");
    std::fs::create_dir_all(downloads.join("sub")).unwrap();
    let a = fixture.stl(1);
    let b = fixture.stl(2);
    let left = fixture.stl(3);
    for (file, target) in [(&a, "a.stl"), (&b, "sub/b.stl"), (&left, "left.stl")] {
        std::fs::rename(file, downloads.join(target)).unwrap();
    }
    let (a, b, left) = (downloads.join("a.stl"), downloads.join("sub/b.stl"), downloads.join("left.stl"));

    let conn = db::connect_in_memory().unwrap();
    let folder = db::insert_folder_with_parent(&conn, "catalog", None, catalog.to_str().unwrap()).unwrap();
    db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, catalog.to_str().unwrap()).unwrap();
    let state = AppState {
        import_jobs: ImportJobs::default(),
        db: Mutex::new(conn),
        trash_dir: fixture.0.join("trash"),
        db_path: fixture.0.join("catalog.db"),
        sensitive_dirs: Vec::new(),
    };
    add_source_folder_with_conn(&state.db.lock().unwrap(), &downloads, &[]).unwrap();
    let chosen = vec![a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()];
    let paths = accepted_import_paths(&state, &chosen, 5000).unwrap();
    let originals: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();

    // What `import_source_files` hands to the queue: the same job a normal import with a
    // catalog folder runs, placing the files into that folder.
    let service = ImportJobs::default();
    let job = service.enqueue(ImportSource::Files, Some(folder.to_string()), true, None).unwrap();
    service.next().unwrap();
    finish_models(&service, &job, &mut state.db.lock().unwrap(), &paths);

    let result = job.result().unwrap();
    assert_eq!(result.counts.imported, 2);
    for (path, original) in paths.iter().zip(&originals) {
        assert!(!path.exists(), "moved out of the source folder: {}", path.display());
        assert_eq!(&std::fs::read(catalog.join(path.file_name().unwrap())).unwrap(), original);
    }
    assert!(left.is_file(), "files that were not chosen stay");
    let stored: Vec<String> = db::list_files(&state.db.lock().unwrap()).unwrap().into_iter().map(|file| file.path).collect();
    assert!(stored.iter().all(|p| Path::new(p).starts_with(&catalog)), "{stored:?}");
    let again = scan_with_state(&state, 5000).unwrap();
    let names: Vec<_> = again[0].files.iter().map(|f| f.relative_path.clone()).collect();
    assert_eq!(names, vec!["left.stl"], "what was taken in is no longer new");
}

#[cfg(unix)]
#[test]
fn a_source_subfolder_swapped_for_a_link_after_the_scan_leaves_the_foreign_file_alone() {
    let fixture = JobFixture::new();
    let catalog = fixture.0.join("catalog");
    let downloads = fixture.0.join("downloads");
    let foreign = fixture.0.join("foreign");
    for dir in [&catalog, &downloads.join("sub"), &foreign] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::rename(fixture.stl(1), downloads.join("sub/b.stl")).unwrap();
    std::fs::copy(downloads.join("sub/b.stl"), foreign.join("b.stl")).unwrap();
    let conn = db::connect_in_memory().unwrap();
    let folder = db::insert_folder_with_parent(&conn, "catalog", None, catalog.to_str().unwrap()).unwrap();
    db::printer_link::set_setting(&conn, folders::SETTING_CATALOG_BASE_DIR, catalog.to_str().unwrap()).unwrap();
    let state = AppState {
        import_jobs: ImportJobs::default(),
        db: Mutex::new(conn),
        trash_dir: fixture.0.join("trash"),
        db_path: fixture.0.join("catalog.db"),
        sensitive_dirs: Vec::new(),
    };
    add_source_folder_with_conn(&state.db.lock().unwrap(), &downloads, &[]).unwrap();
    let chosen = vec![downloads.join("sub/b.stl").to_string_lossy().into_owned()];
    let (paths, roots) = accepted_import_selection(&state, &chosen, 5000).unwrap();

    std::fs::remove_dir_all(downloads.join("sub")).unwrap();
    std::os::unix::fs::symlink(&foreign, downloads.join("sub")).unwrap();

    let service = ImportJobs::default();
    let job = service
        .enqueue_confined(ImportSource::Files, Some(folder.to_string()), true, None, JobInput::Models(paths.clone()), None, roots)
        .unwrap();
    service.next().unwrap();
    finish_models(&service, &job, &mut state.db.lock().unwrap(), &paths);

    let result = job.result().unwrap();
    assert_eq!(result.counts.imported, 0);
    assert_eq!(result.groups.skipped.len(), 1);
    assert!(foreign.join("b.stl").is_file(), "the foreign file stays where it is");
    assert_eq!(std::fs::read_dir(&catalog).unwrap().count(), 0);
    assert!(db::list_files(&state.db.lock().unwrap()).unwrap().is_empty());
}
