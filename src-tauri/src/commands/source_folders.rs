//! Source folders: folders the user wants checked for new model files (a download
//! folder, a slicer export folder). The check only reads. Files the user accepts are
//! imported through the normal import job with a catalog folder set, which moves them
//! into the catalog folder; this module never copies, moves or deletes a file itself.

use super::*;
use crate::db::sources as store;
use tauri::Manager;

/// Subfolder levels below a source folder that are looked at.
const MAX_SCAN_DEPTH: usize = 3;
/// Directory entries inspected per source folder; a whole drive must not stall the check.
const MAX_SCAN_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFolderDto {
    pub id: String,
    pub path: String,
    pub added_at: String,
    /// The folder exists and is the same folder as when it was added (a drive that is
    /// unplugged, or a path that now leads elsewhere, is not reachable).
    pub reachable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFileDto {
    pub path: String,
    /// Path below the source folder, with `/` separators.
    pub relative_path: String,
    pub size_bytes: i64,
    /// Catalog model with identical content, when the content could be compared.
    pub duplicate_of_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScanDto {
    pub folder_id: String,
    pub folder_path: String,
    pub files: Vec<SourceFileDto>,
    /// Entries that could not be read (folders, files, links that were left out).
    pub skipped: u32,
    /// The inspection stopped at its entry limit; there may be more files.
    pub truncated: bool,
    /// The folder could not be checked (missing, moved, or no longer allowed).
    pub unreachable: bool,
}

fn source_dto(record: db::models::SourceFolderRecord) -> SourceFolderDto {
    let reachable = resolve_source(&record.path, &[]).is_ok();
    SourceFolderDto { id: record.id.to_string(), path: record.path, added_at: record.added_at, reachable }
}

/// The folder itself as it stands now: an existing directory that is still the stored
/// (already resolved) path, and not in or around a protected place.
fn resolve_source(stored: &str, expanded_sensitive: &[PathBuf]) -> CmdResult<PathBuf> {
    let path = Path::new(stored);
    let meta = std::fs::symlink_metadata(path).map_err(|_| CmdError::expected("Der Ordner existiert nicht."))?;
    if !meta.is_dir() {
        return Err(CmdError::expected("Das ist kein Ordner."));
    }
    let canonical = path.canonicalize().map_err(|_| CmdError::expected("Der Ordner kann nicht gelesen werden."))?;
    if canonical != path {
        return Err(CmdError::expected("Der Ordner liegt jetzt an einem anderen Ort."));
    }
    archives::check_managed_location(&canonical, expanded_sensitive)?;
    Ok(canonical)
}

fn same_or_inside(candidate: &Path, base: &Path) -> bool {
    folders::comparison_path(candidate).starts_with(folders::comparison_path(base))
}

/// Everything that makes a path unfit as a new source folder.
fn validate_new_source(conn: &Connection, requested: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<PathBuf> {
    if !requested.is_absolute() || requested.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(CmdError::expected("Der Pfad ist nicht gültig."));
    }
    let meta = std::fs::metadata(requested).map_err(|_| CmdError::expected("Der Ordner existiert nicht."))?;
    if !meta.is_dir() {
        return Err(CmdError::expected("Das ist kein Ordner."));
    }
    let canonical = requested.canonicalize().map_err(|_| CmdError::expected("Der Ordner kann nicht gelesen werden."))?;
    archives::check_managed_location(&canonical, &expand_sensitive_dirs(sensitive_dirs))
        .map_err(|_| CmdError::expected("Dieser Ordner liegt in einem geschützten Systembereich oder enthält einen und kann nicht verwendet werden."))?;

    // The catalog folder, and every folder tree the catalog already keeps in place: files
    // there are managed through the catalog, not "new".
    let mut catalog_roots: Vec<PathBuf> = Vec::new();
    if let Some(base) = db::printer_link::get_setting(conn, folders::SETTING_CATALOG_BASE_DIR)? {
        catalog_roots.push(PathBuf::from(base));
    }
    catalog_roots.extend(db::list_folders(conn)?.into_iter().filter(|f| f.parent_id.is_none()).map(|f| PathBuf::from(f.path)));
    for root in catalog_roots {
        let root = root.canonicalize().unwrap_or(root);
        if same_or_inside(&canonical, &root) || same_or_inside(&root, &canonical) {
            return Err(CmdError::expected("Dieser Ordner gehört zum Katalog (Katalogordner, Ordner darin oder darüber) und kann kein Quellordner sein."));
        }
    }
    for existing in store::list_source_folders(conn)? {
        let existing = PathBuf::from(&existing.path);
        if folders::comparison_path(&existing) == folders::comparison_path(&canonical) {
            return Err(CmdError::expected("Dieser Ordner ist schon ein Quellordner."));
        }
        if same_or_inside(&canonical, &existing) || same_or_inside(&existing, &canonical) {
            return Err(CmdError::expected("Der Ordner liegt in einem anderen Quellordner oder enthält einen. Quellordner dürfen sich nicht überschneiden."));
        }
    }
    Ok(canonical)
}

pub(super) fn add_source_folder_with_conn(conn: &Connection, requested: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<SourceFolderDto> {
    let canonical = validate_new_source(conn, requested, sensitive_dirs)?;
    let added_at = chrono::Utc::now().to_rfc3339();
    let id = store::insert_source_folder(conn, &canonical.to_string_lossy(), &added_at)?;
    log::info!(target: "catalog", "Quellordner hinzugefügt");
    Ok(source_dto(db::models::SourceFolderRecord { id, path: canonical.to_string_lossy().into_owned(), added_at }))
}

/// Forgets the folder and the declined files below it. No file is touched.
pub(super) fn remove_source_folder_with_conn(conn: &Connection, id: i64) -> CmdResult<()> {
    let folder = store::get_source_folder(conn, id)?.ok_or_else(|| CmdError::expected("Quellordner nicht gefunden."))?;
    let root = PathBuf::from(&folder.path);
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    store::delete_source_folder(&tx, id)?;
    for rejected in store::list_rejected_sources(&tx)? {
        if same_or_inside(Path::new(&rejected.path), &root) {
            store::delete_rejected_source(&tx, rejected.id)?;
        }
    }
    tx.commit().map_err(DbError::from)?;
    log::info!(target: "catalog", "Quellordner entfernt");
    Ok(())
}

/// Model files found by [`walk_source_folder`], before the catalog is consulted.
#[derive(Debug, Default)]
struct Walked {
    files: Vec<(PathBuf, u64)>,
    skipped: u32,
    truncated: bool,
}

/// Lists supported model files below `root` (depth and entry limits), never following a
/// symlink. Anything that cannot be read is counted and left out.
fn walk_source_folder(root: &Path, max_depth: usize, max_entries: usize) -> Walked {
    let mut walked = Walked::default();
    let mut visited = 0usize;
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                walked.skipped += 1;
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                walked.skipped += 1;
                continue;
            };
            if visited >= max_entries {
                walked.truncated = true;
                return walked;
            }
            visited += 1;
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                walked.skipped += 1;
                continue;
            };
            if kind.is_symlink() {
                // Only links that would have mattered are worth a note.
                if files::is_supported_extension(&path) {
                    walked.skipped += 1;
                }
            } else if kind.is_dir() {
                if depth < max_depth {
                    pending.push((path, depth + 1));
                }
            } else if kind.is_file() && files::is_supported_extension(&path) && !files::is_ignored_import_path(&path) {
                match entry.metadata() {
                    Ok(meta) => walked.files.push((path, meta.len())),
                    Err(_) => walked.skipped += 1,
                }
            }
        }
    }
    walked.files.sort();
    walked
}

