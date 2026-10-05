use super::*;

pub(crate) const MAX_CUSTOM_IMAGE_BYTES: usize = 5 * 1024 * 1024;
// Base64 inflates by 4/3: limit for the still encoded string in `set_render_snapshot`.
const MAX_RENDER_SNAPSHOT_BASE64_BYTES: usize = MAX_CUSTOM_IMAGE_BYTES / 3 * 4 + 4;

/// Slim projection of `ModelFileDto` for grid and list: without `customImage`,
/// `materials` and `tags`, which only the detail page loads via
/// `list_files_by_ids([id])`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummaryDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub file_type: String,
    // Empty string = no folder, as in `ModelFileDto.folder_id` and the frontend.
    pub folder_id: String,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub imported_at: String,
    pub file_modified_at: Option<String>,
    pub print_status: String,
    pub favorite: bool,
    pub queue_position: Option<i64>,
    pub has_thumbnail: bool,
    pub has_render_snapshot: bool,
    pub creator: Option<String>,
    pub last_viewed_at: Option<String>,
    pub content_hash: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCountDto {
    pub label: String,
    pub count: i64,
    pub color_hue: i64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    #[serde(skip)]
    pub(super) duplicate_entries: Vec<super::import_jobs::DuplicateEntry>,
    pub imported: Vec<ModelFileDto>,
    pub duplicate_count: i64,
    /// Individually picked/dropped archives - NOT imported here, but handled by the
    /// frontend through the extract dialog.
    pub pending_archives: Vec<String>,
    /// Files that could not be imported, so the UI can name them.
    pub skipped: Vec<SkippedFileDto>,
}
/// Why a file was left out of an import. The UI shows a translated text per
/// reason; the detailed error only goes to the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    /// The file has 0 bytes.
    Empty,
    /// Not a usable model: damaged, a different format, no geometry, or not readable.
    Invalid,
    /// The catalog could not store it - an app-side error worth a problem report.
    Failed,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFileDto {
    pub path: String,
    pub reason: SkipReason,
}
#[tauri::command]
pub fn list_file_summaries(state: State<AppState>) -> CmdResult<Vec<FileSummaryDto>> {
    let conn = lock_db(&state)?;
    let summaries = db::list_file_summaries(&conn).map_err(|e| e.to_string())?;
    Ok(summaries
        .into_iter()
        .map(|s| FileSummaryDto {
            id: s.id.to_string(),
            name: s.name,
            path: s.path,
            file_type: s.file_type.as_str().to_string(),
            folder_id: s.folder_id.map(|id| id.to_string()).unwrap_or_default(),
            file_size_bytes: s.file_size_bytes,
            dimensions_mm: s.dimensions_mm,
            volume_cm3: s.volume_cm3,
            object_count: s.object_count,
            imported_at: s.imported_at,
            file_modified_at: s.file_modified_at,
            print_status: s.print_status,
            favorite: s.favorite,
            queue_position: s.queue_position,
            has_thumbnail: s.has_thumbnail,
            has_render_snapshot: s.has_render_snapshot,
            creator: s.creator,
            last_viewed_at: s.last_viewed_at,
            content_hash: s.content_hash,
        })
        .collect())
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileImagesDto {
    pub id: String,
    pub thumbnail_image: Option<String>,
    pub render_snapshot_image: Option<String>,
    pub custom_image: Option<String>,
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
            db::list_file_images(&conn, &parsed).map_err(|e| e.to_string())?
        };
        // Encoding can be expensive for uploaded images; release the DB lock first.
        Ok(images.into_iter().map(|images| FileImagesDto {
            id: images.id.to_string(),
            thumbnail_image: encode_image(images.thumbnail_png),
            render_snapshot_image: encode_image(images.render_snapshot_png),
            custom_image: encode_image(images.custom_image_png),
        }).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// All file-tag assignments in one query, because `list_file_summaries` has no
/// tags; the frontend merges them for tag filtering.
#[tauri::command]
pub fn list_all_file_tags(state: State<AppState>) -> CmdResult<HashMap<String, Vec<String>>> {
    let conn = lock_db(&state)?;
    let pairs = db::list_all_file_tags(&conn).map_err(|e| e.to_string())?;
    let mut by_file: HashMap<String, Vec<String>> = HashMap::new();
    for (file_id, tag) in pairs {
        by_file.entry(file_id.to_string()).or_default().push(tag);
    }
    Ok(by_file)
}
/// Loads the full model data `list_file_summaries` doesn't include (the detail page calls it with a single id).
#[tauri::command]
pub fn list_files_by_ids(state: State<AppState>, ids: Vec<String>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let parsed_ids: Vec<i64> = ids
        .iter()
        .map(|id| id.parse().map_err(|_| format!("invalid file id: {id}")))
        .collect::<Result<_, String>>()?;
    let files = db::list_files_by_ids(&conn, &parsed_ids).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}
#[tauri::command]
pub fn list_tag_counts(state: State<AppState>) -> CmdResult<Vec<TagCountDto>> {
    let conn = lock_db(&state)?;
    let tags = db::list_tag_counts(&conn).map_err(|e| e.to_string())?;
    Ok(tags
        .into_iter()
        .map(|t| TagCountDto {
            label: t.name,
            count: t.count,
            color_hue: t.color_hue,
        })
        .collect())
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintLogEntryDto {
    pub id: String,
    pub printed_at: String,
    pub note: Option<String>,
    pub photo_image: Option<String>,
}
fn print_log_entry_to_dto(record: db::models::PrintLogEntryRecord) -> PrintLogEntryDto {
    PrintLogEntryDto {
        id: record.id.to_string(),
        printed_at: record.printed_at,
        note: record.note,
        photo_image: encode_image(record.photo_png),
    }
}
#[tauri::command]
pub fn list_print_log_entries(
    state: State<AppState>,
    file_id: String,
) -> CmdResult<Vec<PrintLogEntryDto>> {
    let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let entries = db::list_print_log_entries(&conn, fid).map_err(|e| e.to_string())?;
    Ok(entries.into_iter().map(print_log_entry_to_dto).collect())
}
#[tauri::command]
pub fn add_print_log_entry(
    state: State<AppState>,
    file_id: String,
    printed_at: String,
    note: Option<String>,
    photo_base64: Option<String>,
) -> CmdResult<PrintLogEntryDto> {
    use base64::Engine;
    let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;

    let photo_png = photo_base64
        .map(|b64| {
            base64::engine::general_purpose::STANDARD
                .decode(&b64)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    if let Some(bytes) = &photo_png {
        if bytes.len() > MAX_CUSTOM_IMAGE_BYTES {
            return Err(format!(
                "Bild ist zu groß ({:.1} MB) - maximal {} MB erlaubt",
                bytes.len() as f64 / (1024.0 * 1024.0),
                MAX_CUSTOM_IMAGE_BYTES / (1024 * 1024)
            )
            .into());
        }
    }

    let conn = lock_db(&state)?;
    let new_entry = db::models::NewPrintLogEntry {
        file_id: fid,
        printed_at,
        note,
        photo_png,
    };
    let id = db::insert_print_log_entry(&conn, &new_entry).map_err(|e| e.to_string())?;
    let entries = db::list_print_log_entries(&conn, fid).map_err(|e| e.to_string())?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| "entry not found after insert".to_string())?;
    Ok(print_log_entry_to_dto(entry))
}
#[tauri::command]
pub fn delete_print_log_entry(state: State<AppState>, entry_id: String) -> CmdResult<()> {
    let id: i64 = entry_id
        .parse()
        .map_err(|_| "invalid entry id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_print_log_entry(&conn, id).map_err(|e| e.to_string().into())
}
/// Dialog filters can be bypassed by typing a path; trust the bytes instead.
pub(crate) fn validate_image_format(bytes: &[u8]) -> CmdResult<()> {
    let supported = bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(&b"WEBP"[..]));
    if supported { Ok(()) } else { Err(CmdError::expected("imageUploadUnsupported")) }
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
fn add_tag_with_conn(conn: &Connection, file_id: i64, tag: &str) -> CmdResult<()> {
    db::add_tag_to_file(conn, file_id, &tagging::canonical_tag(tag)).map_err(|e| e.to_string().into())
}

#[tauri::command]
pub fn add_tag(state: State<AppState>, file_id: String, tag: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    add_tag_with_conn(&conn, id, &tag)
}
#[tauri::command]
pub fn remove_tag(state: State<AppState>, file_id: String, tag: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    // No normalization here (unlike add_tag): the frontend always sends the name
    // actually stored - normalizing would make an alias tag (e.g. the ambiguous
    // "mini") impossible to remove once it no longer matches the key.
    db::remove_tag_from_file(&conn, id, &tag).map_err(|e| e.to_string().into())
}
/// Core logic of `move_file_to_folder`, without `State`, so it's testable.
pub(super) fn move_file_to_folder_with_conn(
    conn: &Connection,
    file_id: i64,
    folder_id: Option<i64>,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    let file = db::get_file(conn, file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;

    let target_dir: std::path::PathBuf = match folder_id {
        Some(fid) => {
            let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
            let folder = folders
                .iter()
                .find(|f| f.id == fid)
                .ok_or_else(|| "folder not found".to_string())?;
            std::path::PathBuf::from(&folder.path)
        }
        None => std::path::Path::new(&file.path)
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| "invalid current path".to_string())?,
    };
    reject_if_sensitive_path(&target_dir, sensitive_dirs)?;

    let new_path = target_dir.join(&file.name);
    let old_path = std::path::PathBuf::from(&file.path);
    move_file(&old_path, &new_path).map_err(|e| e.to_string())?;

    if let Err(db_err) =
        db::update_file_folder(conn, file_id, folder_id, &new_path.to_string_lossy())
    {
        // Undo the physical move so file system and DB don't drift apart. move_file
        // never overwrites anything.
        if let Err(rollback_err) = move_file(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Dateiverschiebung fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB verweist weiter auf {}",
                new_path.display(),
                old_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    log::debug!(target: "datei", "verschoben: {} -> {}", old_path.display(), new_path.display());
    Ok(())
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
/// `name` ends up in `with_file_name` and must be a single harmless path
/// component (CWE-22, like `validate_folder_name`).
fn validate_file_name(name: &str) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err(CmdError::expected("Dateiname darf nicht leer sein"));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(CmdError::expected("Dateiname darf keine Pfad-Trennzeichen enthalten"));
    }
    if name == "." || name == ".." {
        return Err(CmdError::expected("Ungueltiger Dateiname"));
    }
    Ok(())
}
/// Core logic of `rename_file`, without `State`, so it's testable. `fs::rename`
/// instead of `move_file`, because source and target are in the same directory.
fn rename_file_with_conn(
    conn: &Connection,
    id: i64,
    new_name: String,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    validate_file_name(&new_name)?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;

    let old_path = PathBuf::from(&file.path);
    let new_path = old_path.with_file_name(&new_name);
    reject_if_sensitive_path(&old_path, sensitive_dirs)?;
    reject_if_sensitive_path(&new_path, sensitive_dirs)?;

    if new_path == old_path {
        return Ok(());
    }
    if new_path.exists() {
        return Err(CmdError::expected(format!(
            "Zieldatei existiert bereits: {}",
            new_path.display()
        )));
    }
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    if let Err(db_err) = db::rename_file(conn, id, &new_name, &new_path.to_string_lossy()) {
        // Undo the rename so file system and DB don't drift apart.
        if let Err(rollback_err) = std::fs::rename(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Datei-Umbenennung fehlgeschlagen ({rollback_err}) - Datei heisst jetzt {}, DB verweist weiter auf {}",
                new_path.display(),
                old_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    log::debug!(target: "datei", "umbenannt: {} -> {}", old_path.display(), new_path.display());
    Ok(())
}
/// Renames a real file on disk (`std::fs::rename`, same directory) and updates
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
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_print_status(&conn, id, &status).map_err(|e| e.to_string().into())
}
#[tauri::command]
pub fn set_favorite(state: State<AppState>, file_id: String, favorite: bool) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_favorite(&conn, id, favorite).map_err(|e| e.to_string().into())
}
#[tauri::command]
pub fn mark_file_viewed(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::mark_file_viewed(&conn, id).map_err(|e| e.to_string().into())
}
#[tauri::command]
pub fn add_to_queue(state: State<AppState>, file_id: String) -> CmdResult<i64> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let next = db::max_queue_position(&conn)
        .map_err(|e| e.to_string())?
        .unwrap_or(0)
        + 1;
    db::set_queue_position(&conn, id, Some(next)).map_err(|e| e.to_string())?;
    Ok(next)
}
#[tauri::command]
pub fn remove_from_queue(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_queue_position(&conn, id, None).map_err(|e| e.to_string().into())
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuePositionUpdate {
    pub file_id: String,
    pub position: i64,
}
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
    let mut parsed = Vec::with_capacity(updates.len());
    for update in updates {
        let id: i64 = update
            .file_id
            .parse()
            .map_err(|_| "invalid file id".to_string())?;
        if db::get_file(conn, id).map_err(|e| e.to_string())?.is_none() {
            return Err(format!(
                "Datei mit id {id} nicht gefunden - Batch wird nicht angewendet"
            )
            .into());
        }
        parsed.push((id, update.position));
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (id, position) in parsed {
        db::set_queue_position(&tx, id, Some(position)).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
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
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let url = validate_source_url(url)?;
    let conn = lock_db(&state)?;
    db::set_source_url(&conn, id, url.as_deref()).map_err(|e| e.to_string().into())
}
pub(crate) fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_lowercase().as_str(),
                "3mf" | "stl" | "stp" | "step" | "obj"
            )
        })
        .unwrap_or(false)
}
/// Stricter than [`is_supported_extension`]: STEP can be cataloged, but most
/// slicers can't import it. OBJ is a print-ready mesh and therefore allowed.
pub(crate) fn is_sliceable_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "3mf" | "stl" | "obj"))
        .unwrap_or(false)
}
pub(crate) fn compute_content_hash(path: &Path) -> CmdResult<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    const CHUNK_SIZE: usize = 1024 * 1024;
    // Paths can come from an imported backup: `/dev/zero` would be read forever,
    // a FIFO would block the startup backfill.
    let file = crate::safe_file::open_regular(path).map_err(|e| e.to_string())?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];
    loop {
        let bytes_read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
/// One-time content_hash backfill at startup for files imported before the
/// column existed. Needs file access, so it runs here instead of in db::init.
/// Missing files are logged and skipped.
pub(crate) fn backfill_content_hashes(conn: &Connection) {
    let missing = match db::list_files_missing_content_hash(conn) {
        Ok(rows) => rows,
        Err(e) => {
            log::error!(target: "startup", "content_hash backfill: query failed: {e}");
            return;
        }
    };

    for (id, path) in missing {
        match compute_content_hash(Path::new(&path)) {
            Ok(hash) => {
                if let Err(e) = db::set_content_hash(conn, id, &hash) {
                    log::warn!(target: "startup", "content_hash backfill übersprungen (Speichern fehlgeschlagen): {path}: {e}");
                }
            }
            // compute_content_hash already logged the fault itself; this runs on every
            // startup until the file is reachable again, so it stays at Warn, not Error.
            Err(e) => {
                log::warn!(target: "startup", "content_hash backfill übersprungen: {path}: {e}");
            }
        }
    }
}
/// Recursively collects all supported files under `path`; unreadable
/// directories are skipped. Deliberately follows NO symlinks, neither to
/// directories (endless recursion, files outside the selection) nor to files.
/// The check comes before `is_dir()`, because `is_dir()` follows symlinks.
pub(super) fn collect_supported_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_symlink() {
        return;
    }
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect_supported_files(&entry.path(), out);
        }
    } else if is_supported_extension(path) {
        out.push(path.to_path_buf());
    }
}
/// The only place where the STEP preview shows up in the metadata path. On any
/// error the file stays catalogable and just gets no automatic metadata.
#[cfg(feature = "step-preview")]
fn step_metadata(path: &Path) -> (Option<[f64; 3]>, Option<f64>, Option<i64>) {
    match crate::step::parse_step_file(path) {
        Ok(doc) => (
            doc.dimensions_mm,
            doc.volume_cm3,
            Some(doc.object_count as i64),
        ),
        Err(err) => {
            log::warn!(target: "step", "metadata not readable ({}): {err}", path.display());
            (None, None, None)
        }
    }
}

