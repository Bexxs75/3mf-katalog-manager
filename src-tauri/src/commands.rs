use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::db::models::{FileType, MaterialRecord, NewFile, ScannedMetadataUpdate};
use crate::db::{self, models::FileRecord};
use crate::geometry::RenderMesh;
use crate::slicers::{detect_slicers, DetectedSlicer};
use crate::tagging::{self, TaggingContext};
use crate::{stl, threemf};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub trash_dir: std::path::PathBuf,
    pub db_path: std::path::PathBuf,
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
    pub custom_image: Option<String>,
    pub thumbnail_image: Option<String>,
    pub render_snapshot_image: Option<String>,
    pub source_url: Option<String>,
    pub queue_position: Option<i64>,
    pub favorite: bool,
    pub plate_count: Option<i64>,
    pub weight_source: String,
    pub slice_info: Option<SliceInfoDto>,
    pub cost_estimate: Option<CostEstimateDto>,
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
pub struct FilamentUsageDto {
    #[serde(rename = "type")]
    pub filament_type: String,
    pub color: Option<String>,
    pub used_g: f64,
    pub used_m: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlateFilamentUsageDto {
    pub plate_index: u32,
    pub weight_g: f64,
    pub filaments: Vec<FilamentUsageDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceInfoDto {
    pub total_weight_g: f64,
    pub plates: Vec<PlateFilamentUsageDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimateDto {
    pub total_cost: Option<f64>,
    pub has_unpriced_filaments: bool,
}

/// Schaetzt die Materialkosten eines Modells: pro Filament im `slice_info`
/// wird nach Lager-Spulen gesucht, deren `material`-Feld den Filament-Typ
/// als Teilstring enthaelt (case-insensitive, Farbe wird NICHT verglichen -
/// Slicer- und Lager-Farbwerte stimmen selten exakt ueberein). Aus allen
/// Treffern mit gesetztem Preis wird ein Durchschnittspreis pro Gramm
/// gebildet. Filamente ohne Treffer/Preis fliessen nicht in die Summe ein
/// (0 wuerde faelschlich "kostenlos" bedeuten) - stattdessen markiert
/// `has_unpriced_filaments`, dass die Summe unvollstaendig ist.
pub(crate) fn estimate_material_cost(
    slice_info: &threemf::SliceInfo,
    spools: &[db::models::FilamentSpoolRecord],
) -> CostEstimateDto {
    let mut total_cost = 0.0;
    let mut priced_any = false;
    let mut has_unpriced = false;

    for plate in &slice_info.plates {
        for filament in &plate.filaments {
            if filament.filament_type.trim().is_empty() {
                has_unpriced = true;
                continue;
            }

            let type_lower = filament.filament_type.to_lowercase();
            let matching: Vec<&db::models::FilamentSpoolRecord> = spools
                .iter()
                .filter(|s| {
                    s.material.to_lowercase().contains(&type_lower)
                        && s.price.is_some()
                        && s.original_weight_g > 0
                })
                .collect();

            if matching.is_empty() {
                has_unpriced = true;
                continue;
            }

            let avg_price_per_gram: f64 = matching
                .iter()
                .map(|s| s.price.unwrap() / s.original_weight_g as f64)
                .sum::<f64>()
                / matching.len() as f64;

            total_cost += filament.used_g * avg_price_per_gram;
            priced_any = true;
        }
    }

    CostEstimateDto {
        total_cost: if priced_any { Some(total_cost) } else { None },
        has_unpriced_filaments: has_unpriced,
    }
}

impl From<threemf::SliceInfo> for SliceInfoDto {
    fn from(info: threemf::SliceInfo) -> Self {
        SliceInfoDto {
            total_weight_g: info.total_weight_g,
            plates: info
                .plates
                .into_iter()
                .map(|p| PlateFilamentUsageDto {
                    plate_index: p.plate_index,
                    weight_g: p.weight_g,
                    filaments: p
                        .filaments
                        .into_iter()
                        .map(|f| FilamentUsageDto {
                            filament_type: f.filament_type,
                            color: f.color,
                            used_g: f.used_g,
                            used_m: f.used_m,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
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

pub(crate) fn encode_image(bytes: Option<Vec<u8>>) -> Option<String> {
    use base64::Engine;
    bytes.map(|b| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b)
        )
    })
}

pub(crate) fn to_dto(file: FileRecord, spools: &[db::models::FilamentSpoolRecord]) -> ModelFileDto {
    let slice_info: Option<threemf::SliceInfo> = file
        .slice_info_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    let (estimated_weight_g, weight_source) = match &slice_info {
        Some(info) => (Some(info.total_weight_g), "slicer".to_string()),
        None => (
            estimate_weight_g(file.volume_cm3, file.materials.first().map(|m| m.name.as_str())),
            "estimated".to_string(),
        ),
    };
    let cost_estimate = slice_info
        .as_ref()
        .map(|info| estimate_material_cost(info, spools));
    let custom_image = encode_image(file.custom_image_png);
    let thumbnail_image = encode_image(file.thumbnail_png);
    let render_snapshot_image = encode_image(file.render_snapshot_png);
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
        custom_image,
        thumbnail_image,
        render_snapshot_image,
        source_url: file.source_url,
        queue_position: file.queue_position,
        favorite: file.favorite,
        plate_count: file.plate_count,
        weight_source,
        slice_info: slice_info.map(SliceInfoDto::from),
        cost_estimate,
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
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
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
    pub location: Option<String>,
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
        location: spool.location.clone(),
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
                location: s.location,
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
pub fn list_print_log_entries(state: State<AppState>, file_id: String) -> CmdResult<Vec<PrintLogEntryDto>> {
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
        .map(|b64| base64::engine::general_purpose::STANDARD.decode(&b64).map_err(|e| e.to_string()))
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
    let new_entry = db::models::NewPrintLogEntry { file_id: fid, printed_at, note, photo_png };
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
    let id: i64 = entry_id.parse().map_err(|_| "invalid entry id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_print_log_entry(&conn, id).map_err(|e| e.to_string())
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
            // Pfad nicht erreichbar (z.B. umbenannter/verschobener Ordner,
            // nicht eingehaengtes Laufwerk) - es gibt nichts zu
            // verschieben, aber das heisst NICHT zwingend "Datei
            // unwiderruflich weg": eine Cloud-Mount-Umbenennung ist weit
            // haeufiger als ein tatsaechlich geloeschtes Original. Der
            // Katalog-Eintrag wird deshalb trotzdem nur WEICH geloescht
            // (landet im Papierkorb, trash_path bleibt NULL) statt hart
            // entfernt - erst "Endgueltig loeschen" oder Ablauf der
            // 7-Tage-Frist entfernt ihn wirklich. Andere Fehlerarten (z.B.
            // PermissionDenied) durchlaufen stattdessen den regulaeren
            // Fehlerpfad unten, siehe Finding 3 im Review vom 2026-09-10
            // (scan_catalog_issues).
            let deleted_at = chrono::Utc::now().to_rfc3339();
            return db::soft_delete_file(&conn, id, None, &deleted_at).map_err(|e| e.to_string());
        }
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }

    let trash_path = state.trash_dir.join(format!("{id}-{}", file.name));
    move_file(std::path::Path::new(&file.path), &trash_path).map_err(|e| e.to_string())?;

    let deleted_at = chrono::Utc::now().to_rfc3339();
    db::soft_delete_file(&conn, id, Some(&trash_path.to_string_lossy()), &deleted_at)
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
    pub model_count: i64,
}

fn to_collection_dto(record: db::models::CollectionRecord) -> CollectionDto {
    CollectionDto {
        id: record.id.to_string(),
        name: record.name,
        model_count: record.model_count,
    }
}

#[tauri::command]
pub fn list_collections(state: State<AppState>) -> CmdResult<Vec<CollectionDto>> {
    let conn = lock_db(&state)?;
    let collections = db::list_collections(&conn).map_err(|e| e.to_string())?;
    Ok(collections.into_iter().map(to_collection_dto).collect())
}

#[tauri::command]
pub fn create_collection(state: State<AppState>, name: String) -> CmdResult<CollectionDto> {
    let conn = lock_db(&state)?;
    let created_at = chrono::Utc::now().to_rfc3339();
    let id = db::create_collection(&conn, &name, &created_at).map_err(|e| e.to_string())?;
    Ok(CollectionDto { id: id.to_string(), name, model_count: 0 })
}

#[tauri::command]
pub fn rename_collection(state: State<AppState>, collection_id: String, name: String) -> CmdResult<()> {
    let id: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    db::rename_collection(&conn, id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_collection(state: State<AppState>, collection_id: String) -> CmdResult<()> {
    let id: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_collection(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_files_to_collection(state: State<AppState>, collection_id: String, file_ids: Vec<String>) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    let start = db::max_collection_position(&conn, cid).map_err(|e| e.to_string())?.unwrap_or(-1) + 1;
    for (next, file_id) in (start..).zip(file_ids) {
        let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
        db::add_file_to_collection(&conn, cid, fid, next).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn remove_file_from_collection(state: State<AppState>, collection_id: String, file_id: String) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    db::remove_file_from_collection(&conn, cid, fid).map_err(|e| e.to_string())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPositionUpdate {
    pub file_id: String,
    pub position: i64,
}

#[tauri::command]
pub fn reorder_collection(state: State<AppState>, collection_id: String, updates: Vec<CollectionPositionUpdate>) -> CmdResult<()> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    for update in updates {
        let fid: i64 = update.file_id.parse().map_err(|_| "invalid file id".to_string())?;
        db::set_collection_position(&conn, cid, fid, update.position).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_collection_files(state: State<AppState>, collection_id: String) -> CmdResult<Vec<ModelFileDto>> {
    let cid: i64 = collection_id.parse().map_err(|_| "invalid collection id".to_string())?;
    let conn = lock_db(&state)?;
    let ids = db::list_collection_file_ids(&conn, cid).map_err(|e| e.to_string())?;
    let files = db::list_files_by_ids(&conn, &ids).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
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
    let url = validate_source_url(url)?;
    let conn = lock_db(&state)?;
    db::set_source_url(&conn, id, url.as_deref()).map_err(|e| e.to_string())
}

// Nur http(s)-Links zulassen: die URL wird im Frontend unveraendert als
// <a href> gerendert, ein "javascript:"/"data:"-Wert wuerde dort beim Klick
// ausgefuehrt statt navigiert (CWE-79-nah).
fn validate_source_url(url: Option<String>) -> CmdResult<Option<String>> {
    let Some(trimmed) = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()) else {
        return Ok(None);
    };
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(Some(trimmed))
    } else {
        Err("source URL must start with http:// or https://".to_string())
    }
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
        slice_info_json,
    };

    let id = db::insert_file(conn, &new_file).map_err(|e| e.to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "imported file not found after insert".to_string())?;
    let spools = db::list_filament_spools(conn).map_err(|e| e.to_string())?;
    Ok(to_dto(file, &spools))
}

/// Liest die Datei einer bereits katalogisierten `FileRecord` erneut vom
/// gespeicherten Pfad ein und ueberschreibt alle davon abgeleiteten Spalten
/// (Maße, Volumen, Materialien, Metadaten, Thumbnail, Plattenzahl,
/// Slice-Info) - fuer den Fall, dass der Nutzer die Datei inzwischen in
/// OrcaSlicer/Bambu Studio gesliced und am selben Pfad ueberschrieben hat.
/// Existiert die Datei am Pfad nicht mehr, bricht die Funktion mit einem
/// Fehler ab, BEVOR irgendetwas in der DB veraendert wird.
pub(crate) fn rescan_file(conn: &mut Connection, id: i64) -> CmdResult<ModelFileDto> {
    let existing = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Datei nicht im Katalog gefunden".to_string())?;
    let path = Path::new(&existing.path);
    if !path.exists() {
        return Err(format!("Datei nicht gefunden: {}", existing.path));
    }
    let extension = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());

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
                    .map(|m| MaterialRecord { name: m.name, display_color: m.display_color })
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
    let id: i64 = file_id.parse().map_err(|_| "ungueltige Datei-ID".to_string())?;
    let mut conn = lock_db(&state)?;
    rescan_file(&mut conn, id)
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
pub async fn import_folder_as_collection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportResultDto> {
    let picked = app.dialog().file().blocking_pick_folder();

    let Some(picked) = picked else {
        return Ok(ImportResultDto { imported: Vec::new(), duplicate_count: 0 });
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let folder_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Sammlung".to_string());

    let mut candidates = Vec::new();
    collect_supported_files(&path, &mut candidates);

    let result = import_many(&state, vec![path])?;

    let conn = lock_db(&state)?;
    let created_at = chrono::Utc::now().to_rfc3339();
    let collection_id = db::create_collection(&conn, &folder_name, &created_at).map_err(|e| e.to_string())?;

    let mut position = 0i64;
    for candidate in &candidates {
        let path_str = candidate.to_string_lossy().to_string();
        // Erst per Pfad suchen (deckt neu importierte UND bereits vorher am
        // selben Pfad katalogisierte Dateien ab). Schlaegt das fehl, kann die
        // Datei trotzdem schon im Katalog sein - unter einem ANDEREN Pfad,
        // als exaktes Inhalts-Duplikat (von import_many via content_hash
        // erkannt und deshalb nicht neu importiert). Ohne diesen Fallback
        // wuerde so eine Datei beim Sammlung-aus-Ordner-Import stillschweigend
        // uebersprungen, obwohl sie inhaltlich im Ordner liegt.
        let file_id = match db::get_file_id_by_path(&conn, &path_str).map_err(|e| e.to_string())? {
            Some(id) => Some(id),
            None => match compute_content_hash(candidate) {
                Ok(hash) => db::get_file_id_by_content_hash(&conn, &hash).map_err(|e| e.to_string())?,
                Err(_) => None,
            },
        };
        if let Some(file_id) = file_id {
            db::add_file_to_collection(&conn, collection_id, file_id, position).map_err(|e| e.to_string())?;
            position += 1;
        }
    }

    Ok(result)
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

// Wird beim Start unseres eigenen AppImage vom AppImage-Runtime bzw. dem
// linuxdeploy-Gtk-Hook gesetzt und zeigt auf unser eigenes, temporaeres
// Mount-Verzeichnis. std::process::Command vererbt per Default die
// komplette Prozessumgebung an Kindprozesse - startet der Nutzer einen
// Slicer, der selbst ueber ein AppRun-Skript laeuft (z.B. eine ebenfalls
// als AppImage vertriebene Bambu-Studio-/OrcaSlicer-Installation), nutzt
// dessen Skript oft denselben "${APPDIR:-$(dirname ...)}"-Fallback-Trick.
// Da APPDIR durch unsere Vererbung bereits gesetzt ist, uebernimmt der
// Slicer faelschlich UNSER Mount-Verzeichnis statt sein eigenes zu
// berechnen, und sucht eigene Bibliotheken/Hilfsprozesse (z.B. seinen
// WebKit-Netzwerkprozess) an falschen, gebrochenen Pfaden - beobachtet als
// "Unable to spawn a new child process" bei OrcaSlicer. Diese Variablen
// sind ausschliesslich fuer unsere eigene, eingebettete WebView/AppImage-
// Laufzeit gedacht und duerfen nicht an unabhaengig gestartete externe
// Programme weitergegeben werden.
const APPIMAGE_ENV_VARS_TO_STRIP: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "OWD",
    "ARGV0",
    "LD_LIBRARY_PATH",
    "GTK_EXE_PREFIX",
    "GTK_DATA_PREFIX",
    "GTK_THEME",
    "GTK_PATH",
    "GTK_IM_MODULE_FILE",
    "GDK_PIXBUF_MODULE_FILE",
    "GDK_BACKEND",
    "GIO_EXTRA_MODULES",
    "GSETTINGS_SCHEMA_DIR",
    "XDG_DATA_DIRS",
    "PYTHONPATH",
    "QT_PLUGIN_PATH",
    "GST_PLUGIN_SYSTEM_PATH",
    "WEBKIT_DISABLE_DMABUF_RENDERER",
];

// Slicer-Liste wird ausschliesslich im Frontend (localStorage) verwaltet,
// es gibt keine Backend-Quelle fuer eine Pfad-Whitelist. Als Ersatzschranke
// wird hier zumindest sichergestellt, dass der Pfad tatsaechlich auf eine
// existierende, ausfuehrbare Datei zeigt, statt jeden beliebigen String
// klaglos an process::Command zu uebergeben.
fn validate_slicer_path(slicer_path: &str) -> CmdResult<()> {
    let path = Path::new(slicer_path);
    let metadata = std::fs::metadata(path)
        .map_err(|_| "slicer executable not found".to_string())?;
    if !metadata.is_file() {
        return Err("slicer path is not a file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("slicer path is not executable".to_string());
        }
    }
    #[cfg(windows)]
    {
        let is_exe = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("exe"))
            .unwrap_or(false);
        if !is_exe {
            return Err("slicer path must be an .exe file".to_string());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn open_in_slicer(slicer_path: String, file_path: String) -> CmdResult<()> {
    validate_slicer_path(&slicer_path)?;
    let mut cmd = std::process::Command::new(&slicer_path);
    cmd.arg(&file_path);
    for var in APPIMAGE_ENV_VARS_TO_STRIP {
        cmd.env_remove(var);
    }
    if let Some(parent) = std::path::Path::new(&slicer_path).parent() {
        cmd.current_dir(parent);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn scan_installed_slicers() -> CmdResult<Vec<DetectedSlicer>> {
    Ok(detect_slicers())
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

/// Exportiert den kompletten Katalogzustand (DB + Frontend-Settings) als
/// ZIP-Datei. Nutzt SQLite's Online-Backup-API statt eines rohen
/// Datei-Kopierens fuer die DB-Kopie: die laufende Connection kann im
/// WAL-Modus sein, ein fs::copy koennte eine inkonsistente Zwischenstufe
/// der Datei erwischen.
#[tauri::command]
pub async fn export_catalog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings_json: String,
) -> CmdResult<()> {
    use std::io::Write;

    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP-Archiv", &["zip"])
        .set_file_name(format!(
            "3mf-katalog-backup_{}.zip",
            chrono::Utc::now().format("%Y-%m-%d")
        ))
        .blocking_save_file();

    let Some(picked) = picked else {
        return Ok(());
    };
    let dest_path = picked.into_path().map_err(|e| e.to_string())?;

    let backup_db_path =
        std::env::temp_dir().join(format!("3mf-katalog-export-{}.db", std::process::id()));
    let tmp_zip_path = dest_path.with_extension("zip.tmp");

    // Backup+Zip-Schritte in eine Closure gekapselt, damit bei jedem
    // Fehlschlag (nicht nur beim Erfolgspfad) beide Temp-Artefakte
    // aufgeraeumt werden koennen, bevor der Fehler propagiert wird.
    let result: CmdResult<()> = (|| {
        {
            let conn = lock_db(&state)?;
            let mut dst = Connection::open(&backup_db_path).map_err(|e| e.to_string())?;
            let backup =
                rusqlite::backup::Backup::new(&conn, &mut dst).map_err(|e| e.to_string())?;
            backup
                .run_to_completion(5, std::time::Duration::from_millis(250), None)
                .map_err(|e| e.to_string())?;
        }

        let zip_file = std::fs::File::create(&tmp_zip_path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(zip_file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("catalog.db", options).map_err(|e| e.to_string())?;
        let db_bytes = std::fs::read(&backup_db_path).map_err(|e| e.to_string())?;
        zip.write_all(&db_bytes).map_err(|e| e.to_string())?;

        zip.start_file("settings.json", options).map_err(|e| e.to_string())?;
        zip.write_all(settings_json.as_bytes()).map_err(|e| e.to_string())?;

        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();

    // Die temporaere DB-Kopie wird in jedem Fall nicht mehr gebraucht.
    let _ = std::fs::remove_file(&backup_db_path);
    if let Err(e) = result {
        // Kein unvollstaendiges .zip.tmp sichtbar neben dem Zielpfad
        // zuruecklassen, falls das Packen mittendrin fehlschlaegt.
        let _ = std::fs::remove_file(&tmp_zip_path);
        return Err(e);
    }

    // Zip erst nach vollstaendigem, erfolgreichem Schreiben an den
    // eigentlichen Zielpfad verschieben - kein unvollstaendiges Archiv am
    // sichtbaren Zielort, falls das Packen mittendrin fehlschlaegt.
    if let Err(e) = std::fs::rename(&tmp_zip_path, &dest_path) {
        // Schlaegt auch das finale Umbenennen fehl, bleibt keine
        // verwaiste .zip.tmp sichtbar neben dem Zielpfad zurueck.
        let _ = std::fs::remove_file(&tmp_zip_path);
        return Err(e.to_string());
    }
    Ok(())
}

/// Prueft, ob `bytes` eine brauchbare Katalog-Datenbank sind (oeffnbar und
/// mit einer `files`-Tabelle) - Schutz davor, ein falsches/kaputtes ZIP zu
/// importieren, BEVOR die bestehende catalog.db angefasst wird.
fn validate_catalog_db_bytes(bytes: &[u8]) -> Result<(), String> {
    let tmp_path = std::env::temp_dir().join(format!(
        "3mf-katalog-import-check-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    std::fs::write(&tmp_path, bytes).map_err(|e| e.to_string())?;

    let result = Connection::open(&tmp_path)
        .map_err(|e| e.to_string())
        .and_then(|conn| {
            conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))
                .map(|_| ())
                .map_err(|e| e.to_string())
        });

    let _ = std::fs::remove_file(&tmp_path);
    result.map_err(|e| format!("Archiv enthält keine gültige Katalog-Datenbank: {e}"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCatalogResultDto {
    pub imported: bool,
    pub settings_json: Option<String>,
}

/// Importiert einen per `export_catalog` erzeugten Katalog-Export. Ersetzt
/// die laufende `catalog.db` NUR nach erfolgreicher Validierung (siehe
/// `validate_catalog_db_bytes`); die eigentliche Ersetzung (Connection-Swap,
/// Umbenennen der alten DB zu `.bak-<Zeitstempel>`, Kopieren der neuen DB,
/// Restore-bei-Fehler) uebernimmt `replace_catalog_db` - siehe dort fuer
/// Details. Ein Neustart der App wird dem Nutzer danach weiterhin empfohlen
/// (Frontend-Zustand/Caches sind nicht auf einen Katalogwechsel zur Laufzeit
/// ausgelegt), auch wenn das Backend ab dann bereits wieder eine echte
/// Connection auf die neue DB haelt.
#[tauri::command]
pub async fn import_catalog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ImportCatalogResultDto> {
    use std::io::Read;

    let picked = app.dialog().file().add_filter("ZIP-Archiv", &["zip"]).blocking_pick_file();
    let Some(picked) = picked else {
        return Ok(ImportCatalogResultDto { imported: false, settings_json: None });
    };
    let archive_path = picked.into_path().map_err(|e| e.to_string())?;

    let file = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut db_bytes = Vec::new();
    archive
        .by_name("catalog.db")
        .map_err(|_| "Archiv enthält keine catalog.db".to_string())?
        .read_to_end(&mut db_bytes)
        .map_err(|e| e.to_string())?;

    let mut settings_bytes = Vec::new();
    archive
        .by_name("settings.json")
        .map_err(|_| "Archiv enthält keine settings.json".to_string())?
        .read_to_end(&mut settings_bytes)
        .map_err(|e| e.to_string())?;
    let settings_json = String::from_utf8(settings_bytes).map_err(|e| e.to_string())?;

    validate_catalog_db_bytes(&db_bytes)?;

    let tmp_db_path =
        std::env::temp_dir().join(format!("3mf-katalog-import-{}.db", std::process::id()));
    std::fs::write(&tmp_db_path, &db_bytes).map_err(|e| e.to_string())?;

    let replace_result = replace_catalog_db(&state, &tmp_db_path);
    let _ = std::fs::remove_file(&tmp_db_path);
    replace_result?;

    Ok(ImportCatalogResultDto { imported: true, settings_json: Some(settings_json) })
}

/// Ersetzt die laufende `catalog.db` durch die Datei unter `new_db_path`.
/// Herausgezogen aus `import_catalog`, damit die riskante Kernlogik
/// (Connection-Swap, Umbenennen, Kopieren, Restore-bei-Fehler) ohne
/// `AppHandle`/Datei-Dialog direkt getestet werden kann - `import_catalog`
/// selbst bleibt duenne Verdrahtung (Dialog + Zip-Entpacken + Validierung),
/// die auf diese Funktion delegiert.
///
/// Die laufende `Connection` in `state.db` wird vor dem Umbenennen durch eine
/// In-Memory-Platzhalter-Connection ersetzt - ein blosses Freigeben des
/// Mutex-Locks wuerde das zugrundeliegende Datei-Handle NICHT schliessen, was
/// auf Windows das nachfolgende `fs::rename` mit einer Sharing-Violation zum
/// Scheitern braechte.
///
/// Die alte `catalog.db` wird zu `.bak-<Zeitstempel>` umbenannt statt
/// geloescht. Schlaegt das anschliessende Kopieren der neuen DB fehl, wird
/// versucht, die Backup-Datei zurueck nach `catalog.db` umzubenennen; schlaegt
/// *dieser* Wiederherstellungsversuch ebenfalls fehl, wird eine eigene,
/// unmissverstaendliche Fehlermeldung zurueckgegeben, die auf den Pfad der
/// Backup-Datei verweist (siehe Review-Finding: die alte Version taeuschte im
/// Fehlerfall faelschlich eine erfolgreiche Wiederherstellung vor).
fn replace_catalog_db(state: &AppState, new_db_path: &Path) -> CmdResult<()> {
    {
        let mut guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let placeholder = Connection::open_in_memory().map_err(|e| e.to_string())?;
        *guard = placeholder; // alte Connection droppt hier -> OS-Handle auf catalog.db wird geschlossen
    }

    let backup_path = state.db_path.with_file_name(format!(
        "catalog.db.bak-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));

    if let Err(e) = std::fs::rename(&state.db_path, &backup_path) {
        // catalog.db liegt unveraendert unter state.db_path (das rename ist
        // fehlgeschlagen, bevor irgendetwas passiert ist) - Connection muss
        // trotzdem wieder darauf zeigen statt auf dem In-Memory-Platzhalter
        // zu bleiben, sonst ist die App bis zum naechsten Neustart unbenutzbar,
        // obwohl die Datei voellig in Ordnung ist.
        if let Ok(mut guard) = state.db.lock() {
            if let Ok(conn) = db::connect(&state.db_path) {
                *guard = conn;
            }
        }
        return Err(e.to_string());
    }

    if let Err(e) = std::fs::copy(new_db_path, &state.db_path) {
        let restore_result = std::fs::rename(&backup_path, &state.db_path);
        if restore_result.is_ok() {
            // Alte Datei ist wieder unter state.db_path - Connection
            // reconnecten, sonst haengt die App mit dem In-Memory-Platzhalter,
            // obwohl die Datei laengst wiederhergestellt ist.
            if let Ok(mut guard) = state.db.lock() {
                if let Ok(conn) = db::connect(&state.db_path) {
                    *guard = conn;
                }
            }
        }
        return match restore_result {
            Ok(()) => Err(format!(
                "Kopieren der neuen Datenbank fehlgeschlagen, alter Katalog wiederhergestellt: {e}"
            )),
            Err(restore_err) => Err(format!(
                "Kopieren der neuen Datenbank fehlgeschlagen UND Wiederherstellung der alten \
                 Datenbank fehlgeschlagen ({restore_err}). Die vorherige Datenbank liegt noch \
                 unter {}. Bitte manuell nach {} zurückbenennen und anschließend die App neu \
                 starten. Ursprünglicher Fehler: {e}",
                backup_path.display(),
                state.db_path.display()
            )),
        };
    }

    // Neue DB liegt jetzt unter state.db_path - Connection darauf umstellen,
    // statt sie auf dem In-Memory-Platzhalter zu belassen, damit AppState
    // sofort wieder eine echte, funktionierende Verbindung haelt (und dies
    // testbar ist). Bewusst db::connect() statt einem rohen
    // Connection::open(): db::connect() ruft zusaetzlich init() auf, was
    // PRAGMA foreign_keys = ON setzt und alte Schemata per ALTER TABLE auf
    // den aktuellen Stand migriert - beides faellt bei einem rohen
    // Connection::open() weg, was bei einem Import aus einem aelteren
    // Export (mit veraltetem Schema) zu fehlenden Spalten bzw. deaktivierten
    // Fremdschluessel-Kaskaden fuehren wuerde, bis die App neu gestartet
    // wird. Ein Neustart der App bleibt trotzdem empfohlen (siehe
    // Spec/Frontend-Flow), ist fuer die Backend-Korrektheit ab hier aber
    // nicht mehr zwingend.
    let mut guard = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    *guard = db::connect(&state.db_path).map_err(|e| e.to_string())?;

    Ok(())
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
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;

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
                orphaned.push(to_dto(file.clone(), &spools));
            }
        }
    }

    let duplicate_groups: Vec<Vec<ModelFileDto>> = group_duplicates(files, &orphaned_ids)
        .into_iter()
        .map(|group| group.into_iter().map(|f| to_dto(f, &spools)).collect())
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
                // Pfad nicht erreichbar - wie in delete_file: trotzdem nur
                // weich loeschen (Papierkorb-Eintrag ohne physische Datei)
                // statt hart zu entfernen, da das haeufiger eine
                // umbenannte/verschobene Quelle als ein echtes Fehlen ist.
                let deleted_at = chrono::Utc::now().to_rfc3339();
                if let Err(e) = db::soft_delete_file(&conn, id, None, &deleted_at) {
                    eprintln!("[cleanup] DB-Eintrag konnte nicht als geloescht markiert werden fuer Datei-ID {id}: {e}");
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
        if let Err(e) = db::soft_delete_file(&conn, id, Some(&trash_path.to_string_lossy()), &deleted_at) {
            eprintln!("[cleanup] DB-Eintrag konnte nicht als geloescht markiert werden fuer Datei-ID {id}: {e}");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_trash(state: State<AppState>) -> CmdResult<Vec<ModelFileDto>> {
    let conn = lock_db(&state)?;
    let files = db::list_trash(&conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}

#[tauri::command]
pub fn restore_file(state: State<AppState>, file_id: String) -> CmdResult<()> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    if file.deleted_at.is_none() {
        return Err("file is not in trash".to_string());
    }

    let Some(trash_path) = file.trash_path.clone() else {
        // Kein trash_path gesetzt: die Datei war beim Loeschen bereits am
        // Original-Pfad nicht erreichbar (siehe delete_file), es gibt also
        // physisch nichts zurueckzuverschieben - nur den Katalog-Eintrag
        // wieder sichtbar machen. Ist der Pfad inzwischen wieder erreichbar
        // (z.B. Ordner zurueckbenannt), zeigt er dann wieder korrekt darauf.
        return db::restore_file(&conn, id, None).map_err(|e| e.to_string());
    };

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

    fn sample_spool(material: &str, original_weight_g: i64, price: Option<f64>) -> db::models::FilamentSpoolRecord {
        db::models::FilamentSpoolRecord {
            id: 1,
            material: material.to_string(),
            manufacturer: None,
            color: None,
            location: None,
            diameter_mm: 1.75,
            original_weight_g,
            remaining_weight_g: original_weight_g,
            price,
            image_png: None,
        }
    }

    fn sample_slice_info_single_filament(filament_type: &str, used_g: f64) -> threemf::SliceInfo {
        threemf::SliceInfo {
            total_weight_g: used_g,
            plates: vec![threemf::slice_info::PlateFilamentUsage {
                plate_index: 1,
                weight_g: used_g,
                filaments: vec![threemf::slice_info::FilamentUsage {
                    filament_type: filament_type.to_string(),
                    color: None,
                    used_g,
                    used_m: 0.0,
                }],
            }],
        }
    }

    #[test]
    fn validate_catalog_db_bytes_accepts_a_real_sqlite_database_with_files_table() {
        let tmp_path = std::env::temp_dir().join(format!(
            "validate_catalog_db_test_{}.db",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        {
            let conn = crate::db::connect(&tmp_path).expect("connect creates a valid schema");
            drop(conn);
        }
        let bytes = std::fs::read(&tmp_path).expect("read temp db");

        let result = validate_catalog_db_bytes(&bytes);

        let _ = std::fs::remove_file(&tmp_path);
        assert!(result.is_ok(), "expected valid catalog db to pass validation: {result:?}");
    }

    #[test]
    fn validate_catalog_db_bytes_rejects_garbage_bytes() {
        let result = validate_catalog_db_bytes(b"this is not a sqlite database");
        assert!(result.is_err());
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{name}_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    #[test]
    fn replace_catalog_db_backs_up_old_db_and_installs_new_one() {
        let dir = unique_test_dir("replace_catalog_db_success");
        let db_path = dir.join("catalog.db");

        // "Alte" laufende DB: leerer Katalog.
        let old_conn = crate::db::connect(&db_path).expect("connect creates schema");
        drop(old_conn);
        let old_bytes = std::fs::read(&db_path).expect("read old db bytes");

        // "Neue" DB (simuliert das aus dem Zip entpackte catalog.db) mit
        // einem Datensatz, damit sich alt/neu unterscheiden lassen.
        let new_db_path = dir.join("incoming_catalog.db");
        let new_conn = crate::db::connect(&new_db_path).expect("connect creates schema");
        new_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('new.3mf', '/incoming/new.3mf', '3mf', 42, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed new db with a marker row");
        drop(new_conn);
        let new_bytes = std::fs::read(&new_db_path).expect("read new db bytes");
        assert_ne!(old_bytes, new_bytes, "old and new db content must differ for this test to be meaningful");

        // AppState haelt zunaechst die "alte" Connection auf db_path.
        let running_conn = crate::db::connect(&db_path).expect("reopen db for AppState");
        let state = AppState {
            db: Mutex::new(running_conn),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
        };

        let result = replace_catalog_db(&state, &new_db_path);
        assert!(result.is_ok(), "expected successful replacement: {result:?}");

        // Alte DB wurde zu genau einer .bak-* Datei umbenannt (nicht geloescht)
        // und ihr Inhalt entspricht dem alten Katalog.
        let bak_entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert_eq!(bak_entries.len(), 1, "expected exactly one backup file");
        let bak_bytes = std::fs::read(bak_entries[0].path()).expect("read backup db bytes");
        assert_eq!(bak_bytes, old_bytes, "backup file must contain the old db content");

        // Neuer Inhalt liegt jetzt unter db_path.
        let installed_bytes = std::fs::read(&db_path).expect("read installed db bytes");
        assert_eq!(installed_bytes, new_bytes, "db_path must now contain the new db content");

        // AppState's Connection zeigt jetzt tatsaechlich auf den neuen
        // Inhalt (nicht mehr auf den In-Memory-Platzhalter) - der Marker-
        // Datensatz aus der neuen DB ist ueber die laufende Connection
        // sichtbar. Dass `fs::rename` weiter oben ueberhaupt erfolgreich
        // war, beweist implizit, dass der Connection-Swap das alte
        // Datei-Handle vorher freigegeben hat (ein noch offenes Handle
        // haette das Umbenennen auf Windows mit einer Sharing-Violation
        // scheitern lassen).
        let guard = state.db.lock().unwrap();
        let count: i64 =
            guard.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "connection must reflect the newly installed db's content");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_catalog_db_restores_backup_when_copy_of_new_db_fails() {
        let dir = unique_test_dir("replace_catalog_db_copy_failure");
        let db_path = dir.join("catalog.db");

        let old_conn = crate::db::connect(&db_path).expect("connect creates schema");
        old_conn
            .execute(
                "INSERT INTO files (name, path, file_type, file_size_bytes, imported_at)
                 VALUES ('old.3mf', '/old/old.3mf', '3mf', 7, '2026-09-13T00:00:00Z')",
                [],
            )
            .expect("seed old db with a marker row");
        drop(old_conn);
        let old_bytes = std::fs::read(&db_path).expect("read old db bytes");

        // Existiert absichtlich nicht -> fs::copy schlaegt fehl.
        let missing_new_db_path = dir.join("does_not_exist.db");

        let running_conn = crate::db::connect(&db_path).expect("reopen db for AppState");
        let state = AppState {
            db: Mutex::new(running_conn),
            trash_dir: dir.join("trash"),
            db_path: db_path.clone(),
        };

        let result = replace_catalog_db(&state, &missing_new_db_path);
        assert!(result.is_err(), "expected an error when the new db file is missing");
        let err = result.unwrap_err();
        assert!(
            err.contains("wiederhergestellt"),
            "expected error to mention successful restoration, got: {err}"
        );

        // Alte DB wurde nach dem fehlgeschlagenen Kopieren wieder an ihren
        // urspruenglichen Platz zurueckbenannt - kein .bak-* liegt mehr da,
        // db_path enthaelt wieder den alten Inhalt.
        let bak_entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("catalog.db.bak-"))
            .collect();
        assert!(bak_entries.is_empty(), "backup file must have been renamed back after successful restore");
        assert!(db_path.exists(), "catalog.db must exist again after restore");
        let restored_bytes = std::fs::read(&db_path).expect("read restored db bytes");
        assert_eq!(restored_bytes, old_bytes, "restored db must match the original content");

        // AppState's Connection muss nach dem erfolgreichen Restore
        // tatsaechlich wieder nutzbar sein und den alten (wiederhergestellten)
        // Inhalt lesen - nicht auf dem In-Memory-Platzhalter haengen bleiben
        // (Finding I1). Der Marker-Datensatz aus der alten DB muss ueber die
        // laufende Connection sichtbar sein.
        let guard = state.db.lock().unwrap();
        let count: i64 =
            guard.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "connection must reflect the restored old db's content, not the placeholder");
        drop(guard);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn estimate_material_cost_uses_single_matching_spool() {
        let slice_info = sample_slice_info_single_filament("PLA", 20.0);
        let spools = vec![sample_spool("PLA", 1000, Some(20.0))]; // 0.02 pro Gramm
        let cost = estimate_material_cost(&slice_info, &spools);
        assert!((cost.total_cost.expect("cost") - 0.4).abs() < 1e-6);
        assert!(!cost.has_unpriced_filaments);
    }

    #[test]
    fn estimate_material_cost_averages_multiple_matching_spools() {
        let slice_info = sample_slice_info_single_filament("PLA", 10.0);
        let spools = vec![
            sample_spool("PLA", 1000, Some(20.0)), // 0.02/g
            sample_spool("Generic PLA", 1000, Some(30.0)), // 0.03/g
        ];
        let cost = estimate_material_cost(&slice_info, &spools);
        // Durchschnitt 0.025/g * 10g = 0.25
        assert!((cost.total_cost.expect("cost") - 0.25).abs() < 1e-6);
    }

    #[test]
    fn estimate_material_cost_returns_none_when_no_matching_material() {
        let slice_info = sample_slice_info_single_filament("PETG", 10.0);
        let spools = vec![sample_spool("PLA", 1000, Some(20.0))];
        let cost = estimate_material_cost(&slice_info, &spools);
        assert_eq!(cost.total_cost, None);
        assert!(cost.has_unpriced_filaments);
    }

    #[test]
    fn estimate_material_cost_ignores_spools_without_price() {
        let slice_info = sample_slice_info_single_filament("PLA", 10.0);
        let spools = vec![sample_spool("PLA", 1000, None)];
        let cost = estimate_material_cost(&slice_info, &spools);
        assert_eq!(cost.total_cost, None);
        assert!(cost.has_unpriced_filaments);
    }

    #[test]
    fn estimate_material_cost_sums_only_priced_filaments_when_mixed() {
        let slice_info = threemf::SliceInfo {
            total_weight_g: 30.0,
            plates: vec![threemf::slice_info::PlateFilamentUsage {
                plate_index: 1,
                weight_g: 30.0,
                filaments: vec![
                    threemf::slice_info::FilamentUsage {
                        filament_type: "PLA".to_string(),
                        color: None,
                        used_g: 20.0,
                        used_m: 0.0,
                    },
                    threemf::slice_info::FilamentUsage {
                        filament_type: "Nylon".to_string(),
                        color: None,
                        used_g: 10.0,
                        used_m: 0.0,
                    },
                ],
            }],
        };
        let spools = vec![sample_spool("PLA", 1000, Some(20.0))]; // 0.02/g, kein Nylon im Lager
        let cost = estimate_material_cost(&slice_info, &spools);
        assert!((cost.total_cost.expect("cost") - 0.4).abs() < 1e-6); // nur PLA-Anteil
        assert!(cost.has_unpriced_filaments); // Nylon fehlt
    }

    #[test]
    fn estimate_material_cost_treats_empty_filament_type_as_unmatched_not_universal_match() {
        let slice_info = threemf::SliceInfo {
            total_weight_g: 10.0,
            plates: vec![threemf::slice_info::PlateFilamentUsage {
                plate_index: 1,
                weight_g: 10.0,
                filaments: vec![threemf::slice_info::FilamentUsage {
                    filament_type: "".to_string(),
                    color: None,
                    used_g: 10.0,
                    used_m: 0.0,
                }],
            }],
        };
        // Zwei Spulen mit gesetztem Preis im Lager - ohne den Fix wuerde die
        // leere Typ-Zeichenkette beide "matchen" (jede Zeichenkette enthaelt "")
        // und einen Fantasiepreis liefern statt "unbekannt".
        let spools = vec![
            sample_spool("PLA", 1000, Some(20.0)),
            sample_spool("PETG", 1000, Some(25.0)),
        ];
        let cost = estimate_material_cost(&slice_info, &spools);
        assert_eq!(cost.total_cost, None);
        assert!(cost.has_unpriced_filaments);
    }

    #[test]
    fn validate_source_url_accepts_http_and_https() {
        assert_eq!(
            validate_source_url(Some("https://example.com/model".to_string())).unwrap(),
            Some("https://example.com/model".to_string())
        );
        assert_eq!(
            validate_source_url(Some("http://example.com".to_string())).unwrap(),
            Some("http://example.com".to_string())
        );
    }

    #[test]
    fn validate_source_url_rejects_non_http_schemes() {
        assert!(validate_source_url(Some("javascript:alert(1)".to_string())).is_err());
        assert!(validate_source_url(Some("data:text/html,<script>".to_string())).is_err());
    }

    #[test]
    fn validate_source_url_treats_none_and_blank_as_clear() {
        assert_eq!(validate_source_url(None).unwrap(), None);
        assert_eq!(validate_source_url(Some("   ".to_string())).unwrap(), None);
    }

    #[test]
    fn validate_slicer_path_rejects_missing_file() {
        assert!(validate_slicer_path("/does/not/exist/slicer").is_err());
    }

    #[test]
    fn validate_slicer_path_rejects_directory() {
        let dir = std::env::temp_dir();
        assert!(validate_slicer_path(dir.to_str().unwrap()).is_err());
    }

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
    fn encode_image_encodes_bytes_as_data_url() {
        use base64::Engine;
        let result = encode_image(Some(vec![1, 2, 3])).expect("some image");
        let b64 = result.strip_prefix("data:image/png;base64,").expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).expect("valid base64");
        assert_eq!(decoded, vec![1, 2, 3]);
    }

    #[test]
    fn encode_image_returns_none_for_none() {
        assert_eq!(encode_image(None), None);
    }

    #[test]
    fn to_dto_uses_slicer_weight_and_marks_source_when_slice_info_present() {
        let mut file = sample_file_record(1, None, "2026-09-13T00:00:00Z");
        file.volume_cm3 = Some(100.0); // wuerde ohne slice_info eine Schaetzung liefern
        file.slice_info_json = Some(
            r##"{"total_weight_g":42.5,"plates":[{"plate_index":1,"weight_g":42.5,"filaments":[{"filament_type":"PLA","color":"#FFFFFFFF","used_g":42.5,"used_m":15.0}]}]}"##
                .to_string(),
        );

        let dto = to_dto(file, &[]);

        assert_eq!(dto.weight_source, "slicer");
        assert!((dto.estimated_weight_g.expect("weight") - 42.5).abs() < 1e-6);
        let slice_info = dto.slice_info.expect("slice info dto present");
        assert_eq!(slice_info.plates.len(), 1);
        assert_eq!(slice_info.plates[0].filaments[0].filament_type, "PLA");
    }

    #[test]
    fn to_dto_serializes_slice_info_with_camel_case_and_type_rename() {
        let mut file = sample_file_record(3, None, "2026-09-13T00:00:00Z");
        file.volume_cm3 = Some(100.0);
        file.slice_info_json = Some(
            r##"{"total_weight_g":42.5,"plates":[{"plate_index":1,"weight_g":42.5,"filaments":[{"filament_type":"PLA","color":"#FFFFFFFF","used_g":42.5,"used_m":15.0}]}]}"##
                .to_string(),
        );

        let spools = vec![sample_spool("PLA", 1000, Some(20.0))]; // 0.02/g

        let dto = to_dto(file, &spools);
        let dto_json = serde_json::to_value(&dto).expect("dto serializes to JSON");

        assert_eq!(dto_json["weightSource"], "slicer");
        assert_eq!(dto_json["sliceInfo"]["totalWeightG"], 42.5);
        assert_eq!(dto_json["sliceInfo"]["plates"][0]["plateIndex"], 1);
        assert_eq!(dto_json["sliceInfo"]["plates"][0]["weightG"], 42.5);
        // Critical: FilamentUsageDto::filament_type must serialize as "type",
        // not "filamentType", to match the frontend contract.
        assert_eq!(dto_json["sliceInfo"]["plates"][0]["filaments"][0]["type"], "PLA");
        assert_eq!(
            dto_json["sliceInfo"]["plates"][0]["filaments"][0]["color"],
            "#FFFFFFFF"
        );
        assert_eq!(dto_json["sliceInfo"]["plates"][0]["filaments"][0]["usedG"], 42.5);
        assert_eq!(dto_json["sliceInfo"]["plates"][0]["filaments"][0]["usedM"], 15.0);
        // 42.5g * 0.02/g = 0.85
        assert!((dto_json["costEstimate"]["totalCost"].as_f64().expect("total cost number") - 0.85).abs() < 1e-6);
        assert_eq!(dto_json["costEstimate"]["hasUnpricedFilaments"], false);
    }

    #[test]
    fn to_dto_falls_back_to_estimate_when_slice_info_absent() {
        let mut file = sample_file_record(2, None, "2026-09-13T00:00:00Z");
        file.volume_cm3 = Some(10.0);
        file.slice_info_json = None;

        let dto = to_dto(file, &[]);

        assert_eq!(dto.weight_source, "estimated");
        assert!(dto.slice_info.is_none());
        assert!(dto.estimated_weight_g.is_some());
        assert!(dto.cost_estimate.is_none());
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
            slice_info_json: None,
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
            zip.start_file("Metadata/slice_info.config", options).unwrap();
            zip.write_all(slice_info_xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("import_one_slice_info_test_{nanos}.3mf"));
        std::fs::write(&path, &buf).expect("write temp file");

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let dto = import_one(&mut conn, &path, None, None).expect("import should succeed");

        let stored = crate::db::get_file(&conn, dto.id.parse().unwrap()).expect("query").expect("present");
        assert!(stored.slice_info_json.is_some());
        assert!(stored.slice_info_json.unwrap().contains("9.9"));

        let _ = std::fs::remove_file(&path);
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
                    zip.start_file("Metadata/slice_info.config", options).unwrap();
                    zip.write_all(xml.as_bytes()).unwrap();
                }
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("rescan_test_{nanos}.3mf"));
        write_3mf(&path, None);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let imported = import_one(&mut conn, &path, None, None).expect("initial import");
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
                    zip.start_file("Metadata/slice_info.config", options).unwrap();
                    zip.write_all(xml.as_bytes()).unwrap();
                }
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("rescan_hash_test_{nanos}.3mf"));
        write_3mf(&path, None);

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let imported = import_one(&mut conn, &path, None, None).expect("initial import");
        let id: i64 = imported.id.parse().unwrap();

        let before = db::get_file(&conn, id).expect("get_file").expect("file exists");
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

        let after = db::get_file(&conn, id).expect("get_file").expect("file exists");
        assert_ne!(after.file_size_bytes, size_before, "file_size_bytes must reflect rescanned content");
        assert_ne!(after.content_hash, hash_before, "content_hash must reflect rescanned content");
        assert_eq!(after.content_hash, Some(expected_hash), "content_hash must match hash of new disk content");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rescan_file_returns_error_when_file_missing_on_disk() {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("rescan_missing_test_{nanos}.3mf"));
        // Nie geschrieben - Datei existiert nicht auf der Platte.

        let mut conn = crate::db::connect_in_memory().expect("connect");
        let mut new_file = sample_new_file_for_rescan_test(&path);
        new_file.file_type = FileType::ThreeMf;
        let id = crate::db::insert_file(&mut conn, &new_file).expect("insert");

        let result = rescan_file(&mut conn, id);
        assert!(result.is_err());
    }

    /// Minimaler `NewFile` fuer den Fehlerfall-Test oben - nur Pfad/Typ sind
    /// relevant, alle anderen Felder sind fuer `rescan_file` irrelevant, da die
    /// Funktion bei fehlender Datei abbricht, bevor sie sie liest.
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
        // sample_file_record baut nur ein FileRecord in-memory, nicht in der DB -
        // fuer diesen Test wird stattdessen eine minimale echte Datei ueber
        // insert_file angelegt, da add_print_log_entry einen echten file_id
        // Fremdschluessel braucht.
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
}
