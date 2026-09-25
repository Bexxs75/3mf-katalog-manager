//! Tauri-Befehle fuer Drucker, ihre Mehrfarbeinheiten und das Einlegen/
//! Herausnehmen von Spulen. Die Logik liegt in `db::printers`; hier nur
//! ID-Umwandlung, Transaktionen und DTOs. Jede aendernde Operation laeuft in
//! genau einer Transaktion, damit nie eine Spule "zwischen zwei Faechern"
//! haengen bleibt.

use super::*;
use db::printers as p;

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
    /// "filament" oder "resin" (v0.14.0), beim Anlegen fest gewaehlt.
    pub kind: String,
    pub units: Vec<MaterialUnitDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadResultDto {
    pub displaced_spool_id: Option<String>,
}

fn parse_id(value: &str, what: &str) -> CmdResult<i64> {
    value.parse().map_err(|_| format!("ungueltige {what}-ID"))
}

fn unit_dto(u: db::models::MaterialUnitRecord) -> MaterialUnitDto {
    MaterialUnitDto {
        id: u.id.to_string(),
        printer_id: u.printer_id.to_string(),
        name: u.name,
        kind: u.kind,
        slot_count: u.slot_count,
        bambu_ams_index: u.bambu_ams_index,
    }
}

pub(crate) fn list_printers_with_conn(conn: &Connection) -> CmdResult<Vec<PrinterDto>> {
    let printers = p::list_printers(conn).map_err(|e| e.to_string())?;
    let units = p::list_units(conn).map_err(|e| e.to_string())?;
    Ok(printers
        .into_iter()
        .map(|printer| PrinterDto {
            id: printer.id.to_string(),
            name: printer.name,
            kind: printer.kind,
            units: units
                .iter()
                .filter(|u| u.printer_id == printer.id)
                .cloned()
                .map(unit_dto)
                .collect(),
        })
        .collect())
}

/// Fuehrt `f` in einer Transaktion aus und committet nur bei Erfolg.
fn in_tx<T>(conn: &mut Connection, f: impl FnOnce(&Connection) -> Result<T, db::error::DbError>) -> CmdResult<T> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let value = f(&tx).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(value)
}

#[tauri::command]
pub fn list_printers(state: State<AppState>) -> CmdResult<Vec<PrinterDto>> {
    let conn = lock_db(&state)?;
    list_printers_with_conn(&conn)
}

/// `holder_name` ist der (uebersetzte) Name der ersten Einheit: bei
/// Filament-Druckern der Spulenhalter, bei Resin-Druckern die Harzwanne.
pub(crate) fn add_printer_with_conn(conn: &mut Connection, name: &str, holder_name: &str, kind: &str) -> CmdResult<PrinterDto> {
    // Jeder Filament-Drucker bekommt direkt einen Spulenhalter (1 Fach),
    // damit auch Drucker ohne AMS sofort eine Spule aufnehmen koennen; ein
    // Resin-Drucker bekommt stattdessen seine einzige Einheit, die
    // Harzwanne. Beides in einer Transaktion: scheitert die Einheit,
    // entsteht auch kein Drucker.
    let id = in_tx(conn, |tx| {
        let id = p::insert_printer_of_kind(tx, name, kind)?;
        if kind == p::PRINTER_KIND_RESIN {
            p::insert_resin_vat(tx, id, holder_name)?;
        } else {
            p::insert_unit(tx, id, "external", holder_name, None)?;
        }
        Ok(id)
    })?;
    list_printers_with_conn(conn)?
        .into_iter()
        .find(|printer| printer.id == id.to_string())
        .ok_or_else(|| "Drucker nach dem Anlegen nicht gefunden".to_string())
}

#[tauri::command]
pub fn add_printer(state: State<AppState>, name: String, holder_name: String, kind: Option<String>) -> CmdResult<PrinterDto> {
    let mut conn = lock_db(&state)?;
    // Fehlt `kind` (aeltere Aufrufer), gilt Filament.
    let kind = kind.unwrap_or_else(|| p::PRINTER_KIND_FILAMENT.to_string());
    add_printer_with_conn(&mut conn, &name, &holder_name, &kind)
}

