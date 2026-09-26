// Shared types and helpers of the Tauri commands; the commands themselves live
// in the submodules by topic and are re-exported here.
mod archives;
mod backup;
mod collections;
mod dropped_image;
mod files;
mod filament;
mod folders;
mod printer_link;
mod printers;
mod security;
mod slicers;
mod trash;

pub use archives::*;
pub use backup::*;
pub use collections::*;
pub use dropped_image::*;
pub use files::*;
pub use filament::*;
pub use folders::*;
pub use printer_link::*;
pub use printers::*;
pub use security::*;
pub use slicers::*;
pub use trash::*;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::db::error::DbError;
use crate::db::models::{FileType, MaterialRecord, NewFile, ScannedMetadataUpdate};
use crate::db::{self, models::FileRecord};
use crate::geometry::RenderMesh;
use crate::slicers::detect_slicers;
use crate::tagging::{self, TaggingContext};
use crate::{obj, stl, threemf, update_check};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub trash_dir: std::path::PathBuf,
    pub db_path: std::path::PathBuf,
    /// Directories that must never be written to (computed once at startup). An
    /// imported backup can point `folders.path`/`files.path` anywhere, e.g. to
    /// autostart folders.
    pub sensitive_dirs: Vec<std::path::PathBuf>,
}
pub(crate) type CmdResult<T> = Result<T, String>;
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
    pub content_hash: Option<String>,
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
/// Estimates the material cost: for each filament in `slice_info`, stock spools
/// whose `material` contains the type are looked up (ignoring color, which
/// rarely matches) and their average price per gram is used. Filaments without
/// a price don't count (0 would mean "free"); `has_unpriced_filaments` then marks
/// the total as incomplete.
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
                    s.kind == db::models::SPOOL_KIND_FILAMENT
                        && s.material.to_lowercase().contains(&type_lower)
                        && s.price.is_some()
                        && s.original_weight_g > 0.0
                })
                .collect();

            if matching.is_empty() {
                has_unpriced = true;
                continue;
            }

            let avg_price_per_gram: f64 = matching
                .iter()
                .map(|s| s.price.unwrap() / s.original_weight_g)
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
        content_hash: file.content_hash,
        creator: file.creator,
        custom_image,
        thumbnail_image,
        render_snapshot_image,
        source_url: sanitize_source_url(file.source_url),
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
/// Moves a file without ever overwriting an existing target.
///
/// Always copies with `OpenOptions::create_new(true)` (O_EXCL/CREATE_NEW):
/// `std::fs::rename` replaces an existing target on POSIX and Windows, a prior
/// `exists()` check would be a TOCTOU window, and rename-no-replace only exists
/// as three platform-specific syscalls. Not atomic as a whole: after a crash,
/// source and target can briefly both exist.
pub(crate) fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    let mut dst = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;

    let copy_result = (|| -> std::io::Result<()> {
        let mut src = std::fs::File::open(from)?;
        std::io::copy(&mut src, &mut dst)?;
        dst.sync_all()?;
        Ok(())
    })();

    if let Err(e) = copy_result {
        // A partially written target is always cleaned up.
        drop(dst);
        let _ = std::fs::remove_file(to);
        return Err(e);
    }
    drop(dst);

    if let Err(remove_err) = std::fs::remove_file(from) {
        let _ = std::fs::remove_file(to);
        return Err(remove_err);
    }
    Ok(())
}
/// Second barrier next to `validate_folder_name`: checks the resolved target
/// path against sensitive system directories (config, autostart, SSH/GPG, system
/// roots). Applies wherever a path from the DB goes to
/// `fs::rename`/`fs::create_dir`, because an imported backup can inject e.g.
/// `path = ~/.config/autostart`.
///
/// The RESOLVED path is compared (`resolve_path_for_sensitivity_check`),
/// otherwise `..` or symlinks could bypass the check.
pub(crate) fn reject_if_sensitive_path(path: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    reject_if_sensitive_path_expanded(path, &expand_sensitive_dirs(sensitive_dirs))
}
/// Adds the canonical spelling of each protected folder (e.g. /bin -> /usr/bin).
/// Compute once when many paths are checked.
fn expand_sensitive_dirs(sensitive_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded = Vec::with_capacity(sensitive_dirs.len());
    for dir in sensitive_dirs {
        if let Ok(canonical) = dir.canonicalize() {
            if canonical != *dir {
                expanded.push(canonical);
            }
        }
        expanded.push(dir.clone());
    }
    expanded
}
fn reject_if_sensitive_path_expanded(path: &Path, expanded_dirs: &[PathBuf]) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(path)?;
    for dir in expanded_dirs {
        if resolved == *dir || resolved.starts_with(dir) {
            return Err(format!(
                "Zielpfad liegt in einem geschuetzten Systemverzeichnis ({}) und wird abgelehnt",
                dir.display()
            ));
        }
    }
    Ok(())
}
/// Containment instead of a denylist: `path` must lie inside the trash. Otherwise
/// a backup could set `trash_path = ~/Documents/important.pdf` with an old
/// `deleted_at`, and `purge_expired_trash_on_startup` would delete the file on
/// the next start. Both sides are resolved (`..`, symlinks).
fn reject_if_outside_trash_dir(path: &Path, resolved_trash_dir: &Path) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(path)?;
    if resolved == resolved_trash_dir || !resolved.starts_with(resolved_trash_dir) {
        return Err(format!(
            "Papierkorb-Pfad liegt ausserhalb des Papierkorb-Verzeichnisses ({}) und wird abgelehnt",
            resolved_trash_dir.display()
        ));
    }
    Ok(())
}
/// Resolves a path far enough that the prefix comparison in
/// `reject_if_sensitive_path` can't be undermined by `..` components or symlinks.
/// The path need NOT exist - at many call sites it's a target still to be
/// created (new folder, move target, freshly chosen catalog base directory):
/// - if the path exists, `canonicalize` decides (resolves symlinks AND `..`);
/// - if not, every literal `..` component is rejected (it could lead out of the
///   checked subtree) and the longest existing ancestor is resolved instead,
///   with the remaining components appended literally.
fn resolve_path_for_sensitivity_check(path: &Path) -> CmdResult<PathBuf> {
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(
            "Pfad enthaelt \"..\"-Komponenten und wird abgelehnt".to_string(),
        );
    }

    let mut trailing: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor = path;
    while let (Some(name), Some(parent)) = (cursor.file_name(), cursor.parent()) {
        trailing.push(name.to_os_string());
        if let Ok(canonical) = parent.canonicalize() {
            let mut resolved = canonical;
            for component in trailing.iter().rev() {
                resolved.push(component);
            }
            return Ok(resolved);
        }
        cursor = parent;
    }
    Ok(path.to_path_buf())
}
// Only allow http(s) links: the frontend renders the URL unchanged as <a href>,
// and a "javascript:"/"data:" value would be executed on click instead of
// navigated (close to CWE-79).
fn validate_source_url(url: Option<String>) -> CmdResult<Option<String>> {
    let Some(trimmed) = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()) else {
        return Ok(None);
    };
    if is_http_url(&trimmed) {
        Ok(Some(trimmed))
    } else {
        Err("source URL must start with http:// or https://".to_string())
    }
}
fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}
/// Read-side counterpart to `validate_source_url`: an imported catalog brings the
/// column unchecked. Invalid values are silently dropped.
fn sanitize_source_url(url: Option<String>) -> Option<String> {
    url.map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty() && is_http_url(u))
}

