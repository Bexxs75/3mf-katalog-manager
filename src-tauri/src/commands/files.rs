use super::runtime_environment::{DesktopFeature, RuntimeEnvironment};
use super::*;
#[cfg(test)]
use katalog_core::read::summary_to_dto;

pub(crate) use katalog_core::write::MAX_CUSTOM_IMAGE_BYTES;
// Base64 inflates by 4/3: limit for the still encoded string in `set_render_snapshot`.
const MAX_RENDER_SNAPSHOT_BASE64_BYTES: usize = MAX_CUSTOM_IMAGE_BYTES / 3 * 4 + 4;

pub use katalog_core::import::files::{ImportResultDto, SkippedFileDto};
#[cfg(test)] pub(super) use katalog_core::import::files::SkipReason;
#[tauri::command]
pub fn list_file_summaries(state: State<AppState>) -> CmdResult<Vec<FileSummaryDto>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_file_summaries(&conn).map_err(Into::into)
}

#[tauri::command]
pub async fn list_file_images(app: tauri::AppHandle, ids: Vec<String>) -> CmdResult<Vec<FileImagesDto>> {
    if ids.len() > 200 {
        return Err("at most 200 image IDs per batch".into());
    }
    let parsed = ids.iter()
        .map(|id| id.parse::<i64>().map_err(|_| format!("invalid file id: {id}")))
        .collect::<Result<Vec<_>, _>>()?;
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<Vec<FileImagesDto>> {
        let images = {
            let state = app.state::<AppState>();
            let conn = lock_db(&state)?;
            katalog_core::read::load_file_images(&conn, &parsed)?
        };
        // Encoding can be expensive for uploaded images; release the DB lock first.
        Ok(katalog_core::read::encode_file_images(images))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// All file-tag assignments in one query, because `list_file_summaries` has no
/// tags; the frontend merges them for tag filtering.
#[tauri::command]
pub fn list_all_file_tags(state: State<AppState>) -> CmdResult<HashMap<String, Vec<String>>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_all_file_tags(&conn).map_err(Into::into)
}
/// Loads the full model data `list_file_summaries` doesn't include (the detail page calls it with a single id).
#[tauri::command]
pub fn list_files_by_ids(state: State<AppState>, ids: Vec<String>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_files_by_ids(&conn, ids).map_err(Into::into)
}
#[tauri::command]
pub fn list_tag_counts(state: State<AppState>) -> CmdResult<Vec<TagCountDto>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_tag_counts(&conn).map_err(Into::into)
}

#[tauri::command]
pub fn list_print_log_entries(
    state: State<AppState>,
    file_id: String,
) -> CmdResult<Vec<PrintLogEntryDto>> {
    let conn = lock_db(&state)?;
    katalog_core::read::list_print_log_entries(&conn, file_id).map_err(Into::into)
}
#[tauri::command]
pub fn add_print_log_entry(
    state: State<AppState>,
    file_id: String,
    printed_at: String,
    note: Option<String>,
    photo_base64: Option<String>,
) -> CmdResult<PrintLogEntryDto> {
    let entry = katalog_core::write::prepare_print_log_entry(file_id, printed_at, note, photo_base64)?;
    let conn = lock_db(&state)?;
    katalog_core::write::insert_prepared_print_log_entry(&conn, entry).map_err(Into::into)
}
#[tauri::command]
pub fn delete_print_log_entry(state: State<AppState>, entry_id: String) -> CmdResult<()> {
    let _: i64 = entry_id.parse().map_err(|_| "invalid entry id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::delete_print_log_entry(&conn, entry_id).map_err(Into::into)
}
/// Dialog filters can be bypassed by typing a path; trust the bytes instead.
pub(crate) fn validate_image_format(bytes: &[u8]) -> CmdResult<()> {
    katalog_core::write::validate_image_format(bytes).map_err(Into::into)
}

/// Reads a user-picked image file with a size limit, directly through a reader
/// capped at `max_bytes + 1` instead of checking metadata() first: so the limit
/// holds even if the file grows meanwhile.
pub(crate) fn read_image_bounded(path: &std::path::Path, max_bytes: u64) -> CmdResult<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| CmdError::expected("imageUploadUnreadable"))?;
    let mut limited = file.take(max_bytes + 1);
    let mut buffer = Vec::new();
    limited
        .read_to_end(&mut buffer)
        .map_err(|_| CmdError::expected("imageUploadUnreadable"))?;
    if buffer.len() as u64 > max_bytes {
        return Err(CmdError::expected("imageUploadTooLarge"));
    }
    validate_image_format(&buffer)?;
    Ok(buffer)
}
#[tauri::command]
pub async fn pick_and_read_image(app: tauri::AppHandle) -> CmdResult<Option<String>> {
    use base64::Engine;
    let picked = app
        .dialog()
        .file()
        .add_filter("Bilder", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_file();

    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let bytes = read_image_bounded(&path, MAX_CUSTOM_IMAGE_BYTES as u64)?;
    Ok(Some(
        base64::engine::general_purpose::STANDARD.encode(bytes),
    ))
}
/// Core logic of `add_tag`: names of automatic tags in any language are
/// normalized to their key, so e.g. "Multipart" doesn't create a second tag next
/// to "mehrteilig".
pub(super) fn add_tag_with_conn(conn: &Connection, file_id: i64, tag: &str) -> CmdResult<()> {
    katalog_core::write::add_tag_with_conn(conn, file_id, tag).map_err(Into::into)
}

#[tauri::command]
pub fn add_tag(state: State<AppState>, file_id: String, tag: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    add_tag_with_conn(&conn, id, &tag)
}
#[tauri::command]
pub fn remove_tag(state: State<AppState>, file_id: String, tag: String) -> CmdResult<()> {
    let _: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::remove_tag(&conn, file_id, tag).map_err(Into::into)
}
fn blocked_tag_error(e: db::error::DbError) -> CmdError {
    match e {
        db::error::DbError::Invalid(message) => CmdError::expected(message),
        other => other.to_string().into(),
    }
}

/// Thin desktop wrapper; the logic lives in `katalog_core::read`.
pub(super) fn suggested_auto_tags_with_conn(conn: &Connection, file_id: i64) -> CmdResult<Vec<String>> {
    katalog_core::read::suggested_auto_tags_with_conn(conn, file_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_suggested_auto_tags(state: State<AppState>, file_id: String) -> CmdResult<Vec<String>> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    suggested_auto_tags_with_conn(&conn, id)
}

#[tauri::command]
pub fn block_auto_tag(state: State<AppState>, tag: String) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    db::blocked_tags::block_auto_tag(&conn, &tag).map(|_| ()).map_err(blocked_tag_error)
}

#[tauri::command]
pub fn unblock_auto_tag(state: State<AppState>, tag: String) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    db::blocked_tags::unblock_auto_tag(&conn, &tag).map_err(blocked_tag_error)
}

#[tauri::command]
pub fn get_auto_tagging(state: State<AppState>) -> CmdResult<bool> {
    let conn = lock_db(&state)?;
    db::blocked_tags::auto_tagging_enabled(&conn).map_err(blocked_tag_error)
}

#[tauri::command]
pub fn set_auto_tagging(state: State<AppState>, enabled: bool) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    db::blocked_tags::set_auto_tagging_enabled(&conn, enabled).map_err(blocked_tag_error)
}

#[tauri::command]
pub fn list_blocked_auto_tags(state: State<AppState>) -> CmdResult<Vec<String>> {
    let conn = lock_db(&state)?;
    db::blocked_tags::list_blocked_auto_tags(&conn).map_err(blocked_tag_error)
}
/// What `move_file_core` did for one file.
#[derive(Debug, PartialEq, Eq)]
enum MoveOutcome {
    /// Moved (or, for a file already lying in the folder's directory, only
    /// re-assigned in the catalog). Carries the old place for an undo.
    Moved { from_folder_id: Option<i64>, from_path: String },
    /// Already in the target folder, nothing to do.
    AlreadyThere,
}

/// A refused move: a stable machine code for the batch result plus the
/// message the single-file command has always shown.
struct MoveRefusal {
    code: &'static str,
    error: CmdError,
}

impl MoveRefusal {
    fn new(code: &'static str, error: impl Into<CmdError>) -> Self {
        Self { code, error: error.into() }
    }
}

/// Shared by the single and the batch move, so both keep the same safety
/// checks, never overwrite and roll back on a DB failure. The refusal code is
/// what lets the batch report a name clash or a missing source without
/// parsing messages.
fn move_file_core(
    conn: &Connection,
    file_id: i64,
    folder_id: Option<i64>,
    sensitive_dirs: &[PathBuf],
) -> Result<MoveOutcome, MoveRefusal> {
    let file = db::get_file(conn, file_id)
        .map_err(|e| MoveRefusal::new("error", e.to_string()))?
        .ok_or_else(|| MoveRefusal::new("not_found", "file not found".to_string()))?;
    if file.deleted_at.is_some() {
        return Err(MoveRefusal::new("in_trash", CmdError::coded(ErrorCode::FileInTrash)));
    }

    let target_dir: std::path::PathBuf = match folder_id {
        Some(fid) => {
            let folders = db::list_folders(conn).map_err(|e| MoveRefusal::new("error", e.to_string()))?;
            let folder = folders
                .iter()
                .find(|f| f.id == fid)
                .ok_or_else(|| MoveRefusal::new("folder_not_found", "folder not found".to_string()))?;
            std::path::PathBuf::from(&folder.path)
        }
        None => std::path::Path::new(&file.path)
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| MoveRefusal::new("error", "invalid current path".to_string()))?,
    };
    let sensitive = |path: &std::path::Path| {
        reject_if_sensitive_path(path, sensitive_dirs).map_err(|e| MoveRefusal::new("sensitive_path", e))
    };
    sensitive(&target_dir)?;

    let old_path = std::path::PathBuf::from(&file.path);
    let name = old_path
        .file_name()
        .ok_or_else(|| MoveRefusal::new("error", "invalid file path".to_string()))?;
    let new_path = target_dir.join(name);
    sensitive(&old_path)?;
    sensitive(&new_path)?;
    if folders::comparison_path(&old_path) == folders::comparison_path(&new_path) {
        if folder_id.is_some() && file.folder_id == folder_id {
            return Ok(MoveOutcome::AlreadyThere);
        }
        // Importing the target's own contents only needs a catalog assignment.
        db::update_file_folder(conn, file_id, folder_id, &new_path.to_string_lossy())
            .map_err(|e| MoveRefusal::new("error", e.to_string()))?;
        return Ok(MoveOutcome::Moved { from_folder_id: file.folder_id, from_path: file.path });
    }
    // Without these checks the OS error ("No such file or directory") would reach
    // the user as an unexpected fault, although the catalog is merely out of date.
    if !target_dir.is_dir() {
        return Err(MoveRefusal::new(
            "target_missing",
            CmdError::coded_with(ErrorCode::TargetFolderMissingOnDisk, [target_dir.display()]),
        ));
    }
    if !old_path.is_file() {
        return Err(MoveRefusal::new(
            "source_missing",
            CmdError::coded_with(ErrorCode::FileMissingOnDisk, [old_path.display()]),
        ));
    }
    if let Err(e) = move_file(&old_path, &new_path) {
        let code = if e.kind() == std::io::ErrorKind::AlreadyExists { "name_exists" } else { "error" };
        return Err(MoveRefusal::new(code, e.to_string()));
    }
    // The model's companion files travel with it; a clash there undoes the model move.
    let carried = match companions::carry(conn, file_id, &old_path, &new_path) {
        Ok(carried) => carried,
        Err(error) => {
            if let Err(rollback_err) = move_file(&new_path, &old_path) {
                return Err(MoveRefusal::new(
                    "error",
                    format!(
                        "Begleitdateien konnten nicht mitgenommen werden ({error}) UND Rollback der Dateiverschiebung fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}",
                        new_path.display()
                    ),
                ));
            }
            return Err(MoveRefusal::new("error", error));
        }
    };

    if let Err(db_err) =
        db::update_file_folder(conn, file_id, folder_id, &new_path.to_string_lossy())
    {
        carried.undo(conn);
        // Undo the physical move so file system and DB don't drift apart. move_file
        // never overwrites anything.
        if let Err(rollback_err) = move_file(&new_path, &old_path) {
            return Err(MoveRefusal::new(
                "error",
                format!(
                    "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Dateiverschiebung fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB verweist weiter auf {}",
                    new_path.display(),
                    old_path.display()
                ),
            ));
        }
        // files.path is UNIQUE: a catalog row already owning the target path
        // (e.g. a file missing on disk) is a name clash, not a fault.
        let path_taken = conn
            .query_row(
                "SELECT 1 FROM files WHERE path = ?1 AND id != ?2",
                rusqlite::params![new_path.to_string_lossy(), file_id],
                |_| Ok(()),
            )
            .is_ok();
        let code = if path_taken { "name_exists" } else { "error" };
        return Err(MoveRefusal::new(code, db_err.to_string()));
    }
    carried.finish();
    log::debug!(target: "datei", "verschoben: {} -> {}", old_path.display(), new_path.display());
    Ok(MoveOutcome::Moved { from_folder_id: file.folder_id, from_path: file.path })
}

/// Core logic of `move_file_to_folder`, without `State`, so it's testable.
pub(super) fn move_file_to_folder_with_conn(
    conn: &Connection,
    file_id: i64,
    folder_id: Option<i64>,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    move_file_core(conn, file_id, folder_id, sensitive_dirs)
        .map(|_| ())
        .map_err(|refusal| refusal.error)
}
/// Moves a file into a target folder's directory and updates `folder_id`/`path`.
/// `folder_id: None` leaves it where it is.
#[tauri::command]
pub fn move_file_to_folder(
    state: State<AppState>,
    file_id: String,
    folder_id: Option<String>,
) -> CmdResult<()> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let target_id: Option<i64> = folder_id
        .map(|s| {
            s.parse::<i64>()
                .map_err(|_| "invalid folder id".to_string())
        })
        .transpose()?;

    let conn = lock_db(&state)?;
    let sensitive_dirs = state.sensitive_dirs.clone();
    move_file_to_folder_with_conn(&conn, id, target_id, &sensitive_dirs)
}