/// What the catalog knows, loaded once per scan so the disk walk runs without the lock.
struct CatalogView {
    /// Paths of active models.
    paths: HashSet<PathBuf>,
    /// Declined files: path -> size.
    rejected: HashMap<PathBuf, i64>,
    /// Active models by file size, with their stored content hash.
    by_size: HashMap<i64, Vec<(i64, Option<String>)>>,
}

fn load_catalog_view(conn: &Connection) -> CmdResult<CatalogView> {
    let mut paths = HashSet::new();
    let mut by_size: HashMap<i64, Vec<(i64, Option<String>)>> = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT id, path, file_size_bytes, content_hash FROM files WHERE deleted_at IS NULL")
        .map_err(DbError::from)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<String>>(3)?)))
        .map_err(DbError::from)?;
    for row in rows {
        let (id, path, size, hash) = row.map_err(DbError::from)?;
        paths.insert(folders::comparison_path(Path::new(&path)));
        by_size.entry(size).or_default().push((id, hash));
    }
    let rejected = store::list_rejected_sources(conn)?
        .into_iter()
        .map(|r| (folders::comparison_path(Path::new(&r.path)), r.size_bytes))
        .collect();
    Ok(CatalogView { paths, rejected, by_size })
}

/// Hash comparison for one new file; only when a model of the same size exists, so most
/// files are never read.
fn duplicate_of(view: &CatalogView, path: &Path, size: i64) -> Option<String> {
    let candidates = view.by_size.get(&size)?;
    if size as u64 > crate::safe_file::MAX_MODEL_FILE_BYTES || !candidates.iter().any(|(_, hash)| hash.is_some()) {
        return None;
    }
    let hash = files::compute_content_hash(path).ok()?;
    candidates.iter().find(|(_, stored)| stored.as_deref() == Some(hash.as_str())).map(|(id, _)| id.to_string())
}

