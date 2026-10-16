//! Tauri commands for printers, their multi-material units and loading/unloading
//! spools. The logic lives in `db::printers`; here only ID conversion,
//! transactions and DTOs. Every changing operation runs in exactly one
//! transaction, so a spool never gets stuck "between two slots".

use super::*;
use db::printers as p;

pub use katalog_core::read::manager::MaterialUnitDto;

pub use katalog_core::read::manager::PrinterDto;

pub use katalog_core::write::printers::LoadResultDto;

fn parse_id(value: &str, what: &str) -> CmdResult<i64> {
    value
        .parse()
        .map_err(|_| format!("ungueltige {what}-ID").into())
}

pub(crate) fn list_printers_with_conn(conn: &Connection) -> CmdResult<Vec<PrinterDto>> {
    katalog_core::read::manager::list_printers_with_conn(conn).map_err(Into::into)
}

#[tauri::command]
pub fn list_printers(state: State<AppState>) -> CmdResult<Vec<PrinterDto>> {
    let conn = lock_db(&state)?;
    list_printers_with_conn(&conn)
}

/// `holder_name` is the (translated) name of the first unit: the spool holder
/// for filament printers, the resin vat for resin printers.
pub(crate) fn add_printer_with_conn(
    conn: &mut Connection,
    name: &str,
    holder_name: &str,
    kind: &str,
) -> CmdResult<PrinterDto> {
    katalog_core::write::printers::add_printer_with_conn(conn, name, holder_name, kind)
        .map_err(Into::into)
}

#[tauri::command]
pub fn add_printer(
    state: State<AppState>,
    name: String,
    holder_name: String,
    kind: Option<String>,
) -> CmdResult<PrinterDto> {
    let mut conn = lock_db(&state)?;
    // If `kind` is missing (older callers), filament applies.
    let kind = kind.unwrap_or_else(|| p::PRINTER_KIND_FILAMENT.to_string());
    add_printer_with_conn(&mut conn, &name, &holder_name, &kind)
}

#[tauri::command]
pub fn rename_printer(state: State<AppState>, printer_id: String, name: String) -> CmdResult<()> {
    let mut conn = lock_db(&state)?;
    katalog_core::write::printers::rename_printer(&mut conn, printer_id, name).map_err(Into::into)
}

pub use katalog_core::write::printers::PrinterDetails;

fn update_printer_details_with_conn(
    conn: &mut Connection,
    id: i64,
    details: PrinterDetails,
) -> CmdResult<()> {
    katalog_core::write::printers::update_printer_details_with_conn(conn, id, details)
        .map_err(Into::into)
}

#[tauri::command]
pub fn update_printer_details(
    state: State<AppState>,
    id: String,
    details: PrinterDetails,
) -> CmdResult<()> {
    let id = parse_id(&id, "Drucker")?;
    let mut conn = lock_db(&state)?;
    update_printer_details_with_conn(&mut conn, id, details)
}

#[tauri::command]
pub fn reorder_printers(state: State<AppState>, ids: Vec<String>) -> CmdResult<()> {
    let mut conn = lock_db(&state)?;
    katalog_core::write::printers::reorder_printers(&mut conn, ids).map_err(Into::into)
}

/// Returns the number of spools that went back to their home location.
#[tauri::command]
pub fn delete_printer(state: State<AppState>, printer_id: String) -> CmdResult<usize> {
    let mut conn = lock_db(&state)?;
    katalog_core::write::printers::delete_printer(&mut conn, printer_id).map_err(Into::into)
}

#[tauri::command]
pub fn add_unit(
    state: State<AppState>,
    printer_id: String,
    kind: String,
    name: String,
    slot_count: Option<i64>,
) -> CmdResult<MaterialUnitDto> {
    let mut conn = lock_db(&state)?;
    katalog_core::write::printers::add_unit(&mut conn, printer_id, kind, name, slot_count)
        .map_err(Into::into)
}

#[tauri::command]
pub fn update_unit(
    state: State<AppState>,
    unit_id: String,
    name: String,
    slot_count: Option<i64>,
) -> CmdResult<usize> {
    let mut conn = lock_db(&state)?;
    katalog_core::write::printers::update_unit(&mut conn, unit_id, name, slot_count)
        .map_err(Into::into)
}