#[tauri::command]
pub fn rename_printer(state: State<AppState>, printer_id: String, name: String) -> CmdResult<()> {
    let id = parse_id(&printer_id, "Drucker")?;
    let mut conn = lock_db(&state)?;
    in_tx(&mut conn, |tx| p::rename_printer(tx, id, &name))
}

/// Liefert die Anzahl der Spulen, die an ihren Stammplatz zurueckkehrten.
#[tauri::command]
pub fn delete_printer(state: State<AppState>, printer_id: String) -> CmdResult<usize> {
    let id = parse_id(&printer_id, "Drucker")?;
    let mut conn = lock_db(&state)?;
    in_tx(&mut conn, |tx| p::delete_printer(tx, id))
}

#[tauri::command]
pub fn add_unit(
    state: State<AppState>,
    printer_id: String,
    kind: String,
    name: String,
    slot_count: Option<i64>,
) -> CmdResult<MaterialUnitDto> {
    let printer = parse_id(&printer_id, "Drucker")?;
    let mut conn = lock_db(&state)?;
    let id = in_tx(&mut conn, |tx| p::insert_unit(tx, printer, &kind, &name, slot_count))?;
    p::list_units(&conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|u| u.id == id)
        .map(unit_dto)
        .ok_or_else(|| "Einheit nach dem Anlegen nicht gefunden".to_string())
}

#[tauri::command]
pub fn update_unit(state: State<AppState>, unit_id: String, name: String, slot_count: Option<i64>) -> CmdResult<usize> {
    let id = parse_id(&unit_id, "Einheit")?;
    let mut conn = lock_db(&state)?;
    in_tx(&mut conn, |tx| p::update_unit(tx, id, &name, slot_count))
}

#[tauri::command]
pub fn delete_unit(state: State<AppState>, unit_id: String) -> CmdResult<usize> {
    let id = parse_id(&unit_id, "Einheit")?;
    let mut conn = lock_db(&state)?;
    in_tx(&mut conn, |tx| p::delete_unit(tx, id))
}

#[tauri::command]
pub fn reorder_units(state: State<AppState>, printer_id: String, unit_ids: Vec<String>) -> CmdResult<()> {
    let printer = parse_id(&printer_id, "Drucker")?;
    let ids = unit_ids
        .iter()
        .map(|id| parse_id(id, "Einheit"))
        .collect::<CmdResult<Vec<_>>>()?;
    let mut conn = lock_db(&state)?;
    in_tx(&mut conn, |tx| p::reorder_units(tx, printer, &ids))
}

pub(crate) fn load_spool_with_conn(
    conn: &mut Connection,
    spool_id: &str,
    unit_id: &str,
    slot_index: i64,
) -> CmdResult<LoadResultDto> {
    let spool = parse_id(spool_id, "Spulen")?;
    let unit = parse_id(unit_id, "Einheit")?;
    let outcome = in_tx(conn, |tx| p::load_spool(tx, spool, unit, slot_index))?;
    Ok(LoadResultDto {
        displaced_spool_id: outcome.displaced_spool_id.map(|id| id.to_string()),
    })
}

#[tauri::command]
pub fn load_spool(state: State<AppState>, spool_id: String, unit_id: String, slot_index: i64) -> CmdResult<LoadResultDto> {
    let mut conn = lock_db(&state)?;
    load_spool_with_conn(&mut conn, &spool_id, &unit_id, slot_index)
}

pub(crate) fn unload_spool_with_conn(
    conn: &mut Connection,
    spool_id: &str,
    location: Option<String>,
) -> CmdResult<Option<String>> {
    let spool = parse_id(spool_id, "Spulen")?;
    in_tx(conn, |tx| p::unload_spool(tx, spool, location.as_deref()))
}

/// Liefert den neuen Lagerort (fuer den Hinweis "zurueck nach …").
#[tauri::command]
pub fn unload_spool(state: State<AppState>, spool_id: String, location: Option<String>) -> CmdResult<Option<String>> {
    let mut conn = lock_db(&state)?;
    unload_spool_with_conn(&mut conn, &spool_id, location)
}

#[cfg(test)]
mod tests {
    use super::*;

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
