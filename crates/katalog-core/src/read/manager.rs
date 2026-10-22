use super::{ReadError, ReadResult};
use crate::db;
use crate::db::printers as p;
use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilamentSpoolDto {
    pub id: String,
    pub material: String,
    pub manufacturer: Option<String>,
    pub color: Option<String>,
    pub location: Option<String>,
    pub diameter_mm: f64,
    pub original_weight_g: f64,
    pub remaining_weight_g: f64,
    pub price: Option<f64>,
    pub image_png: Option<String>,
    #[serde(default)]
    pub color_hex: Option<String>,
    /// Up to four colours of a multi-colour spool, empty for single-colour. The backend keeps
    /// `color_hex` equal to the first entry, so the slicer colour match stays unchanged.
    #[serde(default)]
    pub color_hexes: Vec<String>,
    /// Read-only: home location, unit and slot only change via `load_spool`/`unload_spool` (commands/printers.rs).
    #[serde(default)]
    pub home_location: Option<String>,
    #[serde(default)]
    pub unit_id: Option<String>,
    #[serde(default)]
    pub slot_index: Option<i64>,
    /// "filament" or "resin"; if missing (older callers), "filament" applies.
    #[serde(default = "default_spool_kind")]
    pub kind: String,
}
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MaterialUnitDto {
    pub id: String,
    pub printer_id: String,
    pub name: String,
    pub kind: String,
    pub slot_count: i64,
    pub bambu_ams_index: Option<i64>,
}
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrinterDto {
    pub id: String,
    pub name: String,
    /// "filament" or "resin", fixed when the printer is added.
    pub kind: String,
    pub units: Vec<MaterialUnitDto>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub nozzle_mm: Option<f64>,
    pub bed_x_mm: Option<f64>,
    pub bed_y_mm: Option<f64>,
    pub bed_z_mm: Option<f64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterHistoryJobDto {
    pub id: String,
    pub file_name: String,
    pub outcome: String,
    pub print_duration_s: f64,
    pub used_mm: f64,
    pub grams: Option<f64>,
    pub state: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPrinterDto {
    pub printer_name: String,
    pub ended_at: f64,
}
fn default_spool_kind() -> String {
    db::models::SPOOL_KIND_FILAMENT.into()
}
pub fn spool_record_to_dto(s: db::models::FilamentSpoolRecord) -> FilamentSpoolDto {
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
        color_hex: s.color_hex,
        color_hexes: s.color_hexes,
        home_location: s.home_location,
        unit_id: s.unit_id.map(|id| id.to_string()),
        slot_index: s.slot_index,
        kind: s.kind,
    }
}
pub fn unit_dto(u: db::models::MaterialUnitRecord) -> MaterialUnitDto {
    MaterialUnitDto {
        id: u.id.to_string(),
        printer_id: u.printer_id.to_string(),
        name: u.name,
        kind: u.kind,
        slot_count: u.slot_count,
        bambu_ams_index: u.bambu_ams_index,
    }
}
pub fn list_printers_with_conn(conn: &Connection) -> ReadResult<Vec<PrinterDto>> {
    let printers = p::list_printers(conn).map_err(ReadError::from)?;
    let units = p::list_units(conn).map_err(ReadError::from)?;
    Ok(printers
        .into_iter()
        .map(|printer| PrinterDto {
            id: printer.id.to_string(),
            name: printer.name,
            kind: printer.kind,
            manufacturer: printer.manufacturer,
            model: printer.model,
            nozzle_mm: printer.nozzle_mm,
            bed_x_mm: printer.bed_x_mm,
            bed_y_mm: printer.bed_y_mm,
            bed_z_mm: printer.bed_z_mm,
            units: units
                .iter()
                .filter(|u| u.printer_id == printer.id)
                .cloned()
                .map(unit_dto)
                .collect(),
        })
        .collect())
}
pub fn check_filament_with_conn(
    conn: &Connection,
    file_ids: &[String],
) -> ReadResult<Vec<crate::filament_check::ModelCheck>> {
    use crate::filament_check::{check_models, SlotRef, SpoolInput};

    let mut models = Vec::new();
    for raw in file_ids {
        let Ok(id) = raw.parse::<i64>() else { continue };
        let Some(file) = db::get_file(conn, id).map_err(ReadError::from)? else {
            continue;
        };
        if file.deleted_at.is_some() {
            continue;
        }
        let slice = file
            .slice_info_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<crate::threemf::SliceInfo>(json).ok());
        models.push((raw.clone(), slice));
    }

    let printers = db::printers::list_printers(conn).map_err(ReadError::from)?;
    let units = db::printers::list_units(conn).map_err(ReadError::from)?;
    let spools = db::list_filament_spools_with_images(conn, false)
        .map_err(ReadError::from)?
        .into_iter()
        // "Is there enough filament?" (and the queue symbol) only with filament.
        .filter(|s| s.kind == db::models::SPOOL_KIND_FILAMENT)
        .map(|s| {
            let slot = match (s.unit_id, s.slot_index) {
                (Some(unit_id), Some(slot_index)) => {
                    units.iter().find(|u| u.id == unit_id).map(|u| SlotRef {
                        printer: printers
                            .iter()
                            .find(|p| p.id == u.printer_id)
                            .map(|p| p.name.clone())
                            .unwrap_or_default(),
                        unit: u.name.clone(),
                        slot_number: slot_index + 1,
                    })
                }
                _ => None,
            };
            SpoolInput {
                id: s.id.to_string(),
                material: s.material,
                manufacturer: s.manufacturer,
                color_name: s.color,
                color_hex: s.color_hex,
                remaining_g: s.remaining_weight_g,
                original_g: s.original_weight_g,
                slot,
                location: s.location,
            }
        })
        .collect::<Vec<_>>();

    Ok(check_models(&models, &spools))
}
pub fn list_printer_history_with_conn(
    conn: &Connection,
    printer_id: i64,
) -> ReadResult<Vec<PrinterHistoryJobDto>> {
    list_printer_history_limit(conn, printer_id, -1)
}
pub fn list_printer_history_limit(
    conn: &Connection,
    printer_id: i64,
    limit: i64,
) -> ReadResult<Vec<PrinterHistoryJobDto>> {
    let mut stmt = conn.prepare("SELECT id, file_name, outcome, print_duration_s, used_mm, booked_g, state FROM printer_jobs WHERE printer_id=?1 ORDER BY ended_at DESC, id DESC LIMIT ?2").map_err(sql_error)?;
    let rows = stmt
        .query_map([printer_id, limit], |r| {
            Ok(PrinterHistoryJobDto {
                id: r.get::<_, i64>(0)?.to_string(),
                file_name: r.get(1)?,
                outcome: r.get(2)?,
                print_duration_s: r.get(3)?,
                used_mm: r.get(4)?,
                grams: r.get(5)?,
                state: r.get(6)?,
            })
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}
pub fn get_last_printer_for_file_with_conn(
    conn: &Connection,
    file_id: i64,
) -> ReadResult<Option<LastPrinterDto>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT p.name, j.ended_at FROM printer_jobs j JOIN printers p ON p.id = j.printer_id WHERE j.state = 'confirmed' AND j.booked_file_id = ?1 ORDER BY j.ended_at DESC, j.id DESC LIMIT 1",
        [file_id],
        |row| Ok(LastPrinterDto { printer_name: row.get(0)?, ended_at: row.get(1)? }),
    ).optional().map_err(sql_error)
}
fn sql_error(e: rusqlite::Error) -> ReadError {
    let message = e.to_string();
    let mut mapped = ReadError::from(db::error::DbError::from(e));
    mapped.message = message;
    mapped
}