#[cfg(not(feature = "step-preview"))]
fn step_metadata(_path: &Path) -> (Option<[f64; 3]>, Option<f64>, Option<i64>) {
    (None, None, None)
}

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

/// A missing or unsupported filesystem timestamp never prevents import.
pub(crate) fn disk_modified_at(path: &Path) -> Option<String> {
    std::fs::metadata(path).ok()?.modified().ok()
        .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339())
}

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
pub(super) fn read_model_file(path: &Path, display_name: Option<&str>) -> CmdResult<NewFile> {
    let file_name = display_name.map(|n| n.to_string()).unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unbenannt")
            .to_string()
    });
    let file_size_bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len() as i64;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    let (
        file_type,
        dimensions_mm,
        volume_cm3,
        object_count,
        materials,
        metadata,
        thumbnail_png,
        plate_count,
        slice_info_json,
    ) = match extension.as_deref() {
        Some("3mf") => {
            let doc = threemf::parse_3mf_file(path).map_err(|e| e.to_string())?;
            (
                FileType::ThreeMf,
                doc.dimensions_mm,
                doc.volume_cm3,
                Some(doc.object_count as i64),
                doc.materials
                    .into_iter()
                    .map(|m| MaterialRecord {
                        name: m.name,
                        display_color: m.display_color,
                    })
                    .collect::<Vec<_>>(),
                doc.metadata,
                doc.thumbnail_png,
                doc.plate_count.map(|c| c as i64),
                doc.slice_info.and_then(|s| serde_json::to_string(&s).ok()),
            )
        }
        Some("stl") => {
            let doc = stl::parse_stl_file(path).map_err(|e| e.to_string())?;
            (
                FileType::Stl,
                doc.dimensions_mm,
                doc.volume_cm3,
                None,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        Some("stp") | Some("step") => {
            let (dimensions_mm, volume_cm3, object_count) = step_metadata(path);
            (
                FileType::Stp,
                dimensions_mm,
                volume_cm3,
                object_count,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        Some("obj") => {
            let doc = obj::parse_obj_file(path).map_err(|e| e.to_string())?;
            (
                FileType::Obj,
                doc.dimensions_mm,
                doc.volume_cm3,
                None,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        _ => return Err(CmdError::expected("nicht unterstütztes Dateiformat")),
    };

    let tags = tagging::suggest_tags(&TaggingContext {
        file_name: &file_name,
        dimensions_mm,
        object_count,
        materials: &materials,
    });

    Ok(NewFile {
        name: file_name,
        path: path.to_string_lossy().to_string(),
        file_type,
        folder_id: None,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes,
        dimensions_mm,
        volume_cm3,
        object_count,
        thumbnail_png,
        imported_at: chrono::Utc::now().to_rfc3339(),
        file_modified_at: disk_modified_at(path),
        materials,
        metadata: metadata.clone(),
        tags,
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: metadata.get("Designer").cloned(),
        content_hash: None,
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        queue_position: None,
        favorite: false,
        plate_count,
        slice_info_json,
    })
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
/// after it was re-sliced. If the file is missing, it aborts before the DB is
/// changed.
pub(crate) fn rescan_file(conn: &mut Connection, id: i64) -> CmdResult<ModelFileDto> {
    let existing = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Datei nicht im Katalog gefunden".to_string())?;
    let path = Path::new(&existing.path);
    if !path.exists() {
        return Err(CmdError::expected(format!("Datei nicht gefunden: {}", existing.path)));
    }
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    let file_size_bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len() as i64;
    let content_hash = compute_content_hash(path).ok();

    let update = match extension.as_deref() {
        Some("3mf") => {
            let doc = threemf::parse_3mf_file(path).map_err(|e| e.to_string())?;
            ScannedMetadataUpdate {
                dimensions_mm: doc.dimensions_mm,
                volume_cm3: doc.volume_cm3,
                object_count: Some(doc.object_count as i64),
                thumbnail_png: doc.thumbnail_png,
                plate_count: doc.plate_count.map(|c| c as i64),
                slice_info_json: doc.slice_info.and_then(|s| serde_json::to_string(&s).ok()),
                materials: doc
                    .materials
                    .into_iter()
                    .map(|m| MaterialRecord {
                        name: m.name,
                        display_color: m.display_color,
                    })
                    .collect(),
                metadata: doc.metadata,
                file_size_bytes,
                file_modified_at: disk_modified_at(path),
                content_hash,
            }
        }
        Some("stl") => {
            let doc = stl::parse_stl_file(path).map_err(|e| e.to_string())?;
            ScannedMetadataUpdate {
                dimensions_mm: doc.dimensions_mm,
                volume_cm3: doc.volume_cm3,
                object_count: None,
                thumbnail_png: None,
                plate_count: None,
                slice_info_json: None,
                materials: Vec::new(),
                metadata: BTreeMap::new(),
                file_size_bytes,
                file_modified_at: disk_modified_at(path),
                content_hash,
            }
        }
        Some("stp") | Some("step") => {
            let (dimensions_mm, volume_cm3, object_count) = step_metadata(path);
            ScannedMetadataUpdate {
                dimensions_mm,
                volume_cm3,
                object_count,
                thumbnail_png: None,
                plate_count: None,
                slice_info_json: None,
                materials: Vec::new(),
                metadata: BTreeMap::new(),
                file_size_bytes,
                file_modified_at: disk_modified_at(path),
                content_hash,
            }
        }
        Some("obj") => {
            let doc = obj::parse_obj_file(path).map_err(|e| e.to_string())?;
            ScannedMetadataUpdate {
                dimensions_mm: doc.dimensions_mm,
                volume_cm3: doc.volume_cm3,
                object_count: None,
                thumbnail_png: None,
                plate_count: None,
                slice_info_json: None,
                materials: Vec::new(),
                metadata: BTreeMap::new(),
                file_size_bytes,
                file_modified_at: disk_modified_at(path),
                content_hash,
            }
        }
        _ => return Err(CmdError::expected("nicht unterstütztes Dateiformat")),
    };

    db::update_scanned_metadata(conn, id, &update).map_err(|e| e.to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Datei nach Aktualisierung nicht mehr gefunden".to_string())?;
    let spools = db::list_filament_spools(conn).map_err(|e| e.to_string())?;
    Ok(to_dto(file, &spools))
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
    import_many_in_batches(&mut db, roots, ImportMode::Batched)
}

/// Archive extraction removes the entire directory on failure, so all its
/// catalog writes must succeed together.
#[cfg(test)]
pub(crate) fn import_many_with_conn_atomic(
    conn: &mut Connection,
    roots: Vec<PathBuf>,
) -> CmdResult<ImportResultDto> {
    let mut db: &mut Connection = conn;
    import_many_in_batches(&mut db, roots, ImportMode::Atomic)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportMode {
    Batched,
    Atomic,
}

/// Files stored per transaction. SQLite fsyncs on every commit, so one commit
/// per file would be slow; one commit for the whole import would hold the lock
/// for minutes.
const IMPORT_BATCH_SIZE: usize = 25;

/// How a batch import reaches the database. With the app's shared connection
/// every call locks it only for its own duration: the UI thread keeps
/// answering while files are read (a synchronous command waiting for the lock
/// freezes the window).
pub(crate) trait ImportDb {
    fn read_file(&mut self, path: &Path) -> CmdResult<NewFile> {
        read_model_file(path, None)
    }
    fn report_in_flight(&self, _count: usize) {}
    fn savepoint_control(&self) -> fn(&Connection, &str) -> rusqlite::Result<()> {
        |conn, sql| conn.execute_batch(sql)
    }
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R>;
}
impl ImportDb for &Mutex<Connection> {
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
        let mut conn = self
            .lock()
            .map_err(|_| "database lock poisoned".to_string())?;
        f(&mut conn)
    }
}
impl ImportDb for &mut Connection {
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
        f(self)
    }
}

/// A parsed file waiting to be stored with the next batch.
pub(super) struct PendingImport {
    pub(super) path: PathBuf,
    /// Set for folder imports: the root the file's subfolders are mirrored from.
    pub(super) folder_root: Option<PathBuf>,
    pub(super) new_file: NewFile,
}

#[cfg(test)]
fn import_many_in_batches(
    db: &mut impl ImportDb,
    roots: Vec<PathBuf>,
    mode: ImportMode,
) -> CmdResult<ImportResultDto> {
    import_many_controlled(db, roots, mode, None)
}
#[cfg(test)]
pub(super) fn import_many_controlled(
    db: &mut impl ImportDb,
    roots: Vec<PathBuf>,
    mode: ImportMode,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> CmdResult<ImportResultDto> {
    import_many_selected(db, roots, mode, cancel, None)
}
pub(super) fn import_many_selected(
    db: &mut impl ImportDb,
    roots: Vec<PathBuf>,
    mode: ImportMode,
    cancel: Option<&std::sync::atomic::AtomicBool>,
    selected: Option<&[PathBuf]>,
) -> CmdResult<ImportResultDto> {
    let mut seen_paths = HashSet::new();
    let mut result = ImportResultDto {
        duplicate_entries: Vec::new(),
        imported: Vec::new(),
        duplicate_count: 0,
        // Archives aren't known here: they're split off and attached by the
        // `#[tauri::command]` callers (`import_files`/`import_dropped`) once
        // `import_many`/`import_many_with_conn` has returned.
        pending_archives: Vec::new(),
        skipped: Vec::new(),
    };
    let mut batch = Vec::with_capacity(IMPORT_BATCH_SIZE);

    for root in roots {
        let folder_root = root.is_dir().then(|| root.clone());
        let mut candidates = Vec::new();
        collect_supported_files(&root, &mut candidates);

        for path in candidates {
            if selected.is_some_and(|paths| !paths.contains(&path)) {
                continue;
            }
            if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
                return Err(CmdError::expected("cancelled"));
            }
            let path_str = path.to_string_lossy().to_string();
            if !seen_paths.insert(path_str.clone()) {
                continue;
            }
            if mode != ImportMode::Atomic {
                match db.with_conn(|conn| {
                    db::file_exists_by_path(conn, &path_str).map_err(|e| e.to_string().into())
                }) {
                    Ok(true) => {
                        result.duplicate_count += 1;
                        result
                            .duplicate_entries
                            .push(super::import_jobs::DuplicateEntry {
                                path: path_str.clone(),
                                kind: "path".into(),
                                existing_file_id: None,
                            });
                        continue;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        log::error!(target: "import", "duplicate check failed for {path_str}: {e}");
                        result.skip(path_str, SkipReason::Failed);
                        continue;
                    }
                }
            }
            if std::fs::metadata(&path).is_ok_and(|m| m.len() == 0) {
                log::warn!(target: "import", "übersprungen (leere Datei): {path_str}");
                result.skip(path_str, SkipReason::Empty);
                continue;
            }

            let content_hash = match compute_content_hash(&path) {
                Ok(h) => h,
                // compute_content_hash already logged the fault itself; this is just the
                // (expected) consequence for the batch.
                Err(e) => {
                    log::warn!(target: "import", "übersprungen (Hashing fehlgeschlagen): {path_str}: {e}");
                    result.skip(path_str, SkipReason::Invalid);
                    continue;
                }
            };
            if mode != ImportMode::Atomic {
                match db.with_conn(|conn| {
                    db::file_exists_by_hash(conn, &content_hash).map_err(|e| e.to_string().into())
                }) {
                    Ok(true) => {
                        result.duplicate_count += 1;
                        result
                            .duplicate_entries
                            .push(super::import_jobs::DuplicateEntry {
                                path: path_str.clone(),
                                kind: "hash".into(),
                                existing_file_id: None,
                            });
                        continue;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        log::error!(target: "import", "duplicate check (hash) failed for {path_str}: {e}");
                        result.skip(path_str, SkipReason::Failed);
                        continue;
                    }
                }
            }
            let mut new_file = match db.read_file(&path) {
                Ok(f) => f,
                Err(e) => {
                    log::warn!(target: "import", "übersprungen: {path_str}: {e}");
                    result.skip(path_str, SkipReason::Invalid);
                    continue;
                }
            };
            new_file.content_hash = Some(content_hash);
            batch.push(PendingImport {
                path,
                folder_root: folder_root.clone(),
                new_file,
            });
            db.report_in_flight(batch.len());
            if mode == ImportMode::Batched && batch.len() >= IMPORT_BATCH_SIZE {
                store_batch(db, &mut batch, &mut result, mode)?;
            }
        }
    }
    if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
        return Err(CmdError::expected("cancelled"));
    }
    if mode == ImportMode::Atomic {
        let (local, _) = store_transaction_controlled(db, &mut batch, mode, cancel);
        merge_import_result(&mut result, local?.0);
    } else {
        store_batch(db, &mut batch, &mut result, mode)?;
    }
    Ok(result)
}

impl ImportResultDto {
    fn skip(&mut self, path: String, reason: SkipReason) {
        self.skipped.push(SkippedFileDto { path, reason });
    }
}

/// Stores the pending files in one transaction and empties `batch`.
pub(super) fn store_batch(
    db: &mut impl ImportDb,
    batch: &mut Vec<PendingImport>,
    result: &mut ImportResultDto,
    mode: ImportMode,
) -> CmdResult<()> {
    store_batch_detailed(db, batch, result, mode, &mut Vec::new())
}
pub(super) fn store_batch_detailed(
    db: &mut impl ImportDb,
    batch: &mut Vec<PendingImport>,
    result: &mut ImportResultDto,
    mode: ImportMode,
    duplicates: &mut Vec<super::import_jobs::DuplicateEntry>,
) -> CmdResult<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let (attempt, commit_failed) = store_transaction(db, batch, mode);
    match attempt {
        Ok((local, committed_duplicates)) => {
            merge_import_result(result, local);
            duplicates.extend(committed_duplicates);
        }
        Err(_) if commit_failed && mode == ImportMode::Batched => {
            for pending in batch.iter_mut() {
                let (single, single_commit_failed) =
                    store_transaction(db, std::slice::from_mut(pending), mode);
                match single {
                    Ok((local, committed_duplicates)) => {
                        merge_import_result(result, local);
                        duplicates.extend(committed_duplicates);
                    }
                    Err(error) if single_commit_failed && error.expected => result.skip(
                        pending.path.to_string_lossy().into_owned(),
                        SkipReason::Failed,
                    ),
                    Err(error) => return Err(error),
                }
            }
        }
        Err(error) => return Err(error),
    }
    batch.clear();
    Ok(())
}

pub(super) fn empty_import_result() -> ImportResultDto {
    ImportResultDto {
        duplicate_entries: Vec::new(),
        imported: Vec::new(),
        duplicate_count: 0,
        pending_archives: Vec::new(),
        skipped: Vec::new(),
    }
}
fn merge_import_result(result: &mut ImportResultDto, local: ImportResultDto) {
    result.imported.extend(local.imported);
    result.skipped.extend(local.skipped);
    result.duplicate_count += local.duplicate_count;
    result.duplicate_entries.extend(local.duplicate_entries);
}
fn database_error(error: impl std::fmt::Display) -> CmdError {
    format!("database: {error}").into()
}
fn file_statement_error(error: &DbError) -> bool {
    matches!(error, DbError::Invalid(_))
        || matches!(error,
        DbError::Sqlite(rusqlite::Error::SqliteFailure(code, _))
        if code.code == rusqlite::ErrorCode::ConstraintViolation)
}
/// Only committed results escape this function. Keeping the inputs borrowed
/// lets the caller recover a failed commit without re-reading the files.
fn store_transaction(
    db: &mut impl ImportDb,
    pending: &mut [PendingImport],
    mode: ImportMode,
) -> (
    CmdResult<(ImportResultDto, Vec<super::import_jobs::DuplicateEntry>)>,
    bool,
) {
    store_transaction_controlled(db, pending, mode, None)
}
fn store_transaction_controlled(
    db: &mut impl ImportDb,
    pending: &mut [PendingImport],
    mode: ImportMode,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> (
    CmdResult<(ImportResultDto, Vec<super::import_jobs::DuplicateEntry>)>,
    bool,
) {
    let control = db.savepoint_control();
    let mut commit_failed = false;
    let result = db.with_conn(|conn| {
        let mut local = empty_import_result();
        let mut duplicates = Vec::new();
        let tx = conn.transaction().map_err(database_error)?;
        for item in pending {
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM files WHERE path = ?1)", [&item.new_file.path], |row| row.get::<_, bool>(0)).map_err(database_error)? {
                local.duplicate_count += 1;
                duplicates.push(super::import_jobs::DuplicateEntry { path: item.new_file.path.clone(), kind: "path".into(), existing_file_id: None });
                continue;
            }
            if let Some(hash) = &item.new_file.content_hash {
                if db::file_exists_by_hash(&tx, hash).map_err(database_error)? {
                    local.duplicate_count += 1;
                    duplicates.push(super::import_jobs::DuplicateEntry { path: item.new_file.path.clone(), kind: "hash".into(), existing_file_id: None });
                    continue;
                }
            }
            control(&tx, "SAVEPOINT import_file").map_err(database_error)?;
            let stored: Result<ModelFileDto, DbError> = (|| {
                item.new_file.folder_id = match item.folder_root.as_deref().zip(item.path.parent()) {
                    Some((root, dir)) => Some(db::ensure_folder_path(&tx, root, dir)?),
                    None => None,
                };
                let id = db::insert_file_within_tx(&tx, &item.new_file)?;
                let file = db::get_file(&tx, id)?.ok_or_else(|| DbError::Other("imported file missing".into()))?;
                let spools = db::list_filament_spools(&tx)?;
                Ok(to_dto(file, &spools))
            })();
            match stored {
                Ok(dto) => {
                    control(&tx, "RELEASE import_file").map_err(database_error)?;
                    local.imported.push(dto);
                }
                Err(error) => {
                    control(&tx, "ROLLBACK TO import_file; RELEASE import_file").map_err(database_error)?;
                    if mode == ImportMode::Atomic || !file_statement_error(&error) { return Err(database_error(error)); }
                    local.skip(item.path.to_string_lossy().into_owned(), SkipReason::Failed);
                }
            }
        }
        if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) { return Err(CmdError::expected("cancelled")); }
        if let Err(error) = tx.commit() {
            // rusqlite rolls back on Drop after a rejected COMMIT. Never retry
            // unless the connection confirms that transaction has ended.
            if !conn.is_autocommit() {
                conn.execute_batch("ROLLBACK").map_err(database_error)?;
            }
            commit_failed = true;
            let file_related = matches!(&error, rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::ConstraintViolation);
            return Err(if file_related { CmdError::expected(format!("database: {error}")) } else { database_error(error) });
        }
        local.duplicate_entries = duplicates.clone();
        Ok((local, duplicates))
    });
    (result, commit_failed)
}

/// One-line summary logged exactly once per user-triggered import, by the
/// callers (`import_files`/`import_folder`/`import_dropped`) after they've
/// attached the real pending-archive count to `result`.
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
/// Opens a path in the system file manager.
#[tauri::command]
pub fn open_in_file_manager(path: String) -> CmdResult<()> {
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
    cmd.arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
// Encodes the extracted geometry as a single binary stream for
// tauri::ipc::Response: 4 bytes header length (u32 LE), then a JSON header
// padded with spaces to a multiple of 4 bytes, followed by the raw
// Float32/Uint32 buffers per mesh in header order. Every section
// (position/normal/index) consists only of 4-byte elements, so the running
// offset stays a multiple of 4 after each mesh - no extra alignment handling
// needed (see also the wire format description in src/lib/parseModelGeometry.ts).
fn encode_render_meshes(meshes: &[RenderMesh]) -> Vec<u8> {
    let mut palette: Vec<crate::geometry::RenderColor> = Vec::new();
    let mut palette_indices = std::collections::HashMap::new();
    let headers: Vec<serde_json::Value> = meshes.iter().map(|m| {
        let local_indices: Vec<usize> = m.palette.iter().map(|color| {
            *palette_indices.entry(color).or_insert_with(|| {
                let index = palette.len();
                palette.push(color.clone());
                index
            })
        }).collect();
        let mut header = serde_json::json!({
            "vertexCount": m.positions.len(), "hasNormal": m.normals.is_some(),
            "indexCount": m.indices.len() * 3,
        });
        if let Some(name) = &m.object_name { header["objectName"] = name.clone().into(); }
        if !m.groups.is_empty() {
            header["groups"] = serde_json::json!(m.groups.iter().map(|g| crate::geometry::RenderGroup {
                start: g.start, count: g.count,
                color_index: g.color_index.map(|i| local_indices[i]),
            }).collect::<Vec<_>>());
        }
        header
    }).collect();
    let mut header_json = serde_json::to_vec(&serde_json::json!({ "meshes": headers, "palette": palette }))
        .expect("mesh header serialization cannot fail");
    while !(4 + header_json.len()).is_multiple_of(4) {
        header_json.push(b' ');
    }

    let payload: usize = meshes
        .iter()
        .map(|m| {
            m.positions.len() * 12
                + m.normals.as_ref().map_or(0, |n| n.len() * 12)
                + m.indices.len() * 12
        })
        .sum();
    let mut out = Vec::with_capacity(4 + header_json.len() + payload);
    out.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&header_json);

    for mesh in meshes {
        for p in &mesh.positions {
            for &c in p {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        if let Some(normals) = &mesh.normals {
            for n in normals {
                for &c in n {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        for tri in &mesh.indices {
            for &idx in tri {
                out.extend_from_slice(&idx.to_le_bytes());
            }
        }
    }

    out
}
fn geometry_io_error(e: std::io::Error) -> CmdError {
    use super::error::GeometryErrorCode as Code;
    let code = if e.kind() == std::io::ErrorKind::NotFound { Code::NotFound } else { Code::Unreadable };
    if matches!(code, Code::TooLarge | Code::NotFound) {
        CmdError::expected(e.to_string()).with_code(code)
    } else {
        CmdError::from(e.to_string()).with_code(code)
    }
}

fn geometry_3mf_error(e: threemf::ThreeMfError) -> CmdError {
    use super::error::GeometryErrorCode as Code;
    let code = match &e {
        threemf::ThreeMfError::EntryTooLarge { .. } | threemf::ThreeMfError::ResourceLimitExceeded(_)
        | threemf::ThreeMfError::MaxDepthExceeded => Code::TooLarge,
        threemf::ThreeMfError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => Code::NotFound,
        _ => Code::Unreadable,
    };
    if matches!(code, Code::TooLarge | Code::NotFound) {
        CmdError::expected(e.to_string()).with_code(code)
    } else {
        CmdError::from(e.to_string()).with_code(code)
    }
}

// Retains the existing safe-file budget and open-handle type check, while
// distinguishing its size failures without inspecting error text.
fn read_geometry_bytes(path: &Path) -> CmdResult<Vec<u8>> {
    use std::io::Read;
    let file = crate::safe_file::open_regular(path).map_err(geometry_io_error)?;
    let max = crate::safe_file::MAX_MODEL_FILE_BYTES;
    let too_large = || CmdError::expected(format!("Datei überschreitet {max} Bytes"))
        .with_code(super::error::GeometryErrorCode::TooLarge);
    if file.metadata().map_err(geometry_io_error)?.len() > max { return Err(too_large()); }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).map_err(geometry_io_error)?;
    if bytes.len() as u64 > max { return Err(too_large()); }
    Ok(bytes)
}

fn load_geometry(path: &Path) -> CmdResult<Vec<RenderMesh>> {
    use super::error::GeometryErrorCode as Code;
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let unreadable = |e: String| CmdError::from(e).with_code(Code::Unreadable);
    let meshes = match extension.as_str() {
        "stl" => vec![stl::parse_stl_geometry(&read_geometry_bytes(path)?).map_err(|e| unreadable(e.to_string()))?],
        "obj" => vec![obj::parse_obj_geometry(&read_geometry_bytes(path)?).map_err(|e| unreadable(e.to_string()))?],
        "3mf" => threemf::extract_render_meshes_from_path(path).map_err(geometry_3mf_error)?,
        "stp" | "step" => step_geometry(path)?,
        _ => return Err(CmdError::expected(format!("nicht unterstütztes Dateiformat: {extension}")).with_code(Code::Unsupported)),
    };
    if meshes.iter().all(|m| m.indices.is_empty() || m.positions.is_empty()) {
        return Err(unreadable("Keine darstellbare Geometrie".into()));
    }
    Ok(meshes)
}

#[tauri::command]
pub async fn get_model_geometry(
    state: State<'_, AppState>, file_id: String,
) -> CmdResult<tauri::ipc::Response> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let file = {
        let conn = lock_db(&state)?;
        db::get_file(&conn, id).map_err(|e| e.to_string())?
            .ok_or_else(|| CmdError::expected("file not found").with_code(super::error::GeometryErrorCode::NotFound))?
    };
    let path = PathBuf::from(file.trash_path.as_deref().unwrap_or(&file.path));
    let meshes = tauri::async_runtime::spawn_blocking(move || load_geometry(&path))
        .await.map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(encode_render_meshes(&meshes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_serialization_has_presence_flags_and_no_image_fields() {
        let summary = FileSummaryDto {
            id: "1".into(), name: "test".into(), path: "/tmp/test.stl".into(), file_type: "stl".into(),
            folder_id: String::new(), file_size_bytes: 10, dimensions_mm: None, volume_cm3: None,
            object_count: None, imported_at: String::new(), file_modified_at: Some("2023-11-14T22:13:20+00:00".into()), print_status: "not_printed".into(),
            favorite: false, queue_position: None, has_thumbnail: true, has_render_snapshot: false,
            creator: None, last_viewed_at: None, content_hash: None,
        };
        let json = serde_json::to_value(summary).unwrap();
        assert_eq!(json["fileModifiedAt"], "2023-11-14T22:13:20+00:00");
        assert_eq!(json["hasThumbnail"], true);
        assert_eq!(json["hasRenderSnapshot"], false);
        assert!(json.get("thumbnailImage").is_none());
        assert!(json.get("renderSnapshotImage").is_none());
        assert!(json.get("customImage").is_none());
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
        let expected = Some(chrono::DateTime::<chrono::Utc>::from(second).to_rfc3339());
        assert_eq!(db::get_file(&conn, id).unwrap().unwrap().file_modified_at, expected);
        assert_eq!(db::list_file_summaries(&conn).unwrap()[0].file_modified_at, expected);
        let full = to_dto(db::get_file(&conn, id).unwrap().unwrap(), &[]);
        assert_eq!(serde_json::to_value(full).unwrap()["fileModifiedAt"], expected.unwrap());
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

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HeaderEntryForTest {
        vertex_count: usize,
        has_normal: bool,
        index_count: usize,
    }
    type DecodedMesh = (Vec<[f32; 3]>, Option<Vec<[f32; 3]>>, Vec<u32>);

    fn decode_for_test(bytes: &[u8]) -> Vec<DecodedMesh> {
        let header_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let header_json = std::str::from_utf8(&bytes[4..4 + header_len]).unwrap();
        let header: serde_json::Value = serde_json::from_str(header_json).unwrap();
        let headers: Vec<HeaderEntryForTest> = serde_json::from_value(header["meshes"].clone()).unwrap();

        let mut offset = 4 + header_len;
        let mut result = Vec::new();
        for h in headers {
            let mut positions = Vec::with_capacity(h.vertex_count);
            for _ in 0..h.vertex_count {
                let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                let y = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
                let z = f32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
                positions.push([x, y, z]);
                offset += 12;
            }

            let normals = if h.has_normal {
                let mut ns = Vec::with_capacity(h.vertex_count);
                for _ in 0..h.vertex_count {
                    let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                    let y = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
                    let z = f32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
                    ns.push([x, y, z]);
                    offset += 12;
                }
                Some(ns)
            } else {
                None
            };

            let mut indices = Vec::with_capacity(h.index_count);
            for _ in 0..h.index_count {
                let idx = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                indices.push(idx);
                offset += 4;
            }

            result.push((positions, normals, indices));
        }
        assert_eq!(
            offset,
            bytes.len(),
            "encoder should not leave trailing bytes"
        );
        result
    }
    #[test]
    fn encode_render_meshes_roundtrips_positions_normals_and_indices() {
        let meshes = vec![
            RenderMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                indices: vec![[0, 1, 2]],
                normals: Some(vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]]),
                ..Default::default()
            },
            RenderMesh {
                positions: vec![
                    [5.0, 5.0, 5.0],
                    [6.0, 5.0, 5.0],
                    [5.0, 6.0, 5.0],
                    [5.0, 5.0, 6.0],
                ],
                indices: vec![[0, 1, 2], [0, 1, 3]],
                normals: None,
                ..Default::default()
            },
        ];

        let bytes = encode_render_meshes(&meshes);
        let decoded = decode_for_test(&bytes);

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].0, meshes[0].positions);
        assert_eq!(decoded[0].1, meshes[0].normals);
        assert_eq!(decoded[0].2, vec![0, 1, 2]);

        assert_eq!(decoded[1].0, meshes[1].positions);
        assert_eq!(decoded[1].1, None);
        assert_eq!(decoded[1].2, vec![0, 1, 2, 0, 1, 3]);
    }
    #[test]
    fn encode_render_meshes_roundtrips_material_groups_and_global_palette() {
        use crate::geometry::{RenderColor, RenderGroup};
        let red = RenderColor { name: "Rötlich".into(), color: "#ff0000".into() };
        let blue = RenderColor { name: "Blue".into(), color: "#0000ff".into() };
        let a = RenderMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![[0, 1, 2], [0, 2, 1]],
            object_name: Some("Töpfchen".into()),
            groups: vec![RenderGroup { start: 0, count: 3, color_index: None },
                RenderGroup { start: 3, count: 3, color_index: Some(0) }],
            palette: vec![red.clone()], ..Default::default()
        };
        let b = RenderMesh { palette: vec![blue.clone(), red.clone()],
            groups: vec![RenderGroup { start: 0, count: 3, color_index: Some(0) },
                RenderGroup { start: 3, count: 3, color_index: Some(1) }], ..a.clone() };
        let bytes = encode_render_meshes(&[a.clone(), b.clone()]);
        let decoded = decode_for_test(&bytes);
        assert_eq!(decoded[0].0, a.positions);
        assert_eq!(decoded[1].2, vec![0, 1, 2, 0, 2, 1]);
        let length = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert_eq!((4 + length) % 4, 0);
        let header: serde_json::Value = serde_json::from_slice(&bytes[4..4+length]).unwrap();
        let palette: Vec<RenderColor> = serde_json::from_value(header["palette"].clone()).unwrap();
        assert_eq!(palette, vec![red, blue]);
        let groups: Vec<RenderGroup> = serde_json::from_value(header["meshes"][0]["groups"].clone()).unwrap();
        assert_eq!(groups, a.groups);
        let groups: Vec<RenderGroup> = serde_json::from_value(header["meshes"][1]["groups"].clone()).unwrap();
        assert_eq!(groups[0].color_index, Some(1));
        assert_eq!(groups[1].color_index, Some(0));
        assert_eq!(header["meshes"][0]["objectName"], "Töpfchen");
    }

    #[test]
    fn geometry_errors_keep_typed_codes_without_parsing_messages() {
        use super::super::error::GeometryErrorCode as Code;
        for error in [threemf::ThreeMfError::MaxDepthExceeded,
            threemf::ThreeMfError::ResourceLimitExceeded("arbitrary text".into()),
            threemf::ThreeMfError::EntryTooLarge { path: "x".into(), size: 2, max: 1 }] {
            assert_eq!(geometry_3mf_error(error).code, Some(Code::TooLarge));
        }
        assert_eq!(geometry_3mf_error(threemf::ThreeMfError::ComponentCycle).code, Some(Code::Unreadable));
        assert_eq!(geometry_io_error(std::io::Error::from(std::io::ErrorKind::NotFound)).code, Some(Code::NotFound));
        assert_eq!(geometry_io_error(std::io::Error::from(std::io::ErrorKind::PermissionDenied)).code, Some(Code::Unreadable));
    }

    #[test]
    fn geometry_loader_distinguishes_missing_oversized_empty_and_unsupported_files() {
        use super::super::error::GeometryErrorCode as Code;
        let dir = super::super::unique_test_dir("viewer_error_codes");
        assert_eq!(load_geometry(&dir.join("missing.stl")).unwrap_err().code, Some(Code::NotFound));
        assert_eq!(load_geometry(&dir.join("x.unknown")).unwrap_err().code, Some(Code::Unsupported));
        let path = dir.join("empty.stl");
        std::fs::write(&path, b"solid empty\nendsolid empty").unwrap();
        assert_eq!(load_geometry(&path).unwrap_err().code, Some(Code::Unreadable));
        let path = dir.join("too-large.obj");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(crate::safe_file::MAX_MODEL_FILE_BYTES + 1).unwrap();
        assert_eq!(load_geometry(&path).unwrap_err().code, Some(Code::TooLarge));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn encode_render_meshes_handles_empty_mesh_list() {
        let bytes = encode_render_meshes(&[]);
        let decoded = decode_for_test(&bytes);
        assert!(decoded.is_empty());
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
        assert!(
            result.is_err(),
            "rename_file_with_conn must reject a name that collides with an existing file"
        );
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
            backfill_content_hashes(&conn);
            let still_missing = db::list_files_missing_content_hash(&conn).unwrap().len();
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
            queue_position: None,
            favorite: false,
            plate_count: None,
            slice_info_json: None,
        }
    }
    #[test]
    fn add_and_list_print_log_entry_roundtrips_through_dto() {
        let mut conn = crate::db::connect_in_memory().expect("connect");
        let file = sample_file_record(1, None, "2026-09-13T00:00:00Z");
        // add_print_log_entry needs a real file_id.
        let _ = file;
        let new_file = crate::db::models::NewFile {
            name: "cube.3mf".to_string(),
            path: "/tmp/print-log-test-cube.3mf".to_string(),
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
            queue_position: None,
            favorite: false,
            plate_count: None,
            slice_info_json: None,
        };
        let file_id = crate::db::insert_file(&mut conn, &new_file).expect("insert file");

        let entry = crate::db::models::NewPrintLogEntry {
            file_id,
            printed_at: "2026-09-13T10:00:00Z".to_string(),
            note: Some("Testdruck".to_string()),
            photo_png: None,
        };
        let id = crate::db::insert_print_log_entry(&conn, &entry).expect("insert entry");

        let entries = crate::db::list_print_log_entries(&conn, file_id).expect("list entries");
        let dtos: Vec<PrintLogEntryDto> = entries.into_iter().map(print_log_entry_to_dto).collect();

        assert_eq!(dtos.len(), 1);
        assert_eq!(dtos[0].id, id.to_string());
        assert_eq!(dtos[0].note.as_deref(), Some("Testdruck"));
        assert_eq!(dtos[0].photo_image, None);
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
    fn add_tag_maps_a_translated_auto_tag_name_to_its_canonical_tag() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let id = crate::db::test_insert_minimal_file(&conn, "/tmp/add_tag_alias.3mf", None).expect("insert");

        add_tag_with_conn(&conn, id, "Multipart").expect("add");
        add_tag_with_conn(&conn, id, "Vase").expect("add");

        let mut tags: Vec<String> = crate::db::list_all_file_tags(&conn)
            .expect("tags")
            .into_iter()
            .filter(|(fid, _)| *fid == id)
            .map(|(_, t)| t)
            .collect();
        tags.sort();
        assert_eq!(tags, vec!["Vase".to_string(), "mehrteilig".to_string()]);
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
        fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
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

#[cfg(test)]
#[path = "import_batch_tests.rs"]
mod import_batch_tests;
