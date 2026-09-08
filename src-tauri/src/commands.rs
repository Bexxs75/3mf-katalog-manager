use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::db::models::{FileType, MaterialRecord, NewFile};
use crate::db::{self, models::FileRecord};
use crate::format;
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
    pub sync_time_label: String,
    pub volume_label: String,
    pub filesize_label: String,
    pub file_size_bytes: i64,
    pub imported_at: String,
    pub meta: Vec<MetaRow>,
}

#[derive(Debug, Serialize)]
pub struct MetaRow {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryPayloadDto {
    pub extension: String,
    pub data_base64: String,
}

#[derive(Debug, Serialize)]
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

fn to_dto(file: FileRecord) -> ModelFileDto {
    let materials_label = if file.materials.is_empty() {
        "–".to_string()
    } else {
        file.materials
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let object_count_label = file
        .object_count
        .map(|c| c.to_string())
        .unwrap_or_else(|| "–".to_string());

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
        sync_time_label: format::format_relative_time_de(&file.imported_at),
        volume_label: format::format_volume_cm3(file.volume_cm3),
        filesize_label: format::format_bytes(file.file_size_bytes),
        file_size_bytes: file.file_size_bytes,
        imported_at: file.imported_at.clone(),
        meta: vec![
            MetaRow {
                label: "Größe".to_string(),
                value: format::format_dimensions(file.dimensions_mm),
            },
            MetaRow {
                label: "Volumen".to_string(),
                value: format::format_volume_cm3(file.volume_cm3),
            },
            MetaRow {
                label: "Objekte".to_string(),
                value: object_count_label,
            },
            MetaRow {
                label: "Material".to_string(),
                value: materials_label,
            },
            MetaRow {
                label: "Dateigröße".to_string(),
                value: format::format_bytes(file.file_size_bytes),
            },
            MetaRow {
                label: "Importiert".to_string(),
                value: format::format_date_de(&file.imported_at),
            },
        ],
    }
}

fn lock_db<'a>(state: &'a State<AppState>) -> CmdResult<std::sync::MutexGuard<'a, Connection>> {
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

fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "3mf" | "stl"))
        .unwrap_or(false)
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

fn import_one(conn: &mut Connection, path: &Path) -> CmdResult<ModelFileDto> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unbenannt")
        .to_string();
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

    let new_file = NewFile {
        name: file_name,
        path: path.to_string_lossy().to_string(),
        file_type,
        folder_id: None,
        file_size_bytes,
        dimensions_mm,
        volume_cm3,
        object_count,
        thumbnail_png,
        imported_at: chrono::Utc::now().to_rfc3339(),
        file_modified_at: None,
        materials,
        metadata,
        tags,
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
fn import_many(state: &State<AppState>, roots: Vec<PathBuf>) -> CmdResult<Vec<ModelFileDto>> {
    let mut candidates = Vec::new();
    for root in roots {
        collect_supported_files(&root, &mut candidates);
    }

    let mut conn = lock_db(state)?;
    let mut seen = HashSet::new();
    let mut imported = Vec::new();

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

        match import_one(&mut conn, &path) {
            Ok(dto) => imported.push(dto),
            Err(e) => eprintln!("[import] Import fehlgeschlagen für {path_str}: {e}"),
        }
    }

    Ok(imported)
}

#[tauri::command]
pub async fn import_files(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Vec<ModelFileDto>> {
    let picked = app
        .dialog()
        .file()
        .add_filter("3D-Modelle", &["3mf", "stl"])
        .blocking_pick_files();

    let Some(picked) = picked else {
        return Ok(Vec::new());
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
) -> CmdResult<Vec<ModelFileDto>> {
    let picked = app.dialog().file().blocking_pick_folder();

    let Some(picked) = picked else {
        return Ok(Vec::new());
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    import_many(&state, vec![path])
}

#[tauri::command]
pub fn import_dropped(state: State<AppState>, paths: Vec<String>) -> CmdResult<Vec<ModelFileDto>> {
    import_many(&state, paths.into_iter().map(PathBuf::from).collect())
}

#[tauri::command]
pub fn get_model_geometry(state: State<AppState>, file_id: String) -> CmdResult<GeometryPayloadDto> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let conn = lock_db(&state)?;
    let file = db::get_file(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;

    let extension = std::path::Path::new(&file.path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or_else(|| "file has no extension".to_string())?;

    let bytes = std::fs::read(&file.path).map_err(|e| e.to_string())?;

    Ok(GeometryPayloadDto {
        extension,
        data_base64: STANDARD.encode(bytes),
    })
}
