use super::{validate_image_format, WriteError, WriteResult, MAX_CUSTOM_IMAGE_BYTES};
use crate::db;
use crate::error_code::ErrorCode;
use crate::read::manager::{spool_record_to_dto, FilamentSpoolDto};
use rusqlite::Connection;
pub fn validate_color_hex(color_hex: &Option<String>) -> WriteResult<()> {
    match color_hex {
        Some(value) if !db::printers::is_valid_color_hex(value) => {
            Err(WriteError::coded(ErrorCode::InvalidColor))
        }
        _ => Ok(()),
    }
}
pub fn validate_color_hexes(color_hexes: &[String]) -> WriteResult<()> {
    if color_hexes.len() > db::printers::MAX_SPOOL_COLORS {
        return Err(WriteError::coded_with(
            ErrorCode::SpoolColorsTooMany,
            [db::printers::MAX_SPOOL_COLORS],
        ));
    }
    if color_hexes.iter().any(|c| !db::printers::is_valid_color_hex(c)) {
        return Err(WriteError::coded(ErrorCode::InvalidColor));
    }
    Ok(())
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
    validate_print_temperatures(spool)?;
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
        nozzle_temp_min: spool.nozzle_temp_min,
        nozzle_temp_max: spool.nozzle_temp_max,
        bed_temp_min: spool.bed_temp_min,
        bed_temp_max: spool.bed_temp_max,
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
    WriteError::coded(ErrorCode::SpoolTemplateNotFound)
}

/// Creates `count` new, full entries modeled on the template inside the caller's
/// transaction (all or nothing). Copies kind, material, manufacturer, color name,
/// color value(s), image, diameter and print temperatures; does NOT copy the slot (`unit_id`/`slot_index`)
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
        nozzle_temp_min: template.nozzle_temp_min,
        nozzle_temp_max: template.nozzle_temp_max,
        bed_temp_min: template.bed_temp_min,
        bed_temp_max: template.bed_temp_max,
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
        return Err(WriteError::coded_with(
            ErrorCode::RestockCountOutOfRange,
            [1, RESTOCK_MAX_COUNT],
        ));
    }
    let weight = db::printers::round_tenth(weight);
    if !weight.is_finite() || weight <= 0.0 {
        return Err(WriteError::coded(ErrorCode::AmountNotPositive));
    }
    if let Some(value) = price {
        if !value.is_finite() || value < 0.0 {
            return Err(WriteError::coded(ErrorCode::InvalidPrice));
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
            return Err(WriteError::coded(ErrorCode::BottleNotFound))
        }
        Err(e) => return Err(e.into()),
    };
    if spool.kind != db::models::SPOOL_KIND_RESIN {
        return Err(WriteError::coded(ErrorCode::ConsumeOnlyResin));
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
        return Err(WriteError::coded(ErrorCode::ConsumeNotPositive));
    }
    let tx = conn.transaction()?;
    let updated = consume_resin_in(&tx, id, amount)?;
    tx.commit()?;
    Ok(updated)
}

pub fn validate_print_temperatures(spool: &FilamentSpoolDto) -> WriteResult<()> {
    validate_temperature_ranges(spool.nozzle_temp_min, spool.nozzle_temp_max, spool.bed_temp_min, spool.bed_temp_max)
}

