//! Disk sync of the catalog: the Tauri commands and thin wrappers around
//! `katalog_core::disk_sync` (issue #20, package P8), which holds the scan and the apply step.
//! The wrappers keep the old names and signatures; disk access goes through the `Fs` boundary
//! (`DiskScanFsAdapter`), the checks that log their own errors stay the desktop's
//! (`DesktopEffects`), so codes, texts and log lines are as before.
use super::*;
use katalog_core::disk_sync::{self as sync_core, DiskScanFs, DiskSyncEffects};
use tauri::Manager;

pub use sync_core::{DiskApplied, DiskScan, DiskSelection, RenameChoice};
use sync_core::{DirectoryEntries, CASE_INSENSITIVE_PATHS};
#[cfg(test)]
use sync_core::{apply_rename, entry, path_key, suppress_case_only_new_files, DiskRename, KeyResolver};

#[derive(Debug, Serialize)]
pub struct DiskSyncError {
    code: &'static str,
    paths: Vec<String>,
}
type SyncResult<T> = Result<T, DiskSyncError>;
impl DiskSyncError {
    fn new(code: &'static str, paths: Vec<String>) -> Self {
        Self { code, paths }
    }
    #[cfg(test)]
    fn path(code: &'static str, path: &Path) -> Self {
        Self::new(code, vec![path.to_string_lossy().into_owned()])
    }
}
impl From<String> for DiskSyncError {
    fn from(_: String) -> Self {
        Self::new("diskSyncFailed", vec![])
    }
}
impl From<CmdError> for DiskSyncError {
    fn from(error: CmdError) -> Self {
        Self::new(
            if error.message == "importActive" {
                "importActive"
            } else {
                "diskSyncFailed"
            },
            vec![],
        )
    }
}
impl From<sync_core::DiskSyncError> for DiskSyncError {
    fn from(error: sync_core::DiskSyncError) -> Self {
        Self::new(error.code, error.paths)
    }
}

/// The desktop file-system boundary as the core's scan file system.
struct DiskScanFsAdapter<'a>(&'a dyn Fs);

impl DiskScanFs for DiskScanFsAdapter<'_> {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.0.canonicalize(path)
    }
    fn metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata> {
        self.0.metadata(path)
    }
    fn symlink_metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata> {
        self.0.symlink_metadata(path)
    }
    fn read_dir_entries(&self, dir: &Path, ignore: &dyn Fn(&Path) -> bool) -> std::io::Result<DirectoryEntries> {
        self.0.read_dir_entries(dir, ignore)
    }
}

/// The desktop's own hash and checks: their failures log themselves (`CmdError`), and the sync
/// keeps exactly those log lines.
struct DesktopEffects;

impl DiskSyncEffects for DesktopEffects {
    fn content_hash(&self, path: &Path) -> Option<String> {
        files::compute_content_hash(path).ok()
    }
    fn check_managed_location(&self, path: &Path, expanded_sensitive: &[PathBuf]) -> Result<(), String> {
        archives::check_managed_location(path, expanded_sensitive).map_err(|e| e.message)
    }
    fn may_relocate_folder(&self, conn: &Connection, path: &Path) -> bool {
        folders::reject_catalog_root_relocation(conn, path).is_ok()
    }
    fn delete_missing_file(&self, conn: &Connection, id: i64) -> Result<(), String> {
        trash::delete_missing_file_with_conn(conn, id).map_err(|e| e.message)
    }
}

#[cfg(test)]
fn no_symlinks(root: &Path, path: &Path) -> std::io::Result<bool> {
    sync_core::no_symlinks(&DiskScanFsAdapter(&RealFs), root, path)
}

fn read_directory(dir: &Path) -> std::io::Result<DirectoryEntries> {
    read_directory_fs(&RealFs, dir)
}

fn read_directory_fs(fs: &dyn Fs, dir: &Path) -> std::io::Result<DirectoryEntries> {
    sync_core::read_directory(&DiskScanFsAdapter(fs), dir)
}

fn scan(conn: &Connection) -> SyncResult<DiskScan> {
    scan_with_reader(conn, &mut read_directory)
}

fn scan_with_reader(
    conn: &Connection,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
) -> SyncResult<DiskScan> {
    scan_with_path_mode(conn, read, CASE_INSENSITIVE_PATHS)
}

fn scan_with_path_mode(
    conn: &Connection,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
    case_insensitive: bool,
) -> SyncResult<DiskScan> {
    scan_with_hasher(conn, read, case_insensitive, &hash_for_match)
}

fn scan_with_hasher(
    conn: &Connection,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
    case_insensitive: bool,
    hash: &dyn Fn(&Path) -> Option<String>,
) -> SyncResult<DiskScan> {
    scan_with_fs(&RealFs, conn, read, case_insensitive, hash)
}

/// The scan on an injected file system (tests); `read` lists a directory, `hash` hashes a file.
fn scan_with_fs(
    fs: &dyn Fs,
    conn: &Connection,
    read: &mut impl FnMut(&Path) -> std::io::Result<DirectoryEntries>,
    case_insensitive: bool,
    hash: &dyn Fn(&Path) -> Option<String>,
) -> SyncResult<DiskScan> {
    sync_core::scan(&DiskScanFsAdapter(fs), conn, read, case_insensitive, hash).map_err(Into::into)
}

