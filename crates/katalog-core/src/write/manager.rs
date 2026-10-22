use super::{validate_image_format, WriteError, WriteResult, MAX_CUSTOM_IMAGE_BYTES};
use crate::db;
use crate::read::manager::{spool_record_to_dto, FilamentSpoolDto};
use rusqlite::Connection;
pub fn validate_color_hex(color_hex: &Option<String>) -> WriteResult<()> {
    match color_hex {
        Some(value) if !db::printers::is_valid_color_hex(value) => {
            Err(format!("ungueltiger Farbwert: {value}").into())
        }
        _ => Ok(()),
    }
}
pub fn validate_color_hexes(color_hexes: &[String]) -> WriteResult<()> {
    db::printers::validate_color_list(color_hexes).map_err(WriteError::expected)
}
pub fn validate_spool_kind(kind: &str) -> WriteResult<()> {
    if db::models::SPOOL_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(format!("ungueltige Art: {kind}").into())
    }
}
pub fn filament_dto_to_record(
    spool: &FilamentSpoolDto,
) -> WriteResult<db::models::NewFilamentSpool> {
    use base64::Engine;
    let image_png = spool
        .image_png
        .as_ref()
        .map(|b64| {
            if b64.len() > MAX_CUSTOM_IMAGE_BYTES.div_ceil(3) * 4 {
                return Err(WriteError::expected("imageUploadTooLarge"));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|_| WriteError::expected("imageUploadUnreadable"))?;
            if bytes.len() > MAX_CUSTOM_IMAGE_BYTES {
                return Err(WriteError::expected("imageUploadTooLarge"));
            }
            validate_image_format(&bytes)?;
            Ok(bytes)
        })
        .transpose()?;
    // Lower-case so equal colours always compare equal; the first list entry wins over a
    // separately sent `color_hex`.
    let color_hexes: Vec<String> = spool.color_hexes.iter().map(|c| c.to_lowercase()).collect();
    let single_hex = spool.color_hex.as_ref().map(|c| c.to_lowercase());
    Ok(db::models::NewFilamentSpool {
        material: spool.material.clone(),
        manufacturer: spool.manufacturer.clone(),
        color: spool.color.clone(),
        location: spool.location.clone(),
        diameter_mm: spool.diameter_mm,
        original_weight_g: spool.original_weight_g,
        remaining_weight_g: spool.remaining_weight_g,
        price: spool.price,
        image_png,
        color_hex: db::printers::primary_color_hex(&color_hexes, single_hex.as_deref()),
        color_hexes,
        kind: spool.kind.clone(),
    })
}
pub fn add_filament_spool(
    conn: &Connection,
    spool: FilamentSpoolDto,
) -> WriteResult<FilamentSpoolDto> {
    validate_spool_kind(&spool.kind)?;
    validate_color_hex(&spool.color_hex)?;
    validate_color_hexes(&spool.color_hexes)?;
    let new_spool = filament_dto_to_record(&spool)?;
    let id = db::insert_filament_spool(conn, &new_spool).map_err(|e| e.to_string())?;
    // New spools always go to storage, even if the caller sends a slot.
    Ok(FilamentSpoolDto {
        id: id.to_string(),
        color_hex: new_spool.color_hex,
        color_hexes: new_spool.color_hexes,
        home_location: None,
        unit_id: None,
        slot_index: None,
        ..spool
    })
}
pub fn update_filament_spool(
    conn: &Connection,
    spool: FilamentSpoolDto,
) -> WriteResult<FilamentSpoolDto> {
    validate_spool_kind(&spool.kind)?;
    validate_color_hex(&spool.color_hex)?;
    validate_color_hexes(&spool.color_hexes)?;
    let id: i64 = spool
        .id
        .parse()
        .map_err(|_| "invalid spool id".to_string())?;
    let new_spool = filament_dto_to_record(&spool)?;
    db::update_filament_spool(conn, id, &new_spool).map_err(|e| e.to_string())?;
    let updated = db::get_filament_spool(conn, id).map_err(|e| e.to_string())?;
    Ok(spool_record_to_dto(updated))
}
pub fn delete_filament_spool(conn: &Connection, spool_id: String) -> WriteResult<()> {
    let id: i64 = spool_id
        .parse()
        .map_err(|_| "invalid spool id".to_string())?;
    db::delete_filament_spool(conn, id).map_err(|e| e.to_string().into())
}
