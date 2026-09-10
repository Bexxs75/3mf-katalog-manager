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

    if let Err(e) = std::fs::remove_file(&file.path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(e.to_string());
        }
    }

    db::delete_file(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_print_status(state: State<AppState>, file_id: String, status: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::set_print_status(&conn, id, &status).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mark_file_viewed(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::mark_file_viewed(&conn, id).map_err(|e| e.to_string())
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
    origin: &str,
    cloud_id: Option<String>,
    display_name: Option<&str>,
    content_hash: Option<String>,
) -> CmdResult<ModelFileDto> {
    // Bei Cloud-Importen ist `path` aus Sicherheitsgruenden (kein Path
    // Traversal ueber den Drive-Dateinamen) ein von der Datei-ID abgeleiteter
    // Cache-Pfad, nicht der echte Dateiname - display_name liefert dann den
    // tatsaechlichen Namen fuer Katalog-Anzeige UND Auto-Tagging.
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

    let (file_type, dimensions_mm, volume_cm3, object_count, materials, metadata, thumbnail_png) =
        match extension.as_deref() {
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

    let sync_status = if origin == "local" { "local-only" } else { "synced" };

    let new_file = NewFile {
        name: file_name,
        path: path.to_string_lossy().to_string(),
        file_type,
        folder_id: None,
        origin: origin.to_string(),
        cloud_id,
        sync_status: sync_status.to_string(),
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

        match import_one(&mut conn, &path, "local", None, None, Some(content_hash)) {
            Ok(dto) => imported.push(dto),
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

    let path = PathBuf::from(file.path);
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
}