fn scan_one(folder: &db::models::SourceFolderRecord, view: &CatalogView, expanded_sensitive: &[PathBuf], max_entries: usize) -> SourceScanDto {
    let empty = |unreachable: bool| SourceScanDto {
        folder_id: folder.id.to_string(),
        folder_path: folder.path.clone(),
        files: Vec::new(),
        skipped: 0,
        truncated: false,
        unreachable,
    };
    let Ok(root) = resolve_source(&folder.path, expanded_sensitive) else { return empty(true) };
    let walked = walk_source_folder(&root, MAX_SCAN_DEPTH, max_entries);
    let mut files = Vec::new();
    for (path, size) in walked.files {
        let key = folders::comparison_path(&path);
        let size = size as i64;
        if view.paths.contains(&key) || view.rejected.get(&key) == Some(&size) {
            continue;
        }
        let relative_path = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let duplicate_of_id = duplicate_of(view, &path, size);
        files.push(SourceFileDto { path: path.to_string_lossy().into_owned(), relative_path, size_bytes: size, duplicate_of_id });
    }
    SourceScanDto {
        folder_id: folder.id.to_string(),
        folder_path: folder.path.clone(),
        files,
        skipped: walked.skipped,
        truncated: walked.truncated,
        unreachable: false,
    }
}

/// New model files in every source folder. Reads only; the database lock is not held
/// while the disk is walked.
pub(super) fn scan_with_state(state: &AppState, max_entries: usize) -> CmdResult<Vec<SourceScanDto>> {
    let (sources, view) = {
        let conn = lock(&state.db)?;
        (store::list_source_folders(&conn)?, load_catalog_view(&conn)?)
    };
    let expanded = expand_sensitive_dirs(&state.sensitive_dirs);
    Ok(sources.iter().map(|folder| scan_one(folder, &view, &expanded, max_entries)).collect())
}

/// Every path of the request must be a file the scan just listed, which also means it
/// lies below a registered source folder. Returns the paths with their scanned sizes.
fn validate_requested(scans: &[SourceScanDto], requested: &[String]) -> CmdResult<Vec<(PathBuf, i64)>> {
    let listed: HashMap<&str, i64> =
        scans.iter().flat_map(|scan| scan.files.iter()).map(|file| (file.path.as_str(), file.size_bytes)).collect();
    let mut seen = HashSet::new();
    let mut accepted = Vec::new();
    for path in requested {
        let size = listed
            .get(path.as_str())
            .ok_or_else(|| CmdError::expected("Die Datei gehört nicht (mehr) zu den neuen Dateien eines Quellordners."))?;
        if seen.insert(path.as_str()) {
            accepted.push((PathBuf::from(path), *size));
        }
    }
    Ok(accepted)
}

pub(super) fn reject_with_state(state: &AppState, requested: &[String], max_entries: usize) -> CmdResult<usize> {
    let scans = scan_with_state(state, max_entries)?;
    let accepted = validate_requested(&scans, requested)?;
    let conn = lock(&state.db)?;
    let rejected_at = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction().map_err(DbError::from)?;
    for (path, size) in &accepted {
        store::upsert_rejected_source(&tx, &path.to_string_lossy(), *size, &rejected_at)?;
    }
    tx.commit().map_err(DbError::from)?;
    Ok(accepted.len())
}

fn lock(db: &Mutex<Connection>) -> CmdResult<std::sync::MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| "database lock poisoned".into())
}

