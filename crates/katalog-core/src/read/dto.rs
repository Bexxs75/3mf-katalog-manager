use crate::db::models::FileRecord;
use crate::{db, threemf};
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
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
    pub file_modified_at: Option<String>,
    pub print_status: String,
    pub estimated_weight_g: Option<f64>,
    pub last_viewed_at: Option<String>,
    pub content_hash: Option<String>,
    pub creator: Option<String>,
    pub custom_image: Option<String>,
    pub thumbnail_image: Option<String>,
    pub render_snapshot_image: Option<String>,
    pub source_url: Option<String>,
    pub license: Option<String>,
    pub source_platform: Option<String>,
    pub source_model_id: Option<String>,
    pub source_profile_id: Option<String>,
    pub queue_position: Option<i64>,
    pub favorite: bool,
    pub plate_count: Option<i64>,
    pub weight_source: String,
    pub slice_info: Option<SliceInfoDto>,
    pub cost_estimate: Option<CostEstimateDto>,
    pub deleted_at: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDto {
    pub name: String,
    pub display_color: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilamentUsageDto {
    #[serde(rename = "type")]
    pub filament_type: String,
    pub color: Option<String>,
    pub used_g: f64,
    pub used_m: f64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlateFilamentUsageDto {
    pub plate_index: u32,
    pub weight_g: f64,
    pub filaments: Vec<FilamentUsageDto>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceInfoDto {
    pub total_weight_g: f64,
    pub plates: Vec<PlateFilamentUsageDto>,
}
#[derive(Debug, Clone, Serialize)]
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
pub fn estimate_material_cost(
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

pub fn estimate_weight_g(volume_cm3: Option<f64>, material_name: Option<&str>) -> Option<f64> {
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
pub fn encode_image(bytes: Option<Vec<u8>>) -> Option<String> {
    use base64::Engine;
    bytes.map(|b| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b)
        )
    })
}
pub fn to_dto(file: FileRecord, spools: &[db::models::FilamentSpoolRecord]) -> ModelFileDto {
    let slice_info: Option<threemf::SliceInfo> = file
        .slice_info_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    let (estimated_weight_g, weight_source) = match &slice_info {
        Some(info) => (Some(info.total_weight_g), "slicer".to_string()),
        None => (
            estimate_weight_g(
                file.volume_cm3,
                file.materials.first().map(|m| m.name.as_str()),
            ),
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
        folder_id: file.folder_id.map(|id| id.to_string()).unwrap_or_default(),
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
        file_modified_at: file.file_modified_at,
        print_status: file.print_status,
        estimated_weight_g,
        last_viewed_at: file.last_viewed_at,
        content_hash: file.content_hash,
        creator: file.creator,
        custom_image,
        thumbnail_image,
        render_snapshot_image,
        source_url: sanitize_source_url(file.source_url),
        license: file.license,
        source_platform: file.source_platform,
        source_model_id: file.source_model_id,
        source_profile_id: file.source_profile_id,
        queue_position: file.queue_position,
        favorite: file.favorite,
        plate_count: file.plate_count,
        weight_source,
        slice_info: slice_info.map(SliceInfoDto::from),
        cost_estimate,
        deleted_at: file.deleted_at,
    }
}
fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}
fn sanitize_source_url(url: Option<String>) -> Option<String> {
    url.map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty() && is_http_url(u))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::FileType;
    use std::collections::BTreeMap;
    pub fn sample_file_record(
        id: i64,
        content_hash: Option<&str>,
        imported_at: &str,
    ) -> FileRecord {
        FileRecord {
            source_platform: None,
            source_model_id: None,
            source_profile_id: None,
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
            license: None,
            queue_position: None,
            favorite: false,
            plate_count: None,
            slice_info_json: None,
            deleted_at: None,
            trash_path: None,
        }
    }
    fn sample_spool(
        material: &str,
        original_weight_g: f64,
        price: Option<f64>,
    ) -> db::models::FilamentSpoolRecord {
        db::models::FilamentSpoolRecord {
            color_hexes: Vec::new(),
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
            sample_spool("PLA", 1000.0, Some(20.0)),         // 0.02/g
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
        let b64 = result
            .strip_prefix("data:image/png;base64,")
            .expect("data url prefix");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("valid base64");
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
        let dto = to_dto(
            sample_file_record(7, Some("hash-7"), "2026-09-24T08:00:00+00:00"),
            &[],
        );
        assert_eq!(dto.content_hash.as_deref(), Some("hash-7"));
        let none = to_dto(
            sample_file_record(8, None, "2026-09-24T08:00:00+00:00"),
            &[],
        );
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
        assert_eq!(
            dto_json["sliceInfo"]["plates"][0]["filaments"][0]["type"],
            "PLA"
        );
        assert_eq!(
            dto_json["sliceInfo"]["plates"][0]["filaments"][0]["color"],
            "#FFFFFFFF"
        );
        assert_eq!(
            dto_json["sliceInfo"]["plates"][0]["filaments"][0]["usedG"],
            42.5
        );
        assert_eq!(
            dto_json["sliceInfo"]["plates"][0]["filaments"][0]["usedM"],
            15.0
        );
        // 42.5g * 0.02/g = 0.85
        assert!(
            (dto_json["costEstimate"]["totalCost"]
                .as_f64()
                .expect("total cost number")
                - 0.85)
                .abs()
                < 1e-6
        );
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
    fn sanitize_source_url_drops_non_http_values_instead_of_erroring() {
        assert_eq!(
            sanitize_source_url(Some("https://example.org/x".to_string())),
            Some("https://example.org/x".to_string())
        );
        assert_eq!(
            sanitize_source_url(Some("http://example.org/x".to_string())),
            Some("http://example.org/x".to_string())
        );
        assert_eq!(
            sanitize_source_url(Some("javascript:alert(1)".to_string())),
            None
        );
        assert_eq!(
            sanitize_source_url(Some("data:text/html,<script>".to_string())),
            None
        );
        assert_eq!(
            sanitize_source_url(Some("file:///etc/passwd".to_string())),
            None
        );
        assert_eq!(sanitize_source_url(Some("   ".to_string())), None);
        assert_eq!(sanitize_source_url(None), None);
    }
    #[test]
    fn to_dto_drops_a_non_http_source_url_from_an_imported_row() {
        let mut file = sample_file_record(1, None, "2026-09-19T00:00:00Z");
        file.source_url = Some("javascript:alert(document.domain)".to_string());
        let dto = to_dto(file, &[]);
        assert_eq!(
            dto.source_url, None,
            "a javascript: URL must never reach the frontend as an href"
        );
    }
    #[test]
    fn to_dto_keeps_a_regular_http_source_url() {
        let mut file = sample_file_record(1, None, "2026-09-19T00:00:00Z");
        file.source_url = Some("https://makerworld.com/de/models/1".to_string());
        let dto = to_dto(file, &[]);
        assert_eq!(
            dto.source_url,
            Some("https://makerworld.com/de/models/1".to_string())
        );
    }
    #[test]
    fn estimate_material_cost_ignores_resin() {
        let slice_info = sample_slice_info_single_filament("PLA", 10.0);
        let resin = db::models::FilamentSpoolRecord {
            kind: "resin".into(),
            ..sample_spool("PLA", 1000.0, Some(20.0))
        };
        let cost = estimate_material_cost(&slice_info, &[resin]);
        assert_eq!(cost.total_cost, None);
        assert!(cost.has_unpriced_filaments);
    }
}