/// Upper bound per `move_files_to_folder` call; keeps one call from holding the
/// catalog gate for an unbounded time.
pub(crate) const MAX_BATCH_MOVE_FILES: usize = 5000;

/// Per-file result of `move_files_to_folder`. `status` is "moved", "skipped"
/// or "failed"; `reason` is a stable code. `from_folder_id`/`from_path` are
/// only set for "moved" and are exactly what an undo call needs.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveFileResult {
    pub id: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_folder_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_path: Option<String>,
}

impl MoveFileResult {
    fn with_reason(id: &str, status: &'static str, reason: &str) -> Self {
        Self { id: id.to_string(), status, reason: Some(reason.to_string()), from_folder_id: None, from_path: None }
    }
}

/// Core of `move_files_to_folder`, without `State`. `lock` hands out the DB
/// connection per file, so the lock is not held across the whole batch (moves
/// across drives can be slow) and never nested.
pub(super) fn move_files_to_folder_with<G, C>(
    mut lock: G,
    file_ids: &[String],
    folder_id: i64,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<Vec<MoveFileResult>>
where
    G: FnMut() -> CmdResult<C>,
    C: std::ops::Deref<Target = Connection>,
{
    if file_ids.len() > MAX_BATCH_MOVE_FILES {
        return Err(CmdError::coded_with(ErrorCode::MoveTooManyFiles, [file_ids.len(), MAX_BATCH_MOVE_FILES]));
    }
    {
        // The target is the same for every file, so a wrong one fails the whole call
        // instead of producing thousands of identical per-file failures.
        let conn = lock()?;
        let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
        let folder = folders
            .iter()
            .find(|f| f.id == folder_id)
            .ok_or_else(|| "folder not found".to_string())?;
        let target_dir = std::path::PathBuf::from(&folder.path);
        reject_if_sensitive_path(&target_dir, sensitive_dirs)?;
        if !target_dir.is_dir() {
            return Err(CmdError::coded_with(ErrorCode::TargetFolderMissingOnDisk, [target_dir.display()]));
        }
    }

    let mut results = Vec::with_capacity(file_ids.len());
    for id_str in file_ids {
        let Ok(id) = id_str.parse::<i64>() else {
            results.push(MoveFileResult::with_reason(id_str, "failed", "invalid_id"));
            continue;
        };
        let conn = lock()?;
        results.push(match move_file_core(&conn, id, Some(folder_id), sensitive_dirs) {
            Ok(MoveOutcome::Moved { from_folder_id, from_path }) => MoveFileResult {
                id: id_str.clone(),
                status: "moved",
                reason: None,
                from_folder_id: from_folder_id.map(|f| f.to_string()),
                from_path: Some(from_path),
            },
            Ok(MoveOutcome::AlreadyThere) => MoveFileResult::with_reason(id_str, "skipped", "already_there"),
            Err(refusal) => {
                let status = if refusal.code == "name_exists" { "skipped" } else { "failed" };
                MoveFileResult::with_reason(id_str, status, refusal.code)
            }
        });
    }
    Ok(results)
}

/// Moves several files into one catalog folder. Every file is moved on its own
/// (same checks and rollback as `move_file_to_folder`); a refusal or failure for
/// one file never stops the rest. Undo is another call with the returned
/// `fromFolderId` per file (files grouped by old folder).
#[tauri::command]
pub fn move_files_to_folder(
    state: State<AppState>,
    file_ids: Vec<String>,
    folder_id: String,
) -> CmdResult<Vec<MoveFileResult>> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let target_id: i64 = folder_id.parse().map_err(|_| "invalid folder id".to_string())?;
    let sensitive_dirs = state.sensitive_dirs.clone();
    move_files_to_folder_with(|| lock_db(&state), &file_ids, target_id, &sensitive_dirs)
}
/// `name` ends up in `with_file_name` and must be a single harmless path
/// component (CWE-22, like `validate_folder_name`).
fn validate_file_name(name: &str) -> CmdResult<()> {
    folders::validate_name(name, true, folders::NameKind::File)
}
/// Core logic of `rename_file`, without `State`, so it's testable. Goes through
/// `rename_file_on_disk`, which never replaces an existing target.
pub(super) fn rename_file_with_conn(
    conn: &Connection,
    id: i64,
    new_name: String,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    validate_file_name(&new_name)?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    if file.deleted_at.is_some() {
        return Err(CmdError::coded(ErrorCode::FileInTrash));
    }

    let old_path = PathBuf::from(&file.path);
    let new_path = old_path.with_file_name(&new_name);
    reject_if_sensitive_path(&old_path, sensitive_dirs)?;
    reject_if_sensitive_path(&new_path, sensitive_dirs)?;

    if new_path == old_path {
        return Ok(());
    }
    if rename::target_collides(&old_path, &new_path).map_err(|e| e.to_string())? {
        return Err(CmdError::coded_with(ErrorCode::FileTargetExists, [new_path.display()]));
    }
    rename::rename_file_on_disk(&old_path, &new_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            // Created by someone else after the check above; reported like the early collision.
            CmdError::coded_with(ErrorCode::FileTargetExists, [new_path.display()])
        } else {
            e.to_string().into()
        }
    })?;

    // The model's companion files follow into the folder named after the new name.
    let carried = match companions::carry(conn, id, &old_path, &new_path) {
        Ok(carried) => carried,
        Err(error) => {
            if let Err(rollback_err) = rename::rename_file_on_disk(&new_path, &old_path) {
                return Err(format!(
                    "Begleitdateien konnten nicht mitgenommen werden ({error}) UND Rollback der Datei-Umbenennung fehlgeschlagen ({rollback_err}) - Datei heisst jetzt {}",
                    new_path.display()
                )
                .into());
            }
            return Err(error);
        }
    };
    if let Err(db_err) = db::rename_file(conn, id, &new_name, &new_path.to_string_lossy()) {
        carried.undo(conn);
        // Undo the rename so file system and DB don't drift apart.
        if let Err(rollback_err) = rename::rename_file_on_disk(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Datei-Umbenennung fehlgeschlagen ({rollback_err}) - Datei heisst jetzt {}, DB verweist weiter auf {}",
                new_path.display(),
                old_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    carried.finish();
    log::debug!(target: "datei", "umbenannt: {} -> {}", old_path.display(), new_path.display());
    Ok(())
}
/// Renames a real file on disk (same directory, never replacing a file) and updates
/// `name`/`path` in the DB accordingly.
#[tauri::command]
pub fn rename_file(state: State<AppState>, file_id: String, name: String) -> CmdResult<()> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let sensitive_dirs = state.sensitive_dirs.clone();
    rename_file_with_conn(&conn, id, name, &sensitive_dirs)
}
#[tauri::command]
pub fn set_print_status(state: State<AppState>, file_id: String, status: String) -> CmdResult<()> {
    let _: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::set_print_status(&conn, file_id, status).map_err(Into::into)
}
#[tauri::command]
pub fn set_favorite(state: State<AppState>, file_id: String, favorite: bool) -> CmdResult<()> {
    let _: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::set_favorite(&conn, file_id, favorite).map_err(Into::into)
}
#[tauri::command]
pub fn mark_file_viewed(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let _: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::mark_file_viewed(&conn, file_id).map_err(Into::into)
}
#[tauri::command]
pub fn add_to_queue(state: State<AppState>, file_id: String) -> CmdResult<i64> {
    let _: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::add_to_queue(&conn, file_id).map_err(Into::into)
}
#[tauri::command]
pub fn remove_from_queue(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let _: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    katalog_core::write::remove_from_queue(&conn, file_id).map_err(Into::into)
}
pub use katalog_core::write::QueuePositionUpdate;
#[tauri::command]
pub fn reorder_queue(state: State<AppState>, updates: Vec<QueuePositionUpdate>) -> CmdResult<()> {
    let mut conn = lock_db(&state)?;
    reorder_queue_with_conn(&mut conn, updates)
}
/// Core logic of `reorder_queue`, without `State`, so it's testable. All ids are
/// checked up front and all updates run in one transaction: never a half-applied
/// batch.
fn reorder_queue_with_conn(
    conn: &mut Connection,
    updates: Vec<QueuePositionUpdate>,
) -> CmdResult<()> {
    katalog_core::write::reorder_queue_with_conn(conn, updates).map_err(Into::into)
}
#[tauri::command]
pub async fn upload_custom_image(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_id: String,
) -> CmdResult<Option<String>> {
    use base64::Engine;
    let picked = app
        .dialog()
        .file()
        .add_filter("Bilder", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_file();

    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let bytes = read_image_bounded(&path, MAX_CUSTOM_IMAGE_BYTES as u64)?;

    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_custom_image_png(&conn, id, &bytes).map_err(|e| e.to_string())?;

    Ok(Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    )))
}
#[tauri::command]
pub fn set_render_snapshot(
    state: State<AppState>,
    file_id: String,
    image_base64: String,
) -> CmdResult<()> {
    use base64::Engine;
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    // Same limit as `upload_custom_image`. Check the base64 length first, so a
    // huge string is never decoded.
    if image_base64.len() > MAX_RENDER_SNAPSHOT_BASE64_BYTES {
        return Err(format!(
            "Bild ist zu groß - maximal {} MB erlaubt",
            MAX_CUSTOM_IMAGE_BYTES / (1024 * 1024)
        )
        .into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image_base64)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_CUSTOM_IMAGE_BYTES {
        return Err(format!(
            "Bild ist zu groß ({:.1} MB) - maximal {} MB erlaubt",
            bytes.len() as f64 / (1024.0 * 1024.0),
            MAX_CUSTOM_IMAGE_BYTES / (1024 * 1024)
        )
        .into());
    }
    let conn = lock_db(&state)?;
    db::set_render_snapshot_png(&conn, id, &bytes).map_err(|e| e.to_string().into())
}
#[tauri::command]
pub fn set_source_url(
    state: State<AppState>,
    file_id: String,
    url: Option<String>,
) -> CmdResult<()> {
    let _: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let url = validate_source_url(url)?;
    let conn = lock_db(&state)?;
    katalog_core::write::set_source_url(&conn, file_id, url).map_err(Into::into)
}
pub(crate) use katalog_core::import::files::{is_ignored_import_path, is_supported_extension};
/// Stricter than [`is_supported_extension`]: STEP can be cataloged, but most
/// slicers can't import it. OBJ is a print-ready mesh and therefore allowed.
pub(crate) fn is_sliceable_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "3mf" | "stl" | "obj"))
        .unwrap_or(false)
}
pub(crate) fn compute_content_hash(path: &Path) -> CmdResult<String> {
    katalog_core::import::files::compute_content_hash(path).map_err(Into::into)
}
/// File I/O runs on a worker without a catalog or database lock, so slow disks
/// cannot delay the window or other catalog operations.
pub(crate) fn backfill_content_hashes(state: &AppState) {
    if let Err(error) = backfill_hashes_with_reader(state, compute_content_hash) {
        log::warn!(target: "startup", "content_hash backfill: {error}");
    }
}

fn backfill_hashes_with_reader(
    state: &AppState,
    mut hash_file: impl FnMut(&Path) -> CmdResult<String>,
) -> Result<(), DbError> {
    let mut cursor = 0_i64;
    loop {
        let (batch, known_folders) = file_modified_backfill::with_catalog(state, |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, path FROM files WHERE content_hash IS NULL AND id > ?1 ORDER BY id LIMIT 200",
            )?;
            let rows = stmt.query_map([cursor], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?;
            Ok((rows.collect::<Result<Vec<_>, _>>()?, imported_paths::folder_paths(conn)?))
        })?;
        if batch.is_empty() {
            return Ok(());
        }
        cursor = batch.last().unwrap().0;
        for (id, path) in batch {
            if backup::validate_imported_local_path(&path, &known_folders).is_err() {
                continue;
            }
            match hash_file(Path::new(&path)) {
                Ok(hash) => {
                    file_modified_backfill::with_catalog(state, |conn| {
                        // A rescan or rename during the read must keep its newer data.
                        conn.execute(
                            "UPDATE files SET content_hash = ?1 WHERE id = ?2 AND path = ?3 AND content_hash IS NULL",
                            rusqlite::params![hash, id, path],
                        )?;
                        Ok(())
                    })?;
                }
                Err(error) => log::warn!(target: "startup", "content_hash backfill übersprungen: {path}: {error}"),
            }
        }
    }
}
/// Recursively collects all supported files under `path`; unreadable
/// directories are skipped. Deliberately follows NO symlinks, neither to
/// directories (endless recursion, files outside the selection) nor to files.
/// The check comes before `is_dir()`, because `is_dir()` follows symlinks.
#[cfg(test)]
pub(super) use katalog_core::import::files::collect_supported_files;
#[cfg(test)]
use katalog_core::import::files::step_metadata;
/// Geometry for the STEP preview; without the feature the view shows the placeholder.
#[cfg(feature = "step-preview")]
fn step_geometry(path: &Path) -> CmdResult<Vec<RenderMesh>> {
    // Same treatment as the sibling "3mf" arm in get_model_geometry: an
    // unreadable file is an unexpected CmdError, not CmdError::expected.
    crate::step::parse_step_geometry(path).map_err(|e| {
        use super::error::GeometryErrorCode as Code;
        let code = match &e {
            crate::step::StepError::TooLarge { .. } | crate::step::StepError::TooComplex { .. } => Code::TooLarge,
            crate::step::StepError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => Code::NotFound,
            _ => Code::Unreadable,
        };
        if matches!(code, Code::TooLarge | Code::NotFound) {
            CmdError::expected(e.to_string()).with_code(code)
        } else { CmdError::from(e.to_string()).with_code(code) }
    })
}