#[tauri::command]
pub fn delete_unit(state: State<AppState>, unit_id: String) -> CmdResult<usize> {
    let mut conn = lock_db(&state)?;
    katalog_core::write::printers::delete_unit(&mut conn, unit_id).map_err(Into::into)
}

#[tauri::command]
pub fn reorder_units(
    state: State<AppState>,
    printer_id: String,
    unit_ids: Vec<String>,
) -> CmdResult<()> {
    let mut conn = lock_db(&state)?;
    katalog_core::write::printers::reorder_units(&mut conn, printer_id, unit_ids)
        .map_err(Into::into)
}

pub(crate) fn load_spool_with_conn(
    conn: &mut Connection,
    spool_id: &str,
    unit_id: &str,
    slot_index: i64,
) -> CmdResult<LoadResultDto> {
    katalog_core::write::printers::load_spool_with_conn(conn, spool_id, unit_id, slot_index)
        .map_err(Into::into)
}

#[tauri::command]
pub fn load_spool(
    state: State<AppState>,
    spool_id: String,
    unit_id: String,
    slot_index: i64,
) -> CmdResult<LoadResultDto> {
    let mut conn = lock_db(&state)?;
    load_spool_with_conn(&mut conn, &spool_id, &unit_id, slot_index)
}

pub(crate) fn unload_spool_with_conn(
    conn: &mut Connection,
    spool_id: &str,
    location: Option<String>,
) -> CmdResult<Option<String>> {
    katalog_core::write::printers::unload_spool_with_conn(conn, spool_id, location)
        .map_err(Into::into)
}

/// Returns the new location (for the "back to ..." notice).
#[tauri::command]
pub fn unload_spool(
    state: State<AppState>,
    spool_id: String,
    location: Option<String>,
) -> CmdResult<Option<String>> {
    let mut conn = lock_db(&state)?;
    unload_spool_with_conn(&mut conn, &spool_id, location)
}

pub use katalog_core::read::manager::PrinterHistoryJobDto;

