use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::db::models::{FileType, MaterialRecord, NewFile};
use crate::db::{self, models::FileRecord};
use crate::geometry::RenderMesh;
use crate::tagging::{self, TaggingContext};
use crate::{stl, threemf};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub trash_dir: std::path::PathBuf,
}

type CmdResult<T> = Result<T, String>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
    pub tags: Vec<String>,
    pub origin: String,
    pub sync: String,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub materials: Vec<MaterialDto>,
    pub file_size_bytes: i64,
    pub imported_at: String,
    pub print_status: String,
    pub estimated_weight_g: Option<f64>,
    pub last_viewed_at: Option<String>,
    pub creator: Option<String>,
    pub display_image: Option<String>,
    pub source_url: Option<String>,
    pub queue_position: Option<i64>,
    pub favorite: bool,
    pub plate_count: Option<i64>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDto {
    pub name: String,
    pub display_color: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    pub id: String,
    pub name: String,
    pub count: i64,
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

const MATERIAL_DENSITY_G_CM3: &[(&str, f64)] = &[
    ("pla", 1.24),
    ("petg", 1.27),
    ("abs", 1.04),
    ("tpu", 1.21),
    ("asa", 1.05),
    ("pc", 1.20),
    ("nylon", 1.14),
];
const DEFAULT_DENSITY_G_CM3: f64 = 1.24;
const MAX_CUSTOM_IMAGE_BYTES: usize = 5 * 1024 * 1024; // 5 MB

pub(crate) fn estimate_weight_g(volume_cm3: Option<f64>, material_name: Option<&str>) -> Option<f64> {
    let volume = volume_cm3?;
    let density = material_name
        .and_then(|name| {
            let lower = name.to_lowercase();
            MATERIAL_DENSITY_G_CM3
                .iter()
                .find(|(key, _)| lower.contains(key))
                .map(|(_, d)| *d)
        })
        .unwrap_or(DEFAULT_DENSITY_G_CM3);
    Some(volume * density)
}

pub(crate) fn resolve_display_image(
    custom_image_png: Option<Vec<u8>>,
    thumbnail_png: Option<Vec<u8>>,
    render_snapshot_png: Option<Vec<u8>>,
) -> Option<String> {
    use base64::Engine;
    custom_image_png
        .or(thumbnail_png)
        .or(render_snapshot_png)
        .map(|bytes| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        })
}

pub(crate) fn to_dto(file: FileRecord) -> ModelFileDto {
    let estimated_weight_g =
        estimate_weight_g(file.volume_cm3, file.materials.first().map(|m| m.name.as_str()));
    let display_image = resolve_display_image(
        file.custom_image_png,
        file.thumbnail_png,
        file.render_snapshot_png,
    );
    ModelFileDto {
        id: file.id.to_string(),
        name: file.name,
        path: file.path,
        folder_id: file
            .folder_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        tags: file.tags,
        origin: file.origin,
        sync: file.sync_status,
        dimensions_mm: file.dimensions_mm,
        volume_cm3: file.volume_cm3,
        object_count: file.object_count,
        materials: file
            .materials
            .into_iter()
            .map(|m| MaterialDto {
                name: m.name,
                display_color: m.display_color,
            })
            .collect(),
        file_size_bytes: file.file_size_bytes,
        imported_at: file.imported_at,
        print_status: file.print_status,
        estimated_weight_g,
        last_viewed_at: file.last_viewed_at,
        creator: file.creator,
        display_image,
        source_url: file.source_url,
        queue_position: file.queue_position,
        favorite: file.favorite,
        plate_count: file.plate_count,
        deleted_at: file.deleted_at,
    }
}

pub(crate) fn lock_db<'a>(state: &'a State<AppState>) -> CmdResult<std::sync::MutexGuard<'a, Connection>> {
    state.db.lock().map_err(|_| "database lock poisoned".to_string())
}

#[tauri::command]
pub fn list_files(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(to_dto).collect())
}