#[cfg(not(feature = "step-preview"))]
fn step_geometry(_path: &Path) -> CmdResult<Vec<RenderMesh>> {
    // Not a bug: this build was made without STEP support (only possible for
    // source builds). The viewer shows the typed "unsupported" error as an explanation.
    Err(CmdError::expected("STEP-Vorschau ist in diesem Build nicht enthalten").with_code(super::error::GeometryErrorCode::Unsupported))
}

pub(crate) use katalog_core::import::files::disk_modified_at;
/// Reads and stores a single file in one step; tests use this shortcut.
#[cfg(test)]
pub(crate) fn import_one(
    conn: &Connection,
    path: &Path,
    display_name: Option<&str>,
    content_hash: Option<String>,
    folder_id: Option<i64>,
) -> CmdResult<ModelFileDto> {
    let mut new_file = read_model_file(path, display_name)?;
    new_file.content_hash = content_hash;
    new_file.folder_id = folder_id;
    store_model_file(conn, &new_file)
}

/// Reads and parses a model file into the row to insert, without touching the
/// database. This is the slow part of an import (whole file read, geometry
/// parsed), so batch imports run it without holding the database lock.
#[cfg(test)]
pub(super) fn read_model_file(path: &Path, display_name: Option<&str>) -> CmdResult<NewFile> {
    read_model_file_inner(path, display_name).map_err(|message| if message == "nicht unterstütztes Dateiformat" { CmdError::expected(message) } else { CmdError::from(message) })
}

pub(super) fn read_import_model_file(path: &Path, display_name: Option<&str>) -> CmdResult<NewFile> {
    read_model_file_inner(path, display_name).map_err(CmdError::import_input)
}

fn read_model_file_inner(path: &Path, display_name: Option<&str>) -> Result<NewFile, String> {
    katalog_core::import::files::read_model_file(path, display_name)
}
/// Inserts a row prepared by [`read_model_file`] and returns it as a DTO.
#[cfg(test)]
fn store_model_file(conn: &Connection, new_file: &NewFile) -> CmdResult<ModelFileDto> {
    let id = match db::insert_file_within_tx(conn, new_file) {
        Ok(id) => id,
        // The path still belongs to a trash row (files.path is UNIQUE, even for
        // soft-deleted rows): for the user this is just a normal duplicate, not a fault.
        Err(e) if e.to_string().contains("UNIQUE constraint failed") => {
            return Err(CmdError::expected(e.to_string()));
        }
        Err(e) => return Err(e.to_string().into()),
    };
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "imported file not found after insert".to_string())?;
    let spools = db::list_filament_spools(conn).map_err(|e| e.to_string())?;
    Ok(to_dto(file, &spools))
}
/// Re-reads a cataloged file and overwrites all columns derived from it, e.g.
/// after it was re-sliced. The logic lives in the core so the web server shares it.
pub(crate) fn rescan_file(conn: &mut Connection, id: i64) -> CmdResult<ModelFileDto> {
    use katalog_core::rescan::RescanError;
    katalog_core::rescan::rescan_file(conn, id).map_err(|e| match e {
        RescanError::NotInCatalog => "Datei nicht im Katalog gefunden".to_string().into(),
        RescanError::Missing(path) => CmdError::expected(format!("Datei nicht gefunden: {path}")),
        RescanError::Unsupported => CmdError::expected("nicht unterstütztes Dateiformat"),
        RescanError::Failed(message) => message.into(),
        // Only the web variant produces the remaining kinds.
        other => format!("{other:?}").into(),
    })
}
#[tauri::command]
pub fn rescan_file_metadata(state: State<AppState>, file_id: String) -> CmdResult<ModelFileDto> {
    let id: i64 = file_id
        .parse()
        .map_err(|_| "ungueltige Datei-ID".to_string())?;
    let mut conn = lock_db(&state)?;
    rescan_file(&mut conn, id)
}
/// Expands `roots` (files and/or directories) into the supported model files
/// they contain, skips paths already present in the catalog, and imports the
/// rest. A single unreadable/unparsable file is logged and skipped rather
/// than aborting the whole batch.
/// Batched import on a held connection for unit tests; commands use `State`.
#[cfg(test)]
pub(crate) fn import_many_with_conn(
    conn: &mut Connection,
    roots: Vec<PathBuf>,
) -> CmdResult<ImportResultDto> {
    let mut db: &mut Connection = conn;
    import_many_in_batches(&mut db, roots, ImportMode::Batched).map_err(Into::into)
}

/// Archive extraction removes the entire directory on failure, so all its
/// catalog writes must succeed together.
#[cfg(test)]
pub(crate) fn import_many_with_conn_atomic(
    conn: &mut Connection,
    roots: Vec<PathBuf>,
) -> CmdResult<ImportResultDto> {
    let mut db: &mut Connection = conn;
    import_many_in_batches(&mut db, roots, ImportMode::Atomic).map_err(Into::into)
}

pub(crate) use katalog_core::import::files::{ImportDb, ImportMode, empty_import_result};
#[cfg(test)]
use katalog_core::import::files::{import_many_in_batches, IMPORT_BATCH_SIZE};
pub(super) fn import_many_selected(
    database: &mut impl ImportDb, roots: Vec<PathBuf>, mode: ImportMode,
    cancel: Option<&std::sync::atomic::AtomicBool>, selected: Option<&[PathBuf]>,
) -> CmdResult<ImportResultDto> {
    katalog_core::import::files::import_many_selected(database, roots, mode, cancel, selected).map_err(Into::into)
}
/// Formats the legacy result shape to test its pending-archive accounting.
#[cfg(test)]
pub(super) fn import_summary(result: &ImportResultDto, secs: f64) -> String {
    format!(
        "{} importiert, {} Duplikate, {} übersprungen, {} Archive offen, {:.1} s",
        result.imported.len(),
        result.duplicate_count,
        result.skipped.len(),
        result.pending_archives.len(),
        secs
    )
}
/// Separates archives (real files with an archive extension) from everything
/// else. A DIRECTORY named `x.zip` stays in the normal import.
#[cfg(test)]
fn split_archives(paths: Vec<PathBuf>) -> (Vec<PathBuf>, Vec<String>) {
    let (archives, others): (Vec<PathBuf>, Vec<PathBuf>) = paths
        .into_iter()
        .partition(|p| p.is_file() && crate::archive::is_archive_path(p));
    (
        others,
        archives
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
    )
}
#[tauri::command]
pub async fn import_files(app: tauri::AppHandle, target_folder_id: Option<String>) -> CmdResult<ImportResultDto> {
    super::import_jobs::legacy_models(app, ImportSource::Files, None, target_folder_id).await
}
#[tauri::command]
pub async fn import_folder(app: tauri::AppHandle) -> CmdResult<ImportResultDto> {
    super::import_jobs::legacy_models(app, ImportSource::Folder, None, None).await
}
#[cfg(test)]
fn import_dropped_paths(
    paths: Vec<PathBuf>, service: &ImportJobs, sensitive: &[PathBuf],
    import: impl FnOnce(Vec<PathBuf>) -> CmdResult<ImportResultDto>,
) -> CmdResult<ImportResultDto> {
    service.authorize_models(ImportSource::Dropped, &paths, true, sensitive)?;
    import(paths)
}

/// Also used by the setup dialog to adopt an existing catalog folder, which can
/// hold thousands of files. `async` + `spawn_blocking`: a synchronous command
/// runs on the UI thread and would freeze the window for the whole import.
#[tauri::command]
pub async fn import_dropped(app: tauri::AppHandle, paths: Vec<String>) -> CmdResult<ImportResultDto> {
    super::import_jobs::legacy_models(app, ImportSource::Dropped, Some(paths.into_iter().map(PathBuf::from).collect()), None).await
}
/// Pure command construction keeps platform quoting independent of the host OS.
#[derive(Debug, PartialEq)]
enum RevealArgs { Standard(Vec<String>), WindowsRaw(String) }