fn validate_temperature_ranges(nozzle_min: Option<i64>, nozzle_max: Option<i64>, bed_min: Option<i64>, bed_max: Option<i64>) -> WriteResult<()> {
    for (min, max) in [(nozzle_min, nozzle_max), (bed_min, bed_max)] {
        if min.is_some_and(|v| !(0..=500).contains(&v))
            || max.is_some_and(|v| !(0..=500).contains(&v))
            || matches!((min, max), (Some(a), Some(b)) if a > b)
        {
            return Err(WriteError::coded(ErrorCode::InvalidPrintTemperature));
        }
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpoolPrintTemperatures {
    pub ids: Vec<i64>,
    pub nozzle_temp_min: Option<i64>,
    pub nozzle_temp_max: Option<i64>,
    pub bed_temp_min: Option<i64>,
    pub bed_temp_max: Option<i64>,
}

pub fn set_spool_print_temperatures(conn: &Connection, input: SpoolPrintTemperatures) -> WriteResult<u64> {
    use rusqlite::{params, OptionalExtension};
    validate_temperature_ranges(input.nozzle_temp_min, input.nozzle_temp_max, input.bed_temp_min, input.bed_temp_max)?;
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let mut changes = 0;
    // A repeated id would be counted as a second change and update the row twice.
    let mut seen = std::collections::HashSet::new();
    for id in input.ids.into_iter().filter(|id| seen.insert(*id)) {
        let kind: Option<String> = tx.query_row("SELECT kind FROM filament_spools WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        match kind.as_deref() {
            None => return Err(WriteError::coded_not_found(ErrorCode::SpoolNotFound)),
            Some("filament") => {},
            Some(_) => return Err(WriteError::coded(ErrorCode::InvalidMaterialKind)),
        }
        changes += tx.execute("UPDATE filament_spools SET nozzle_temp_min = ?1, nozzle_temp_max = ?2, bed_temp_min = ?3, bed_temp_max = ?4 WHERE id = ?5",
            params![input.nozzle_temp_min, input.nozzle_temp_max, input.bed_temp_min, input.bed_temp_max, id])? as u64;
    }
    tx.commit()?;
    Ok(changes)
}

#[cfg(test)]
mod print_temperature_tests {
    use super::*;

    fn spool() -> FilamentSpoolDto {
        serde_json::from_value(serde_json::json!({
            "id": "", "material": "PLA", "diameterMm": 1.75,
            "originalWeightG": 1000.0, "remainingWeightG": 800.0,
            "nozzleTempMin": 200, "nozzleTempMax": 240,
            "bedTempMin": 50, "bedTempMax": 65
        }))
        .unwrap()
    }

    #[test]
    fn group_temperatures_only_change_four_columns_and_roll_back_on_rejection() {
        let conn = db::connect_in_memory().unwrap();
        let first = add_filament_spool(&conn, spool()).unwrap();
        let second = add_filament_spool(&conn, spool()).unwrap();
        let mut resin = spool();
        resin.kind = "resin".into();
        let resin = add_filament_spool(&conn, resin).unwrap();
        let a: i64 = first.id.parse().unwrap();
        let b: i64 = second.id.parse().unwrap();
        conn.execute("UPDATE filament_spools SET image_png = X'010203', location = 'Shelf', remaining_weight_g = 123 WHERE id = ?1", [b]).unwrap();
        let before = db::get_filament_spool(&conn, b).unwrap();
        let input = |ids| SpoolPrintTemperatures { ids, nozzle_temp_min: None, nozzle_temp_max: Some(230), bed_temp_min: None, bed_temp_max: Some(60) };
        for (bad_id, expected) in [(999999, ErrorCode::SpoolNotFound), (resin.id.parse().unwrap(), ErrorCode::InvalidMaterialKind)] {
            let error = set_spool_print_temperatures(&conn, input(vec![a, bad_id])).unwrap_err();
            assert!(matches!(error, WriteError::Coded { code, .. } if code == expected));
            assert_eq!(db::get_filament_spool(&conn, a).unwrap().nozzle_temp_min, Some(200));
        }
        let mut invalid = input(vec![a, b]);
        invalid.nozzle_temp_min = Some(501);
        assert!(matches!(set_spool_print_temperatures(&conn, invalid), Err(WriteError::Coded { code: ErrorCode::InvalidPrintTemperature, .. })));
        assert_eq!(set_spool_print_temperatures(&conn, input(vec![a, b, a, b, a])).unwrap(), 2);
        let after = db::get_filament_spool(&conn, b).unwrap();
        assert_eq!(after.image_png, before.image_png);
        // Compare the full detail DTO, including the image omitted from web list DTOs.
        let mut expected = serde_json::to_value(spool_record_to_dto(before)).unwrap();
        for (key, value) in [("nozzleTempMin", serde_json::Value::Null), ("nozzleTempMax", 230.into()), ("bedTempMin", serde_json::Value::Null), ("bedTempMax", 60.into())] { expected[key] = value; }
        assert_eq!(serde_json::to_value(spool_record_to_dto(after)).unwrap(), expected);
        assert_eq!(db::get_filament_spool(&conn, a).unwrap().nozzle_temp_min, None);
    }

    #[test]
    fn print_temperatures_round_trip_update_and_restock() {
        let mut conn = db::connect_in_memory().unwrap();
        let added = add_filament_spool(&conn, spool()).unwrap();
        let read = db::get_filament_spool(&conn, added.id.parse().unwrap()).unwrap();
        assert_eq!(
            (
                read.nozzle_temp_min,
                read.nozzle_temp_max,
                read.bed_temp_min,
                read.bed_temp_max
            ),
            (Some(200), Some(240), Some(50), Some(65))
        );
        let mut updated = spool_record_to_dto(read);
        updated.nozzle_temp_min = None;
        updated.nozzle_temp_max = Some(215);
        updated.bed_temp_min = Some(0);
        updated.bed_temp_max = Some(500);
        update_filament_spool(&conn, updated).unwrap();
        let copies = restock_filament_spool(&mut conn, &added.id, 1, 1000.0, None, None).unwrap();
        let copy = &copies[0];
        assert_eq!(
            (
                copy.nozzle_temp_min,
                copy.nozzle_temp_max,
                copy.bed_temp_min,
                copy.bed_temp_max
            ),
            (None, Some(215), Some(0), Some(500))
        );
        let listed = db::list_filament_spools(&conn).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed
            .iter()
            .all(|s| s.nozzle_temp_max == Some(215) && s.bed_temp_max == Some(500)));
    }

    #[test]
    fn print_temperatures_reject_invalid_values_before_writing() {
        let conn = db::connect_in_memory().unwrap();
        let added = add_filament_spool(&conn, spool()).unwrap();
        for (min, max) in [(Some(501), None), (Some(-1), None), (Some(240), Some(200))] {
            for bed in [false, true] {
                for update in [false, true] {
                    let mut input = spool();
                    input.id = added.id.clone();
                    if bed {
                        input.bed_temp_min = min;
                        input.bed_temp_max = max;
                    } else {
                        input.nozzle_temp_min = min;
                        input.nozzle_temp_max = max;
                    }
                    let result = if update {
                        update_filament_spool(&conn, input)
                    } else {
                        add_filament_spool(&conn, input)
                    };
                    assert!(matches!(
                        result,
                        Err(WriteError::Coded {
                            code: ErrorCode::InvalidPrintTemperature,
                            ..
                        })
                    ));
                }
            }
        }
        assert_eq!(db::list_filament_spools(&conn).unwrap().len(), 1);
        assert_eq!(
            db::get_filament_spool(&conn, added.id.parse().unwrap())
                .unwrap()
                .nozzle_temp_min,
            Some(200)
        );
    }
}