pub fn list_filament_spools(
    conn: &Connection,
    include_images: bool,
) -> ReadResult<Vec<FilamentSpoolDto>> {
    Ok(db::list_filament_spools_with_images(conn, include_images)?
        .into_iter()
        .map(spool_record_to_dto)
        .collect())
}

/// Web lists omit image data entirely. Images are fetched individually on demand.
pub fn list_filament_spools_web(conn: &Connection) -> ReadResult<Vec<serde_json::Value>> {
    db::list_filament_spools_with_image_flags(conn, false)
        .map_err(ReadError::from)?
        .into_iter()
        .map(|(s, has_image)| {
            let s = spool_record_to_dto(s);
            let mut value = serde_json::to_value(s).map_err(|e| ReadError::from(e.to_string()))?;
            let object = value.as_object_mut().expect("spool object");
            object.remove("imagePng");
            object.insert("hasImage".into(), has_image.into());
            Ok(value)
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilamentSpoolImage {
    pub image_png: Option<String>,
}
pub fn get_filament_spool_image(conn: &Connection, id: i64) -> ReadResult<FilamentSpoolImage> {
    use base64::Engine;
    use rusqlite::OptionalExtension;
    let bytes: Option<Option<Vec<u8>>> = conn
        .query_row(
            "SELECT image_png FROM filament_spools WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let bytes = bytes.ok_or_else(|| {
        ReadError::expected("Spule nicht gefunden").with_code(super::GeometryErrorCode::NotFound)
    })?;
    Ok(FilamentSpoolImage {
        image_png: bytes.map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
    })
}