#[tauri::command]
pub async fn list_source_folders(app: tauri::AppHandle) -> CmdResult<Vec<SourceFolderDto>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = lock(&state.db)?;
        let records = store::list_source_folders(&conn)?;
        drop(conn);
        Ok(records.into_iter().map(source_dto).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Remembers a folder the user picked with `pick_folder_path`.
#[tauri::command]
pub async fn add_source_folder(app: tauri::AppHandle, path: String) -> CmdResult<SourceFolderDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        if !app.state::<ApprovedTargets>().contains(Path::new(&path)) {
            return Err(CmdError::expected("Der Ordner wurde nicht in der App ausgewählt."));
        }
        let conn = lock(&state.db)?;
        add_source_folder_with_conn(&conn, Path::new(&path), &state.sensitive_dirs)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_source_folder(app: tauri::AppHandle, id: String) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        let id = catalog_removal::parse_id(&id)?;
        let conn = lock(&state.db)?;
        remove_source_folder_with_conn(&conn, id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_source_check_on_start(state: State<AppState>) -> CmdResult<bool> {
    let conn = lock(&state.db)?;
    Ok(store::source_check_on_start(&conn)?)
}

#[tauri::command]
pub fn set_source_check_on_start(state: State<AppState>, enabled: bool) -> CmdResult<()> {
    let conn = lock(&state.db)?;
    Ok(store::set_source_check_on_start(&conn, enabled)?)
}

/// Checks every source folder for model files that are not in the catalog yet.
#[tauri::command]
pub async fn scan_source_folders(app: tauri::AppHandle) -> CmdResult<Vec<SourceScanDto>> {
    tauri::async_runtime::spawn_blocking(move || scan_with_state(&app.state::<AppState>(), MAX_SCAN_ENTRIES))
        .await
        .map_err(|e| e.to_string())?
}

/// The files an import from source folders may take: a catalog folder must be set up
/// (otherwise the import would register the files where they are instead of moving
/// them) and every path must be listed by a fresh scan.
pub(super) fn accepted_import_paths(state: &AppState, requested: &[String], max_entries: usize) -> CmdResult<Vec<PathBuf>> {
    {
        let conn = lock(&state.db)?;
        if db::printer_link::get_setting(&conn, folders::SETTING_CATALOG_BASE_DIR)?.is_none() {
            return Err(CmdError::expected("Es ist noch kein Katalogordner eingerichtet. Ohne ihn können Dateien nicht aus einem Quellordner übernommen werden."));
        }
    }
    let scans = scan_with_state(state, max_entries)?;
    let accepted = validate_requested(&scans, requested)?;
    if accepted.is_empty() {
        return Err(CmdError::expected("Es wurden keine Dateien ausgewählt."));
    }
    Ok(accepted.into_iter().map(|(path, _)| path).collect())
}

/// Takes the chosen files into the catalog through the normal import job, which moves
/// them into the catalog folder. Only files a fresh scan lists for a registered source
/// folder are accepted. Returns the job to follow, like `start_import`.
#[tauri::command]
pub async fn import_source_files(app: tauri::AppHandle, paths: Vec<String>) -> CmdResult<StartedImport> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let paths = accepted_import_paths(&state, &paths, MAX_SCAN_ENTRIES)?;
        let job = submit_models(&app, ImportSource::Files, paths, None, false)?;
        Ok(StartedImport { job_id: Some(job.id.clone()) })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Remembers that the user does not want these files offered again (path and size).
#[tauri::command]
pub async fn reject_source_files(app: tauri::AppHandle, paths: Vec<String>) -> CmdResult<usize> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _catalog_share = state.import_jobs.gate.exclusive()?;
        reject_with_state(&state, &paths, MAX_SCAN_ENTRIES)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn list_rejected_count(state: State<AppState>) -> CmdResult<i64> {
    let conn = lock(&state.db)?;
    Ok(store::count_rejected_sources(&conn)?)
}

/// Forgets all declined files, so they are offered again. Returns how many.
#[tauri::command]
pub fn reset_rejected_source_files(state: State<AppState>) -> CmdResult<i64> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let conn = lock(&state.db)?;
    Ok(store::clear_rejected_sources(&conn)?)
}

#[cfg(test)]
#[path = "source_folders_tests.rs"]
mod tests;