#[tauri::command]
pub fn list_folders(state: State<AppState>) -> CmdResult<Vec<FolderDto>> {
    let conn = lock_db(&state)?;
    let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;

    let mut dtos = vec![FolderDto {
        id: "all".to_string(),
        name: "Alle Modelle".to_string(),
        count: files.len() as i64,
    }];

    for folder in folders {
        let count = files
            .iter()
            .filter(|f| f.folder_id == Some(folder.id))
            .count() as i64;
        dtos.push(FolderDto {
            id: folder.id.to_string(),
            name: folder.name,
            count,
        });
    }

    Ok(dtos)
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

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilamentSpoolDto {
    pub id: String,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: i64,
    pub remaining_weight_g: i64,
    pub price: Option<f64>,
    pub image_png: Option<String>,
}

fn filament_dto_to_record(spool: &FilamentSpoolDto) -> db::models::NewFilamentSpool {
    use base64::Engine;
    db::models::NewFilamentSpool {
        material: spool.material.clone(),
        manufacturer: spool.manufacturer.clone(),
        color: spool.color.clone(),
        diameter_mm: spool.diameter_mm,
        original_weight_g: spool.original_weight_g,
        remaining_weight_g: spool.remaining_weight_g,
        price: spool.price,
        image_png: spool
            .image_png
            .as_ref()
            .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok()),
    }
}

#[tauri::command]
pub fn list_filament_spools(state: State<AppState>) -> CmdResult<Vec<FilamentSpoolDto>> {
    let conn = lock_db(&state)?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(spools
        .into_iter()
        .map(|s| {
            use base64::Engine;
            FilamentSpoolDto {
                id: s.id.to_string(),
                material: s.material,
                manufacturer: s.manufacturer,
                color: s.color,
                diameter_mm: s.diameter_mm,
                original_weight_g: s.original_weight_g,
                remaining_weight_g: s.remaining_weight_g,
                price: s.price,
                image_png: s
                    .image_png
                    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
            }
        })
        .collect())
}

#[tauri::command]
pub fn add_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto> {
    let conn = lock_db(&state)?;
    let new_spool = filament_dto_to_record(&spool);
    let id = db::insert_filament_spool(&conn, &new_spool).map_err(|e| e.to_string())?;
    Ok(FilamentSpoolDto { id: id.to_string(), ..spool })
}

#[tauri::command]
pub fn update_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto> {
    let id: i64 = spool.id.parse().map_err(|_| "invalid spool id".to_string())?;
    let conn = lock_db(&state)?;
    let new_spool = filament_dto_to_record(&spool);
    db::update_filament_spool(&conn, id, &new_spool).map_err(|e| e.to_string())?;
    Ok(spool)
}

