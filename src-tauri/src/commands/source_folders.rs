//! Source folders: folders the user wants checked for new model files (a download
//! folder, a slicer export folder). The check only reads. Files the user accepts are
//! imported through the normal import job with a catalog folder set, which moves them
//! into the catalog folder; this module never copies, moves or deletes a file itself.

use super::*;
use crate::db::sources as store;
use tauri::Manager;

use katalog_core::import::sources::{self, SourceState, StdSourceFs, MAX_SCAN_ENTRIES};
pub use katalog_core::import::sources::{SourceFolderDto, SourceScanDto};

fn source_state(state: &AppState) -> SourceState<'_> {
    SourceState { db: &state.db, sensitive_dirs: &state.sensitive_dirs, fs: &StdSourceFs }
}

pub(super) fn add_source_folder_with_conn(conn: &Connection, requested: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<SourceFolderDto> {
    let folder = sources::add_source_folder_with_conn(&StdSourceFs, conn, requested, sensitive_dirs)?;
    log::info!(target: "catalog", "Quellordner hinzugefügt");
    Ok(folder)
}

/// Forgets the folder and the declined files below it. No file is touched.
pub(super) fn remove_source_folder_with_conn(conn: &Connection, id: i64) -> CmdResult<()> {
    sources::remove_source_folder_with_conn(conn, id)?;
    log::info!(target: "catalog", "Quellordner entfernt");
    Ok(())
}

/// The core releases the database lock before walking the disk.
pub(super) fn scan_with_state(state: &AppState, max_entries: usize) -> CmdResult<Vec<SourceScanDto>> {
    Ok(sources::scan_with_state(&source_state(state), max_entries)?)
}

pub(super) fn reject_with_state(state: &AppState, requested: &[String], max_entries: usize) -> CmdResult<usize> {
    Ok(sources::reject_with_state(&source_state(state), requested, max_entries)?)
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
        Ok(records.into_iter().map(|record| sources::source_dto(&StdSourceFs, record)).collect())
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
            return Err(CmdError::coded(ErrorCode::FolderNotPicked));
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
#[cfg(test)]
pub(super) fn accepted_import_paths(state: &AppState, requested: &[String], max_entries: usize) -> CmdResult<Vec<PathBuf>> {
    accepted_import_selection(state, requested, max_entries).map(|(paths, _)| paths)
}

/// Like [`accepted_import_paths`], and also the source folders the files were listed
/// under, which the import worker holds the files to.
pub(super) fn accepted_import_selection(
    state: &AppState,
    requested: &[String],
    max_entries: usize,
) -> CmdResult<sources::ImportSelection> {
    Ok(sources::accepted_import_selection(&source_state(state), requested, max_entries)?)
}

/// Takes the chosen files into the catalog through the normal import job, which moves
/// them into the catalog folder. Only files a fresh scan lists for a registered source
/// folder are accepted. Returns the job to follow, like `start_import`.
#[tauri::command]
pub async fn import_source_files(app: tauri::AppHandle, paths: Vec<String>) -> CmdResult<StartedImport> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (paths, roots) = accepted_import_selection(&state, &paths, MAX_SCAN_ENTRIES)?;
        let job = submit_models_confined(&app, ImportSource::Files, paths, None, false, roots)?;
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