/// Hash used to recognise a file again. Same function as the import and the stored
/// `content_hash`, so equal content always yields equal hashes. Files above the import
/// size limit and unreadable files get no hash and simply stay "new".
fn hash_for_match(path: &Path) -> Option<String> {
    hash_for_match_fs(&RealFs, path)
}

fn hash_for_match_fs(fs: &dyn Fs, path: &Path) -> Option<String> {
    sync_core::hash_for_match(&DiskScanFsAdapter(fs), &DesktopEffects, path)
}

#[cfg(test)]
fn skip_known_copies(conn: &Connection, scan: &mut DiskScan, case_insensitive: bool) -> SyncResult<()> {
    sync_core::skip_known_copies(&DiskScanFsAdapter(&RealFs), conn, scan, case_insensitive).map_err(Into::into)
}

#[cfg(test)]
fn classify_by_content(
    conn: &Connection,
    scan: &mut DiskScan,
    case_insensitive: bool,
    hash: &dyn Fn(&Path) -> Option<String>,
) -> SyncResult<()> {
    sync_core::classify_by_content(&DiskScanFsAdapter(&RealFs), conn, scan, case_insensitive, hash).map_err(Into::into)
}

#[cfg(test)]
fn verify_rename(
    conn: &Connection,
    scan: &DiskScan,
    rename: &DiskRename,
    expanded_sensitive: &[PathBuf],
) -> Result<(), String> {
    sync_core::verify_rename(&DiskScanFsAdapter(&RealFs), &DesktopEffects, conn, scan, rename, expanded_sensitive)
}

/// Cheap existence check for the folder tree: ids of catalog folders whose directory is
/// gone. Folders of a root that is missing, unreadable or empty (unplugged drive, mount
/// point without its volume) are "unknown" and not listed; only NotFound counts as gone.
pub(super) fn missing_folder_ids(folders: &[db::models::FolderRecord]) -> HashSet<i64> {
    let by_id: HashMap<i64, &db::models::FolderRecord> = folders.iter().map(|f| (f.id, f)).collect();
    let mut root_readable: HashMap<i64, bool> = HashMap::new();
    let mut gone = HashSet::new();
    for folder in folders {
        if folder.parent_id.is_none() || folder.path.is_empty() {
            continue;
        }
        let mut root = folder;
        let mut steps = 0;
        while let Some(parent) = root.parent_id.and_then(|p| by_id.get(&p)) {
            root = parent;
            steps += 1;
            if steps > folders.len() {
                break;
            }
        }
        if root.parent_id.is_some() {
            continue;
        }
        let readable = *root_readable.entry(root.id).or_insert_with(|| {
            std::fs::read_dir(&root.path).is_ok_and(|mut entries| entries.next().is_some())
        });
        if readable && backup::is_missing(Path::new(&folder.path)).unwrap_or(false) {
            gone.insert(folder.id);
        }
    }
    gone
}


/// Reads a model for import including its content hash, which rename detection needs later.
fn read_model_with_hash(path: &Path) -> CmdResult<db::models::NewFile> {
    let mut file = files::read_import_model_file(path, None)?;
    file.content_hash = Some(files::compute_content_hash(path)?);
    Ok(file)
}
fn apply(conn: &mut Connection, selection: DiskSelection) -> SyncResult<DiskApplied> {
    apply_with_reader(conn, selection, read_model_with_hash)
}
fn apply_with_reader(
    conn: &mut Connection,
    selection: DiskSelection,
    read: impl FnMut(&Path) -> CmdResult<db::models::NewFile>,
) -> SyncResult<DiskApplied> {
    apply_with_path_mode(conn, selection, read, CASE_INSENSITIVE_PATHS)
}

fn apply_with_path_mode(
    conn: &mut Connection,
    selection: DiskSelection,
    read: impl FnMut(&Path) -> CmdResult<db::models::NewFile>,
    case_insensitive: bool,
) -> SyncResult<DiskApplied> {
    apply_with_fs(&RealFs, conn, selection, read, case_insensitive)
}

/// The apply step on an injected file system (tests); the re-scan uses it too.
fn apply_with_fs(
    fs: &dyn Fs,
    conn: &mut Connection,
    selection: DiskSelection,
    mut read: impl FnMut(&Path) -> CmdResult<db::models::NewFile>,
    case_insensitive: bool,
) -> SyncResult<DiskApplied> {
    let reader = |path: &Path| read(path).map_err(|error| error.message);
    sync_core::apply(&DiskScanFsAdapter(fs), &DesktopEffects, conn, selection, reader, case_insensitive).map_err(Into::into)
}

