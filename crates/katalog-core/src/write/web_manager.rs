//! Strict web admission around the same transaction-free database operations
//! used by the desktop adapters. No nested transactions or stringified SQL errors.
use super::{
    manager as m, printers as desktop,
    web::{exists, invalid, outcome, transaction, Outcome},
    WriteError, WriteResult,
};
use crate::{db, read::manager as read};
use db::printers as p;
use rusqlite::Connection;

pub const MAX_SPOOLS: i64 = 5000;
pub const MAX_PRINTERS: i64 = 200;
pub const MAX_UNITS: i64 = 50;
/// Multiplication by ten in round_tenth must remain representable.
pub const MAX_WEIGHT: f64 = f64::MAX / 10.0;

// In these DB-only manager helpers Other denotes a business rejection. SQL
// errors retain their typed Busy/Locked classification through WriteError.
fn business(e: db::error::DbError) -> WriteError {
    match e {
        db::error::DbError::Other(message) => WriteError::expected(message),
        e => e.into(),
    }
}
fn text(value: &str) -> WriteResult<()> {
    if value.encode_utf16().count() > 120 {
        return Err(invalid("Text darf höchstens 120 UTF-16-Einheiten haben"));
    }
    Ok(())
}
fn optional_text(value: &Option<String>) -> WriteResult<()> {
    if let Some(s) = value {
        text(s)?;
    }
    Ok(())
}
fn quota(c: &Connection, sql: &str, args: &[&dyn rusqlite::ToSql], max: i64) -> WriteResult<()> {
    let n: i64 = c.query_row(sql, args, |r| r.get(0))?;
    if n >= max {
        return Err(invalid("Web-Obergrenze erreicht"));
    }
    Ok(())
}
fn prepare(s: &read::FilamentSpoolDto) -> WriteResult<db::models::NewFilamentSpool> {
    if !db::models::SPOOL_KINDS.contains(&s.kind.as_str()) {
        return Err(invalid("Ungültige Materialart"));
    }
    if s.material.trim().is_empty() {
        return Err(invalid("Material darf nicht leer sein"));
    }
    text(&s.material)?;
    for v in [&s.manufacturer, &s.color, &s.location] {
        optional_text(v)?;
    }
    if s.color_hex
        .as_ref()
        .is_some_and(|s| !p::is_valid_color_hex(s))
    {
        return Err(invalid("Ungültiger Farbwert"));
    }
    if !s.diameter_mm.is_finite()
        || (s.kind == "filament" && !(s.diameter_mm > 0.0 && s.diameter_mm <= 5.0))
    {
        return Err(invalid("Ungültiger Durchmesser"));
    }
    if !s.original_weight_g.is_finite()
        || s.original_weight_g <= 0.0
        || s.original_weight_g > MAX_WEIGHT
        || p::round_tenth(s.original_weight_g) <= 0.0
        || !s.remaining_weight_g.is_finite()
        || s.remaining_weight_g < 0.0
        || s.remaining_weight_g > s.original_weight_g
    {
        return Err(invalid("Ungültige Menge"));
    }
    if s.price.is_some_and(|p| !p.is_finite() || p < 0.0) {
        return Err(invalid("Ungültiger Preis"));
    }
    m::filament_dto_to_record(s)
}
pub fn add_filament_spool(
    c: &mut Connection,
    s: read::FilamentSpoolDto,
) -> WriteResult<Outcome<read::FilamentSpoolDto>> {
    let record = prepare(&s)?;
    transaction(c, |c| {
        quota(c, "SELECT count(*) FROM filament_spools", &[], MAX_SPOOLS)?;
        let id = db::insert_filament_spool(c, &record)?;
        let changes = c.changes();
        Ok(outcome(
            read::spool_record_to_dto(db::get_filament_spool(c, id)?),
            changes,
        ))
    })
}
pub fn update_filament_spool(
    c: &mut Connection,
    id: i64,
    s: read::FilamentSpoolDto,
    replace_image: bool,
) -> WriteResult<Outcome<read::FilamentSpoolDto>> {
    let record = prepare(&s)?;
    transaction(c, |c| {
        exists(c, "filament_spools", id)?;
        db::update_filament_spool_image_policy(c, id, &record, replace_image).map_err(business)?;
        let changes = c.changes();
        Ok(outcome(
            read::spool_record_to_dto(db::get_filament_spool(c, id)?),
            changes,
        ))
    })
}
pub fn delete_filament_spool(c: &mut Connection, id: i64) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        exists(c, "filament_spools", id)?;
        db::delete_filament_spool(c, id)?;
        Ok(outcome((), c.changes()))
    })
}
pub fn add_printer(
    c: &mut Connection,
    name: &str,
    holder: &str,
    kind: &str,
) -> WriteResult<Outcome<read::PrinterDto>> {
    transaction(c, |c| {
        quota(c, "SELECT count(*) FROM printers", &[], MAX_PRINTERS)?;
        let id = p::insert_printer_of_kind(c, name, kind).map_err(business)?;
        let changes = c.changes();
        quota(
            c,
            "SELECT count(*) FROM material_units WHERE printer_id=?1",
            &[&id],
            MAX_UNITS,
        )?;
        if kind == p::PRINTER_KIND_RESIN {
            p::insert_resin_vat(c, id, holder).map_err(business)?;
        } else {
            p::insert_unit(c, id, "external", holder, None).map_err(business)?;
        }
        let printer = read::list_printers_with_conn(c)
            .map_err(read_error)?
            .into_iter()
            .find(|p| p.id == id.to_string())
            .expect("inserted printer");
        Ok(outcome(printer, changes))
    })
}
fn read_error(e: crate::read::ReadError) -> WriteError {
    if e.busy {
        WriteError::Busy { message: e.message }
    } else {
        WriteError::from(e.message)
    }
}
pub fn rename_printer(c: &mut Connection, id: i64, name: &str) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        exists(c, "printers", id)?;
        p::rename_printer(c, id, name).map_err(business)?;
        Ok(outcome((), c.changes()))
    })
}
pub fn update_printer_details(
    c: &mut Connection,
    id: i64,
    details: desktop::PrinterDetails,
) -> WriteResult<Outcome<()>> {
    desktop::validate_printer_details(&details)?;
    transaction(c, |c| {
        exists(c, "printers", id)?;
        desktop::update_details(c, id, details).map_err(business)?;
        Ok(outcome((), c.changes()))
    })
}
pub fn reorder_printers(c: &mut Connection, ids: &[i64]) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        p::reorder_printers(c, ids).map_err(business)?;
        Ok(outcome((), ids.len() as u64))
    })
}
pub fn delete_printer(c: &mut Connection, id: i64) -> WriteResult<Outcome<usize>> {
    transaction(c, |c| {
        exists(c, "printers", id)?;
        let n = p::delete_printer(c, id).map_err(business)?;
        Ok(outcome(n, c.changes()))
    })
}
pub fn add_unit(
    c: &mut Connection,
    printer: i64,
    kind: &str,
    name: &str,
    slots: Option<i64>,
) -> WriteResult<Outcome<read::MaterialUnitDto>> {
    transaction(c, |c| {
        exists(c, "printers", printer)?;
        quota(
            c,
            "SELECT count(*) FROM material_units WHERE printer_id=?1",
            &[&printer],
            MAX_UNITS,
        )?;
        let id = p::insert_unit(c, printer, kind, name, slots).map_err(business)?;
        let changes = c.changes();
        let unit = p::list_units(c)?
            .into_iter()
            .find(|u| u.id == id)
            .expect("inserted unit");
        Ok(outcome(read::unit_dto(unit), changes))
    })
}
pub fn update_unit(
    c: &mut Connection,
    id: i64,
    name: &str,
    slots: Option<i64>,
) -> WriteResult<Outcome<usize>> {
    transaction(c, |c| {
        exists(c, "material_units", id)?;
        let n = p::update_unit(c, id, name, slots).map_err(business)?;
        Ok(outcome(n, c.changes()))
    })
}
pub fn delete_unit(c: &mut Connection, id: i64) -> WriteResult<Outcome<usize>> {
    transaction(c, |c| {
        exists(c, "material_units", id)?;
        let n = p::delete_unit(c, id).map_err(business)?;
        Ok(outcome(n, c.changes()))
    })
}
pub fn reorder_units(c: &mut Connection, printer: i64, ids: &[i64]) -> WriteResult<Outcome<()>> {
    transaction(c, |c| {
        exists(c, "printers", printer)?;
        p::reorder_units(c, printer, ids).map_err(business)?;
        Ok(outcome((), ids.len() as u64))
    })
}
pub fn load_spool(
    c: &mut Connection,
    spool: i64,
    unit: i64,
    slot: i64,
) -> WriteResult<Outcome<desktop::LoadResultDto>> {
    transaction(c, |c| {
        exists(c, "filament_spools", spool)?;
        exists(c, "material_units", unit)?;
        // Prevent overflow in the desktop helper's human-readable slot number.
        if !(0..16).contains(&slot) {
            return Err(invalid("Ungültiger Slot"));
        }
        let same: bool = c.query_row(
            "SELECT unit_id IS ?1 AND slot_index IS ?2 FROM filament_spools WHERE id=?3",
            rusqlite::params![unit, slot, spool],
            |r| r.get(0),
        )?;
        let loaded = p::load_spool(c, spool, unit, slot).map_err(business)?;
        let changes = if same { 0 } else { c.changes() };
        Ok(outcome(
            desktop::LoadResultDto {
                displaced_spool_id: loaded.displaced_spool_id.map(|id| id.to_string()),
            },
            changes,
        ))
    })
}
pub fn unload_spool(
    c: &mut Connection,
    spool: i64,
    location: Option<String>,
) -> WriteResult<Outcome<Option<String>>> {
    optional_text(&location)?;
    transaction(c, |c| {
        exists(c, "filament_spools", spool)?;
        let location = p::unload_spool(c, spool, location.as_deref()).map_err(business)?;
        Ok(outcome(location, c.changes()))
    })
}