// Von Tests in mehreren Untermodulen gebraucht.
#[cfg(test)]
pub(crate) fn unique_test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "{name}_{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create test dir");
    dir
}

/// Minimal `FileRecord` for tests, remaining fields with placeholders.
#[cfg(test)]
pub(crate) fn sample_file_record(id: i64, content_hash: Option<&str>, imported_at: &str) -> FileRecord {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spool(material: &str, original_weight_g: f64, price: Option<f64>) -> db::models::FilamentSpoolRecord {
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
            color_hex: None,
            home_location: None,
            unit_id: None,
            slot_index: None,
            kind: "filament".to_string(),
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
    fn estimate_material_cost_uses_single_matching_spool() {
        let slice_info = sample_slice_info_single_filament("PLA", 20.0);
        let spools = vec![sample_spool("PLA", 1000.0, Some(20.0))]; // 0.02 per gram
        let cost = estimate_material_cost(&slice_info, &spools);
        assert!((cost.total_cost.expect("cost") - 0.4).abs() < 1e-6);
        assert!(!cost.has_unpriced_filaments);
    }
    #[test]
    fn estimate_material_cost_averages_multiple_matching_spools() {
        let slice_info = sample_slice_info_single_filament("PLA", 10.0);
        let spools = vec![
            sample_spool("PLA", 1000.0, Some(20.0)), // 0.02/g
            sample_spool("Generic PLA", 1000.0, Some(30.0)), // 0.03/g
        ];
        let cost = estimate_material_cost(&slice_info, &spools);
        // Durchschnitt 0.025/g * 10g = 0.25
        assert!((cost.total_cost.expect("cost") - 0.25).abs() < 1e-6);
    }
    #[test]
    fn estimate_material_cost_returns_none_when_no_matching_material() {
        let slice_info = sample_slice_info_single_filament("PETG", 10.0);
        let spools = vec![sample_spool("PLA", 1000.0, Some(20.0))];
        let cost = estimate_material_cost(&slice_info, &spools);
        assert_eq!(cost.total_cost, None);
        assert!(cost.has_unpriced_filaments);
    }
    #[test]
    fn estimate_material_cost_ignores_spools_without_price() {
        let slice_info = sample_slice_info_single_filament("PLA", 10.0);
        let spools = vec![sample_spool("PLA", 1000.0, None)];
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
        let spools = vec![sample_spool("PLA", 1000.0, Some(20.0))]; // 0.02/g, no nylon in stock
        let cost = estimate_material_cost(&slice_info, &spools);
        assert!((cost.total_cost.expect("cost") - 0.4).abs() < 1e-6); // PLA share only
        assert!(cost.has_unpriced_filaments); // Nylon is missing
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
        // The empty type string must not "match" every spool (every string contains "").
        let spools = vec![
            sample_spool("PLA", 1000.0, Some(20.0)),
            sample_spool("PETG", 1000.0, Some(25.0)),
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
        file.volume_cm3 = Some(100.0); // would give an estimate without slice_info
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
    fn to_dto_includes_content_hash() {
        let dto = to_dto(sample_file_record(7, Some("hash-7"), "2026-09-24T08:00:00+00:00"), &[]);
        assert_eq!(dto.content_hash.as_deref(), Some("hash-7"));
        let none = to_dto(sample_file_record(8, None, "2026-09-24T08:00:00+00:00"), &[]);
        assert_eq!(none.content_hash, None);
    }
    #[test]
    fn to_dto_serializes_slice_info_with_camel_case_and_type_rename() {
        let mut file = sample_file_record(3, None, "2026-09-13T00:00:00Z");
        file.volume_cm3 = Some(100.0);
        file.slice_info_json = Some(
            r##"{"total_weight_g":42.5,"plates":[{"plate_index":1,"weight_g":42.5,"filaments":[{"filament_type":"PLA","color":"#FFFFFFFF","used_g":42.5,"used_m":15.0}]}]}"##
                .to_string(),
        );

        let spools = vec![sample_spool("PLA", 1000.0, Some(20.0))]; // 0.02/g

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
    #[test]
    fn reject_if_sensitive_path_rejects_exact_match_and_descendants_but_not_siblings() {
        // Deliberately no "/home/..." prefix: on macOS "/home" is an automounter
        // symlink and would be resolved while canonicalizing.
        let sensitive = vec![PathBuf::from("/nonexistent-3mf-test-root/.config")];
        assert!(reject_if_sensitive_path(Path::new("/nonexistent-3mf-test-root/.config"), &sensitive).is_err());
        assert!(reject_if_sensitive_path(Path::new("/nonexistent-3mf-test-root/.config/autostart"), &sensitive).is_err());
        assert!(reject_if_sensitive_path(Path::new("/nonexistent-3mf-test-root/.config-backup"), &sensitive).is_ok());
        assert!(reject_if_sensitive_path(Path::new("/nonexistent-3mf-test-root/3D-Drucke"), &sensitive).is_ok());
    }
    #[test]
    fn reject_if_sensitive_path_rejects_parent_dir_traversal_into_sensitive_dir() {
        // Without resolving, the prefix comparison would say "no match" although the
        // path really lands in the protected directory.
        let base = unique_test_dir("reject_sensitive_traversal");
        let sensitive = base.join(".config");
        std::fs::create_dir_all(&sensitive).unwrap();
        let catalog = base.join("Modelle");
        std::fs::create_dir_all(&catalog).unwrap();

        let escaping = catalog.join("../.config/autostart");
        let result = reject_if_sensitive_path(&escaping, std::slice::from_ref(&sensitive));
        let inside = reject_if_sensitive_path(&catalog.join("Unterordner"), &[sensitive]);

        let _ = std::fs::remove_dir_all(&base);
        assert!(result.is_err(), "a path traversing into a sensitive dir must be rejected");
        assert!(inside.is_ok(), "a regular new subfolder of the catalog must stay allowed: {inside:?}");
    }
    #[test]
    fn reject_if_sensitive_path_rejects_symlink_into_sensitive_dir() {
        #[cfg(unix)]
        {
            let base = unique_test_dir("reject_sensitive_symlink");
            let sensitive = base.join(".config");
            std::fs::create_dir_all(&sensitive).unwrap();
            let link = base.join("harmlos");
            std::os::unix::fs::symlink(&sensitive, &link).unwrap();

            let result = reject_if_sensitive_path(&link, &[sensitive]);

            let _ = std::fs::remove_dir_all(&base);
            assert!(result.is_err(), "a symlink pointing into a sensitive dir must be rejected");
        }
    }
    #[test]
    fn reject_if_sensitive_path_allows_a_not_yet_existing_fresh_catalog_dir() {
        // Most important non-regression case of canonicalization: the catalog base
        // directory chosen during setup doesn't exist yet.
        let base = unique_test_dir("reject_sensitive_fresh");
        let fresh = base.join("Neuer Katalog").join("Unterordner");
        let result = reject_if_sensitive_path(&fresh, &[std::env::temp_dir().join("3mf-nichts-davon")]);
        let _ = std::fs::remove_dir_all(&base);
        assert!(result.is_ok(), "a fresh, not yet created catalog dir must remain a valid choice: {result:?}");
    }
    #[test]
    fn sanitize_source_url_drops_non_http_values_instead_of_erroring() {
        assert_eq!(
            sanitize_source_url(Some("https://example.org/x".to_string())),
            Some("https://example.org/x".to_string())
        );
        assert_eq!(
            sanitize_source_url(Some("http://example.org/x".to_string())),
            Some("http://example.org/x".to_string())
        );
        assert_eq!(sanitize_source_url(Some("javascript:alert(1)".to_string())), None);
        assert_eq!(sanitize_source_url(Some("data:text/html,<script>".to_string())), None);
        assert_eq!(sanitize_source_url(Some("file:///etc/passwd".to_string())), None);
        assert_eq!(sanitize_source_url(Some("   ".to_string())), None);
        assert_eq!(sanitize_source_url(None), None);
    }
    #[test]
    fn to_dto_drops_a_non_http_source_url_from_an_imported_row() {
        let mut file = sample_file_record(1, None, "2026-09-19T00:00:00Z");
        file.source_url = Some("javascript:alert(document.domain)".to_string());
        let dto = to_dto(file, &[]);
        assert_eq!(dto.source_url, None, "a javascript: URL must never reach the frontend as an href");
    }
    #[test]
    fn to_dto_keeps_a_regular_http_source_url() {
        let mut file = sample_file_record(1, None, "2026-09-19T00:00:00Z");
        file.source_url = Some("https://makerworld.com/de/models/1".to_string());
        let dto = to_dto(file, &[]);
        assert_eq!(dto.source_url, Some("https://makerworld.com/de/models/1".to_string()));
    }
    #[test]
    fn move_file_refuses_to_overwrite_an_existing_destination() {
        let dir = unique_test_dir("move_file_no_clobber");
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("source.3mf");
        let to = dir.join("dest.3mf");
        std::fs::write(&from, b"SOURCE-CONTENT").unwrap();
        std::fs::write(&to, b"EXISTING-DEST-CONTENT").unwrap();

        let result = move_file(&from, &to);

        assert!(result.is_err(), "move_file must refuse to overwrite an existing destination");
        assert_eq!(
            std::fs::read(&to).unwrap(),
            b"EXISTING-DEST-CONTENT",
            "destination content must be unchanged after a refused move"
        );
        assert!(from.exists(), "source must still exist after a refused move");
    }
    #[test]
    fn move_file_cleans_up_a_partially_written_destination_on_copy_failure() {
        // A partial copy that failed during io::copy()/sync_all() must always be cleaned up.
        let dir = unique_test_dir("move_file_partial_copy_cleanup");
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("source.3mf");
        std::fs::write(&from, vec![0u8; 10 * 1024 * 1024]).unwrap(); // 10 MB
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&from, std::fs::Permissions::from_mode(0o000)).unwrap();
            let to = dir.join("dest.3mf");
            let result = move_file(&from, &to);
            // Reset permissions so the unique_test_dir cleanup (if any) isn't blocked.
            std::fs::set_permissions(&from, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(result.is_err());
            assert!(!to.exists(), "partially written destination must be cleaned up on copy failure");
        }
    }
    #[test]
    fn estimate_material_cost_ignores_resin() {
        let slice_info = sample_slice_info_single_filament("PLA", 10.0);
        let resin = db::models::FilamentSpoolRecord { kind: "resin".into(), ..sample_spool("PLA", 1000.0, Some(20.0)) };
        let cost = estimate_material_cost(&slice_info, &[resin]);
        assert_eq!(cost.total_cost, None);
        assert!(cost.has_unpriced_filaments);
    }
}