fn reveal_command(os: &str, path: &str) -> CmdResult<(String, RevealArgs)> {
    Ok(match os {
        "windows" => {
            if path.contains('"') { return Err(CmdError::expected("Ungültiger Windows-Pfad")); }
            let normalized = path.replace('/', "\\");
            let normalized = if let Some(unc) = normalized.strip_prefix(r"\\?\UNC\") {
                format!(r"\\{unc}")
            } else { normalized.strip_prefix(r"\\?\").unwrap_or(&normalized).to_string() };
            ("explorer".into(), RevealArgs::WindowsRaw(format!("/select,\"{normalized}\"")))
        },
        "macos" => ("open".into(), RevealArgs::Standard(vec!["-R".into(), "--".into(), path.into()])),
        _ => {
            let mut uri = String::from("file://");
            for byte in path.bytes() {
                if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
                    uri.push(byte as char);
                } else { uri.push_str(&format!("%{byte:02X}")); }
            }
            ("gdbus".into(), RevealArgs::Standard(vec!["call".into(), "--session".into(), "--timeout".into(), "3".into(),
                "--dest".into(), "org.freedesktop.FileManager1".into(), "--object-path".into(),
                "/org/freedesktop/FileManager1".into(), "--method".into(),
                "org.freedesktop.FileManager1.ShowItems".into(), format!("['{uri}']"), String::new()]))
        }
    })
}

fn reveal_target(path: &Path) -> CmdResult<(PathBuf, bool)> {
    // Preserve the catalog's directory entry (including symlinks), while
    // ensuring even a relative dash-prefixed filename cannot become an option.
    let absolute = std::path::absolute(path).map_err(katalog_core::read::geometry_io_error)?;
    if absolute.is_file() { return Ok((absolute, true)); }
    let parent = absolute.parent().filter(|p| p.is_dir()).ok_or_else(||
        CmdError::expected("Datei nicht gefunden").with_code(super::error::GeometryErrorCode::NotFound))?;
    Ok((parent.to_path_buf(), false))
}

fn reveal_catalog_path(path: &Path) -> CmdResult<()> {
    let (absolute, is_file) = reveal_target(path)?;
    if !is_file { return open_directory_in_file_manager(absolute.to_string_lossy().into_owned()); }
    let (program, args) = reveal_command(std::env::consts::OS, &absolute.to_string_lossy())?;
    let mut command = std::process::Command::new(program);
    super::external_env::sanitize_external_command(&mut command);
    match args {
        RevealArgs::Standard(args) => { command.args(args); }
        RevealArgs::WindowsRaw(raw) => {
            #[cfg(target_os = "windows")]
            { use std::os::windows::process::CommandExt; command.raw_arg(raw); }
            #[cfg(not(target_os = "windows"))]
            { let _ = raw; unreachable!("Windows arguments on another platform"); }
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Only the DBus response is awaited on this blocking worker. A failed
        // service must trigger the folder fallback; the file manager is not waited on.
        let success = command.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().is_ok_and(|s| s.success());
        if !success {
            let parent = absolute.parent().filter(|p| p.is_dir()).ok_or_else(||
                CmdError::expected("Datei nicht gefunden").with_code(super::error::GeometryErrorCode::NotFound))?;
            return open_directory_in_file_manager(parent.to_string_lossy().into_owned());
        }
    }
    #[cfg(not(target_os = "linux"))]
    command.spawn().map_err(|e| CmdError::expected(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn reveal_in_file_manager(state: State<'_, AppState>, file_id: String, environment: State<'_, RuntimeEnvironment>) -> CmdResult<()> {
    environment.require_desktop(DesktopFeature::FileManager)?;
    let id: i64 = file_id.parse().map_err(|_| CmdError::expected("invalid file id"))?;
    let path = {
        let conn = lock_db(&state)?;
        let file = db::get_file(&conn, id).map_err(|e| e.to_string())?
            .ok_or_else(|| CmdError::expected("Datei nicht gefunden").with_code(super::error::GeometryErrorCode::NotFound))?;
        PathBuf::from(file.current_path())
    };
    tauri::async_runtime::spawn_blocking(move || reveal_catalog_path(&path)).await.map_err(|e| e.to_string())?
}

/// Opens a path in the system file manager.
#[tauri::command]
pub fn open_in_file_manager(path: String, environment: State<'_, RuntimeEnvironment>) -> CmdResult<()> {
    environment.require_desktop(DesktopFeature::FileManager)?;
    open_directory_in_file_manager(path)
}

fn open_directory_in_file_manager(path: String) -> CmdResult<()> {
    // Only existing directories: a path starting with "-" could otherwise be read
    // as an option by xdg-open/open/explorer.
    if !std::path::Path::new(&path).is_dir() {
        return Err(CmdError::expected("Pfad ist kein existierendes Verzeichnis"));
    }

    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");

    super::external_env::sanitize_external_command(&mut cmd);
    let absolute = std::path::absolute(&path).map_err(|e| CmdError::expected(e.to_string()))?;
    #[cfg(target_os = "macos")]
    cmd.arg("--");
    cmd.arg(&absolute).spawn().map_err(|e| CmdError::expected(e.to_string()))?;
    Ok(())
}
// Encodes the extracted geometry as a single binary stream for
// tauri::ipc::Response: 4 bytes header length (u32 LE), then a JSON header
// padded with spaces to a multiple of 4 bytes, followed by the raw
// Float32/Uint32 buffers per mesh in header order. Every section
// (position/normal/index) consists only of 4-byte elements, so the running
// offset stays a multiple of 4 after each mesh - no extra alignment handling
// needed (see also the wire format description in src/lib/parseModelGeometry.ts).
#[tauri::command]
pub async fn get_model_geometry(
    state: State<'_, AppState>, file_id: String,
) -> CmdResult<tauri::ipc::Response> {
    let source = {
        let conn = lock_db(&state)?;
        katalog_core::read::load_geometry_source(&conn, &file_id)?
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Vec<u8>> {
        // STEP remains a desktop-only path, including its existing feature gate.
        let extension = source.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if matches!(extension.as_str(), "stp" | "step") {
            let meshes = step_geometry(&source.path)?;
            if meshes.iter().all(|m| m.indices.is_empty() || m.positions.is_empty()) {
                return Err(CmdError::from("Keine darstellbare Geometrie").with_code(GeometryErrorCode::Unreadable));
            }
            Ok(katalog_core::read::encode_render_meshes(&meshes))
        } else {
            katalog_core::read::get_model_geometry(&source.path, None).map_err(Into::into)
        }
    }).await.map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(test)]
mod tests {
    #[test]
    fn creality_import_tags_use_plate_extent_but_store_overall_dimensions() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/katalog-core/tests/fixtures/creality-3plates.3mf");
        let model = super::read_import_model_file(&path, None).unwrap();
        assert_eq!(model.dimensions_mm, Some([334.0, 326.586525, 83.0]));
        assert!(!model.tags.iter().any(|tag| tag == "grossformat" || tag == "miniatur"));
    }

    use super::*;

    fn hash_backfill_state(conn: Connection) -> AppState {
        AppState {
            import_jobs: ImportJobs::default(), db: Mutex::new(conn),
            trash_dir: PathBuf::new(), db_path: PathBuf::new(), sensitive_dirs: Vec::new(),
        }
    }

    #[test]
    fn hash_backfill_reads_without_locks_and_preserves_concurrent_changes() {
        let conn = db::connect_in_memory().unwrap();
        let dir = unique_test_dir("hash_backfill_locks");
        for name in ["hash-a.stl", "hash-b.stl", "hash-c.stl"] {
            db::test_insert_minimal_file(&conn, dir.join(name).to_str().unwrap(), None).unwrap();
        }
        let state = hash_backfill_state(conn);
        let mut reads = 0;
        backfill_hashes_with_reader(&state, |_| {
            reads += 1;
            let _gate = state.import_jobs.gate.exclusive().unwrap();
            let conn = state.db.try_lock().unwrap();
            conn.execute("UPDATE files SET content_hash = 'new' WHERE id = 2", []).unwrap();
            conn.execute("UPDATE files SET path = '/tmp/renamed.stl' WHERE id = 3", []).unwrap();
            Ok("computed".into())
        }).unwrap();
        assert_eq!(reads, 3);
        let conn = state.db.lock().unwrap();
        assert_eq!(db::get_file(&conn, 1).unwrap().unwrap().content_hash.as_deref(), Some("computed"));
        assert_eq!(db::get_file(&conn, 2).unwrap().unwrap().content_hash.as_deref(), Some("new"));
        assert!(db::get_file(&conn, 3).unwrap().unwrap().content_hash.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hash_backfill_reads_known_unc_folder() {
        let conn = db::connect_in_memory().unwrap();
        db::insert_folder_with_parent(&conn, "NAS", None, r"\\nas\share").unwrap();
        let id = db::test_insert_minimal_file(&conn, r"\\nas\share\x.stl", None).unwrap();
        let state = hash_backfill_state(conn);
        backfill_hashes_with_reader(&state, |_| Ok("hash".into())).unwrap();
        assert_eq!(db::get_file(&state.db.lock().unwrap(), id).unwrap().unwrap().content_hash.as_deref(), Some("hash"));
    }

    #[test]
    fn hash_backfill_leaves_nonlocal_rows_unhashed() {
        let conn = db::connect_in_memory().unwrap();
        let id = db::test_insert_minimal_file(&conn, r"\\192.0.2.10\probe\x", None).unwrap();
        let state = hash_backfill_state(conn);
        backfill_hashes_with_reader(&state, |_| panic!("unknown UNC reached reader")).unwrap();
        assert!(db::get_file(&state.db.lock().unwrap(), id).unwrap().unwrap().content_hash.is_none());
    }

    #[test]
    fn file_names_reject_windows_devices_and_non_components() {
        for name in ["C:x", "CON.3mf", "LPT¹.stl", "x.", "x ", "x\n.stl", "sub/x", "sub\\x"] {
            assert!(validate_file_name(name).is_err(), "{name:?}");
        }
        assert!(validate_file_name("normal..stl").is_ok());
        // Files get their own codes: the wording must not borrow words from the folder text.
        assert!(validate_file_name("a:b").unwrap_err().is(ErrorCode::FileNameForbiddenChar, &[":"]));
        assert!(validate_file_name("").unwrap_err().is(ErrorCode::FileNameEmpty, &[]));
        assert!(validate_file_name("CON.3mf").unwrap_err().is(ErrorCode::FileNameWindowsReserved, &[]));
    }

    #[test]
    fn move_uses_disk_basename_and_trashed_rows_cannot_move_or_rename() {
        let dir = unique_test_dir("move_actual_name");
        let target = dir.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let source = dir.join("actual.stl");
        std::fs::write(&source, b"model").unwrap();
        let conn = db::connect_in_memory().unwrap();
        let id = db::test_insert_minimal_file(&conn, source.to_str().unwrap(), None).unwrap();
        conn.execute("UPDATE files SET name = 'C:fake.stl'", []).unwrap();
        let folder = db::insert_folder_with_parent(&conn, "target", None, target.to_str().unwrap()).unwrap();
        move_file_to_folder_with_conn(&conn, id, Some(folder), &[]).unwrap();
        assert!(target.join("actual.stl").exists());
        db::soft_delete_file(&conn, id, Some("existing-trash"), "now").unwrap();
        assert!(move_file_to_folder_with_conn(&conn, id, Some(folder), &[]).is_err());
        assert!(rename_file_with_conn(&conn, id, "other.stl".into(), &[]).is_err());
        assert!(target.join("actual.stl").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reveal_arguments_do_not_interpret_filenames_as_options() {
        for (path, normalized) in [
            ("C:/Druck/Grüße hier (2).3mf", r"C:\Druck\Grüße hier (2).3mf"),
            ("-name.3mf", "-name.3mf"),
            (r"\\?\C:\Druck\file.3mf", r"C:\Druck\file.3mf"),
            (r"\\?\UNC\server\share\file.3mf", r"\\server\share\file.3mf"),
            (r"\\server\share\Grüße hier (2).3mf", r"\\server\share\Grüße hier (2).3mf"),
        ] {
            assert_eq!(reveal_command("windows", path).unwrap(),
                ("explorer".into(), RevealArgs::WindowsRaw(format!("/select,\"{normalized}\""))));
        }
        assert!(reveal_command("windows", "C:/bad\"name.3mf").is_err());
        assert_eq!(reveal_command("macos", "-Grüße hier.3mf").unwrap().1,
            RevealArgs::Standard(vec!["-R".into(), "--".into(), "-Grüße hier.3mf".into()]));
        let RevealArgs::Standard(args) = reveal_command("linux", "/Druck/-Grüße #'% hier.3mf").unwrap().1 else { panic!() };
        assert_eq!(args[args.len() - 2], "['file:///Druck/-Gr%C3%BC%C3%9Fe%20%23%27%25%20hier.3mf']");
        assert_eq!(args.last().unwrap(), "");
    }

    #[test]
    fn vanished_catalog_file_uses_its_existing_parent() {
        let parent = std::env::temp_dir();
        let missing = parent.join(format!("p22-missing-{}.3mf", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        assert_eq!(reveal_target(&missing).unwrap(), (parent, false));
        let (target, _) = reveal_target(Path::new("-missing.3mf")).unwrap();
        assert!(target.is_absolute());
    }

    #[test]
    fn reveal_missing_parent_is_an_expected_not_found() {
        let missing = std::env::temp_dir().join(format!("p22-nonexistent-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let error = reveal_catalog_path(&missing.join("missing/file.3mf")).unwrap_err();
        let value = serde_json::to_value(error).unwrap();
        assert_eq!(value["expected"], true);
        assert_eq!(value["code"], "notFound");
    }

    #[test]
    fn import_and_rescan_store_disk_modified_time() {
        let dir = unique_test_dir("modified_time");
        let path = dir.join("model.stp");
        std::fs::write(&path, b"ISO-10303-21;").unwrap();
        let first = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(first).unwrap();
        let mut conn = db::connect_in_memory().unwrap();
        let dto = import_one(&conn, &path, None, None, None).unwrap();
        let id = dto.id.parse().unwrap();
        assert_eq!(db::get_file(&conn, id).unwrap().unwrap().file_modified_at,
            Some(chrono::DateTime::<chrono::Utc>::from(first).to_rfc3339()));
        let second = first + std::time::Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(second).unwrap();
        rescan_file(&mut conn, id).unwrap();
        let expected = chrono::DateTime::<chrono::Utc>::from(second).to_rfc3339();
        assert_eq!(db::get_file(&conn, id).unwrap().unwrap().file_modified_at.as_deref(), Some(expected.as_str()));
        assert_eq!(db::list_file_summaries(&conn).unwrap()[0].file_modified_at.as_deref(), Some(expected.as_str()));
        let full = to_dto(db::get_file(&conn, id).unwrap().unwrap(), &[]);
        assert_eq!(serde_json::to_value(full).unwrap()["fileModifiedAt"], expected);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_summary_reports_the_real_pending_archive_count() {
        let result = ImportResultDto {
            duplicate_entries: Vec::new(),
            imported: Vec::new(),
            duplicate_count: 3,
            pending_archives: vec!["a.zip".to_string(), "b.zip".to_string()],
            skipped: vec![SkippedFileDto { path: "x.stl".to_string(), reason: SkipReason::Empty }],
        };
        assert_eq!(
            import_summary(&result, 1.34),
            "0 importiert, 3 Duplikate, 1 übersprungen, 2 Archive offen, 1.3 s"
        );
    }

    #[test]
    fn import_one_stores_slice_info_json_when_present() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let slice_info_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="9.90"/>
    <filament id="1" type="PLA" color="#112233FF" used_m="3.0" used_g="9.90"/>
  </plate>
</config>"##;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
            zip.start_file("Metadata/slice_info.config", options)
                .unwrap();
            zip.write_all(slice_info_xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("import_one_slice_info_test_{nanos}.3mf"));
        std::fs::write(&path, &buf).expect("write temp file");

        let conn = crate::db::connect_in_memory().expect("connect");
        let dto = import_one(&conn, &path, None, None, None).expect("import should succeed");

        let stored = crate::db::get_file(&conn, dto.id.parse().unwrap())
            .expect("query")
            .expect("present");
        assert!(stored.slice_info_json.is_some());
        assert!(stored.slice_info_json.unwrap().contains("9.9"));

        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn import_many_assigns_folder_id_for_folder_roots() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        let tmp = unique_test_dir("import_many_folder_id");
        let sub = tmp.join("Tabletop");
        std::fs::create_dir(&sub).unwrap();
        let file_path = sub.join("model.3mf");
        write_minimal_3mf(&file_path);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let result =
            import_many_with_conn(&mut conn, vec![tmp.clone()]).expect("import should succeed");
        assert_eq!(result.imported.len(), 1);

        let files = db::list_files(&conn).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].folder_id.is_some());

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn import_one_succeeds_inside_an_already_open_transaction() {
        // import_many_with_conn opens one transaction for the whole batch; import_one
        // must not open its own inside it.
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        let tmp = unique_test_dir("import_one_in_open_tx");
        std::fs::create_dir_all(&tmp).unwrap();
        let file_path = tmp.join("model.3mf");
        write_minimal_3mf(&file_path);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let tx = conn.transaction().expect("begin outer transaction");

        let dto = import_one(&tx, &file_path, None, None, None)
            .expect("import_one must work inside an already-open transaction");
        assert_eq!(dto.name, "model.3mf");

        tx.commit().expect("commit outer transaction");
        assert_eq!(db::list_files(&conn).unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn move_file_to_folder_updates_path_and_db() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        let tmp = unique_test_dir("move_file_to_folder");
        let src_dir = tmp.join("A");
        let dst_dir = tmp.join("B");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        let file_path = src_dir.join("model.3mf");
        write_minimal_3mf(&file_path);

        let conn = crate::db::connect_in_memory().expect("connect");
        let imported =
            import_one(&conn, &file_path, None, None, None).expect("import should succeed");
        let file_id: i64 = imported.id.parse().unwrap();

        let folder_id = db::ensure_folder_path(&conn, &tmp, &dst_dir).expect("ensure_folder_path");

        move_file_to_folder_with_conn(&conn, file_id, Some(folder_id), &[])
            .expect("move should succeed");

        let expected_path = dst_dir.join("model.3mf");
        assert!(
            expected_path.exists(),
            "file must physically exist under dst_dir after move"
        );
        assert!(
            !file_path.exists(),
            "file must no longer exist at the original location"
        );

        let after = db::get_file(&conn, file_id)
            .expect("get_file")
            .expect("file exists");
        assert_eq!(
            after.path,
            expected_path.to_string_lossy().to_string(),
            "db path must reflect the new location"
        );
        assert_eq!(
            after.folder_id,
            Some(folder_id),
            "db folder_id must reflect the target folder"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn move_file_to_folder_with_conn_rejects_sensitive_target_path() {
        let tmp = unique_test_dir("move_file_sensitive");
        let folder_dir = tmp.join("Zielordner");
        std::fs::create_dir_all(&folder_dir).unwrap();
        let file_path = tmp.join("model.3mf");
        std::fs::write(&file_path, b"dummy").unwrap();
        let sensitive = vec![tmp.clone()];

        let conn = crate::db::connect_in_memory().expect("connect");
        let folder_id =
            db::insert_folder_with_parent(&conn, "Zielordner", None, &folder_dir.to_string_lossy())
                .expect("insert folder");
        let file_id =
            db::insert_file_within_tx(&conn, &sample_new_file_for_rescan_test(&file_path))
                .expect("insert file");

        let result = move_file_to_folder_with_conn(&conn, file_id, Some(folder_id), &sensitive);
        assert!(
            result.is_err(),
            "move_file_to_folder_with_conn must reject a target folder under a sensitive directory"
        );
        assert!(
            file_path.exists(),
            "original file must be untouched after a rejected move"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn case_only_file_rename_and_db_failure_restore_exact_spelling() {
        for (old_name, new_name) in [("A.stl", "a.stl"), ("a.stl", "A.stl")] {
            for fail_db in [false, true] {
                let dir = unique_test_dir("case_only_file");
                let old = dir.join(old_name);
                let new = dir.join(new_name);
                std::fs::write(&old, b"original").unwrap();
                let conn = db::connect_in_memory().unwrap();
                let id = db::test_insert_minimal_file(&conn, &old.to_string_lossy(), None).unwrap();
                let before = db::get_file(&conn, id).unwrap().unwrap();
                if fail_db {
                    conn.execute_batch("CREATE TRIGGER block_case_rename BEFORE UPDATE ON files BEGIN SELECT RAISE(ABORT, 'blocked'); END;").unwrap();
                }
                assert_eq!(rename_file_with_conn(&conn, id, new_name.into(), &[]).is_err(), fail_db);
                let expected = if fail_db { &old } else { &new };
                let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
                assert_eq!(entries, vec![expected.file_name().unwrap()]);
                assert_eq!(std::fs::read(expected).unwrap(), b"original");
                let after = db::get_file(&conn, id).unwrap().unwrap();
                assert_eq!(after.path, expected.to_string_lossy());
                assert_eq!(after.name, if fail_db { before.name.as_str() } else { new_name });
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }

    #[test]
    fn rename_file_renames_on_disk_and_updates_name_and_path() {
        let tmp = unique_test_dir("rename_file");
        std::fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("alt.3mf");
        std::fs::write(&old_path, b"dummy").unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id =
            db::test_insert_minimal_file(&conn, &old_path.to_string_lossy(), None).unwrap();

        rename_file_with_conn(&conn, file_id, "neu.3mf".to_string(), &[])
            .expect("rename should succeed");

        let new_path = tmp.join("neu.3mf");
        assert!(
            new_path.exists(),
            "file must physically exist under the new name"
        );
        assert!(
            !old_path.exists(),
            "file must no longer exist under the old name"
        );

        let after = db::get_file(&conn, file_id).unwrap().unwrap();
        assert_eq!(after.name, "neu.3mf");
        assert_eq!(after.path, new_path.to_string_lossy().to_string());

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_file_rejects_a_name_that_already_exists_in_the_same_directory() {
        let tmp = unique_test_dir("rename_file_collision");
        std::fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("alt.3mf");
        let existing_path = tmp.join("existiert-schon.3mf");
        std::fs::write(&old_path, b"dummy").unwrap();
        std::fs::write(&existing_path, b"dummy2").unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id =
            db::test_insert_minimal_file(&conn, &old_path.to_string_lossy(), None).unwrap();

        let result = rename_file_with_conn(&conn, file_id, "existiert-schon.3mf".to_string(), &[]);
        let refusal = result.expect_err("rename_file_with_conn must reject a name that collides with an existing file");
        assert!(refusal.is(ErrorCode::FileTargetExists, &[&existing_path.display().to_string()]), "{refusal:?}");
        assert!(
            old_path.exists(),
            "original file must be untouched after a rejected rename"
        );
        assert_eq!(
            std::fs::read(&existing_path).unwrap(),
            b"dummy2",
            "the pre-existing file must not be overwritten"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_file_rejects_names_with_path_separators_or_traversal() {
        let tmp = unique_test_dir("rename_file_traversal");
        std::fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("alt.3mf");
        std::fs::write(&old_path, b"dummy").unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id =
            db::test_insert_minimal_file(&conn, &old_path.to_string_lossy(), None).unwrap();

        for bad_name in ["../escaped.3mf", "sub/dir.3mf", "..", "."] {
            let result = rename_file_with_conn(&conn, file_id, bad_name.to_string(), &[]);
            assert!(
                result.is_err(),
                "rename_file_with_conn must reject name {bad_name:?}"
            );
        }
        assert!(
            old_path.exists(),
            "original file must be untouched after rejected renames"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_file_with_conn_rejects_sensitive_target_path() {
        let tmp = unique_test_dir("rename_file_sensitive");
        std::fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("alt.3mf");
        std::fs::write(&old_path, b"dummy").unwrap();
        let sensitive = vec![tmp.clone()];

        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id =
            db::test_insert_minimal_file(&conn, &old_path.to_string_lossy(), None).unwrap();

        let result = rename_file_with_conn(&conn, file_id, "neu.3mf".to_string(), &sensitive);
        assert!(
            result.is_err(),
            "rename_file_with_conn must reject a rename under a sensitive directory"
        );
        assert!(
            old_path.exists(),
            "original file must be untouched after a rejected rename"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_file_compensates_when_db_update_fails() {
        let tmp = unique_test_dir("rename_file_compensation");
        std::fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("alt.3mf");
        std::fs::write(&old_path, b"dummy").unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id =
            db::test_insert_minimal_file(&conn, &old_path.to_string_lossy(), None).unwrap();
        // The row stays; a trigger only makes the UPDATE fail after the file was
        // renamed (otherwise get_file() would abort first).
        conn.execute_batch(&format!(
            "CREATE TRIGGER block_rename BEFORE UPDATE ON files
             WHEN NEW.id = {file_id}
             BEGIN SELECT RAISE(ABORT, 'simulierter Fehler bei rename_file'); END;"
        ))
        .unwrap();

        let result = rename_file_with_conn(&conn, file_id, "neu.3mf".to_string(), &[]);

        assert!(
            result.is_err(),
            "must surface the db::rename_file failure (0 rows affected)"
        );
        assert!(
            old_path.exists(),
            "file must be renamed back to its original name after the failed DB update"
        );
        assert!(
            !tmp.join("neu.3mf").exists(),
            "file must not remain stranded under the new name"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_file_never_overwrites_a_target_created_after_the_check() {
        let tmp = unique_test_dir("rename_file_race");
        std::fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("alt.3mf");
        let target = tmp.join("neu.3mf");
        std::fs::write(&old_path, b"original").unwrap();
        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id = db::test_insert_minimal_file(&conn, &old_path.to_string_lossy(), None).unwrap();
        let racer = target.clone();
        import_rename::set_race_hook(Some(Box::new(move |stage, _| {
            if stage == "before-file-rename" && !racer.exists() {
                std::fs::write(&racer, b"created meanwhile").unwrap();
            }
        })));
        let result = rename_file_with_conn(&conn, file_id, "neu.3mf".to_string(), &[]);
        import_rename::set_race_hook(None);
        assert!(result.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"created meanwhile");
        assert_eq!(std::fs::read(&old_path).unwrap(), b"original");
        assert_eq!(db::get_file(&conn, file_id).unwrap().unwrap().name, "alt.3mf");
        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_file_rollback_never_overwrites_a_source_recreated_meanwhile() {
        let tmp = unique_test_dir("rename_file_rollback_race");
        std::fs::create_dir_all(&tmp).unwrap();
        let old_path = tmp.join("alt.3mf");
        std::fs::write(&old_path, b"original").unwrap();
        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id = db::test_insert_minimal_file(&conn, &old_path.to_string_lossy(), None).unwrap();
        conn.execute_batch(&format!(
            "CREATE TRIGGER block_rename BEFORE UPDATE ON files WHEN NEW.id = {file_id}
             BEGIN SELECT RAISE(ABORT, 'simulierter Fehler bei rename_file'); END;"
        ))
        .unwrap();
        // Another program recreates the old name right before the rollback runs.
        let recreated = old_path.clone();
        import_rename::set_race_hook(Some(Box::new(move |stage, path| {
            if stage == "before-file-rename" && path == recreated && !recreated.exists() {
                std::fs::write(&recreated, b"recreated meanwhile").unwrap();
            }
        })));
        let result = rename_file_with_conn(&conn, file_id, "neu.3mf".to_string(), &[]);
        import_rename::set_race_hook(None);
        assert!(result.unwrap_err().to_string().contains("Rollback"));
        assert_eq!(std::fs::read(&old_path).unwrap(), b"recreated meanwhile");
        assert_eq!(std::fs::read(tmp.join("neu.3mf")).unwrap(), b"original");
        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rescan_file_updates_plate_count_and_slice_info_from_current_disk_contents() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_3mf(path: &std::path::Path, slice_info_xml: Option<&str>) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                if let Some(xml) = slice_info_xml {
                    zip.start_file("Metadata/slice_info.config", options)
                        .unwrap();
                    zip.write_all(xml.as_bytes()).unwrap();
                }
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rescan_test_{nanos}.3mf"));
        write_3mf(&path, None);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let imported = import_one(&conn, &path, None, None, None).expect("initial import");
        let id: i64 = imported.id.parse().unwrap();
        assert_eq!(imported.weight_source, "estimated");

        let slice_info_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="7.70"/>
    <filament id="1" type="PLA" color="#00FF00FF" used_m="2.5" used_g="7.70"/>
  </plate>
</config>"##;
        write_3mf(&path, Some(slice_info_xml));

        let rescanned = rescan_file(&mut conn, id).expect("rescan should succeed");
        assert_eq!(rescanned.weight_source, "slicer");
        assert!((rescanned.estimated_weight_g.expect("weight") - 7.70).abs() < 1e-6);

        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn rescan_file_updates_file_size_and_content_hash_from_current_disk_contents() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_3mf(path: &std::path::Path, slice_info_xml: Option<&str>) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                if let Some(xml) = slice_info_xml {
                    zip.start_file("Metadata/slice_info.config", options)
                        .unwrap();
                    zip.write_all(xml.as_bytes()).unwrap();
                }
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rescan_hash_test_{nanos}.3mf"));
        write_3mf(&path, None);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let imported = import_one(&conn, &path, None, None, None).expect("initial import");
        let id: i64 = imported.id.parse().unwrap();

        let before = db::get_file(&conn, id)
            .expect("get_file")
            .expect("file exists");
        let hash_before = before.content_hash.clone();
        let size_before = before.file_size_bytes;

        // Overwrites the file at the same path with DIFFERENT content (simulates a real
        // re-slice) - the embedded slice_info.config guarantees a different length.
        let slice_info_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="12.34"/>
    <filament id="1" type="PLA" color="#00FF00FF" used_m="4.0" used_g="12.34"/>
  </plate>
</config>"##;
        write_3mf(&path, Some(slice_info_xml));

        let expected_hash = compute_content_hash(&path).expect("hash new content");

        rescan_file(&mut conn, id).expect("rescan should succeed");

        let after = db::get_file(&conn, id)
            .expect("get_file")
            .expect("file exists");
        assert_ne!(
            after.file_size_bytes, size_before,
            "file_size_bytes must reflect rescanned content"
        );
        assert_ne!(
            after.content_hash, hash_before,
            "content_hash must reflect rescanned content"
        );
        assert_eq!(
            after.content_hash,
            Some(expected_hash),
            "content_hash must match hash of new disk content"
        );

        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn rescan_file_returns_error_when_file_missing_on_disk() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rescan_missing_test_{nanos}.3mf"));
        // Never written - the file doesn't exist on disk.

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let mut new_file = sample_new_file_for_rescan_test(&path);
        new_file.file_type = FileType::ThreeMf;
        let id = crate::db::insert_file(&mut conn, &new_file).expect("insert");

        let result = rescan_file(&mut conn, id);
        assert!(result.is_err());
    }
    #[cfg(unix)]
    #[test]
    fn hash_backfill_skips_devices_and_fifos_instead_of_hanging() {
        let fifo = unique_test_dir("backfill_fifo").join("pipe.stl");
        let c_path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let conn = crate::db::connect_in_memory().expect("connect");
            for path in [std::path::Path::new("/dev/zero"), fifo.as_path()] {
                let tx = conn.unchecked_transaction().unwrap();
                db::insert_file_within_tx(&tx, &sample_new_file_for_rescan_test(path)).unwrap();
                tx.commit().unwrap();
            }
            let state = hash_backfill_state(conn);
            backfill_content_hashes(&state);
            let still_missing = db::list_files_missing_content_hash(&state.db.lock().unwrap()).unwrap().len();
            let _ = done_tx.send(still_missing);
        });
        let still_missing = done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the backfill must not hang on /dev/zero or a FIFO");
        assert_eq!(still_missing, 2, "neither path gets a hash");
    }

    fn sample_new_file_for_rescan_test(path: &std::path::Path) -> NewFile {
        NewFile {
            name: "missing.3mf".to_string(),
            path: path.to_string_lossy().to_string(),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            cloud_id: None,
            sync_status: "local-only".to_string(),
            file_size_bytes: 0,
            dimensions_mm: None,
            volume_cm3: None,
            object_count: None,
            thumbnail_png: None,
            imported_at: "2026-09-13T00:00:00Z".to_string(),
            file_modified_at: None,
            materials: Vec::new(),
            metadata: BTreeMap::new(),
            tags: Vec::new(),
            print_status: "not_printed".to_string(),
            last_viewed_at: None,
            creator: None,
            content_hash: None,
            render_snapshot_png: None,
            custom_image_png: None,
            source_url: None,
            license: None,
            source: Default::default(),
            queue_position: None,
            favorite: false,
            plate_count: None,
            slice_info_json: None,
        }
    }

    #[test]
    fn move_file_to_folder_compensates_when_db_update_fails() {
        let dir = unique_test_dir("move_file_to_folder_compensation");
        std::fs::create_dir_all(dir.join("source")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        let src_path = dir.join("source/model.3mf");
        std::fs::write(&src_path, b"CONTENT").unwrap();
        let colliding_target_path = dir.join("target/model.3mf");

        let conn = db::connect_in_memory().unwrap();
        let folder_id = db::insert_folder_with_parent(
            &conn,
            "target",
            None,
            &dir.join("target").to_string_lossy(),
        )
        .unwrap();
        let file_id =
            db::test_insert_minimal_file(&conn, &src_path.to_string_lossy(), None).unwrap();
        // A second row with exactly the target path: files.path is UNIQUE, so the DB
        // update only fails after the physical move.
        db::test_insert_minimal_file(
            &conn,
            &colliding_target_path.to_string_lossy(),
            Some(folder_id),
        )
        .unwrap();

        let result = move_file_to_folder_with_conn(&conn, file_id, Some(folder_id), &[]);

        assert!(
            result.is_err(),
            "must surface the UNIQUE constraint failure from the DB update"
        );
        assert!(
            src_path.exists(),
            "source file must be moved back after the DB update failed"
        );
        assert!(
            !colliding_target_path.exists()
                || std::fs::read(&colliding_target_path).unwrap() != b"CONTENT",
            "the orphaned copy at the destination must not remain with the moved file's content"
        );
        let file_after = db::get_file(&conn, file_id).unwrap().unwrap();
        assert_eq!(
            file_after.path,
            src_path.to_string_lossy(),
            "DB must still point at the original path"
        );
    }
    /// Two folders (src, dst) with real files; returns (conn, dir, src_folder, dst_folder).
    fn batch_move_fixture(tag: &str) -> (Connection, PathBuf, i64, i64) {
        let dir = unique_test_dir(tag);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("dst")).unwrap();
        let conn = db::connect_in_memory().unwrap();
        let src = db::insert_folder_with_parent(&conn, "src", None, &dir.join("src").to_string_lossy()).unwrap();
        let dst = db::insert_folder_with_parent(&conn, "dst", None, &dir.join("dst").to_string_lossy()).unwrap();
        (conn, dir, src, dst)
    }
    fn batch_add_file(conn: &Connection, dir: &std::path::Path, sub: &str, name: &str, folder: i64) -> i64 {
        let path = dir.join(sub).join(name);
        std::fs::write(&path, format!("content of {sub}/{name}")).unwrap();
        db::test_insert_minimal_file(conn, &path.to_string_lossy(), Some(folder)).unwrap()
    }
    fn run_batch(conn: &Connection, ids: &[String], folder: i64) -> CmdResult<Vec<MoveFileResult>> {
        move_files_to_folder_with(|| Ok(conn), ids, folder, &[])
    }
    fn ids(v: &[i64]) -> Vec<String> {
        v.iter().map(|i| i.to_string()).collect()
    }

    #[test]
    fn move_files_to_folder_moves_several_files_and_reports_old_places() {
        let (conn, dir, src, dst) = batch_move_fixture("batch_move_ok");
        let a = batch_add_file(&conn, &dir, "src", "a.3mf", src);
        let b = batch_add_file(&conn, &dir, "src", "b.3mf", src);

        let res = run_batch(&conn, &ids(&[a, b]), dst).unwrap();

        assert_eq!(res.len(), 2);
        for (r, (id, name)) in res.iter().zip([(a, "a.3mf"), (b, "b.3mf")]) {
            assert_eq!(r.status, "moved");
            assert_eq!(r.id, id.to_string());
            assert_eq!(r.reason, None);
            assert_eq!(r.from_folder_id.as_deref(), Some(src.to_string().as_str()));
            assert_eq!(r.from_path.as_deref(), Some(dir.join("src").join(name).to_string_lossy().as_ref()));
            let rec = db::get_file(&conn, id).unwrap().unwrap();
            assert_eq!(rec.folder_id, Some(dst));
            assert_eq!(rec.path, dir.join("dst").join(name).to_string_lossy());
            assert!(dir.join("dst").join(name).is_file());
            assert!(!dir.join("src").join(name).exists());
        }
        // The result is enough for an undo: moving back per old folder restores everything.
        let undo = run_batch(&conn, &ids(&[a, b]), src).unwrap();
        assert!(undo.iter().all(|r| r.status == "moved"));
        assert!(dir.join("src/a.3mf").is_file() && dir.join("src/b.3mf").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_files_to_folder_mixed_batch_reports_each_file_and_changes_only_the_ok_one() {
        let (conn, dir, src, dst) = batch_move_fixture("batch_move_mixed");
        let ok = batch_add_file(&conn, &dir, "src", "ok.3mf", src);
        let clash = batch_add_file(&conn, &dir, "src", "clash.3mf", src);
        // Same name already in the target, on disk and in the catalog.
        std::fs::write(dir.join("dst/clash.3mf"), b"TARGET").unwrap();
        db::test_insert_minimal_file(&conn, &dir.join("dst/clash.3mf").to_string_lossy(), Some(dst)).unwrap();
        let there = batch_add_file(&conn, &dir, "dst", "there.3mf", dst);
        let trashed = batch_add_file(&conn, &dir, "src", "trash.3mf", src);
        db::soft_delete_file(&conn, trashed, None, "2026-01-01T00:00:00Z").unwrap();
        let vanished = batch_add_file(&conn, &dir, "src", "vanished.3mf", src);
        std::fs::remove_file(dir.join("src/vanished.3mf")).unwrap();
        let mut input = ids(&[clash, there, 987654, ok, trashed, vanished]);
        input.push("abc".into());

        let res = run_batch(&conn, &input, dst).unwrap();

        let got: Vec<(&str, &str)> = res
            .iter()
            .map(|r| (r.status, r.reason.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(
            got,
            vec![
                ("skipped", "name_exists"),
                ("skipped", "already_there"),
                ("failed", "not_found"),
                ("moved", ""),
                ("failed", "in_trash"),
                ("failed", "source_missing"),
                ("failed", "invalid_id"),
            ]
        );
        // The ok file is moved ...
        assert!(dir.join("dst/ok.3mf").is_file() && !dir.join("src/ok.3mf").exists());
        assert_eq!(db::get_file(&conn, ok).unwrap().unwrap().folder_id, Some(dst));
        // ... and nothing else changed; the existing target was not overwritten.
        assert_eq!(std::fs::read(dir.join("dst/clash.3mf")).unwrap(), b"TARGET");
        assert_eq!(std::fs::read(dir.join("src/clash.3mf")).unwrap(), b"content of src/clash.3mf");
        for (id, name) in [(clash, "clash.3mf"), (trashed, "trash.3mf"), (vanished, "vanished.3mf")] {
            let rec = db::get_file(&conn, id).unwrap().unwrap();
            assert_eq!(rec.folder_id, Some(src), "{name}");
            assert_eq!(rec.path, dir.join("src").join(name).to_string_lossy(), "{name}");
        }
        assert!(dir.join("src/trash.3mf").is_file());
        let there_rec = db::get_file(&conn, there).unwrap().unwrap();
        assert_eq!(there_rec.path, dir.join("dst/there.3mf").to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_files_to_folder_skips_a_name_taken_only_in_the_catalog() {
        let (conn, dir, src, dst) = batch_move_fixture("batch_move_catalog_clash");
        let f = batch_add_file(&conn, &dir, "src", "m.3mf", src);
        // Catalog row owns the target path although the file is gone from disk.
        db::test_insert_minimal_file(&conn, &dir.join("dst/m.3mf").to_string_lossy(), Some(dst)).unwrap();

        let res = run_batch(&conn, &ids(&[f]), dst).unwrap();

        assert_eq!((res[0].status, res[0].reason.as_deref()), ("skipped", Some("name_exists")));
        assert!(dir.join("src/m.3mf").is_file());
        assert!(!dir.join("dst/m.3mf").exists());
        assert_eq!(db::get_file(&conn, f).unwrap().unwrap().folder_id, Some(src));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_files_to_folder_rejects_more_than_the_limit_and_a_bad_target() {
        let (conn, dir, src, dst) = batch_move_fixture("batch_move_limit");
        let f = batch_add_file(&conn, &dir, "src", "m.3mf", src);

        let too_many: Vec<String> = (0..5001).map(|i| i.to_string()).collect();
        assert!(run_batch(&conn, &too_many, dst).is_err());
        // Exactly the limit is accepted (all unknown ids -> per-file failures).
        let at_limit: Vec<String> = (900_000..905_000).map(|i| i.to_string()).collect();
        assert_eq!(run_batch(&conn, &at_limit, dst).unwrap().len(), 5000);

        assert!(run_batch(&conn, &ids(&[f]), 424242).is_err(), "target must be a catalog folder");
        let sensitive = [dir.join("dst")];
        let err = move_files_to_folder_with(|| Ok(&conn), &ids(&[f]), dst, &sensitive);
        assert!(err.is_err(), "sensitive target must be refused");
        assert!(dir.join("src/m.3mf").is_file());
        assert_eq!(db::get_file(&conn, f).unwrap().unwrap().folder_id, Some(src));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_files_to_folder_assigns_a_file_lying_in_the_directory_but_in_another_folder() {
        let (conn, dir, src, dst) = batch_move_fixture("batch_move_assign");
        // Physically in dst, but catalogued under src.
        let f = batch_add_file(&conn, &dir, "dst", "m.3mf", src);

        let res = run_batch(&conn, &ids(&[f]), dst).unwrap();

        assert_eq!(res[0].status, "moved");
        assert_eq!(db::get_file(&conn, f).unwrap().unwrap().folder_id, Some(dst));
        assert!(dir.join("dst/m.3mf").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_files_to_folder_result_serializes_as_camel_case() {
        let r = MoveFileResult {
            id: "1".into(),
            status: "moved",
            reason: None,
            from_folder_id: Some("2".into()),
            from_path: Some("/a/b.3mf".into()),
        };
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            serde_json::json!({"id":"1","status":"moved","fromFolderId":"2","fromPath":"/a/b.3mf"})
        );
    }
    #[test]
    fn move_file_to_folder_explains_a_target_folder_missing_on_disk() {
        let dir = unique_test_dir("move_file_missing_target");
        std::fs::create_dir_all(dir.join("source")).unwrap();
        let src_path = dir.join("source/model.3mf");
        std::fs::write(&src_path, b"CONTENT").unwrap();

        let conn = db::connect_in_memory().unwrap();
        let folder_id = db::insert_folder_with_parent(&conn, "gone", None, &dir.join("gone").to_string_lossy()).unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &src_path.to_string_lossy(), None).unwrap();

        let err = move_file_to_folder_with_conn(&conn, file_id, Some(folder_id), &[]).unwrap_err();

        assert!(err.expected, "a vanished folder is feedback, not a fault");
        assert!(err.is(ErrorCode::TargetFolderMissingOnDisk, &[&dir.join("gone").display().to_string()]), "{err:?}");
        assert!(src_path.exists(), "the file must stay where it was");
        assert_eq!(db::get_file(&conn, file_id).unwrap().unwrap().path, src_path.to_string_lossy());
    }
    #[test]
    fn a_batch_over_the_limit_is_refused_with_the_numbers_as_params() {
        let ids: Vec<String> = (0..=MAX_BATCH_MOVE_FILES).map(|i| i.to_string()).collect();
        let err = move_files_to_folder_with(|| -> CmdResult<Box<Connection>> { unreachable!("refused before the first lock") }, &ids, 1, &[]).unwrap_err();
        assert!(err.is(ErrorCode::MoveTooManyFiles, &[&ids.len().to_string(), &MAX_BATCH_MOVE_FILES.to_string()]), "{err:?}");
    }
    #[test]
    fn move_file_to_folder_explains_a_source_file_missing_on_disk() {
        let dir = unique_test_dir("move_file_missing_source");
        std::fs::create_dir_all(dir.join("target")).unwrap();
        let src_path = dir.join("source/model.3mf");

        let conn = db::connect_in_memory().unwrap();
        let folder_id = db::insert_folder_with_parent(&conn, "target", None, &dir.join("target").to_string_lossy()).unwrap();
        let file_id = db::test_insert_minimal_file(&conn, &src_path.to_string_lossy(), None).unwrap();

        let err = move_file_to_folder_with_conn(&conn, file_id, Some(folder_id), &[]).unwrap_err();

        assert!(err.expected);
        assert!(err.is(ErrorCode::FileMissingOnDisk, &[&src_path.display().to_string()]), "{err:?}");
        assert!(!dir.join("target/model.3mf").exists(), "no empty file may be left behind");
    }
    #[test]
    fn collect_supported_files_does_not_follow_a_self_referential_symlink() {
        let dir = unique_test_dir("collect_supported_files_symlink_cycle");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cube.3mf"), b"x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, dir.join("again")).unwrap();

        let mut out = Vec::new();
        // Must terminate (no stack overflow / endless loop) and still find cube.3mf
        // exactly once.
        collect_supported_files(&dir, &mut out);

        assert_eq!(out.len(), 1);
        assert_eq!(out[0], dir.join("cube.3mf"));
    }
    #[test]
    fn collect_supported_files_does_not_traverse_a_symlink_outside_the_root() {
        let root = unique_test_dir("collect_supported_files_symlink_outside_root");
        let outside = unique_test_dir("collect_supported_files_symlink_outside_target");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.3mf"), b"x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link-out")).unwrap();
        std::fs::write(root.join("cube.3mf"), b"x").unwrap();

        let mut out = Vec::new();
        collect_supported_files(&root, &mut out);

        assert_eq!(
            out.len(),
            1,
            "must not traverse into the externally-linked directory"
        );
        assert_eq!(out[0], root.join("cube.3mf"));
    }
    #[test]
    fn collect_supported_files_does_not_follow_a_symlink_to_a_file_outside_the_root() {
        let root = unique_test_dir("collect_supported_files_file_symlink");
        let outside = unique_test_dir("collect_supported_files_file_symlink_target");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.3mf"), b"x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("secret.3mf"), root.join("link.3mf")).unwrap();
        std::fs::write(root.join("cube.3mf"), b"x").unwrap();

        let mut out = Vec::new();
        collect_supported_files(&root, &mut out);

        assert_eq!(
            out.len(),
            1,
            "must not follow a file symlink, even one matching the supported extension"
        );
        assert_eq!(out[0], root.join("cube.3mf"));
    }
    #[test]
    fn compute_content_hash_matches_previous_full_read_implementation_for_known_content() {
        let path = unique_test_dir("hash_streaming").join("test.bin");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let content: Vec<u8> = (0..5_000_000u32).map(|i| (i % 256) as u8).collect(); // 5 MB, mehrere Chunks
        std::fs::write(&path, &content).unwrap();

        let streamed = compute_content_hash(&path).unwrap();

        use sha2::{Digest, Sha256};
        let expected = format!("{:x}", Sha256::digest(&content));
        assert_eq!(
            streamed, expected,
            "streaming hash must match full-buffer hash for identical content"
        );
    }
    #[test]
    fn compute_content_hash_does_not_allocate_proportional_to_file_size() {
        // Smoke test: 50 MB must hash without panic or OOM.
        let path = unique_test_dir("hash_large").join("big.bin");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let chunk = vec![0xABu8; 1024 * 1024];
        let mut file = std::fs::File::create(&path).unwrap();
        for _ in 0..50 {
            std::io::Write::write_all(&mut file, &chunk).unwrap();
        }
        let hash = compute_content_hash(&path).unwrap();
        // 50 MB per run: remove right away, /tmp is often a RAM tmpfs.
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(hash.len(), 64);
    }
    #[test]
    fn queue_reorder_batch_is_all_or_nothing_on_a_mid_batch_failure() {
        let mut conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, "/tmp/1.3mf", None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, "/tmp/2.3mf", None).unwrap();
        // id 999999 doesn't exist; none of the valid updates may be committed.
        let updates = vec![
            QueuePositionUpdate {
                file_id: id1.to_string(),
                position: 1,
            },
            QueuePositionUpdate {
                file_id: "999999".to_string(),
                position: 2,
            },
            QueuePositionUpdate {
                file_id: id2.to_string(),
                position: 3,
            },
        ];

        let result = reorder_queue_with_conn(&mut conn, updates);

        assert!(result.is_err());
        let file1 = db::get_file(&conn, id1).unwrap().unwrap();
        let file2 = db::get_file(&conn, id2).unwrap().unwrap();
        assert_eq!(
            file1.queue_position, None,
            "kein Teil-Update darf committed sein"
        );
        assert_eq!(
            file2.queue_position, None,
            "kein Teil-Update darf committed sein"
        );
    }
    #[test]
    fn a_fully_valid_queue_reorder_batch_remains_functionally_identical() {
        let mut conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, "/tmp/1.3mf", None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, "/tmp/2.3mf", None).unwrap();
        let updates = vec![
            QueuePositionUpdate {
                file_id: id1.to_string(),
                position: 1,
            },
            QueuePositionUpdate {
                file_id: id2.to_string(),
                position: 2,
            },
        ];

        reorder_queue_with_conn(&mut conn, updates).unwrap();

        assert_eq!(
            db::get_file(&conn, id1).unwrap().unwrap().queue_position,
            Some(1)
        );
        assert_eq!(
            db::get_file(&conn, id2).unwrap().unwrap().queue_position,
            Some(2)
        );
    }
    #[test]
    fn queue_reorder_batch_rolls_back_an_already_applied_earlier_update_when_a_later_one_fails_inside_the_transaction(
    ) {
        // The variant above already fails in the up-front check. Here a trigger makes
        // the second update fail inside the transaction; the first must be rolled back.
        let mut conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, "/tmp/1.3mf", None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, "/tmp/2.3mf", None).unwrap();
        let id3 = db::test_insert_minimal_file(&conn, "/tmp/3.3mf", None).unwrap();

        conn.execute_batch(
            "CREATE TRIGGER block_second_queue_update BEFORE UPDATE ON files
             WHEN NEW.queue_position = 20
             BEGIN SELECT RAISE(ABORT, 'simulierter Fehler beim zweiten Queue-Update'); END;",
        )
        .unwrap();

        let updates = vec![
            QueuePositionUpdate {
                file_id: id1.to_string(),
                position: 10,
            },
            QueuePositionUpdate {
                file_id: id2.to_string(),
                position: 20,
            },
            QueuePositionUpdate {
                file_id: id3.to_string(),
                position: 30,
            },
        ];

        let result = reorder_queue_with_conn(&mut conn, updates);

        assert!(
            result.is_err(),
            "must surface the trigger-raised failure on the second update"
        );
        let file1 = db::get_file(&conn, id1).unwrap().unwrap();
        assert_eq!(
            file1.queue_position, None,
            "the first update must be rolled back even though it succeeded inside the transaction before the second one failed"
        );
    }
    #[test]
    fn image_format_is_checked_by_content_not_extension() {
        for bytes in [&b"\x89PNG\r\n\x1a\n"[..], &b"\xff\xd8\xff"[..], &b"RIFF\x04\0\0\0WEBP"[..]] {
            assert!(validate_image_format(bytes).is_ok());
        }
        for bytes in [&b"GIF89a"[..], &b"not an image"[..], &b""[..]] {
            let error = validate_image_format(bytes).unwrap_err();
            assert!(error.expected);
            assert_eq!(error.message, "imageUploadUnsupported");
        }
    }
    #[test]
    fn read_image_bounded_rejects_a_file_over_the_limit() {
        let path = unique_test_dir("image_over_limit").join("big.png");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![0u8; 6 * 1024 * 1024]).unwrap(); // 6 MB
        let result = read_image_bounded(&path, 5 * 1024 * 1024);
        assert!(result.is_err());
    }
    #[test]
    fn read_image_bounded_accepts_a_file_under_the_limit() {
        let path = unique_test_dir("image_under_limit").join("small.png");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
        assert!(read_image_bounded(&path, 5 * 1024 * 1024).is_ok());
    }
    #[test]
    fn is_supported_extension_accepts_stp_and_step_case_insensitively() {
        assert!(is_supported_extension(Path::new("teil.stp")));
        assert!(is_supported_extension(Path::new("teil.STEP")));
    }
    #[test]
    fn is_sliceable_extension_rejects_stp_and_step() {
        // Catalogable, but can't be opened in a slicer.
        assert!(!is_sliceable_extension(Path::new("teil.stp")));
        assert!(!is_sliceable_extension(Path::new("teil.step")));
        assert!(is_sliceable_extension(Path::new("teil.3mf")));
        assert!(is_sliceable_extension(Path::new("teil.stl")));
    }
    #[test]
    fn is_supported_and_sliceable_extension_both_accept_obj() {
        // OBJ must be true in both checks.
        assert!(is_supported_extension(Path::new("teil.obj")));
        assert!(is_supported_extension(Path::new("teil.OBJ")));
        assert!(is_sliceable_extension(Path::new("teil.obj")));
    }
    #[cfg(not(feature = "step-preview"))]
    #[test]
    fn step_metadata_yields_nothing_when_the_feature_is_off() {
        let path = unique_test_dir("step_metadata_feature_off").join("teil.stp");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;").unwrap();

        assert_eq!(step_metadata(&path), (None, None, None));
    }
    #[cfg(feature = "step-preview")]
    #[test]
    fn step_metadata_yields_nothing_for_an_empty_step_document() {
        let path = unique_test_dir("step_metadata_empty").join("teil.stp");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;").unwrap();

        assert_eq!(step_metadata(&path), (None, None, None));
    }
    #[test]
    fn reimporting_a_path_still_occupied_by_a_trash_row_is_an_expected_duplicate() {
        let path = unique_test_dir("import_duplicate_of_trashed_path").join("teil.stp");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;").unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let first = import_one(&conn, &path, None, None, None).expect("first import should succeed");
        db::soft_delete_file(&conn, first.id.parse().unwrap(), None, "2026-01-01T00:00:00Z")
            .expect("soft delete");

        // file_exists_by_path ignores soft-deleted rows, so import_one reaches the
        // INSERT, which then fails on the UNIQUE(path) constraint.
        let result = import_one(&conn, &path, None, None, None);

        let err = result.expect_err("path still occupied by a trash row must fail");
        assert!(err.expected, "a normal duplicate must be an expected error, not an unexpected fault");
    }
    #[test]
    fn import_one_catalogs_an_empty_stp_file_without_metadata() {
        let path = unique_test_dir("import_stp").join("teil.stp");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;").unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let dto =
            import_one(&conn, &path, None, None, None).expect("stp import should succeed");

        let stored = crate::db::get_file(&conn, dto.id.parse().unwrap())
            .expect("query")
            .expect("present");
        assert_eq!(stored.file_type, FileType::Stp);
        assert_eq!(stored.dimensions_mm, None);
        assert_eq!(stored.thumbnail_png, None);
    }
    #[test]
    fn import_one_catalogs_a_step_file_under_the_same_canonical_file_type_as_stp() {
        // .stp and .step must map to the same canonical DB value ("stp"), regardless
        // of the original extension.
        let path = unique_test_dir("import_step").join("teil.step");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;").unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let dto =
            import_one(&conn, &path, None, None, None).expect(".step import should succeed");

        let stored = crate::db::get_file(&conn, dto.id.parse().unwrap())
            .expect("query")
            .expect("present");
        assert_eq!(stored.file_type, FileType::Stp);
        assert_eq!(stored.file_type.as_str(), "stp");
    }
    #[test]
    fn rescan_file_refreshes_an_empty_stp_file_without_error() {
        let path = unique_test_dir("rescan_stp").join("teil.stp");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;").unwrap();

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let dto = import_one(&conn, &path, None, None, None).expect("import should succeed");
        let id: i64 = dto.id.parse().unwrap();

        let rescanned = rescan_file(&mut conn, id).expect("rescan of an stp file should succeed");
        assert_eq!(rescanned.id, dto.id);
    }
    #[test]
    fn import_one_catalogs_an_obj_file_with_dimensions_and_no_thumbnail() {
        let path = unique_test_dir("import_obj").join("wuerfel.obj");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "v 0.0 0.0 0.0\nv 10.0 0.0 0.0\nv 10.0 10.0 0.0\nv 0.0 10.0 0.0\n\
             v 0.0 0.0 10.0\nv 10.0 0.0 10.0\nv 10.0 10.0 10.0\nv 0.0 10.0 10.0\n\
             f 1 3 2\nf 1 4 3\nf 5 6 7\nf 5 7 8\nf 1 2 6\nf 1 6 5\n\
             f 4 7 3\nf 4 8 7\nf 1 8 4\nf 1 5 8\nf 2 3 7\nf 2 7 6\n",
        )
        .unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let dto =
            import_one(&conn, &path, None, None, None).expect("obj import should succeed");

        let stored = crate::db::get_file(&conn, dto.id.parse().unwrap())
            .expect("query")
            .expect("present");
        assert_eq!(stored.file_type, FileType::Obj);
        assert_eq!(stored.dimensions_mm, Some([10.0, 10.0, 10.0]));
        assert!(stored.volume_cm3.unwrap() > 0.0);
        // The preview image is created later in the frontend (snapshot), as for STL.
        assert_eq!(stored.thumbnail_png, None);
    }
    #[test]
    fn rescan_file_refreshes_an_obj_file() {
        let path = unique_test_dir("rescan_obj").join("wuerfel.obj");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "v 0.0 0.0 0.0\nv 1.0 0.0 0.0\nv 0.0 1.0 0.0\nf 1 2 3\n",
        )
        .unwrap();

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let dto = import_one(&conn, &path, None, None, None).expect("import should succeed");
        let id: i64 = dto.id.parse().unwrap();

        let rescanned = rescan_file(&mut conn, id).expect("rescan of an obj file should succeed");
        assert_eq!(rescanned.id, dto.id);
    }
    #[test]
    fn tags_enforce_length_before_inserting() {
        let conn = db::connect_in_memory().unwrap();
        let id = db::test_insert_minimal_file(&conn, "/tmp/tag-limit.obj", None).unwrap();
        assert!(add_tag_with_conn(&conn, id, &"a".repeat(100)).is_ok());
        for tag in ["a".repeat(101), "😀".repeat(51)] {
            assert!(add_tag_with_conn(&conn, id, &tag).is_err());
        }
        assert_eq!(db::list_all_file_tags(&conn).unwrap().len(), 1);
    }

    #[test]
    fn import_succeeds_with_an_overlong_filename_tag() {
        let dir = unique_test_dir("long_filename_tag");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{}.obj", "a".repeat(120)));
        std::fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let conn = db::connect_in_memory().unwrap();
        let imported = import_one(&conn, &path, None, None, None).unwrap();
        let id: i64 = imported.id.parse().unwrap();
        assert!(db::get_file(&conn, id).is_ok());
        assert!(db::list_all_file_tags(&conn).unwrap().iter()
            .all(|(_, tag)| tag.encode_utf16().count() <= 100));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn add_tag_maps_a_translated_auto_tag_name_to_its_canonical_tag() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let id = crate::db::test_insert_minimal_file(&conn, "/tmp/add_tag_alias.3mf", None).expect("insert");

        add_tag_with_conn(&conn, id, "Multipart").expect("add");
        add_tag_with_conn(&conn, id, "Zierde").expect("add");

        let mut tags: Vec<String> = crate::db::list_all_file_tags(&conn)
            .expect("tags")
            .into_iter()
            .filter(|(fid, _)| *fid == id)
            .map(|(_, t)| t)
            .collect();
        tags.sort();
        assert_eq!(tags, vec!["Zierde".to_string(), "mehrteilig".to_string()]);
    }
    #[test]
    fn add_tag_still_maps_the_ambiguous_alias_mini_when_typed_by_hand() {
        // The ambiguity of "mini" (see tagging::AMBIGUOUS_ALIASES) only applies to
        // automatic sources (file names/material) - typed in manually via add_tag,
        // "mini" stays an alias for "miniatur".
        let conn = crate::db::connect_in_memory().expect("connect");
        let id = crate::db::test_insert_minimal_file(&conn, "/tmp/add_tag_mini.3mf", None).expect("insert");

        add_tag_with_conn(&conn, id, "mini").expect("add");

        let tags: Vec<String> = crate::db::list_all_file_tags(&conn)
            .expect("tags")
            .into_iter()
            .filter(|(fid, _)| *fid == id)
            .map(|(_, t)| t)
            .collect();
        assert_eq!(tags, vec!["miniatur".to_string()]);
    }
    #[test]
    fn split_archives_separates_archive_files_from_models_and_folders() {
        let dir = unique_test_dir("split_archives");
        let zip = dir.join("Paket.ZIP");
        let tgz = dir.join("Paket.tar.gz");
        let stl = dir.join("teil.stl");
        let folder_named_like_zip = dir.join("Ordner.zip");
        std::fs::write(&zip, b"x").unwrap();
        std::fs::write(&tgz, b"x").unwrap();
        std::fs::write(&stl, b"x").unwrap();
        std::fs::create_dir(&folder_named_like_zip).unwrap();

        let (models, archives) = split_archives(vec![
            zip.clone(),
            stl.clone(),
            folder_named_like_zip.clone(),
            tgz.clone(),
        ]);

        assert_eq!(models, vec![stl, folder_named_like_zip]);
        assert_eq!(
            archives,
            vec![zip.to_string_lossy().to_string(), tgz.to_string_lossy().to_string()]
        );
    }

    /// Binary STL with one triangle; `seed` makes the content (and hash) unique.
    fn write_binary_stl(path: &Path, seed: u32) {
        let mut bytes = vec![0u8; 80];
        bytes.extend_from_slice(&1u32.to_le_bytes());
        let s = seed as f32;
        for v in [0.0f32, 0.0, 1.0, 0.0, 0.0, s, 10.0, 0.0, s, 0.0, 10.0, s + 1.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        std::fs::write(path, bytes).expect("write stl");
    }

    #[test]
    fn dropped_directory_requires_observation_or_picker_approval() {
        let dir = unique_test_dir("drop_unapproved_directory");
        write_binary_stl(&dir.join("model.stl"), 1);
        let mut conn = db::connect_in_memory().unwrap();
        let error = import_dropped_paths(vec![dir.clone()], &ImportJobs::default(), &[],
            |paths| import_many_with_conn(&mut conn, paths)).unwrap_err();
        assert_eq!(error.message, "unauthorized");
        assert!(db::list_files(&conn).unwrap().is_empty());
        assert!(db::list_folders(&conn).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn dropped_directory_observation_is_consumed_by_import() {
        let dir = unique_test_dir("drop_observed_directory");
        write_binary_stl(&dir.join("model.stl"), 1);
        let service = ImportJobs::default(); service.observe_drop(std::slice::from_ref(&dir));
        let mut conn = db::connect_in_memory().unwrap();
        let first = import_dropped_paths(vec![dir.clone()], &service, &[], |paths| import_many_with_conn(&mut conn, paths)).unwrap();
        assert_eq!(first.imported.len(), 1);
        assert_eq!(db::list_folders(&conn).unwrap().len(), 1);
        let mut second_conn = db::connect_in_memory().unwrap();
        let error = import_dropped_paths(vec![dir.clone()], &service, &[], |paths| import_many_with_conn(&mut second_conn, paths)).unwrap_err();
        assert_eq!(error.message, "unauthorized");
        assert!(db::list_folders(&second_conn).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn dropped_directory_from_picker_is_imported() {
        let dir = unique_test_dir("drop_picked_directory");
        write_binary_stl(&dir.join("model.stl"), 1);
        let service = ImportJobs::default(); service.observe_picker(&dir);
        let mut conn = db::connect_in_memory().unwrap();
        let result = import_dropped_paths(vec![dir.clone()], &service, &[], |paths| import_many_with_conn(&mut conn, paths)).unwrap();
        assert_eq!(result.imported.len(), 1); assert!(result.skipped.is_empty());
        assert_eq!(db::list_folders(&conn).unwrap().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn dropped_directory_sensitive_path_is_rejected_even_when_observed_and_approved() {
        let dir = unique_test_dir("drop_sensitive_directory");
        write_binary_stl(&dir.join("model.stl"), 1);
        let service = ImportJobs::default(); service.observe_drop(std::slice::from_ref(&dir)); service.observe_picker(&dir);
        let mut conn = db::connect_in_memory().unwrap();
        let error = import_dropped_paths(vec![dir.clone()], &service, std::slice::from_ref(&dir), |paths| import_many_with_conn(&mut conn, paths)).unwrap_err();
        assert_eq!(error.message, "unauthorized");
        assert!(db::list_files(&conn).unwrap().is_empty()); assert!(db::list_folders(&conn).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn dropped_individual_model_requires_backend_observation() {
        let dir = unique_test_dir("drop_individual_model"); let model = dir.join("model.stl"); write_binary_stl(&model, 1);
        let mut conn = db::connect_in_memory().unwrap(); let service = ImportJobs::default();
        let error = import_dropped_paths(vec![model.clone()], &service, &[], |paths| import_many_with_conn(&mut conn, paths)).unwrap_err();
        assert_eq!(error.message, "unauthorized"); assert!(db::list_files(&conn).unwrap().is_empty());
        service.observe_drop(std::slice::from_ref(&model));
        let result = import_dropped_paths(vec![model], &service, &[], |paths| import_many_with_conn(&mut conn, paths)).unwrap();
        assert_eq!(result.imported.len(), 1); assert!(result.skipped.is_empty()); assert!(db::list_folders(&conn).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn assert_archive_import_rolls_back(sql: &str, expected_error: &str) {
        let tmp = unique_test_dir("archive_atomic_failure");
        for i in 0..30 {
            write_binary_stl(&tmp.join(format!("part_{i}.stl")), i);
        }
        let mut conn = crate::db::connect_in_memory().unwrap();
        conn.execute_batch(sql).unwrap();
        let result = super::super::archives::import_extracted_dir(&mut conn, &tmp, &[]);
        assert!(result.is_err(), "archive import must fail");
        assert!(result.unwrap_err().to_string().contains(expected_error));
        assert!(db::list_files(&conn).unwrap().is_empty());
        assert!(db::list_folders(&conn).unwrap().is_empty());
        assert!(conn.is_autocommit());
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn archive_import_rolls_back_statement_failure_after_26_models() {
        assert_archive_import_rolls_back(
            "CREATE TRIGGER fail_late BEFORE INSERT ON files
             WHEN (SELECT COUNT(*) FROM files) >= 26
             BEGIN SELECT RAISE(ABORT, 'forced late insert failure'); END;",
            "forced late insert failure",
        );
    }

    #[test]
    fn archive_import_rolls_back_commit_failure_after_26_models() {
        // Inserts succeed; only COMMIT checks this deferred foreign key.
        assert_archive_import_rolls_back(
            "CREATE TABLE commit_failure (file_id INTEGER REFERENCES files(id)
                 DEFERRABLE INITIALLY DEFERRED);
             CREATE TRIGGER fail_commit AFTER INSERT ON files
             WHEN (SELECT COUNT(*) FROM files) = 27
             BEGIN INSERT INTO commit_failure VALUES (-1); END;",
            "FOREIGN KEY constraint failed",
        );
    }

    #[test]
    fn archive_import_rolls_back_folder_failure() {
        assert_archive_import_rolls_back(
            "CREATE TRIGGER fail_folder BEFORE INSERT ON folders
             BEGIN SELECT RAISE(ABORT, 'forced folder failure'); END;",
            "forced folder failure",
        );
    }

    /// Counts database accesses; every access locks the mutex only for its own duration.
    struct CountingDb<'a> {
        db: &'a Mutex<Connection>,
        accesses: usize,
    }
    impl ImportDb for CountingDb<'_> {
        fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> core::ImportResult<R>) -> core::ImportResult<R> {
            self.accesses += 1;
            let mut conn = self.db.lock().expect("lock");
            f(&mut conn)
        }
    }

    #[test]
    fn batch_import_stores_in_several_short_transactions() {
        let tmp = unique_test_dir("import_batches");
        let count = IMPORT_BATCH_SIZE * 2 + 3;
        for i in 0..count {
            write_binary_stl(&tmp.join(format!("part_{i}.stl")), i as u32);
        }
        let db = Mutex::new(crate::db::connect_in_memory().expect("connect"));
        let mut counting = CountingDb { db: &db, accesses: 0 };

        let result = import_many_in_batches(&mut counting, vec![tmp.clone()], ImportMode::Batched).expect("import");

        assert_eq!(result.imported.len(), count);
        // Three stored batches plus the short per-file duplicate checks: the lock
        // is taken many times instead of once for the whole import.
        assert!(counting.accesses > count, "only {} database accesses", counting.accesses);
        assert_eq!(db::list_files(&db.lock().unwrap()).unwrap().len(), count);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn database_stays_available_while_a_batch_import_runs() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let tmp = unique_test_dir("import_concurrent_lock");
        for i in 0..400u32 {
            write_binary_stl(&tmp.join(format!("part_{i}.stl")), i);
        }
        let db = Arc::new(Mutex::new(crate::db::connect_in_memory().expect("connect")));
        let done = Arc::new(AtomicBool::new(false));

        let worker = {
            let (db, done, tmp) = (db.clone(), done.clone(), tmp.clone());
            std::thread::spawn(move || {
                let mut access: &Mutex<Connection> = &db;
                let result = import_many_in_batches(&mut access, vec![tmp], ImportMode::Batched);
                done.store(true, Ordering::SeqCst);
                result
            })
        };
        // Wait until the import has stored its first batch, then another caller
        // (like the UI thread) must get the lock before the import is finished.
        while !done.load(Ordering::SeqCst) {
            if db.lock().map(|c| db::list_files(&c).map(|f| !f.is_empty()).unwrap_or(false)).unwrap_or(false) {
                break;
            }
            std::thread::yield_now();
        }
        assert!(!done.load(Ordering::SeqCst), "the import held the database until it was finished");

        let result = worker.join().expect("worker").expect("import");
        assert_eq!(result.imported.len(), 400);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn identical_files_in_one_import_count_as_duplicates() {
        let tmp = unique_test_dir("import_same_content");
        write_binary_stl(&tmp.join("a.stl"), 7);
        write_binary_stl(&tmp.join("b.stl"), 7);
        let mut conn = crate::db::connect_in_memory().expect("connect");

        let result = import_many_with_conn(&mut conn, vec![tmp.clone()]).expect("import");

        assert_eq!(result.imported.len(), 1);
        assert_eq!(result.duplicate_count, 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unreadable_files_are_reported_instead_of_skipped_silently() {
        let tmp = unique_test_dir("import_skipped");
        write_binary_stl(&tmp.join("good.stl"), 1);
        std::fs::write(tmp.join("empty.stl"), b"").unwrap();
        std::fs::write(tmp.join("no_triangles.stl"), b"solid x\nendsolid x\n").unwrap();
        std::fs::write(tmp.join("broken.3mf"), b"not a zip").unwrap();
        let mut conn = crate::db::connect_in_memory().expect("connect");

        let result = import_many_with_conn(&mut conn, vec![tmp.clone()]).expect("import");

        assert_eq!(result.imported.len(), 1);
        let mut skipped: Vec<(String, SkipReason)> = result
            .skipped
            .iter()
            .map(|s| (Path::new(&s.path).file_name().unwrap().to_string_lossy().to_string(), s.reason))
            .collect();
        skipped.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            skipped,
            vec![
                ("broken.3mf".to_string(), SkipReason::Invalid),
                ("empty.stl".to_string(), SkipReason::Empty),
                ("no_triangles.stl".to_string(), SkipReason::Invalid),
            ]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn importing_the_same_folder_again_adds_nothing() {
        let tmp = unique_test_dir("import_twice");
        for i in 0..3u32 {
            write_binary_stl(&tmp.join(format!("part_{i}.stl")), i);
        }
        let mut conn = crate::db::connect_in_memory().expect("connect");

        import_many_with_conn(&mut conn, vec![tmp.clone()]).expect("first import");
        let again = import_many_with_conn(&mut conn, vec![tmp.clone()]).expect("second import");

        assert!(again.imported.is_empty());
        assert_eq!(db::list_files(&conn).unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}




#[tauri::command]
pub fn set_license(state: State<AppState>, file_ids: Vec<String>, license: Option<String>) -> CmdResult<()> {
    for id in &file_ids { let _: i64 = id.parse().map_err(|_| "invalid file id".to_string())?; }
    let mut conn = lock_db(&state)?;
    katalog_core::write::set_license(&mut conn, file_ids, license).map_err(Into::into)
}

#[cfg(test)]
mod source_platform_tests;