#[tauri::command]
pub fn delete_filament_spool(state: State<AppState>, spool_id: String) -> CmdResult<()> {
    let id: i64 = spool_id.parse().map_err(|_| "invalid spool id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_filament_spool(&conn, id).map_err(|e| e.to_string())
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
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(Some(base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[tauri::command]
pub fn add_tag(state: State<AppState>, file_id: String, tag: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::add_tag_to_file(&conn, id, &tag).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_tag(state: State<AppState>, file_id: String, tag: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::remove_tag_from_file(&conn, id, &tag).map_err(|e| e.to_string())
}

// Cloud-Löschung ist noch nicht möglich, da es keine Cloud-Anbindung gibt;
// gelöscht werden nur der DB-Eintrag und die lokale Datei.
#[tauri::command]
pub fn delete_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;

    match std::fs::metadata(&file.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Datei existiert schon wirklich nicht mehr (z.B. bereinigter
            // verwaister Pfad) - nichts zu verschieben, Katalog-Eintrag
            // direkt hart loeschen. Andere Fehlerarten (z.B.
            // PermissionDenied oder ein temporär nicht eingehängtes
            // Netzlaufwerk) duerfen NICHT wie "fehlt wirklich" behandelt
            // werden, siehe Finding 3 im Review vom 2026-09-10
            // (scan_catalog_issues).
            return db::delete_file(&conn, id).map_err(|e| e.to_string());
        }
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }

    let trash_path = state.trash_dir.join(format!("{id}-{}", file.name));
    move_file(std::path::Path::new(&file.path), &trash_path).map_err(|e| e.to_string())?;

    let deleted_at = chrono::Utc::now().to_rfc3339();
    db::soft_delete_file(&conn, id, &trash_path.to_string_lossy(), &deleted_at)
        .map_err(|e| e.to_string())
}

/// Verschiebt eine Datei; faellt bei "CrossesDevices" (Ziel auf anderem
/// Dateisystem) auf Kopieren+Loeschen des Originals zurueck.
fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            std::fs::copy(from, to)?;
            if let Err(remove_err) = std::fs::remove_file(from) {
                let _ = std::fs::remove_file(to); // Kopie aufraeumen, kein verwaister Papierkorb-Eintrag
                return Err(remove_err);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
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
    let next = db::max_queue_position(&conn).map_err(|e| e.to_string())?.unwrap_or(0) + 1;
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
    let conn = lock_db(&state)?;
    for update in updates {
        let id: i64 = update.file_id.parse().map_err(|_| "invalid file id".to_string())?;
        db::set_queue_position(&conn, id, Some(update.position)).map_err(|e| e.to_string())?;
    }
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
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_CUSTOM_IMAGE_BYTES {
        return Err(format!(
            "Bild ist zu groß ({:.1} MB) - maximal {} MB erlaubt",
            bytes.len() as f64 / (1024.0 * 1024.0),
            MAX_CUSTOM_IMAGE_BYTES / (1024 * 1024)
        ));
    }

    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_custom_image_png(&conn, id, &bytes).map_err(|e| e.to_string())?;

    Ok(Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    )))
}

#[tauri::command]
pub fn set_render_snapshot(state: State<AppState>, file_id: String, image_base64: String) -> CmdResult<()> {
    use base64::Engine;
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image_base64)
        .map_err(|e| e.to_string())?;
    let conn = lock_db(&state)?;
    db::set_render_snapshot_png(&conn, id, &bytes).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_source_url(state: State<AppState>, file_id: String, url: Option<String>) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_source_url(&conn, id, url.as_deref()).map_err(|e| e.to_string())
}

fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "3mf" | "stl"))
        .unwrap_or(false)
}

pub(crate) fn compute_content_hash(path: &Path) -> CmdResult<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

/// Einmaliger Startup-Backfill fuer content_hash: die Spalte wurde erst mit
/// dieser Version eingefuehrt und ist sonst nur fuer neu importierte Dateien
/// gesetzt (import_one) - fuer den kompletten Bestand vor diesem Upgrade
/// bliebe die Duplikaterkennung sonst dauerhaft blind. Anders als der
/// creator-Backfill in db::repository::init() braucht dieser Schritt echten
/// Datei-Zugriff (Hash ueber die tatsaechlichen Bytes), laeuft deshalb hier
/// statt dort und wird beim Start aus lib.rs aufgerufen, nachdem die
/// Connection steht. Eine seit dem Import verschobene/geloeschte Datei
/// (compute_content_hash schlaegt fehl) wird geloggt und uebersprungen, nicht
/// abgebrochen - gleiche Fehlerbehandlung wie in import_one/import_many.
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

/// Recursively walks `path`, collecting every supported model file found.
/// A plain file is included as-is if its extension matches; unreadable
/// directories are skipped rather than failing the whole scan.
fn collect_supported_files(path: &Path, out: &mut Vec<PathBuf>) {
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

pub(crate) fn import_one(
    conn: &mut Connection,
    path: &Path,
    display_name: Option<&str>,
    content_hash: Option<String>,
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
    };

    let id = db::insert_file(conn, &new_file).map_err(|e| e.to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "imported file not found after insert".to_string())?;
    Ok(to_dto(file))
}