fn connection(state: &AppState) -> CmdResult<Connection> {
    let conn =
        Connection::open_with_flags(&state.db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}
#[tauri::command]
pub async fn scan_catalog_disk(app: tauri::AppHandle) -> SyncResult<DiskScan> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.import_jobs.gate.exclusive().map_err(CmdError::from)?;
        scan(&connection(&state)?)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn apply_catalog_disk_sync(
    app: tauri::AppHandle,
    selection: DiskSelection,
) -> SyncResult<DiskApplied> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.import_jobs.gate.exclusive().map_err(CmdError::from)?;
        let mut selection = selection;
        selection.sensitive_dirs = state.sensitive_dirs.clone();
        apply(&mut connection(&state)?, selection)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestDir(PathBuf);
    impl TestDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    struct Fixture {
        dir: TestDir,
        conn: Connection,
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = TestDir(
                std::fs::canonicalize(std::env::temp_dir())
                    .unwrap()
                    .join(format!(
                        "disk-sync-{}-{}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    )),
            );
            std::fs::create_dir(&dir.0).unwrap();
            let root = dir.path().join("models");
            std::fs::create_dir(&root).unwrap();
            let conn = db::connect_in_memory().unwrap();
            db::ensure_folder_path(&conn, &root, &root).unwrap();
            Self { dir, conn, root }
        }
        fn select(&self, remove_missing: bool) -> DiskSelection {
            let preview = scan(&self.conn).unwrap();
            let renames = preview
                .renamed_files
                .iter()
                .map(|r| RenameChoice {
                    id: r.id.clone(),
                    new_path: r.new_path.clone(),
                })
                .collect();
            DiskSelection {
                renames,
                new_paths: preview
                    .new_folders
                    .into_iter()
                    .chain(preview.new_files)
                    // Copies of present models are opt-in in the dialog; these helpers
                    // model a user who accepts everything.
                    .chain(preview.duplicate_files.into_iter().map(|d| d.entry))
                    .map(|e| e.path)
                    .collect(),
                missing_ids: preview
                    .missing_files
                    .into_iter()
                    .filter_map(|e| e.id)
                    .collect(),
                remove_missing,
                ..Default::default()
            }
        }
    }
    #[test]
    fn path_keys_fold_only_when_requested() {
        let lower = Path::new("Spielzeug/Grün.3mf");
        let upper = Path::new("SPIELZEUG/GRÜN.3MF");
        assert_eq!(path_key(lower, true), path_key(upper, true));
        assert_ne!(path_key(lower, false), path_key(upper, false));
        assert_eq!(path_key(lower, false), lower);
        assert_eq!(
            CASE_INSENSITIVE_PATHS,
            cfg!(any(windows, target_os = "macos"))
        );
    }

    #[test]
    fn renamed_folder_is_reused_for_new_files_without_rewriting_paths() {
        let mut f = Fixture::new();
        let stored = f.root.join("Haushalt");
        let actual = f.root.join("HAUSHALT");
        std::fs::create_dir(&actual).unwrap();
        let folder_id = db::ensure_folder_path(&f.conn, &f.root, &stored).unwrap();
        let old_file = stored.join("Kreisel.obj");
        db::test_insert_minimal_file(&f.conn, old_file.to_str().unwrap(), Some(folder_id)).unwrap();
        std::fs::write(actual.join("KREISEL.OBJ"), "mesh").unwrap();
        let new_file = actual.join("new.obj");
        std::fs::write(&new_file, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let preview = scan_with_path_mode(&f.conn, &mut read_directory, true).unwrap();
        assert!(preview.new_folders.is_empty());
        assert!(preview.missing_files.is_empty());
        assert_eq!(preview.new_files.len(), 1);
        assert_eq!(preview.new_files[0].path, new_file.to_string_lossy());
        let selection = DiskSelection {
            new_paths: vec![new_file.to_string_lossy().into_owned()],
            missing_ids: vec![],
            remove_missing: false,
            ..Default::default()
        };
        let applied = apply_with_path_mode(
            &mut f.conn,
            selection,
            |path| files::read_import_model_file(path, None),
            true,
        )
        .unwrap();
        assert_eq!(applied.file_ids.len(), 1);
        assert!(applied.folder_ids.is_empty());
        let files = db::list_files(&f.conn).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files
            .iter()
            .any(|file| file.path == old_file.to_string_lossy()));
        assert_eq!(
            files
                .iter()
                .find(|file| file.path == new_file.to_string_lossy())
                .unwrap()
                .folder_id,
            Some(folder_id)
        );
        assert!(db::list_folders(&f.conn)
            .unwrap()
            .iter()
            .any(|folder| folder.path == stored.to_string_lossy()));
    }

    #[test]
    fn apply_rechecks_case_folded_duplicates_after_parsing() {
        let mut f = Fixture::new();
        let database = f.dir.path().join("catalog.db");
        f.conn = db::connect(&database).unwrap();
        db::ensure_folder_path(&f.conn, &f.root, &f.root).unwrap();
        let path = f.root.join("KREISEL.OBJ");
        let stored = f.root.join("Kreisel.obj");
        std::fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let other = Connection::open(&database).unwrap();
        let selection = f.select(false);
        let applied = apply_with_path_mode(
            &mut f.conn,
            selection,
            |path| {
                db::test_insert_minimal_file(&other, stored.to_str().unwrap(), None).unwrap();
                files::read_import_model_file(path, None)
            },
            true,
        )
        .unwrap();
        assert!(applied.file_ids.is_empty());
        let files = db::list_files(&f.conn).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, stored.to_string_lossy());
    }

    #[test]
    fn scan_case_only_rename_is_neither_new_nor_missing() {
        let f = Fixture::new();
        let folder = f.root.join("Spielzeug");
        std::fs::create_dir(&folder).unwrap();
        db::ensure_folder_path(&f.conn, &f.root, &folder).unwrap();
        let stored = folder.join("Kreisel.3mf");
        std::fs::write(&stored, "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, stored.to_str().unwrap(), None).unwrap();
        // Simulate read_dir returning a new spelling while metadata still finds the old one.
        let preview = scan_with_path_mode(
            &f.conn,
            &mut |dir| {
                let mut entries = read_directory(dir)?;
                for (path, kind) in &mut entries {
                    if kind.is_file() {
                        *path = dir.join("KREISEL.3MF");
                    }
                }
                Ok(entries)
            },
            true,
        )
        .unwrap();
        assert!(preview.new_files.is_empty());
        assert!(preview.new_folders.is_empty());
        assert!(preview.missing_files.is_empty());
    }

    #[test]
    fn scan_is_read_only_and_apply_preserves_missing_until_selected() {
        let mut f = Fixture::new();
        let missing = f.root.join("missing.stl");
        let id = db::test_insert_minimal_file(&f.conn, missing.to_str().unwrap(), None).unwrap();
        let nested = f.root.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let model = nested.join("new.obj");
        std::fs::write(&model, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        for name in [".hidden.obj", "~$temp.obj", "skip.part"] {
            std::fs::write(f.root.join(name), "ignored").unwrap();
        }
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.new_files.len(), 1);
        assert_eq!(preview.new_folders.len(), 1);
        assert_eq!(preview.missing_files.len(), 1);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
        assert_eq!(db::list_folders(&f.conn).unwrap().len(), 1);
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert_eq!(result.file_ids.len(), 1);
        assert_eq!(result.folder_ids.len(), 1);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 2);
        let imported = db::get_file(&f.conn, result.file_ids[0].parse().unwrap())
            .unwrap()
            .unwrap();
        assert!(imported.content_hash.is_some());
        assert!(imported.folder_id.is_some());
        assert!(scan(&f.conn).unwrap().new_files.is_empty());
        let selection = f.select(true);
        assert_eq!(apply(&mut f.conn, selection).unwrap().removed_count, 1);
        assert!(db::list_trash(&f.conn)
            .unwrap()
            .iter()
            .any(|file| file.id == id));
        assert!(model.exists());
    }
    #[test]
    fn missing_only_apply_requires_selection_and_moves_entries_to_trash() {
        let mut f = Fixture::new();
        let missing = f.root.join("missing.stl");
        // A non-empty root is needed, otherwise it counts as an unmounted volume.
        std::fs::write(f.root.join("notes.txt"), "present").unwrap();
        let id = db::test_insert_minimal_file(&f.conn, missing.to_str().unwrap(), None).unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.new_files.is_empty());
        assert!(preview.new_folders.is_empty());
        assert_eq!(preview.missing_files.len(), 1);

        let selection = f.select(false);
        assert_eq!(apply(&mut f.conn, selection).unwrap().removed_count, 0);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
        assert!(db::list_trash(&f.conn).unwrap().is_empty());

        let selection = f.select(true);
        assert!(selection.new_paths.is_empty());
        let result = apply_with_reader(&mut f.conn, selection, |_| {
            panic!("Removing missing entries must not attempt to import a file")
        })
        .unwrap();
        assert_eq!(result.removed_count, 1);
        assert!(result.file_ids.is_empty());
        assert!(result.folder_ids.is_empty());
        assert!(db::list_files(&f.conn).unwrap().is_empty());
        let trash = db::list_trash(&f.conn).unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].id, id);
        assert_eq!(trash[0].path, missing.to_string_lossy());
        assert!(!missing.exists());
    }

    #[test]
    fn unavailable_root_suppresses_all_changes_and_apply() {
        let mut f = Fixture::new();
        db::test_insert_minimal_file(&f.conn, f.root.join("missing.stl").to_str().unwrap(), None)
            .unwrap();
        let selection = f.select(true);
        std::fs::remove_dir(&f.root).unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.unreachable_roots.len(), 1);
        assert!(preview.missing_files.is_empty());
        assert!(apply(&mut f.conn, selection).is_err());
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
    }
    #[test]
    fn empty_root_with_catalogued_files_is_unreachable_and_apply_changes_nothing() {
        let mut f = Fixture::new();
        for name in ["a.stl", "b.stl"] {
            db::test_insert_minimal_file(&f.conn, f.root.join(name).to_str().unwrap(), None)
                .unwrap();
        }
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.unreachable_roots, vec![f.root.to_string_lossy()]);
        assert!(preview.missing_files.is_empty());
        let selection = DiskSelection {
            new_paths: vec![],
            missing_ids: db::list_files(&f.conn)
                .unwrap()
                .iter()
                .map(|file| file.id.to_string())
                .collect(),
            remove_missing: true,
            ..Default::default()
        };
        assert!(apply(&mut f.conn, selection).is_err());
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 2);
        assert!(db::list_trash(&f.conn).unwrap().is_empty());
    }

    #[test]
    fn root_with_only_some_files_missing_still_offers_them() {
        let f = Fixture::new();
        std::fs::write(f.root.join("here.stl"), "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, f.root.join("here.stl").to_str().unwrap(), None)
            .unwrap();
        db::test_insert_minimal_file(&f.conn, f.root.join("gone.stl").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.unreachable_roots.is_empty());
        assert_eq!(preview.missing_files.len(), 1);
        assert_eq!(
            preview.missing_files[0].path,
            f.root.join("gone.stl").to_string_lossy()
        );
    }

    #[test]
    fn empty_root_without_catalogued_files_is_reachable() {
        let f = Fixture::new();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.unreachable_roots.is_empty());
        assert!(preview.missing_files.is_empty());
    }

    #[test]
    fn loose_file_with_existing_parent_is_offered_as_missing() {
        let f = Fixture::new();
        let outside = f.dir.path().join("loose");
        std::fs::create_dir(&outside).unwrap();
        db::test_insert_minimal_file(&f.conn, outside.join("gone.stl").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.unreachable_roots.is_empty());
        assert_eq!(preview.missing_files.len(), 1);
    }

    #[test]
    fn loose_file_with_absent_parent_is_unreachable_not_missing() {
        let mut f = Fixture::new();
        let drive = f.dir.path().join("stick");
        db::test_insert_minimal_file(&f.conn, drive.join("gone.stl").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.unreachable_roots, vec![drive.to_string_lossy()]);
        assert!(preview.missing_files.is_empty());
        let selection = DiskSelection {
            new_paths: vec![],
            missing_ids: vec![],
            remove_missing: true,
            ..Default::default()
        };
        assert!(apply(&mut f.conn, selection).is_err());
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
    }

    #[test]
    fn stale_missing_selection_and_outside_paths_are_rejected() {
        let mut f = Fixture::new();
        let missing = f.root.join("missing.stl");
        std::fs::write(f.root.join("notes.txt"), "present").unwrap();
        db::test_insert_minimal_file(&f.conn, missing.to_str().unwrap(), None).unwrap();
        let selection = f.select(true);
        std::fs::write(&missing, "returned").unwrap();
        assert!(apply(&mut f.conn, selection).is_err());
        let selection = DiskSelection {
            new_paths: vec![f.dir.path().join("outside.obj").to_string_lossy().into()],
            missing_ids: vec![],
            remove_missing: false,
            ..Default::default()
        };
        assert!(apply(&mut f.conn, selection).is_err());
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
    }
    #[test]
    fn invalid_import_is_reported_without_creating_empty_folder_rows() {
        let mut f = Fixture::new();
        std::fs::create_dir(f.root.join("new")).unwrap();
        std::fs::write(f.root.join("new/broken.stl"), "broken").unwrap();
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert_eq!(result.failed_paths.len(), 1);
        assert!(result.file_ids.is_empty());
        assert!(result.folder_ids.is_empty());
        assert_eq!(result.removed_count, 0);
        assert_eq!(db::list_folders(&f.conn).unwrap().len(), 1);
        assert!(db::list_files(&f.conn).unwrap().is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_and_overlapping_roots_are_not_followed_or_duplicated() {
        let f = Fixture::new();
        let nested = f.root.join("nested");
        std::fs::create_dir(&nested).unwrap();
        db::ensure_folder_path(&f.conn, &nested, &nested).unwrap();
        std::fs::write(nested.join("new.obj"), "mesh").unwrap();
        std::os::unix::fs::symlink(&nested, f.root.join("link")).unwrap();
        std::os::unix::fs::symlink(f.dir.path().join("absent"), f.root.join("broken.obj")).unwrap();
        db::test_insert_minimal_file(&f.conn, f.root.join("broken.obj").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.new_files.len(), 1);
        assert!(preview.missing_files.is_empty());
        assert!(preview.new_folders.is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn non_not_found_io_error_is_not_missing() {
        let f = Fixture::new();
        let file = f.root.join("not_a_directory");
        std::fs::write(&file, "x").unwrap();
        assert!(backup::is_missing(&file.join("child.stl")).is_err());
        db::test_insert_minimal_file(&f.conn, file.join("child.stl").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.missing_files.is_empty());
        assert!(preview.unreachable_roots.is_empty());
    }
    #[test]
    fn scan_ignores_paths_reserved_by_trash_and_empty_or_unsupported_folders() {
        let mut f = Fixture::new();
        let path = f.root.join("trashed.obj");
        std::fs::write(&path, "mesh").unwrap();
        let id = db::test_insert_minimal_file(&f.conn, path.to_str().unwrap(), None).unwrap();
        trash::delete_missing_file_with_conn(&f.conn, id).unwrap();
        std::fs::create_dir(f.root.join("empty")).unwrap();
        std::fs::create_dir_all(f.root.join("docs/images")).unwrap();
        std::fs::write(f.root.join("docs/readme.txt"), "docs").unwrap();
        assert!(scan(&f.conn).unwrap().new_files.is_empty());
        assert!(scan(&f.conn).unwrap().new_folders.is_empty());
        assert_eq!(scan(&f.conn).unwrap().trash_blocked_count, 1);
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert!(result.folder_ids.is_empty());
        assert!(result.file_ids.is_empty());
    }
    #[test]
    fn trash_blocked_count_covers_only_files_present_on_disk() {
        let f = Fixture::new();
        let held = f.root.join("held.obj");
        std::fs::write(&held, "mesh").unwrap();
        let held_id = db::test_insert_minimal_file(&f.conn, held.to_str().unwrap(), None).unwrap();
        trash::delete_missing_file_with_conn(&f.conn, held_id).unwrap();
        // A trash row whose file is gone from disk blocks nothing the scan could report.
        let gone = f.root.join("gone.obj");
        let gone_id = db::test_insert_minimal_file(&f.conn, gone.to_str().unwrap(), None).unwrap();
        trash::delete_missing_file_with_conn(&f.conn, gone_id).unwrap();
        // Unsupported files are ignored by the scan anyway.
        std::fs::write(f.root.join("notes.txt"), "text").unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.trash_blocked_count, 1);
        assert!(preview.new_files.is_empty());
        // An ordinary new file is offered, not counted as blocked.
        std::fs::write(f.root.join("fresh.obj"), "mesh").unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.trash_blocked_count, 1);
        assert_eq!(preview.new_files.len(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn root_with_symlinked_ancestor_works_without_rewriting_catalog_paths() {
        let mut f = Fixture::new();
        let actual = f.dir.path().join("actual");
        std::fs::create_dir_all(actual.join("nested/known")).unwrap();
        let alias = f.dir.path().join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        let root = alias.join("nested");
        let folder_id = db::ensure_folder_path(&f.conn, &root, &root.join("known")).unwrap();
        let known = root.join("known/old.obj");
        std::fs::write(&known, "mesh").unwrap();
        let known_id =
            db::test_insert_minimal_file(&f.conn, known.to_str().unwrap(), Some(folder_id))
                .unwrap();
        // A canonical file path in an older catalog must also be recognized.
        let canonical = actual.join("nested/canonical.obj");
        std::fs::write(&canonical, "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, canonical.to_str().unwrap(), None).unwrap();
        let gone = root.join("gone.obj");
        db::test_insert_minimal_file(&f.conn, gone.to_str().unwrap(), None).unwrap();
        std::fs::create_dir_all(root.join("new/deep")).unwrap();
        std::fs::write(
            root.join("new/deep/new.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(&f.root, root.join("link")).unwrap();
        std::fs::write(f.root.join("outside.obj"), "mesh").unwrap();
        let preview = scan(&f.conn).unwrap();
        assert!(preview.unreachable_roots.is_empty());
        assert_eq!(preview.new_files.len(), 2);
        assert_eq!(preview.new_folders.len(), 2);
        assert_eq!(preview.missing_files[0].path, gone.to_string_lossy());
        assert!(
            !no_symlinks(&actual.join("nested"), &actual.join("nested/../escape.obj")).unwrap()
        );
        assert!(!no_symlinks(&actual.join("nested"), &f.root.join("outside.obj")).unwrap());
        let mut selection = f.select(false);
        selection
            .new_paths
            .retain(|p| p.starts_with(root.to_str().unwrap()));
        let result = apply(&mut f.conn, selection).unwrap();
        assert_eq!(result.file_ids.len(), 1);
        assert_eq!(result.folder_ids.len(), 2);
        assert_eq!(
            db::get_file(&f.conn, known_id).unwrap().unwrap().path,
            known.to_string_lossy()
        );
        assert_eq!(
            db::list_folders(&f.conn)
                .unwrap()
                .iter()
                .find(|folder| folder.id == folder_id)
                .unwrap()
                .path,
            root.join("known").to_string_lossy()
        );
        assert!(scan(&f.conn)
            .unwrap()
            .new_files
            .iter()
            .all(|e| !e.path.starts_with(root.to_str().unwrap())));
    }

    #[test]
    fn unreadable_files_do_not_prevent_valid_imports() {
        let mut f = Fixture::new();
        let broken = f.root.join("broken.3mf");
        std::fs::write(&broken, "incomplete archive").unwrap();
        let valid = f.root.join("valid.obj");
        std::fs::write(&valid, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert_eq!(result.file_ids.len(), 1);
        assert_eq!(result.failed_paths.len(), 1);
        assert_eq!(result.failed_paths[0].path, broken.to_string_lossy());
        assert!(!result.failed_paths[0].reason.is_empty());
        assert_eq!(
            db::list_files(&f.conn).unwrap()[0].path,
            valid.to_string_lossy()
        );
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(
            serialized["failedPaths"][0]["path"],
            broken.to_string_lossy().as_ref()
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_read_does_not_bypass_path_validation() {
        let mut f = Fixture::new();
        let path = f.root.join("broken.3mf");
        std::fs::write(&path, "incomplete archive").unwrap();
        let selection = f.select(false);
        let result = apply_with_reader(&mut f.conn, selection, |path| {
            std::fs::remove_file(path).unwrap();
            std::os::unix::fs::symlink(f.dir.path().join("elsewhere.3mf"), path).unwrap();
            Err("Cannot read file".to_string().into())
        });
        assert_eq!(result.err().unwrap().code, "diskSyncPathChanged");
        assert!(db::list_files(&f.conn).unwrap().is_empty());
    }

    #[test]
    fn parsing_all_files_happens_before_any_sqlite_transaction() {
        let mut f = Fixture::new();
        let database = f.dir.path().join("catalog.db");
        f.conn = db::connect(&database).unwrap();
        db::ensure_folder_path(&f.conn, &f.root, &f.root).unwrap();
        std::fs::create_dir(f.root.join("new")).unwrap();
        for name in ["a.obj", "b.obj"] {
            std::fs::write(
                f.root.join("new").join(name),
                "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
            )
            .unwrap();
        }
        let other = Connection::open(&database).unwrap();
        other.busy_timeout(std::time::Duration::ZERO).unwrap();
        let selection = f.select(false);
        let mut reads = 0;
        let result = apply_with_reader(&mut f.conn, selection, |path| {
            // A second writer must succeed even while preparing the second file.
            other
                .execute_batch("BEGIN IMMEDIATE; UPDATE folders SET name = name; COMMIT;")
                .unwrap();
            assert_eq!(db::list_folders(&other).unwrap().len(), 1);
            assert!(db::list_files(&other).unwrap().is_empty());
            reads += 1;
            let mut file = files::read_import_model_file(path, None)?;
            file.content_hash = Some(files::compute_content_hash(path)?);
            Ok(file)
        })
        .unwrap();
        assert_eq!(reads, 2);
        assert_eq!(result.file_ids.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_swap_during_parsing_rolls_back_everything() {
        let mut f = Fixture::new();
        std::fs::create_dir(f.root.join("new")).unwrap();
        let path = f.root.join("new/model.obj");
        std::fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let selection = f.select(false);
        let result = apply_with_reader(&mut f.conn, selection, |path| {
            let file = files::read_import_model_file(path, None)?;
            std::fs::remove_file(path).unwrap();
            std::os::unix::fs::symlink(f.dir.path().join("elsewhere.obj"), path).unwrap();
            Ok(file)
        });
        assert_eq!(result.err().unwrap().code, "diskSyncPathChanged");
        assert_eq!(db::list_folders(&f.conn).unwrap().len(), 1);
        assert!(db::list_files(&f.conn).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn root_retargeted_during_parsing_is_rejected() {
        let mut f = Fixture::new();
        let alias = f.dir.path().join("alias");
        std::os::unix::fs::symlink(&f.root, &alias).unwrap();
        f.conn
            .execute("UPDATE folders SET path = ?1", [alias.to_str().unwrap()])
            .unwrap();
        std::fs::write(
            f.root.join("model.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let elsewhere = f.dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let selection = f.select(false);
        let error = apply_with_reader(&mut f.conn, selection, |path| {
            let file = files::read_import_model_file(path, None)?;
            std::fs::remove_file(&alias).unwrap();
            std::os::unix::fs::symlink(&elsewhere, &alias).unwrap();
            Ok(file)
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "diskSyncRootUnreachable");
        assert!(db::list_files(&f.conn).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn canonical_folder_rows_and_duplicate_root_aliases_are_reused() {
        let mut f = Fixture::new();
        let alias = f.dir.path().join("alias");
        std::os::unix::fs::symlink(&f.root, &alias).unwrap();
        db::ensure_folder_path(&f.conn, &alias, &alias).unwrap();
        std::fs::create_dir(f.root.join("known")).unwrap();
        let id = db::ensure_folder_path(&f.conn, &f.root, &f.root.join("known")).unwrap();
        let known = alias.join("old.obj");
        std::fs::write(&known, "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, known.to_str().unwrap(), None).unwrap();
        std::fs::write(
            f.root.join("known/new.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.new_files.len(), 1);
        assert!(preview.new_folders.is_empty());
        let selection = f.select(false);
        let result = apply(&mut f.conn, selection).unwrap();
        assert!(result.folder_ids.is_empty());
        let file = db::get_file(&f.conn, result.file_ids[0].parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(file.folder_id, Some(id));
        assert_eq!(db::list_folders(&f.conn).unwrap().len(), 3);
    }

    #[test]
    fn unassigned_files_outside_roots_are_checked_and_can_be_removed() {
        let mut f = Fixture::new();
        let gone = f.dir.path().join("gone.obj");
        let id = db::test_insert_minimal_file(&f.conn, gone.to_str().unwrap(), None).unwrap();
        let present = f.dir.path().join("present.obj");
        std::fs::write(&present, "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, present.to_str().unwrap(), None).unwrap();
        let trashed = db::test_insert_minimal_file(
            &f.conn,
            f.dir.path().join("trashed.obj").to_str().unwrap(),
            None,
        )
        .unwrap();
        trash::delete_missing_file_with_conn(&f.conn, trashed).unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.missing_files.len(), 1);
        assert_eq!(preview.missing_files[0].id, Some(id.to_string()));
        assert!(preview.missing_files[0].root.is_empty());
        let selection = f.select(false);
        assert_eq!(apply(&mut f.conn, selection).unwrap().removed_count, 0);
        let selection = f.select(true);
        assert_eq!(apply(&mut f.conn, selection).unwrap().removed_count, 1);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 1);
    }

    #[test]
    fn sync_imports_distinct_paths_with_identical_content_hashes() {
        let mut f = Fixture::new();
        let original = f.root.join("original.obj");
        std::fs::write(&original, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let selection = f.select(false);
        let first = apply(&mut f.conn, selection).unwrap();
        std::fs::copy(&original, f.root.join("copy.obj")).unwrap();
        let selection = f.select(false);
        let second = apply(&mut f.conn, selection).unwrap();
        assert_eq!(second.file_ids.len(), 1);
        let first = db::get_file(&f.conn, first.file_ids[0].parse().unwrap())
            .unwrap()
            .unwrap();
        let second = db::get_file(&f.conn, second.file_ids[0].parse().unwrap())
            .unwrap()
            .unwrap();
        assert!(first.content_hash.is_some());
        assert_eq!(first.content_hash, second.content_hash);
        assert_ne!(first.path, second.path);
        assert_eq!(db::list_files(&f.conn).unwrap().len(), 2);
    }

    #[test]
    fn child_read_errors_preserve_siblings_and_protect_missing_files() {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
        ] {
            let f = Fixture::new();
            let child = f.root.join("child");
            std::fs::create_dir(&child).unwrap();
            std::fs::write(f.root.join("new.obj"), "mesh").unwrap();
            db::test_insert_minimal_file(&f.conn, child.join("gone.obj").to_str().unwrap(), None)
                .unwrap();
            db::test_insert_minimal_file(&f.conn, f.root.join("gone.obj").to_str().unwrap(), None)
                .unwrap();
            let preview = scan_with_reader(&f.conn, &mut |dir| {
                if dir == child {
                    Err(std::io::Error::from(kind))
                } else {
                    read_directory(dir)
                }
            })
            .unwrap();
            assert!(preview.unreachable_roots.is_empty());
            assert_eq!(preview.new_files.len(), 1);
            if kind == std::io::ErrorKind::NotFound {
                assert!(preview.skipped_paths.is_empty());
                assert_eq!(preview.missing_files.len(), 2);
            } else {
                assert_eq!(preview.skipped_paths, vec![child.to_string_lossy()]);
                assert_eq!(preview.missing_files.len(), 1);
                assert_eq!(
                    preview.missing_files[0].path,
                    f.root.join("gone.obj").to_string_lossy()
                );
            }
        }
    }

    #[test]
    fn root_read_errors_still_suppress_all_changes() {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
        ] {
            let f = Fixture::new();
            db::test_insert_minimal_file(&f.conn, f.root.join("gone.obj").to_str().unwrap(), None)
                .unwrap();
            let preview =
                scan_with_reader(&f.conn, &mut |_| Err(std::io::Error::from(kind))).unwrap();
            assert_eq!(preview.unreachable_roots, vec![f.root.to_string_lossy()]);
            assert!(preview.skipped_paths.is_empty());
            assert!(preview.missing_files.is_empty());
            assert!(preview.new_files.is_empty());
        }
    }

    #[test]
    fn case_only_filter_preserves_missing_and_unrelated_new_files() {
        let root = Path::new("models");
        let mut preview = DiskScan {
            new_files: vec![
                entry(root, Path::new("models/Part/A.obj"), None),
                entry(root, Path::new("models/other/b.obj"), None),
            ],
            new_folders: vec![
                entry(root, Path::new("models/Part"), None),
                entry(root, Path::new("models/other"), None),
            ],
            missing_files: vec![entry(
                root,
                Path::new("models/part/a.OBJ"),
                Some("1".into()),
            )],
            ..DiskScan::default()
        };
        suppress_case_only_new_files(&mut preview, true);
        assert_eq!(preview.new_files.len(), 1);
        assert_eq!(preview.new_files[0].path, "models/other/b.obj");
        assert_eq!(preview.new_folders.len(), 1);
        assert_eq!(preview.new_folders[0].path, "models/other");
        assert_eq!(preview.missing_files.len(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_keeps_case_distinct_paths() {
        let f = Fixture::new();
        std::fs::write(f.root.join("A.obj"), "mesh").unwrap();
        db::test_insert_minimal_file(&f.conn, f.root.join("a.obj").to_str().unwrap(), None)
            .unwrap();
        let preview = scan(&f.conn).unwrap();
        assert_eq!(preview.new_files.len(), 1);
        assert_eq!(preview.missing_files.len(), 1);
    }

    #[test]
    fn error_contract_keeps_codes_and_paths_separate() {
        let error = DiskSyncError::path("diskSyncPathChanged", Path::new("/models/a.obj"));
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({"code": "diskSyncPathChanged", "paths": ["/models/a.obj"]})
        );
    }
}

mod start_sync;
pub use start_sync::*;

mod known_copies;
pub use known_copies::*;

#[cfg(test)]
mod start_sync_tests;

#[cfg(test)]
mod roadmap_1g_tests;

#[cfg(test)]
mod rename_tests;

#[cfg(test)]
mod missing_folder_tests;

#[cfg(test)]
mod companion_tests;

#[cfg(test)]
mod blocked_tags_tests;

#[cfg(test)]
mod known_copy_tests;

#[cfg(test)]
mod golden_sync_tests;
