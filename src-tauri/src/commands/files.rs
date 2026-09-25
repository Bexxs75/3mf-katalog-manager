use super::*;

pub(crate) const MAX_CUSTOM_IMAGE_BYTES: usize = 5 * 1024 * 1024;
// Base64 blaeht um 4/3 auf: Obergrenze fuer den noch kodierten String in `set_render_snapshot`.
const MAX_RENDER_SNAPSHOT_BASE64_BYTES: usize = MAX_CUSTOM_IMAGE_BYTES / 3 * 4 + 4;

/// Schlanke Projektion von `ModelFileDto` fuer Grid und Liste: ohne
/// `customImage`, `materials` und `tags`, die laedt nur die Detailseite ueber
/// `list_files_by_ids([id])` nach.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummaryDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub file_type: String,
    // Leerer String = kein Ordner, wie in `ModelFileDto.folder_id` und im Frontend.
    pub folder_id: String,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub imported_at: String,
    pub print_status: String,
    pub favorite: bool,
    pub queue_position: Option<i64>,
    pub thumbnail_image: Option<String>,
    pub render_snapshot_image: Option<String>,
    // Praktisch redundant, seit renderSnapshotImage selbst mitgeliefert
    // wird - siehe Kommentar an `db::FileSummary::has_render_snapshot`.
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
    pub imported: Vec<ModelFileDto>,
    pub duplicate_count: i64,
    /// Einzeln gewaehlte/gezogene Archive - werden NICHT hier importiert,
    /// sondern vom Frontend ueber den Entpack-Dialog behandelt.
    pub pending_archives: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorCountDto {
    pub label: String,
    pub count: i64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilterDto {
    pub id: String,
    pub name: String,
    pub folder_id: Option<String>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub query: Option<String>,
    pub sort: String,
}
fn saved_filter_to_dto(record: db::models::SavedFilterRecord) -> SavedFilterDto {
    SavedFilterDto {
        id: record.id.to_string(),
        name: record.name,
        folder_id: record.folder_id.map(|id| id.to_string()),
        tag: record.tag,
        creator: record.creator,
        query: record.query,
        sort: record.sort,
    }
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilterInputDto {
    pub name: String,
    pub folder_id: Option<String>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub query: Option<String>,
    pub sort: String,
}
#[tauri::command]
pub fn list_files(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
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
            print_status: s.print_status,
            favorite: s.favorite,
            queue_position: s.queue_position,
            thumbnail_image: encode_image(s.thumbnail_png),
            render_snapshot_image: encode_image(s.render_snapshot_png),
            has_render_snapshot: s.has_render_snapshot,
            creator: s.creator,
            last_viewed_at: s.last_viewed_at,
            content_hash: s.content_hash,
        })
        .collect())
}
/// Alle Datei-Tag-Zuordnungen in einer Abfrage, weil `list_file_summaries`
/// keine Tags liefert; das Frontend fuehrt sie fuer die Tag-Filterung zusammen.
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
/// Laedt die vollen Modelldaten, die `list_file_summaries` nicht liefert
/// (die Detailseite ruft das mit einer einzelnen id auf).
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
#[tauri::command]
pub fn list_creators(state: State<AppState>) -> CmdResult<Vec<CreatorCountDto>> {
    let conn = lock_db(&state)?;
    let creators = db::list_creator_counts(&conn).map_err(|e| e.to_string())?;
    Ok(creators
        .into_iter()
        .map(|c| CreatorCountDto {
            label: c.name,
            count: c.count,
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
            ));
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
    db::delete_print_log_entry(&conn, id).map_err(|e| e.to_string())
}
/// Liest eine vom Nutzer gewaehlte Bilddatei mit Groessenlimit, direkt ueber
/// einen auf `max_bytes + 1` begrenzten Reader statt einer Vorabpruefung per
/// metadata(): so gilt das Limit auch, wenn die Datei waehrenddessen waechst.
pub(crate) fn read_image_bounded(path: &std::path::Path, max_bytes: u64) -> CmdResult<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut limited = file.take(max_bytes + 1);
    let mut buffer = Vec::new();
    limited
        .read_to_end(&mut buffer)
        .map_err(|e| e.to_string())?;
    if buffer.len() as u64 > max_bytes {
        return Err(format!("Bilddatei ist zu gross (> {max_bytes} Bytes)"));
    }
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
/// Kernlogik von `add_tag`: Namen automatischer Tags werden in jeder Sprache
/// auf die Kennung normalisiert, damit z.B. "Multipart" keinen zweiten Tag
/// neben "mehrteilig" anlegt.
fn add_tag_with_conn(conn: &Connection, file_id: i64, tag: &str) -> CmdResult<()> {
    db::add_tag_to_file(conn, file_id, &tagging::canonical_tag(tag)).map_err(|e| e.to_string())
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
    // Keine Normalisierung hier (anders als add_tag): das Frontend sendet
    // immer den tatsaechlich gespeicherten Namen - eine Normalisierung
    // wuerde einen Alias-Tag (z. B. den mehrdeutigen "mini") unentfernbar
    // machen, sobald er nicht (mehr) der Kennung entspricht.
    db::remove_tag_from_file(&conn, id, &tag).map_err(|e| e.to_string())
}
/// Kernlogik von `move_file_to_folder`, ohne `State`, damit testbar.
fn move_file_to_folder_with_conn(
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
        // Physischen Move rueckgaengig machen, damit Dateisystem und DB nicht
        // auseinanderlaufen. move_file ueberschreibt nie etwas.
        if let Err(rollback_err) = move_file(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Dateiverschiebung fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB verweist weiter auf {}",
                new_path.display(),
                old_path.display()
            ));
        }
        return Err(db_err.to_string());
    }
    Ok(())
}
/// Verschiebt eine Datei in das Verzeichnis eines Zielordners und aktualisiert
/// `folder_id`/`path`. `folder_id: None` laesst sie am aktuellen Ort.
#[tauri::command]
pub fn move_file_to_folder(
    state: State<AppState>,
    file_id: String,
    folder_id: Option<String>,
) -> CmdResult<()> {
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
/// `name` landet in `with_file_name` und muss eine einzelne, harmlose
/// Pfad-Komponente sein (CWE-22, wie `validate_folder_name`).
fn validate_file_name(name: &str) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err("Dateiname darf nicht leer sein".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("Dateiname darf keine Pfad-Trennzeichen enthalten".to_string());
    }
    if name == "." || name == ".." {
        return Err("Ungueltiger Dateiname".to_string());
    }
    Ok(())
}
/// Kernlogik von `rename_file`, ohne `State`, damit testbar. `fs::rename` statt
/// `move_file`, weil Quelle und Ziel im selben Verzeichnis liegen.
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
        return Err(format!(
            "Zieldatei existiert bereits: {}",
            new_path.display()
        ));
    }
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    if let Err(db_err) = db::rename_file(conn, id, &new_name, &new_path.to_string_lossy()) {
        // Umbenennung rueckgaengig machen, damit Dateisystem und DB nicht auseinanderlaufen.
        if let Err(rollback_err) = std::fs::rename(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Datei-Umbenennung fehlgeschlagen ({rollback_err}) - Datei heisst jetzt {}, DB verweist weiter auf {}",
                new_path.display(),
                old_path.display()
            ));
        }
        return Err(db_err.to_string());
    }
    Ok(())
}
/// Benennt eine echte Datei auf der Platte um (`std::fs::rename`, gleiches
/// Verzeichnis) und aktualisiert `name`/`path` in der DB entsprechend.
#[tauri::command]
pub fn rename_file(state: State<AppState>, file_id: String, name: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let sensitive_dirs = state.sensitive_dirs.clone();
    rename_file_with_conn(&conn, id, name, &sensitive_dirs)
}
#[tauri::command]
pub fn set_print_status(state: State<AppState>, file_id: String, status: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_print_status(&conn, id, &status).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn set_favorite(state: State<AppState>, file_id: String, favorite: bool) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_favorite(&conn, id, favorite).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn mark_file_viewed(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::mark_file_viewed(&conn, id).map_err(|e| e.to_string())
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
    db::set_queue_position(&conn, id, None).map_err(|e| e.to_string())
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
/// Kernlogik von `reorder_queue`, ohne `State`, damit testbar. Alle ids werden
/// vorab geprueft und alle Updates laufen in einer Transaktion: nie ein halb
/// angewendeter Batch.
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
            ));
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
    // Obergrenze wie bei `upload_custom_image`. Erst die Base64-Laenge pruefen,
    // damit ein riesiger String gar nicht erst dekodiert wird.
    if image_base64.len() > MAX_RENDER_SNAPSHOT_BASE64_BYTES {
        return Err(format!(
            "Bild ist zu groß - maximal {} MB erlaubt",
            MAX_CUSTOM_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image_base64)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_CUSTOM_IMAGE_BYTES {
        return Err(format!(
            "Bild ist zu groß ({:.1} MB) - maximal {} MB erlaubt",
            bytes.len() as f64 / (1024.0 * 1024.0),
            MAX_CUSTOM_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    let conn = lock_db(&state)?;
    db::set_render_snapshot_png(&conn, id, &bytes).map_err(|e| e.to_string())
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
    db::set_source_url(&conn, id, url.as_deref()).map_err(|e| e.to_string())
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
/// Enger als [`is_supported_extension`]: STEP ist katalogisierbar, aber die
/// meisten Slicer koennen es nicht importieren. OBJ ist ein druckfertiges Mesh
/// und deshalb erlaubt.
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
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
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
/// Einmaliger Backfill von content_hash beim Start fuer Dateien, die vor
/// Einfuehrung der Spalte importiert wurden. Braucht Dateizugriff und laeuft
/// deshalb hier statt in db::init. Fehlende Dateien werden geloggt und uebersprungen.
pub(crate) fn backfill_content_hashes(conn: &Connection) {
    let missing = match db::list_files_missing_content_hash(conn) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("[startup] content_hash-Backfill: Abfrage fehlgeschlagen: {e}");
            return;
        }
    };

    for (id, path) in missing {
        match compute_content_hash(Path::new(&path)) {
            Ok(hash) => {
                if let Err(e) = db::set_content_hash(conn, id, &hash) {
                    eprintln!("[startup] content_hash-Backfill: Speichern fehlgeschlagen fuer {path}: {e}");
                }
            }
            Err(e) => {
                eprintln!("[startup] content_hash-Backfill: Hash fehlgeschlagen fuer {path}: {e}");
            }
        }
    }
}
/// Sammelt rekursiv alle unterstuetzten Dateien unter `path`; unlesbare
/// Verzeichnisse werden uebersprungen. Folgt bewusst KEINEN Symlinks, weder auf
/// Verzeichnisse (Endlosrekursion, Dateien ausserhalb der Auswahl) noch auf
/// Dateien. Die Pruefung steht vor `is_dir()`, weil `is_dir()` Symlinks folgt.
fn collect_supported_files(path: &Path, out: &mut Vec<PathBuf>) {
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
/// Einzige Stelle, an der die STEP-Vorschau im Metadaten-Pfad sichtbar wird.
/// Bei jedem Fehler bleibt die Datei katalogisierbar und erhaelt nur keine
/// automatisch ermittelten Metadaten.
#[cfg(feature = "step-preview")]
fn step_metadata(path: &Path) -> (Option<[f64; 3]>, Option<f64>, Option<i64>) {
    match crate::step::parse_step_file(path) {
        Ok(doc) => (
            doc.dimensions_mm,
            doc.volume_cm3,
            Some(doc.object_count as i64),
        ),
        Err(err) => {
            eprintln!("STEP-Metadaten nicht lesbar ({}): {err}", path.display());
            (None, None, None)
        }
    }
}

#[cfg(not(feature = "step-preview"))]
fn step_metadata(_path: &Path) -> (Option<[f64; 3]>, Option<f64>, Option<i64>) {
    (None, None, None)
}

/// Geometrie fuer die STEP-Vorschau; ohne das Feature zeigt die Ansicht den Platzhalter.
#[cfg(feature = "step-preview")]
fn step_geometry(path: &Path) -> CmdResult<Vec<RenderMesh>> {
    crate::step::parse_step_geometry(path).map_err(|e| e.to_string())
}

#[cfg(not(feature = "step-preview"))]
fn step_geometry(_path: &Path) -> CmdResult<Vec<RenderMesh>> {
    Err("STEP-Vorschau ist in diesem Build nicht enthalten".to_string())
}

pub(crate) fn import_one(
    conn: &Connection,
    path: &Path,
    display_name: Option<&str>,
    content_hash: Option<String>,
    folder_id: Option<i64>,
) -> CmdResult<ModelFileDto> {
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
        _ => return Err("nicht unterstütztes Dateiformat".to_string()),
    };

    let tags = tagging::suggest_tags(&TaggingContext {
        file_name: &file_name,
        dimensions_mm,
        object_count,
        materials: &materials,
    });

    let new_file = NewFile {
        name: file_name,
        path: path.to_string_lossy().to_string(),
        file_type,
        folder_id,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes,
        dimensions_mm,
        volume_cm3,
        object_count,
        thumbnail_png,
        imported_at: chrono::Utc::now().to_rfc3339(),
        file_modified_at: None,
        materials,
        metadata: metadata.clone(),
        tags,
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: metadata.get("Designer").cloned(),
        content_hash,
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        queue_position: None,
        favorite: false,
        plate_count,
        slice_info_json,
    };

    let id = db::insert_file_within_tx(conn, &new_file).map_err(|e| e.to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "imported file not found after insert".to_string())?;
    let spools = db::list_filament_spools(conn).map_err(|e| e.to_string())?;
    Ok(to_dto(file, &spools))
}
/// Liest eine katalogisierte Datei erneut ein und ueberschreibt alle daraus
/// abgeleiteten Spalten, z.B. nachdem sie im Slicer neu gesliced wurde. Fehlt
/// die Datei, bricht die Funktion ab, bevor die DB veraendert wird.
pub(crate) fn rescan_file(conn: &mut Connection, id: i64) -> CmdResult<ModelFileDto> {
    let existing = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Datei nicht im Katalog gefunden".to_string())?;
    let path = Path::new(&existing.path);
    if !path.exists() {
        return Err(format!("Datei nicht gefunden: {}", existing.path));
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
                content_hash,
            }
        }
        _ => return Err("nicht unterstütztes Dateiformat".to_string()),
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
fn import_many(state: &State<AppState>, roots: Vec<PathBuf>) -> CmdResult<ImportResultDto> {
    let mut conn = lock_db(state)?;
    import_many_with_conn(&mut conn, roots)
}
/// Core of [`import_many`], parameterized over a plain [`Connection`] instead
/// of a Tauri-managed `State` so it is directly unit-testable (a
/// `State<AppState>` cannot be constructed outside of a running Tauri app).
pub(crate) fn import_many_with_conn(conn: &mut Connection, roots: Vec<PathBuf>) -> CmdResult<ImportResultDto> {
    let mut seen = HashSet::new();
    let mut imported = Vec::new();
    let mut duplicate_count = 0i64;

    // Eine Transaktion fuer den ganzen Batch: SQLite fsynct bei jedem Commit.
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    for root in roots {
        let is_folder_root = root.is_dir();
        let mut candidates = Vec::new();
        collect_supported_files(&root, &mut candidates);

        for path in candidates {
            let path_str = path.to_string_lossy().to_string();
            if !seen.insert(path_str.clone()) {
                continue;
            }
            match db::file_exists_by_path(&tx, &path_str) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => {
                    eprintln!("[import] Duplikatprüfung fehlgeschlagen für {path_str}: {e}");
                    continue;
                }
            }

            let content_hash = match compute_content_hash(&path) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("[import] Hash fehlgeschlagen für {path_str}: {e}");
                    continue;
                }
            };
            match db::file_exists_by_hash(&tx, &content_hash) {
                Ok(true) => {
                    duplicate_count += 1;
                    continue;
                }
                Ok(false) => {}
                Err(e) => {
                    eprintln!("[import] Duplikatprüfung (Hash) fehlgeschlagen für {path_str}: {e}");
                    continue;
                }
            }

            let folder_id = if is_folder_root {
                path.parent()
                    .and_then(|dir| db::ensure_folder_path(&tx, &root, dir).ok())
            } else {
                None
            };

            match import_one(&tx, &path, None, Some(content_hash), folder_id) {
                Ok(dto) => imported.push(dto),
                Err(e) if e.contains("UNIQUE constraint failed") => {
                    // Der Pfad gehoert noch einer Papierkorb-Zeile (files.path ist UNIQUE);
                    // fuer den Nutzer ist das ein Duplikat.
                    duplicate_count += 1;
                }
                Err(e) => eprintln!("[import] Import fehlgeschlagen für {path_str}: {e}"),
            }
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(ImportResultDto {
        imported,
        duplicate_count,
        pending_archives: Vec::new(),
    })
}
/// Trennt Archive (echte Dateien mit Archiv-Endung) von allem anderen. Ein
/// VERZEICHNIS namens `x.zip` bleibt beim normalen Import.
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
pub async fn import_files(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingArchives>,
) -> CmdResult<ImportResultDto> {
    // Ein gemeinsamer Filter statt zwei: unter GTK zeigt der Dialog sonst
    // nur den ersten Filter an, Archive waeren erst nach Umschalten sichtbar.
    let mut extensions = vec!["3mf", "stl", "stp", "step", "obj"];
    extensions.extend_from_slice(crate::archive::DIALOG_EXTENSIONS);
    let picked = app
        .dialog()
        .file()
        .add_filter("3D-Modelle & Archive", &extensions)
        .blocking_pick_files();

    let Some(picked) = picked else {
        return Ok(ImportResultDto {
            imported: Vec::new(),
            duplicate_count: 0,
            pending_archives: Vec::new(),
        });
    };
    let paths = picked
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect();
    let (models, archives) = split_archives(paths);
    let mut result = import_many(&state, models)?;
    pending.register(&archives);
    result.pending_archives = archives;
    Ok(result)
}
#[tauri::command]
pub async fn import_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportResultDto> {
    let picked = app.dialog().file().blocking_pick_folder();

    let Some(picked) = picked else {
        return Ok(ImportResultDto {
            imported: Vec::new(),
            duplicate_count: 0,
            pending_archives: Vec::new(),
        });
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    import_many(&state, vec![path])
}
#[tauri::command]
pub fn import_dropped(
    state: State<AppState>,
    pending: State<PendingArchives>,
    paths: Vec<String>,
) -> CmdResult<ImportResultDto> {
    let (models, archives) = split_archives(paths.into_iter().map(PathBuf::from).collect());
    let mut result = import_many(&state, models)?;
    // Archive nur, wenn das Backend den Drop selbst beobachtet hat (siehe
    // `on_window_event` in lib.rs); andere Archiv-Pfade werden ignoriert.
    result.pending_archives = pending.claim_dropped(archives);
    Ok(result)
}
/// Oeffnet einen Pfad im Datei-Manager des Systems.
#[tauri::command]
pub fn open_in_file_manager(path: String) -> CmdResult<()> {
    // Nur existierende Verzeichnisse: ein Pfad mit fuehrendem "-" koennte sonst
    // von xdg-open/open/explorer als Option gelesen werden.
    if !std::path::Path::new(&path).is_dir() {
        return Err("Pfad ist kein existierendes Verzeichnis".to_string());
    }

    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");

    cmd.arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
// Encodiert die extrahierte Geometrie als einzelnen Binaerstrom fuer
// tauri::ipc::Response: 4 Bytes Headerlaenge (u32 LE), dann ein mit
// Leerzeichen auf ein Vielfaches von 4 Bytes aufgepolsterter JSON-Header,
// gefolgt von den rohen Float32/Uint32-Puffern je Mesh in Header-
// Reihenfolge. Jeder Abschnitt (Position/Normale/Index) besteht
// ausschliesslich aus 4-Byte-Elementen, daher bleibt der laufende Offset
// nach jedem Mesh automatisch ein Vielfaches von 4 - keine zusaetzliche
// Ausrichtungs-Behandlung noetig (siehe auch die Wire-Format-Beschreibung
// in src/lib/parseModelGeometry.ts auf der Frontend-Seite).
fn encode_render_meshes(meshes: &[RenderMesh]) -> Vec<u8> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct MeshHeaderEntry {
        vertex_count: usize,
        has_normal: bool,
        index_count: usize,
    }

    let headers: Vec<MeshHeaderEntry> = meshes
        .iter()
        .map(|m| MeshHeaderEntry {
            vertex_count: m.positions.len(),
            has_normal: m.normals.is_some(),
            index_count: m.indices.len() * 3,
        })
        .collect();

    let mut header_json =
        serde_json::to_vec(&headers).expect("mesh header serialization cannot fail");
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
#[tauri::command]
pub async fn get_model_geometry(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<tauri::ipc::Response, String> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;

    // Der MutexGuard muss per Scope vor dem .await enden; ein drop() reicht dem
    // Compiler nicht (rust-lang/rust#57478), der Handler waere sonst nicht Send.
    let file = {
        let conn = lock_db(&state)?;
        db::get_file(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "file not found".to_string())?
    };

    let path = PathBuf::from(file.trash_path.as_deref().unwrap_or(&file.path));
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or_else(|| "file has no extension".to_string())?;

    let meshes = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Vec<RenderMesh>> {
        match extension.as_str() {
            "stl" => {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let mesh = stl::parse_stl_geometry(&bytes).map_err(|e| e.to_string())?;
                Ok(vec![mesh])
            }
            "obj" => {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let mesh = obj::parse_obj_geometry(&bytes).map_err(|e| e.to_string())?;
                Ok(vec![mesh])
            }
            "3mf" => threemf::extract_render_meshes_from_path(&path).map_err(|e| e.to_string()),
            "stp" | "step" => step_geometry(&path),
            other => Err(format!("nicht unterstütztes Dateiformat: {other}")),
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(tauri::ipc::Response::new(encode_render_meshes(&meshes)))
}
#[tauri::command]
pub fn save_filter(
    state: State<AppState>,
    filter: SavedFilterInputDto,
) -> CmdResult<SavedFilterDto> {
    let folder_id = filter
        .folder_id
        .map(|s| {
            s.parse::<i64>()
                .map_err(|_| "invalid folder id".to_string())
        })
        .transpose()?;
    let conn = lock_db(&state)?;
    let new_filter = db::models::NewSavedFilter {
        name: filter.name,
        folder_id,
        tag: filter.tag,
        creator: filter.creator,
        query: filter.query,
        sort: filter.sort,
    };
    let id = db::insert_saved_filter(&conn, &new_filter).map_err(|e| e.to_string())?;
    Ok(saved_filter_to_dto(db::models::SavedFilterRecord {
        id,
        name: new_filter.name,
        folder_id: new_filter.folder_id,
        tag: new_filter.tag,
        creator: new_filter.creator,
        query: new_filter.query,
        sort: new_filter.sort,
        created_at: String::new(),
    }))
}
#[tauri::command]
pub fn list_saved_filters(state: State<AppState>) -> CmdResult<Vec<SavedFilterDto>> {
    let conn = lock_db(&state)?;
    let filters = db::list_saved_filters(&conn).map_err(|e| e.to_string())?;
    Ok(filters.into_iter().map(saved_filter_to_dto).collect())
}
#[tauri::command]
pub fn delete_saved_filter(state: State<AppState>, filter_id: String) -> CmdResult<()> {
    let id: i64 = filter_id
        .parse()
        .map_err(|_| "invalid filter id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_saved_filter(&conn, id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let headers: Vec<HeaderEntryForTest> = serde_json::from_str(header_json).unwrap();

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
        // import_many_with_conn oeffnet eine Transaktion fuer den ganzen Batch;
        // import_one darf darin keine eigene oeffnen.
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
        // Die Zeile bleibt, ein Trigger laesst nur das UPDATE scheitern, nachdem die
        // Datei umbenannt wurde (sonst bricht get_file() vorher ab).
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

        // Ueberschreibt die Datei am selben Pfad mit ANDEREM Inhalt (echtes
        // Re-Slicing simulieren) - die eingebettete slice_info.config macht
        // die Bytes garantiert unterschiedlich lang.
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
        // Nie geschrieben - Datei existiert nicht auf der Platte.

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let mut new_file = sample_new_file_for_rescan_test(&path);
        new_file.file_type = FileType::ThreeMf;
        let id = crate::db::insert_file(&mut conn, &new_file).expect("insert");

        let result = rescan_file(&mut conn, id);
        assert!(result.is_err());
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
        // add_print_log_entry braucht eine echte file_id.
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
        // Zweite Zeile mit genau dem Zielpfad: files.path ist UNIQUE, das DB-Update
        // scheitert also erst nach dem physischen Move.
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
        // Muss terminieren (kein Stack Overflow / keine Endlosschleife) und
        // darf cube.3mf trotzdem genau einmal finden.
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
        // Rauchtest: 50 MB muessen ohne Panik oder OOM hashen.
        let path = unique_test_dir("hash_large").join("big.bin");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let chunk = vec![0xABu8; 1024 * 1024];
        let mut file = std::fs::File::create(&path).unwrap();
        for _ in 0..50 {
            std::io::Write::write_all(&mut file, &chunk).unwrap();
        }
        let hash = compute_content_hash(&path).unwrap();
        // 50 MB pro Lauf: sofort wieder weg, /tmp ist haeufig ein RAM-tmpfs.
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(hash.len(), 64);
    }
    #[test]
    fn queue_reorder_batch_is_all_or_nothing_on_a_mid_batch_failure() {
        let mut conn = db::connect_in_memory().unwrap();
        let id1 = db::test_insert_minimal_file(&conn, "/tmp/1.3mf", None).unwrap();
        let id2 = db::test_insert_minimal_file(&conn, "/tmp/2.3mf", None).unwrap();
        // id 999999 existiert nicht; keines der gueltigen Updates darf committed sein.
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
        // Die Variante oben scheitert schon in der Vorabpruefung. Hier scheitert per
        // Trigger das zweite Update innerhalb der Transaktion; das erste muss
        // zurueckgerollt sein.
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
        std::fs::write(&path, vec![0u8; 1024]).unwrap();
        assert!(read_image_bounded(&path, 5 * 1024 * 1024).is_ok());
    }
    #[test]
    fn is_supported_extension_accepts_stp_and_step_case_insensitively() {
        assert!(is_supported_extension(Path::new("teil.stp")));
        assert!(is_supported_extension(Path::new("teil.STEP")));
    }
    #[test]
    fn is_sliceable_extension_rejects_stp_and_step() {
        // Katalogisierbar, aber nicht im Slicer oeffenbar.
        assert!(!is_sliceable_extension(Path::new("teil.stp")));
        assert!(!is_sliceable_extension(Path::new("teil.step")));
        assert!(is_sliceable_extension(Path::new("teil.3mf")));
        assert!(is_sliceable_extension(Path::new("teil.stl")));
    }
    #[test]
    fn is_supported_and_sliceable_extension_both_accept_obj() {
        // OBJ muss in beiden Checks true sein.
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
        // .stp und .step muessen auf denselben kanonischen DB-Wert ("stp")
        // mappen, unabhaengig von der urspruenglichen Dateiendung.
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
        // Das Vorschaubild entsteht erst im Frontend (Snapshot), wie bei STL.
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
        // Die Mehrdeutigkeit von "mini" (siehe tagging::AMBIGUOUS_ALIASES)
        // gilt nur fuer automatische Quellen (Dateinamen/Material) - bei
        // manueller Eingabe ueber add_tag bleibt "mini" weiterhin ein Alias
        // fuer "miniatur".
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
}