/// Expands `roots` (files and/or directories) into the supported model files
/// they contain, skips paths already present in the catalog, and imports the
/// rest. A single unreadable/unparsable file is logged and skipped rather
/// than aborting the whole batch.
fn import_many(state: &State<AppState>, roots: Vec<PathBuf>) -> CmdResult<ImportResultDto> {
    let mut candidates = Vec::new();
    for root in roots {
        collect_supported_files(&root, &mut candidates);
    }

    let mut conn = lock_db(state)?;
    let mut seen = HashSet::new();
    let mut imported = Vec::new();
    let mut duplicate_count = 0i64;

    for path in candidates {
        let path_str = path.to_string_lossy().to_string();
        if !seen.insert(path_str.clone()) {
            continue;
        }
        match db::file_exists_by_path(&conn, &path_str) {
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
        match db::file_exists_by_hash(&conn, &content_hash) {
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

        match import_one(&mut conn, &path, None, Some(content_hash)) {
            Ok(dto) => imported.push(dto),
            Err(e) if e.contains("UNIQUE constraint failed") => {
                // Pfad gehoert noch einer Papierkorb-Zeile (files.path ist
                // weiterhin UNIQUE, file_exists_by_path sieht geloeschte
                // Zeilen aber nicht mehr) - fuer den Nutzer ist das ein
                // Duplikat, kein stiller Fehlschlag (Finding 3, Review
                // 2026-09-12).
                duplicate_count += 1;
            }
            Err(e) => eprintln!("[import] Import fehlgeschlagen für {path_str}: {e}"),
        }
    }

    Ok(ImportResultDto { imported, duplicate_count })
}

#[tauri::command]
pub async fn import_files(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportResultDto> {
    let picked = app
        .dialog()
        .file()
        .add_filter("3D-Modelle", &["3mf", "stl"])
        .blocking_pick_files();

    let Some(picked) = picked else {
        return Ok(ImportResultDto { imported: Vec::new(), duplicate_count: 0 });
    };
    let paths = picked
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect();
    import_many(&state, paths)
}

#[tauri::command]
pub async fn import_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportResultDto> {
    let picked = app.dialog().file().blocking_pick_folder();

    let Some(picked) = picked else {
        return Ok(ImportResultDto { imported: Vec::new(), duplicate_count: 0 });
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    import_many(&state, vec![path])
}

#[tauri::command]
pub fn import_dropped(state: State<AppState>, paths: Vec<String>) -> CmdResult<ImportResultDto> {
    import_many(&state, paths.into_iter().map(PathBuf::from).collect())
}

// Muss async sein, obwohl kein .await im Rumpf steht: eine synchrone
// Tauri-Command-Funktion laeuft direkt auf dem IPC-Dispatch-Thread (siehe
// import_files/import_folder, die aus demselben Grund schon async sind).
// blocking_pick_file() blockiert diesen Thread, bis der native Dialog
// geschlossen wird - lief die Funktion synchron, waere das genau der
// Thread, den GTK fuer die eigene Fensterschleife (und damit fuer den
// Dialog selbst) braucht: ein Deadlock, der die App komplett einfrieren
// liess. Als async fn dispatcht Tauri sie stattdessen auf den
// Async-Runtime-Thread-Pool.
#[tauri::command]
pub async fn pick_slicer_executable(app: tauri::AppHandle) -> CmdResult<Option<String>> {
    let dialog = app.dialog().file();
    // #[cfg] direkt auf dem let-Statement (Shadowing) statt "let mut" +
    // bedingter Neuzuweisung: unter Linux faellt diese Zeile komplett weg,
    // ein "mut"-Binding waere dort nie mutiert und wuerde eine
    // unused_mut-Warnung ausloesen.
    #[cfg(target_os = "windows")]
    let dialog = dialog.add_filter("Programme", &["exe"]);
    let picked = dialog.blocking_pick_file();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn open_in_slicer(slicer_path: String, file_path: String) -> CmdResult<()> {
    std::process::Command::new(&slicer_path)
        .arg(&file_path)
        .spawn()
        .map_err(|e| e.to_string())?;
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
    while (4 + header_json.len()) % 4 != 0 {
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

    // Der MutexGuard aus lock_db muss vor dem .await unten aus dem Scope
    // laufen (nicht nur per drop()): std::sync::MutexGuard ist nicht Send,
    // und der Compiler haelt ihn sonst faelschlich fuer potenziell ueber die
    // .await-Grenze hinweg lebendig, was den Command-Handler nicht mehr
    // Send-kompatibel macht (siehe rust-lang/rust#57478 - ein expliziter
    // drop()-Aufruf allein genuegt dafuer nicht).
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
            "3mf" => threemf::extract_render_meshes_from_path(&path).map_err(|e| e.to_string()),
            other => Err(format!("nicht unterstütztes Dateiformat: {other}")),
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(tauri::ipc::Response::new(encode_render_meshes(&meshes)))
}

#[tauri::command]
pub fn save_filter(state: State<AppState>, filter: SavedFilterInputDto) -> CmdResult<SavedFilterDto> {
    let folder_id = filter
        .folder_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
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
    let id: i64 = filter_id.parse().map_err(|_| "invalid filter id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_saved_filter(&conn, id).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIssuesDto {
    pub orphaned: Vec<ModelFileDto>,
    pub duplicate_groups: Vec<Vec<ModelFileDto>>,
}

/// Groups `files` by `content_hash`, excluding any file whose id is in
/// `orphaned_ids` (a file confirmed missing on disk has nothing worth
/// "keeping" - see Finding 1 of the 2026-09-10 final review: without this
/// exclusion, an orphaned file could end up as a duplicate group's
/// index-0 "keep the oldest" anchor even though it has no surviving copy
/// on disk). Only groups with 2+ remaining members are returned, each
/// sorted oldest-first by `imported_at`, and the groups themselves are
/// sorted by their first (oldest) member's `imported_at`.
fn group_duplicates(files: Vec<FileRecord>, orphaned_ids: &HashSet<i64>) -> Vec<Vec<FileRecord>> {
    let mut by_hash: BTreeMap<String, Vec<FileRecord>> = BTreeMap::new();
    for file in files {
        if orphaned_ids.contains(&file.id) {
            continue;
        }
        if let Some(hash) = file.content_hash.clone() {
            by_hash.entry(hash).or_default().push(file);
        }
    }

    let mut groups: Vec<Vec<FileRecord>> = by_hash
        .into_values()
        .filter(|group| group.len() >= 2)
        .map(|mut group| {
            group.sort_by(|a, b| a.imported_at.cmp(&b.imported_at));
            group
        })
        .collect();
    groups.sort_by(|a, b| a[0].imported_at.cmp(&b[0].imported_at));
    groups
}

#[tauri::command]
pub fn scan_catalog_issues(state: State<AppState>) -> CmdResult<CatalogIssuesDto> {
    let conn = lock_db(&state)?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;

    // Nur ein fs::metadata-Fehler vom Typ NotFound bedeutet wirklich "Datei
    // fehlt" - PermissionDenied/IO-Fehler auf einem (noch) nicht
    // eingehaengten Netzlaufwerk sollen nicht als verwaist gelten (Finding 3
    // im finalen Review vom 2026-09-10: sonst wuerden dort liegende, aber
    // gerade nicht erreichbare Dateien faelschlich zum Loeschen markiert).
    let mut orphaned_ids: HashSet<i64> = HashSet::new();
    let mut orphaned: Vec<ModelFileDto> = Vec::new();
    for file in &files {
        if let Err(e) = std::fs::metadata(&file.path) {
            if e.kind() == std::io::ErrorKind::NotFound {
                orphaned_ids.insert(file.id);
                orphaned.push(to_dto(file.clone()));
            }
        }
    }

    let duplicate_groups: Vec<Vec<ModelFileDto>> = group_duplicates(files, &orphaned_ids)
        .into_iter()
        .map(|group| group.into_iter().map(to_dto).collect())
        .collect();

    Ok(CatalogIssuesDto { orphaned, duplicate_groups })
}

#[tauri::command]
pub fn delete_files(state: State<AppState>, file_ids: Vec<String>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    for file_id in file_ids {
        let id: i64 = match file_id.parse() {
            Ok(id) => id,
            Err(_) => {
                eprintln!("[cleanup] Ungueltige Datei-ID uebersprungen: {file_id}");
                continue;
            }
        };
        // Bereinigungs-Batch: eine zwischenzeitlich bereits geloeschte Datei
        // (z.B. doppelt in der Auswahl) wird uebersprungen statt den ganzen
        // Batch abzubrechen.
        let file = match db::get_file(&conn, id) {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("[cleanup] Datei-ID {id} konnte nicht geladen werden: {e}");
                continue;
            }
        };
        // Batch nicht abbrechen (Finding 2 im finalen Review vom
        // 2026-09-10): jede ID wird einzeln versucht, ein fehlgeschlagener
        // Einzelfall wird geloggt und uebersprungen, damit das Frontend am
        // Ende zuverlaessig resyncen kann statt auf einem abgebrochenen
        // Batch mit veraltetem Zustand zu stehen.
        match std::fs::metadata(&file.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Err(e) = db::delete_file(&conn, id) {
                    eprintln!("[cleanup] DB-Eintrag konnte nicht geloescht werden fuer Datei-ID {id}: {e}");
                }
                continue;
            }
            Err(e) => {
                // Andere Fehlerarten als NotFound (z.B. PermissionDenied
                // oder ein temporär nicht eingehängtes Netzlaufwerk) sind
                // KEIN "Datei fehlt wirklich" - siehe Finding 3 im Review
                // vom 2026-09-10 (scan_catalog_issues). Log-and-continue
                // wie die anderen Fehlerpfade in diesem Batch.
                eprintln!("[cleanup] Datei-Metadaten konnten nicht gelesen werden fuer Datei-ID {id}: {e}");
                continue;
            }
            Ok(_) => {}
        }
        let trash_path = state.trash_dir.join(format!("{id}-{}", file.name));
        if let Err(e) = move_file(std::path::Path::new(&file.path), &trash_path) {
            eprintln!("[cleanup] Verschieben in Papierkorb fehlgeschlagen fuer Datei-ID {id}: {e}");
            continue;
        }
        let deleted_at = chrono::Utc::now().to_rfc3339();
        if let Err(e) = db::soft_delete_file(&conn, id, &trash_path.to_string_lossy(), &deleted_at) {
            eprintln!("[cleanup] DB-Eintrag konnte nicht als geloescht markiert werden fuer Datei-ID {id}: {e}");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_trash(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let files = db::list_trash(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(to_dto).collect())
}

#[tauri::command]
pub fn restore_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    let trash_path = file
        .trash_path
        .clone()
        .ok_or_else(|| "file is not in trash".to_string())?;

    let original = std::path::Path::new(&file.path);
    let target_path = if original.exists() {
        let stem = original.file_stem().and_then(|s| s.to_str()).unwrap_or("datei");
        let ext = original.extension().and_then(|s| s.to_str());
        let parent = original.parent().unwrap_or_else(|| std::path::Path::new("."));
        let new_name = match ext {
            Some(ext) => format!("{stem} (wiederhergestellt).{ext}"),
            None => format!("{stem} (wiederhergestellt)"),
        };
        parent.join(new_name)
    } else {
        original.to_path_buf()
    };

    move_file(std::path::Path::new(&trash_path), &target_path).map_err(|e| e.to_string())?;

    let new_path_str = target_path.to_string_lossy().to_string();
    let new_path_arg = if new_path_str == file.path { None } else { Some(new_path_str.as_str()) };
    db::restore_file(&conn, id, new_path_arg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_file_permanently(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    if file.deleted_at.is_none() {
        return Err("file is not in trash".to_string());
    }
    if let Some(trash_path) = &file.trash_path {
        if let Err(e) = std::fs::remove_file(trash_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.to_string());
            }
        }
    }
    db::delete_file(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn empty_trash(state: State<AppState>) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    let files = db::list_trash(&conn).map_err(|e| e.to_string())?;
    for file in files {
        if let Some(trash_path) = &file.trash_path {
            if let Err(e) = std::fs::remove_file(trash_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[trash] Entfernen fehlgeschlagen fuer Datei-ID {}: {e}", file.id);
                    continue;
                }
            }
        }
        if let Err(e) = db::delete_file(&conn, file.id) {
            eprintln!("[trash] DB-Eintrag konnte nicht geloescht werden fuer Datei-ID {}: {e}", file.id);
        }
    }
    Ok(())
}

/// Beim App-Start aufgerufen: entfernt alle Papierkorb-Eintraege, die
/// laenger als 7 Tage zurueckliegen, endgueltig. Einzelne fehlschlagende
/// Datei wird geloggt und uebersprungen, bricht den Rest nicht ab -
/// gleiches Muster wie der bestehende content_hash-Backfill.
pub fn purge_expired_trash_on_startup(conn: &Connection) {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let expired = match db::purge_expired_trash(conn, &cutoff) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("[startup] Papierkorb-Aufraeumen: Abfrage fehlgeschlagen: {e}");
            return;
        }
    };
    for file in expired {
        if let Some(trash_path) = &file.trash_path {
            if let Err(e) = std::fs::remove_file(trash_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[startup] Papierkorb-Aufraeumen: Datei fehlgeschlagen fuer ID {}: {e}", file.id);
                    continue;
                }
            }
        }
        if let Err(e) = db::delete_file(conn, file.id) {
            eprintln!("[startup] Papierkorb-Aufraeumen: DB-Eintrag fehlgeschlagen fuer ID {}: {e}", file.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::RenderMesh;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HeaderEntryForTest {
        vertex_count: usize,
        has_normal: bool,
        index_count: usize,
    }

    fn decode_for_test(bytes: &[u8]) -> Vec<(Vec<[f32; 3]>, Option<Vec<[f32; 3]>>, Vec<u32>)> {
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
        assert_eq!(offset, bytes.len(), "encoder should not leave trailing bytes");
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
    fn open_in_slicer_returns_error_for_nonexistent_executable() {
        let result = open_in_slicer(
            "/definitely/does/not/exist/xyz123".to_string(),
            "/tmp/model.3mf".to_string(),
        );
        assert!(result.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn open_in_slicer_spawns_successfully_for_a_real_executable() {
        // "/usr/bin/true" ist auf jedem Unix-System vorhanden und beendet
        // sich sofort mit Exit-Code 0 - deterministischer Erfolgstest ohne
        // einen echten Slicer zu benoetigen. Unter Windows existiert dieser
        // Pfad nicht, daher hier per #[cfg(unix)] komplett ausgeklammert
        // statt eines fragilen plattformabhaengigen Ersatzpfads.
        let result = open_in_slicer("/usr/bin/true".to_string(), "/tmp/model.3mf".to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn estimate_weight_g_uses_known_material_density() {
        let grams = estimate_weight_g(Some(10.0), Some("Generic PLA")).expect("weight");
        assert!((grams - 12.4).abs() < 0.001);
    }

    #[test]
    fn estimate_weight_g_falls_back_to_default_density_for_unknown_material() {
        let grams = estimate_weight_g(Some(10.0), Some("Mystery-Filament")).expect("weight");
        assert!((grams - 12.4).abs() < 0.001);
    }

    #[test]
    fn estimate_weight_g_returns_none_without_volume() {
        assert_eq!(estimate_weight_g(None, Some("PLA")), None);
    }

    #[test]
    fn resolve_display_image_prefers_custom_over_embedded_over_snapshot() {
        use base64::Engine;
        let result = resolve_display_image(Some(vec![1]), Some(vec![2]), Some(vec![3])).expect("some image");
        let b64 = result.strip_prefix("data:image/png;base64,").expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).expect("valid base64");
        assert_eq!(decoded, vec![1]);
    }

    #[test]
    fn resolve_display_image_falls_back_to_embedded_thumbnail() {
        use base64::Engine;
        let result = resolve_display_image(None, Some(vec![2]), Some(vec![3])).expect("some image");
        let b64 = result.strip_prefix("data:image/png;base64,").expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).expect("valid base64");
        assert_eq!(decoded, vec![2]);
    }

    #[test]
    fn resolve_display_image_falls_back_to_render_snapshot() {
        use base64::Engine;
        let result = resolve_display_image(None, None, Some(vec![3])).expect("some image");
        let b64 = result.strip_prefix("data:image/png;base64,").expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).expect("valid base64");
        assert_eq!(decoded, vec![3]);
    }

    #[test]
    fn resolve_display_image_returns_none_without_any_source() {
        assert_eq!(resolve_display_image(None, None, None), None);
    }

    /// Minimal `FileRecord` for `group_duplicates` tests: only `id`,
    /// `content_hash` and `imported_at` are read by that function, so
    /// everything else is filled with cheap placeholder values.
    fn sample_file_record(id: i64, content_hash: Option<&str>, imported_at: &str) -> FileRecord {
        FileRecord {
            id,
            name: format!("file-{id}.3mf"),
            path: format!("/tmp/file-{id}.3mf"),
            file_type: FileType::ThreeMf,
            folder_id: None,
            origin: "local".to_string(),
            sync_status: "local-only".to_string(),
            cloud_id: None,
            file_size_bytes: 1024,
            dimensions_mm: None,
            volume_cm3: None,
            object_count: None,
            thumbnail_png: None,
            imported_at: imported_at.to_string(),
            file_modified_at: None,
            materials: Vec::new(),
            metadata: BTreeMap::new(),
            tags: Vec::new(),
            print_status: "not_printed".to_string(),
            last_viewed_at: None,
            creator: None,
            content_hash: content_hash.map(|s| s.to_string()),
            render_snapshot_png: None,
            custom_image_png: None,
            source_url: None,
            queue_position: None,
            favorite: false,
            plate_count: None,
            deleted_at: None,
            trash_path: None,
        }
    }

    #[test]
    fn group_duplicates_groups_two_matching_hashes_oldest_first() {
        let older = sample_file_record(1, Some("hash-a"), "2026-09-01T00:00:00Z");
        let newer = sample_file_record(2, Some("hash-a"), "2026-09-05T00:00:00Z");
        let files = vec![newer.clone(), older.clone()];

        let groups = group_duplicates(files, &HashSet::new());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[0][0].id, older.id);
        assert_eq!(groups[0][1].id, newer.id);
    }

    #[test]
    fn group_duplicates_excludes_orphaned_member_leaving_no_group() {
        // Regression test for Finding 1 (final review, 2026-09-10): if one
        // of two same-hash files is orphaned (its file is confirmed gone),
        // it must be excluded from grouping entirely - leaving only one
        // surviving file, which is below the size-2 duplicate threshold and
        // therefore must NOT form a group. Previously the orphaned file
        // could be selected as the group's "keep the oldest" anchor while
        // also being pre-checked for deletion via the orphaned list, which
        // could wipe the last surviving copy.
        let orphaned = sample_file_record(1, Some("hash-a"), "2026-09-01T00:00:00Z");
        let surviving = sample_file_record(2, Some("hash-a"), "2026-09-05T00:00:00Z");
        let files = vec![orphaned.clone(), surviving.clone()];
        let mut orphaned_ids = HashSet::new();
        orphaned_ids.insert(orphaned.id);

        let groups = group_duplicates(files, &orphaned_ids);

        assert!(groups.is_empty());
    }

    #[test]
    fn group_duplicates_ignores_files_without_content_hash() {
        let a = sample_file_record(1, None, "2026-09-01T00:00:00Z");
        let b = sample_file_record(2, None, "2026-09-02T00:00:00Z");

        let groups = group_duplicates(vec![a, b], &HashSet::new());

        assert!(groups.is_empty());
    }

    #[test]
    fn group_duplicates_groups_three_matching_hashes_sorted() {
        let a = sample_file_record(1, Some("hash-a"), "2026-09-03T00:00:00Z");
        let b = sample_file_record(2, Some("hash-a"), "2026-09-01T00:00:00Z");
        let c = sample_file_record(3, Some("hash-a"), "2026-09-02T00:00:00Z");

        let groups = group_duplicates(vec![a, b, c], &HashSet::new());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
        assert_eq!(
            groups[0].iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }
}
