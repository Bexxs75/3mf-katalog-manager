use super::{WriteError, WriteResult};
use crate::db;
use crate::db::printers as p;
use crate::read::manager::{list_printers_with_conn, unit_dto, MaterialUnitDto, PrinterDto};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrinterDetails {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub nozzle_mm: Option<f64>,
    pub bed_x_mm: Option<f64>,
    pub bed_y_mm: Option<f64>,
    pub bed_z_mm: Option<f64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadResultDto {
    pub displaced_spool_id: Option<String>,
}
#[track_caller]
pub fn in_tx<T>(
    conn: &mut Connection,
    f: impl FnOnce(&Connection) -> Result<T, db::error::DbError>,
) -> WriteResult<T> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    // No `.map_err(|e| e.to_string())` here: going through `From<DbError>` directly
    // keeps `DbError::Invalid` (e.g. a rejected printer/unit name) marked `expected`.
    let value = f(&tx)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(value)
}
pub fn parse_id(value: &str, what: &str) -> WriteResult<i64> {
    value
        .parse()
        .map_err(|_| format!("ungueltige {what}-ID").into())
}
pub fn add_printer_with_conn(
    conn: &mut Connection,
    name: &str,
    holder_name: &str,
    kind: &str,
) -> WriteResult<PrinterDto> {
    // Every filament printer gets a spool holder (1 slot) right away, so printers
    // without an AMS can take a spool immediately; a resin printer gets its only
    // unit, the resin vat, instead. Both in one transaction: if the unit fails, no
    // printer is created either.
    let id = in_tx(conn, |tx| {
        let id = p::insert_printer_of_kind(tx, name, kind)?;
        if kind == p::PRINTER_KIND_RESIN {
            p::insert_resin_vat(tx, id, holder_name)?;
        } else {
            p::insert_unit(tx, id, "external", holder_name, None)?;
        }
        Ok(id)
    })?;
    list_printers_with_conn(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|printer| printer.id == id.to_string())
        .ok_or_else(|| "Drucker nach dem Anlegen nicht gefunden".to_string().into())
}
pub fn validate_printer_details(details: &PrinterDetails) -> WriteResult<()> {
    for (value, label) in [
        (&details.manufacturer, "Hersteller"),
        (&details.model, "Modellname"),
    ] {
        if let Some(value) = value {
            db::error::validate_text_length(value, 120, label)
                .map_err(|e| WriteError::expected(e.to_string()))?;
        }
    }
    for (value, min, max, label) in [
        (details.nozzle_mm, 0.1, 2.0, "Düse"),
        (details.bed_x_mm, 1.0, 2000.0, "Bauraum X"),
        (details.bed_y_mm, 1.0, 2000.0, "Bauraum Y"),
        (details.bed_z_mm, 1.0, 2000.0, "Bauraum Z"),
    ] {
        if value.is_some_and(|v| !v.is_finite() || v < min || v > max) {
            return Err(WriteError::expected(format!(
                "{label} muss zwischen {min} und {max} mm liegen"
            )));
        }
    }
    Ok(())
}
pub fn update_details(
    tx: &Connection,
    id: i64,
    details: PrinterDetails,
) -> Result<(), db::error::DbError> {
    let clean = |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let changed = tx.execute(
            "UPDATE printers SET manufacturer=?1, model=?2, nozzle_mm=?3, bed_x_mm=?4, bed_y_mm=?5, bed_z_mm=?6 WHERE id=?7",
            rusqlite::params![clean(details.manufacturer), clean(details.model), details.nozzle_mm, details.bed_x_mm, details.bed_y_mm, details.bed_z_mm, id],
        )?;
    if changed == 0 {
        return Err(db::error::DbError::Invalid("Drucker nicht gefunden".into()));
    }
    Ok(())
}
pub fn update_printer_details_with_conn(
    conn: &mut Connection,
    id: i64,
    details: PrinterDetails,
) -> WriteResult<()> {
    validate_printer_details(&details)?;
    in_tx(conn, |tx| update_details(tx, id, details))
}
pub fn load_spool_with_conn(
    conn: &mut Connection,
    spool_id: &str,
    unit_id: &str,
    slot_index: i64,
) -> WriteResult<LoadResultDto> {
    let spool = parse_id(spool_id, "Spulen")?;
    let unit = parse_id(unit_id, "Einheit")?;
    let outcome = in_tx(conn, |tx| p::load_spool(tx, spool, unit, slot_index))?;
    Ok(LoadResultDto {
        displaced_spool_id: outcome.displaced_spool_id.map(|id| id.to_string()),
    })
}
pub fn unload_spool_with_conn(
    conn: &mut Connection,
    spool_id: &str,
    location: Option<String>,
) -> WriteResult<Option<String>> {
    let spool = parse_id(spool_id, "Spulen")?;
    in_tx(conn, |tx| p::unload_spool(tx, spool, location.as_deref()))
}
pub fn rename_printer(conn: &mut Connection, printer_id: String, name: String) -> WriteResult<()> {
    let id = parse_id(&printer_id, "Drucker")?;
    in_tx(conn, |tx| p::rename_printer(tx, id, &name))
}
pub fn reorder_printers(conn: &mut Connection, ids: Vec<String>) -> WriteResult<()> {
    let ids = ids
        .iter()
        .map(|id| parse_id(id, "Drucker"))
        .collect::<WriteResult<Vec<_>>>()?;
    in_tx(conn, |tx| p::reorder_printers(tx, &ids))
}
pub fn delete_printer(conn: &mut Connection, printer_id: String) -> WriteResult<usize> {
    let id = parse_id(&printer_id, "Drucker")?;
    in_tx(conn, |tx| p::delete_printer(tx, id))
}
pub fn add_unit(
    conn: &mut Connection,
    printer_id: String,
    kind: String,
    name: String,
    slot_count: Option<i64>,
) -> WriteResult<MaterialUnitDto> {
    let printer = parse_id(&printer_id, "Drucker")?;
    let id = in_tx(conn, |tx| {
        p::insert_unit(tx, printer, &kind, &name, slot_count)
    })?;
    p::list_units(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|u| u.id == id)
        .map(unit_dto)
        .ok_or_else(|| "Einheit nach dem Anlegen nicht gefunden".to_string().into())
}
pub fn update_unit(
    conn: &mut Connection,
    unit_id: String,
    name: String,
    slot_count: Option<i64>,
) -> WriteResult<usize> {
    let id = parse_id(&unit_id, "Einheit")?;
    in_tx(conn, |tx| p::update_unit(tx, id, &name, slot_count))
}
pub fn delete_unit(conn: &mut Connection, unit_id: String) -> WriteResult<usize> {
    let id = parse_id(&unit_id, "Einheit")?;
    in_tx(conn, |tx| p::delete_unit(tx, id))
}
pub fn reorder_units(
    conn: &mut Connection,
    printer_id: String,
    unit_ids: Vec<String>,
) -> WriteResult<()> {
    let printer = parse_id(&printer_id, "Drucker")?;
    let ids = unit_ids
        .iter()
        .map(|id| parse_id(id, "Einheit"))
        .collect::<WriteResult<Vec<_>>>()?;
    in_tx(conn, |tx| p::reorder_units(tx, printer, &ids))
}