fn list_printer_history_with_conn(
    conn: &Connection,
    printer_id: i64,
) -> CmdResult<Vec<PrinterHistoryJobDto>> {
    katalog_core::read::manager::list_printer_history_with_conn(conn, printer_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_printer_history(
    state: State<AppState>,
    printer_id: String,
) -> CmdResult<Vec<PrinterHistoryJobDto>> {
    let conn = lock_db(&state)?;
    list_printer_history_with_conn(&conn, parse_id(&printer_id, "Drucker")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printer_details_reject_long_text_before_writing() {
        let mut conn = db::connect_in_memory().unwrap();
        let printer = add_printer_with_conn(&mut conn, "Test", "Holder", "filament").unwrap();
        let id = printer.id.parse().unwrap();
        update_printer_details_with_conn(&mut conn, id, PrinterDetails {
            manufacturer: Some("a".repeat(120)), model: Some("😀".repeat(60)), ..Default::default()
        }).unwrap();
        for details in [
            PrinterDetails { manufacturer: Some("a".repeat(121)), ..Default::default() },
            PrinterDetails { model: Some("😀".repeat(61)), ..Default::default() },
        ] {
            let error = update_printer_details_with_conn(&mut conn, id, details).unwrap_err();
            assert!(error.expected);
            assert!(error.message.contains("120"));
        }
        let saved = list_printers_with_conn(&conn).unwrap().remove(0);
        assert_eq!(saved.manufacturer, Some("a".repeat(120)));
        assert_eq!(saved.model, Some("😀".repeat(60)));
    }

    #[test]
    fn last_printer_only_uses_confirmed_jobs_for_the_requested_file() {
        let mut conn = db::connect_in_memory().unwrap();
        let a: i64 = add_printer_with_conn(&mut conn, "A", "Holder", "filament").unwrap().id.parse().unwrap();
        let b: i64 = add_printer_with_conn(&mut conn, "B", "Holder", "filament").unwrap().id.parse().unwrap();
        conn.execute("INSERT INTO files (id, name, path, file_type, file_size_bytes, imported_at) VALUES (1, 'one', '/one.3mf', '3mf', 1, ''), (2, 'two', '/two.3mf', '3mf', 1, '')", []).unwrap();
        assert!(get_last_printer_for_file_with_conn(&conn, 1).unwrap().is_none());
        let insert = |printer, remote, state, time| {
            conn.execute("INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm, state, booked_file_id) VALUES (?1, ?2, 'one', 'completed', 'completed', ?4, 100, 100, ?3, 1)", rusqlite::params![printer, remote, state, time]).unwrap();
        };
        insert(a, "open", "open", 900.0);
        assert!(get_last_printer_for_file_with_conn(&conn, 1).unwrap().is_none());
        insert(a, "first", "confirmed", 100.0);
        let first = get_last_printer_for_file_with_conn(&conn, 1).unwrap().unwrap();
        assert_eq!(first.printer_name, "A");
        assert_eq!(first.ended_at, 100.0);
        insert(a, "newest", "confirmed", 300.0);
        insert(b, "older", "confirmed", 200.0);
        assert_eq!(get_last_printer_for_file_with_conn(&conn, 1).unwrap().unwrap().ended_at, 300.0);
        insert(b, "other-printer", "confirmed", 400.0);
        let newest = get_last_printer_for_file_with_conn(&conn, 1).unwrap().unwrap();
        assert_eq!(newest.printer_name, "B");
        assert_eq!(newest.ended_at, 400.0);
        assert!(get_last_printer_for_file_with_conn(&conn, 2).unwrap().is_none());
        assert!(get_last_printer_for_file_with_conn(&conn, 999).unwrap().is_none());
        let json = serde_json::to_value(newest).unwrap();
        assert_eq!(json["printerName"], "B");
        assert_eq!(json["endedAt"], 400.0);
    }


    #[test]
    fn history_contains_all_states_but_only_the_requested_printer() {
        let mut conn = db::connect_in_memory().unwrap();
        let a: i64 = add_printer_with_conn(&mut conn, "A", "Holder", "filament").unwrap().id.parse().unwrap();
        let b: i64 = add_printer_with_conn(&mut conn, "B", "Holder", "filament").unwrap().id.parse().unwrap();
        for (printer, remote, state, grams) in [(a, "1", "open", None), (a, "2", "confirmed", Some(18.4)), (a, "3", "ignored", None), (b, "4", "open", None)] {
            conn.execute("INSERT INTO printer_jobs (printer_id, remote_id, file_name, outcome, raw_status, ended_at, print_duration_s, used_mm, state, booked_g) VALUES (?1, ?2, 'test.gcode', 'completed', 'completed', ?2, 120, 100, ?3, ?4)", rusqlite::params![printer, remote, state, grams]).unwrap();
        }
        let jobs = list_printer_history_with_conn(&conn, a).unwrap();
        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs.iter().map(|j| j.state.as_str()).collect::<Vec<_>>(), vec!["ignored", "confirmed", "open"]);
        assert_eq!(jobs[1].grams, Some(18.4));
        assert_eq!(list_printer_history_with_conn(&conn, b).unwrap().len(), 1);
    }

    #[test]
    fn details_dto_uses_the_frontend_field_names() {
        let details: PrinterDetails = serde_json::from_value(serde_json::json!({
            "manufacturer": null, "model": "X", "nozzleMm": 0.4, "bedXMm": 200.0, "bedYMm": 201.0, "bedZMm": 202.0
        })).unwrap();
        let mut conn = db::connect_in_memory().unwrap();
        let p = add_printer_with_conn(&mut conn, "A", "Holder", "filament").unwrap();
        update_printer_details_with_conn(&mut conn, p.id.parse().unwrap(), details).unwrap();
        let value = serde_json::to_value(list_printers_with_conn(&conn).unwrap().remove(0)).unwrap();
        assert_eq!(value["nozzleMm"], 0.4);
        assert_eq!(value["bedXMm"], 200.0);
        assert_eq!(value["bedYMm"], 201.0);
        assert_eq!(value["bedZMm"], 202.0);
    }

    #[test]
    fn printer_details_validate_boundaries_and_clear_nulls() {
        let mut conn = db::connect_in_memory().unwrap();
        let printer = add_printer_with_conn(&mut conn, "Test", "Holder", "filament").unwrap();
        let id = printer.id.parse().unwrap();
        for nozzle in [0.1, 2.0] {
            let details = PrinterDetails { manufacturer: Some(" Qidi ".into()), model: Some(" ".into()), nozzle_mm: Some(nozzle), bed_x_mm: Some(1.0), bed_y_mm: Some(2000.0), bed_z_mm: None };
            update_printer_details_with_conn(&mut conn, id, details).unwrap();
            let result = list_printers_with_conn(&conn).unwrap().remove(0);
            assert_eq!(result.manufacturer.as_deref(), Some("Qidi"));
            assert_eq!(result.model, None);
            assert_eq!(result.nozzle_mm, Some(nozzle));
            assert_eq!(result.bed_x_mm, Some(1.0));
            assert_eq!(result.bed_y_mm, Some(2000.0));
        }
        for nozzle in [0.099, 2.001, f64::NAN, f64::INFINITY] {
            assert!(update_printer_details_with_conn(&mut conn, id, PrinterDetails { nozzle_mm: Some(nozzle), ..Default::default() }).unwrap_err().expected);
        }
        for axis in 0..3 {
            for value in [0.9, 2000.1, f64::NAN] {
                let mut details = PrinterDetails::default();
                match axis { 0 => details.bed_x_mm = Some(value), 1 => details.bed_y_mm = Some(value), _ => details.bed_z_mm = Some(value) };
                assert!(update_printer_details_with_conn(&mut conn, id, details).unwrap_err().expected);
            }
        }
        update_printer_details_with_conn(&mut conn, id, PrinterDetails::default()).unwrap();
        let cleared = list_printers_with_conn(&conn).unwrap().remove(0);
        assert!(cleared.manufacturer.is_none() && cleared.nozzle_mm.is_none() && cleared.bed_y_mm.is_none());
        assert!(update_printer_details_with_conn(&mut conn, 999, PrinterDetails::default()).unwrap_err().expected);
    }

    #[test]
    fn reorder_printers_requires_exact_permutation() {
        let mut conn = db::connect_in_memory().unwrap();
        let a: i64 = add_printer_with_conn(&mut conn, "A", "Holder", "filament").unwrap().id.parse().unwrap();
        let b: i64 = add_printer_with_conn(&mut conn, "B", "Vat", "resin").unwrap().id.parse().unwrap();
        in_tx(&mut conn, |tx| p::reorder_printers(tx, &[b, a])).unwrap();
        assert_eq!(list_printers_with_conn(&conn).unwrap()[0].name, "B");
        for ids in [vec![a], vec![a, a], vec![a, 999]] {
            assert!(in_tx(&mut conn, |tx| p::reorder_printers(tx, &ids)).unwrap_err().expected);
            assert_eq!(list_printers_with_conn(&conn).unwrap()[0].name, "B");
        }
    }

    #[test]
    fn in_tx_keeps_a_dberror_invalid_from_the_closure_marked_expected() {
        let mut conn = db::connect_in_memory().unwrap();
        let result: CmdResult<()> = in_tx(&mut conn, |_tx| {
            Err(db::error::DbError::Invalid("Name darf nicht leer sein".into()))
        });
        let err = result.unwrap_err();
        assert!(err.expected, "DbError::Invalid routed through in_tx must stay `expected`");
    }

    fn spool(conn: &Connection, location: &str) -> String {
        db::insert_filament_spool(
            conn,
            &db::models::NewFilamentSpool {
                material: "PLA".into(),
                manufacturer: None,
                color: None,
                location: Some(location.into()),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 1000.0,
                price: None,
                image_png: None,
                color_hex: None,
                kind: "filament".into(),
            },
        )
        .unwrap()
        .to_string()
    }

    #[test]
    fn printers_are_listed_with_their_units_in_order() {
        let conn = db::connect_in_memory().unwrap();
        let x1c = p::insert_printer(&conn, "X1C").unwrap();
        let a1 = p::insert_printer(&conn, "A1").unwrap();
        p::insert_unit(&conn, x1c, "bambu_ams", "AMS A", None).unwrap();
        p::insert_unit(&conn, x1c, "external", "Spulenhalter", None).unwrap();
        p::insert_unit(&conn, a1, "bambu_ams_lite", "AMS lite", None).unwrap();

        let printers = list_printers_with_conn(&conn).unwrap();

        assert_eq!(printers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["X1C", "A1"]);
        assert_eq!(
            printers[0].units.iter().map(|u| (u.name.as_str(), u.slot_count)).collect::<Vec<_>>(),
            vec![("AMS A", 4), ("Spulenhalter", 1)]
        );
        assert_eq!(printers[1].units[0].bambu_ams_index, Some(0), "Nummern zaehlen pro Drucker");
        assert_eq!(printers[0].units[0].printer_id, x1c.to_string());
    }

    #[test]
    fn load_and_unload_round_trip_through_the_command_layer() {
        let mut conn = db::connect_in_memory().unwrap();
        let printer = p::insert_printer(&conn, "X1C").unwrap();
        let unit = p::insert_unit(&conn, printer, "bambu_ams", "AMS A", None).unwrap().to_string();
        let first = spool(&conn, "Regal 1");
        let second = spool(&conn, "Regal 2");

        assert_eq!(load_spool_with_conn(&mut conn, &first, &unit, 0).unwrap().displaced_spool_id, None);
        let swap = load_spool_with_conn(&mut conn, &second, &unit, 0).unwrap();
        assert_eq!(swap.displaced_spool_id, Some(first.clone()));
        assert_eq!(unload_spool_with_conn(&mut conn, &second, None).unwrap(), Some("Regal 2".into()));
    }

    #[test]
    fn a_failed_operation_leaves_nothing_half_done() {
        let mut conn = db::connect_in_memory().unwrap();
        let printer = p::insert_printer(&conn, "X1C").unwrap();
        let unit = p::insert_unit(&conn, printer, "bambu_ams", "AMS A", None).unwrap().to_string();
        let first = spool(&conn, "Regal 1");

        assert!(load_spool_with_conn(&mut conn, &first, &unit, 9).is_err());
        assert!(load_spool_with_conn(&mut conn, "abc", &unit, 0).is_err());
        assert!(unload_spool_with_conn(&mut conn, &first, None).is_err());
        let location: Option<String> = conn
            .query_row("SELECT location FROM filament_spools WHERE id = ?1", [first.parse::<i64>().unwrap()], |r| r.get(0))
            .unwrap();
        assert_eq!(location, Some("Regal 1".into()));
    }

    #[test]
    fn a_new_printer_gets_a_spool_holder_with_one_slot() {
        let mut conn = db::connect_in_memory().unwrap();

        let printer = add_printer_with_conn(&mut conn, "A1 mini", "Spulenhalter", "filament").unwrap();

        assert_eq!(printer.name, "A1 mini");
        assert_eq!(printer.units.len(), 1);
        let holder = &printer.units[0];
        assert_eq!((holder.name.as_str(), holder.kind.as_str(), holder.slot_count), ("Spulenhalter", "external", 1));
        assert_eq!(holder.bambu_ams_index, None);
    }

    #[test]
    fn a_printer_is_not_created_when_its_spool_holder_is_rejected() {
        let mut conn = db::connect_in_memory().unwrap();

        assert!(add_printer_with_conn(&mut conn, "A1 mini", "   ", "filament").is_err());

        assert!(list_printers_with_conn(&conn).unwrap().is_empty(), "keine halbe Anlage");
    }

    #[test]
    fn a_new_resin_printer_gets_exactly_one_vat_instead_of_a_spool_holder() {
        let mut conn = db::connect_in_memory().unwrap();

        let printer = add_printer_with_conn(&mut conn, "Saturn 4", "Harzwanne", "resin").unwrap();

        assert_eq!((printer.name.as_str(), printer.kind.as_str()), ("Saturn 4", "resin"));
        assert_eq!(printer.units.len(), 1);
        let vat = &printer.units[0];
        assert_eq!((vat.name.as_str(), vat.kind.as_str(), vat.slot_count), ("Harzwanne", "resin_vat", 1));
        let filament = add_printer_with_conn(&mut conn, "X1C", "Spulenhalter", "filament").unwrap();
        assert_eq!(filament.kind, "filament");
    }

    #[test]
    fn an_unknown_printer_kind_creates_nothing() {
        let mut conn = db::connect_in_memory().unwrap();
        assert!(add_printer_with_conn(&mut conn, "X", "Halter", "toast").is_err());
        assert!(add_printer_with_conn(&mut conn, "Saturn", "  ", "resin").is_err(), "Wanne ohne Namen");
        assert!(list_printers_with_conn(&conn).unwrap().is_empty());
    }

    #[test]
    fn bottles_and_spools_only_go_where_they_belong_through_the_command_layer() {
        let mut conn = db::connect_in_memory().unwrap();
        let saturn = add_printer_with_conn(&mut conn, "Saturn 4", "Harzwanne", "resin").unwrap();
        let x1c = add_printer_with_conn(&mut conn, "X1C", "Spulenhalter", "filament").unwrap();
        let vat = saturn.units[0].id.clone();
        let holder = x1c.units[0].id.clone();
        let pla = spool(&conn, "Regal 1");
        let bottle = db::insert_filament_spool(
            &conn,
            &db::models::NewFilamentSpool {
                material: "Standard".into(),
                manufacturer: None,
                color: None,
                location: Some("Resin-Schrank".into()),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 620.0,
                price: None,
                image_png: None,
                color_hex: None,
                kind: "resin".into(),
            },
        )
        .unwrap()
        .to_string();

        assert!(load_spool_with_conn(&mut conn, &bottle, &holder, 0).is_err(), "Resin nie in einen Filament-Drucker");
        assert!(load_spool_with_conn(&mut conn, &pla, &vat, 0).is_err(), "Filament nie in die Harzwanne");
        load_spool_with_conn(&mut conn, &bottle, &vat, 0).unwrap();
        assert_eq!(unload_spool_with_conn(&mut conn, &bottle, None).unwrap(), Some("Resin-Schrank".into()));
    }
}

pub use katalog_core::read::manager::LastPrinterDto;

pub(crate) fn get_last_printer_for_file_with_conn(
    conn: &Connection,
    file_id: i64,
) -> CmdResult<Option<LastPrinterDto>> {
    katalog_core::read::manager::get_last_printer_for_file_with_conn(conn, file_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn get_last_printer_for_file(
    state: State<AppState>,
    file_id: String,
) -> CmdResult<Option<LastPrinterDto>> {
    let conn = lock_db(&state)?;
    get_last_printer_for_file_with_conn(&conn, parse_id(&file_id, "Datei")?)
}

#[cfg(test)]
fn in_tx<T>(
    conn: &mut Connection,
    f: impl FnOnce(&Connection) -> Result<T, db::error::DbError>,
) -> CmdResult<T> {
    katalog_core::write::printers::in_tx(conn, f).map_err(Into::into)
}

#[cfg(test)]
#[test]
fn manager_desktop_error_flags_remain_distinct_from_web() {
    let mut c = db::connect_in_memory().unwrap();
    let bad_kind = add_printer_with_conn(&mut c, "P", "H", "other").unwrap_err();
    assert!(!bad_kind.expected);
    assert_eq!(bad_kind.message, "unbekannte Druckerart: other");
    let empty = add_printer_with_conn(&mut c, " ", "H", "filament").unwrap_err();
    assert!(empty.expected);
    assert_eq!(empty.message, "Name darf nicht leer sein");
    let missing = unload_spool_with_conn(&mut c, "999", None).unwrap_err();
    assert!(!missing.expected);
    assert_eq!(missing.message, "Spule 999 existiert nicht");
    let invalid_id = load_spool_with_conn(&mut c, "bad", "1", 0).unwrap_err();
    assert!(!invalid_id.expected);
    assert_eq!(invalid_id.message, "ungueltige Spulen-ID");
    let missing =
        update_printer_details_with_conn(&mut c, 999, PrinterDetails::default()).unwrap_err();
    assert!(missing.expected);
    assert_eq!(missing.message, "Drucker nicht gefunden");
}
