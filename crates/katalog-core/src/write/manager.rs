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

/// Limit for "Restock" (1 to 20).
pub const RESTOCK_MAX_COUNT: i64 = 20;

fn missing_template() -> WriteError {
    WriteError::expected("Vorlage nicht gefunden (die Spule wurde inzwischen geloescht)")
}

/// Creates `count` new, full entries modeled on the template inside the caller's
/// transaction (all or nothing). Copies kind, material, manufacturer, color name,
/// color value(s), image and diameter; does NOT copy the slot (`unit_id`/`slot_index`)
/// or home location - new spools/bottles always go to storage. `weight` is grams
/// (filament) or milliliters (resin), rounded to 0.1. Argument validation is the
/// caller's job; only the template lookup is checked here.
pub fn restock_in(
    tx: &Connection,
    template_id: i64,
    count: i64,
    weight: f64,
    price: Option<f64>,
    location: Option<String>,
) -> WriteResult<Vec<FilamentSpoolDto>> {
    let template = match db::get_filament_spool(tx, template_id) {
        Ok(record) => record,
        Err(db::error::DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => {
            return Err(missing_template())
        }
        Err(e) => return Err(e.into()),
    };
    let weight = db::printers::round_tenth(weight);
    let location = location
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());
    let new_spool = db::models::NewFilamentSpool {
        material: template.material,
        manufacturer: template.manufacturer,
        color: template.color,
        location,
        diameter_mm: template.diameter_mm,
        original_weight_g: weight,
        remaining_weight_g: weight,
        price,
        image_png: template.image_png,
        color_hex: template.color_hex,
        color_hexes: template.color_hexes,
        kind: template.kind,
    };
    let mut created = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let id = db::insert_filament_spool(tx, &new_spool)?;
        created.push(spool_record_to_dto(db::get_filament_spool(tx, id)?));
    }
    Ok(created)
}

/// Desktop "Restock": validates, then runs `restock_in` in one transaction.
pub fn restock_filament_spool(
    conn: &mut Connection,
    template_id: &str,
    count: i64,
    weight: f64,
    price: Option<f64>,
    location: Option<String>,
) -> WriteResult<Vec<FilamentSpoolDto>> {
    let template_id: i64 = template_id
        .parse()
        .map_err(|_| "invalid spool id".to_string())?;
    if !(1..=RESTOCK_MAX_COUNT).contains(&count) {
        return Err(WriteError::expected(format!(
            "Anzahl muss zwischen 1 und {RESTOCK_MAX_COUNT} liegen"
        )));
    }
    let weight = db::printers::round_tenth(weight);
    if !weight.is_finite() || weight <= 0.0 {
        return Err(WriteError::expected("Menge muss groesser als 0 sein"));
    }
    if let Some(value) = price {
        if !value.is_finite() || value < 0.0 {
            return Err(WriteError::expected("ungueltiger Preis"));
        }
    }
    let tx = conn.transaction()?;
    let created = restock_in(&tx, template_id, count, weight, price, location)?;
    tx.commit()?;
    Ok(created)
}

/// Subtracts `amount_ml` (rounded to 0.1 ml, > 0) from a resin bottle's remainder,
/// never below 0, inside the caller's transaction. Resin only (filament is tracked
/// via prints/the form). The amount must already be validated.
pub fn consume_resin_in(tx: &Connection, id: i64, amount: f64) -> WriteResult<FilamentSpoolDto> {
    let spool = match db::get_filament_spool(tx, id) {
        Ok(record) => record,
        Err(db::error::DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => {
            return Err(WriteError::expected(
                "Flasche nicht gefunden (wurde sie inzwischen geloescht?)",
            ))
        }
        Err(e) => return Err(e.into()),
    };
    if spool.kind != db::models::SPOOL_KIND_RESIN {
        return Err("Verbrauch abbuchen gibt es nur fuer Resin".to_string().into());
    }
    let remaining = db::printers::round_tenth((spool.remaining_weight_g - amount).max(0.0));
    tx.execute(
        "UPDATE filament_spools SET remaining_weight_g = ?1 WHERE id = ?2",
        rusqlite::params![remaining, id],
    )?;
    Ok(spool_record_to_dto(db::get_filament_spool(tx, id)?))
}

/// Desktop "- Use": validates, then runs `consume_resin_in` in one transaction.
pub fn consume_resin(
    conn: &mut Connection,
    spool_id: &str,
    amount_ml: f64,
) -> WriteResult<FilamentSpoolDto> {
    let id: i64 = spool_id
        .parse()
        .map_err(|_| "invalid spool id".to_string())?;
    let amount = db::printers::round_tenth(amount_ml);
    if !amount.is_finite() || amount <= 0.0 {
        return Err(WriteError::expected("Verbrauch muss groesser als 0 sein"));
    }
    let tx = conn.transaction()?;
    let updated = consume_resin_in(&tx, id, amount)?;
    tx.commit()?;
    Ok(updated)
}
