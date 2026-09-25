use super::*;

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
    /// Nur lesend: Stammplatz, Einheit und Fach aendern sich ausschliesslich
    /// ueber `load_spool`/`unload_spool` (commands/printers.rs).
    #[serde(default)]
    pub home_location: Option<String>,
    #[serde(default)]
    pub unit_id: Option<String>,
    #[serde(default)]
    pub slot_index: Option<i64>,
    /// "filament" oder "resin" (v0.13.1). Fehlt es (aeltere Aufrufer), gilt "filament".
    #[serde(default = "default_spool_kind")]
    pub kind: String,
}
fn validate_color_hex(color_hex: &Option<String>) -> CmdResult<()> {
    match color_hex {
        Some(value) if !db::printers::is_valid_color_hex(value) => {
            Err(format!("ungueltiger Farbwert: {value}"))
        }
        _ => Ok(()),
    }
}

fn default_spool_kind() -> String {
    db::models::SPOOL_KIND_FILAMENT.to_string()
}

fn validate_spool_kind(kind: &str) -> CmdResult<()> {
    if db::models::SPOOL_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(format!("ungueltige Art: {kind}"))
    }
}
/// Gegenstueck zu `filament_dto_to_record`: baut das nach aussen gehende DTO
/// aus dem tatsaechlichen Datenbankstand, statt (wie vor dem finalen Review
/// bei `update_filament_spool`) das ungeprueft vom Aufrufer geschickte DTO
/// zu spiegeln - siehe `update_filament_spool` fuer die Begruendung.
fn spool_record_to_dto(s: db::models::FilamentSpoolRecord) -> FilamentSpoolDto {
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
        image_png: s.image_png.map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
        color_hex: s.color_hex,
        home_location: s.home_location,
        unit_id: s.unit_id.map(|id| id.to_string()),
        slot_index: s.slot_index,
        kind: s.kind,
    }
}
fn filament_dto_to_record(spool: &FilamentSpoolDto) -> db::models::NewFilamentSpool {
    use base64::Engine;
    db::models::NewFilamentSpool {
        material: spool.material.clone(),
        manufacturer: spool.manufacturer.clone(),
        color: spool.color.clone(),
        location: spool.location.clone(),
        diameter_mm: spool.diameter_mm,
        original_weight_g: spool.original_weight_g,
        remaining_weight_g: spool.remaining_weight_g,
        price: spool.price,
        image_png: spool
            .image_png
            .as_ref()
            .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok()),
        color_hex: spool.color_hex.as_ref().map(|c| c.to_lowercase()),
        kind: spool.kind.clone(),
    }
}
#[tauri::command]
pub fn list_filament_spools(state: State<AppState>) -> CmdResult<Vec<FilamentSpoolDto>> {
    let conn = lock_db(&state)?;
    let spools = db::list_filament_spools(&conn).map_err(|e| e.to_string())?;
    Ok(spools.into_iter().map(spool_record_to_dto).collect())
}
#[tauri::command]
pub fn add_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto> {
    validate_spool_kind(&spool.kind)?;
    validate_color_hex(&spool.color_hex)?;
    let conn = lock_db(&state)?;
    let new_spool = filament_dto_to_record(&spool);
    let id = db::insert_filament_spool(&conn, &new_spool).map_err(|e| e.to_string())?;
    // Neue Spulen liegen immer im Lager, auch wenn der Aufrufer ein Fach
    // mitschickt.
    Ok(FilamentSpoolDto {
        id: id.to_string(),
        color_hex: new_spool.color_hex,
        home_location: None,
        unit_id: None,
        slot_index: None,
        ..spool
    })
}
/// Finaler Review 2026-09-23, Finding 8: gab frueher einfach `spool`, das
/// unveraendert vom Aufrufer stammende DTO, zurueck - inkonsistent mit
/// `add_filament_spool` (das ein neu gebautes DTO mit normalisiertem
/// `color_hex` und ohne die vom Aufrufer geschickten Fach-Felder
/// zurueckgibt). Ein Aufrufer, der z.B. `colorHex: "#ABCDEF"` (nicht
/// kleingeschrieben) oder einen erfundenen `unitId`/`slotIndex` schickt,
/// haette dieselben, ungeprueften Werte zurueckbekommen, obwohl die
/// Datenbank (siehe `db::update_filament_spool`s Kommentar: Fach-Felder
/// aendern sich NIE ueber diesen Pfad) etwas anderes gespeichert hat. Liest
/// die Zeile deshalb nach dem UPDATE frisch aus der Datenbank, genau wie
/// `add_filament_spool` es fuer die neu eingefuegte Zeile tut.
#[tauri::command]
pub fn update_filament_spool(state: State<AppState>, spool: FilamentSpoolDto) -> CmdResult<FilamentSpoolDto> {
    validate_spool_kind(&spool.kind)?;
    validate_color_hex(&spool.color_hex)?;
    let id: i64 = spool.id.parse().map_err(|_| "invalid spool id".to_string())?;
    let conn = lock_db(&state)?;
    let new_spool = filament_dto_to_record(&spool);
    db::update_filament_spool(&conn, id, &new_spool).map_err(|e| e.to_string())?;
    let updated = db::get_filament_spool(&conn, id).map_err(|e| e.to_string())?;
    Ok(spool_record_to_dto(updated))
}
#[tauri::command]
pub fn delete_filament_spool(state: State<AppState>, spool_id: String) -> CmdResult<()> {
    let id: i64 = spool_id.parse().map_err(|_| "invalid spool id".to_string())?;
    let conn = lock_db(&state)?;
    db::delete_filament_spool(&conn, id).map_err(|e| e.to_string())
}

/// Obergrenze fuer "Nachkaufen" (Spec v0.13.1: Anzahl 1 bis 20).
pub(crate) const RESTOCK_MAX_COUNT: i64 = 20;

/// Kernlogik von `restock_filament_spool` (Test-Huelle wie andere `*_with_conn`).
/// Legt `count` neue, volle Eintraege nach dem Vorbild der Vorlage an. Kopiert
/// werden Art, Material, Hersteller, Farbname, Farbwert, Bild und Durchmesser;
/// NICHT kopiert werden Fach (`unit_id`/`slot_index`) und Stammplatz - neue
/// Spulen/Flaschen liegen immer im Lager. `weight` ist Gramm (Filament) bzw.
/// Milliliter (Resin), auf 0,1 gerundet. Alles in EINER Transaktion:
/// scheitert eine Einfuegung, wird nichts angelegt.
pub(crate) fn restock_filament_spool_with_conn(
    conn: &mut Connection,
    template_id: &str,
    count: i64,
    weight: f64,
    price: Option<f64>,
    location: Option<String>,
) -> CmdResult<Vec<FilamentSpoolDto>> {
    let template_id: i64 = template_id.parse().map_err(|_| "invalid spool id".to_string())?;
    if !(1..=RESTOCK_MAX_COUNT).contains(&count) {
        return Err(format!("Anzahl muss zwischen 1 und {RESTOCK_MAX_COUNT} liegen"));
    }
    let weight = db::printers::round_tenth(weight);
    if !weight.is_finite() || weight <= 0.0 {
        return Err("Menge muss groesser als 0 sein".to_string());
    }
    if let Some(value) = price {
        if !value.is_finite() || value < 0.0 {
            return Err("ungueltiger Preis".to_string());
        }
    }
    let location = location
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let template = match db::get_filament_spool(&tx, template_id) {
        Ok(record) => record,
        Err(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => {
            return Err("Vorlage nicht gefunden (die Spule wurde inzwischen geloescht)".to_string());
        }
        Err(e) => return Err(e.to_string()),
    };
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
        kind: template.kind,
    };
    let mut created = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let id = db::insert_filament_spool(&tx, &new_spool).map_err(|e| e.to_string())?;
        let record = db::get_filament_spool(&tx, id).map_err(|e| e.to_string())?;
        created.push(spool_record_to_dto(record));
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(created)
}

/// "Nachkaufen" (v0.13.1). `async` + `spawn_blocking`: bis zu 20 Einfuegungen
/// mit Bild-Blob sollen den UI-Thread nicht blockieren.
#[tauri::command]
pub async fn restock_filament_spool(
    app: tauri::AppHandle,
    template_id: String,
    count: i64,
    weight: f64,
    price: Option<f64>,
    location: Option<String>,
) -> CmdResult<Vec<FilamentSpoolDto>> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut conn = lock_db(&state)?;
        restock_filament_spool_with_conn(&mut conn, &template_id, count, weight, price, location)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Kernlogik von `check_filament` (Test-Huelle wie andere `*_with_conn`):
/// laedt Slicer-Daten der Modelle in der uebergebenen Reihenfolge, alle Spulen
/// und die Namen von Drucker/Einheit fuer eingelegte Spulen. Unbekannte,
/// geloeschte oder ungueltige IDs werden uebersprungen.
pub(crate) fn check_filament_with_conn(
    conn: &Connection,
    file_ids: &[String],
) -> CmdResult<Vec<crate::filament_check::ModelCheck>> {
    use crate::filament_check::{check_models, SlotRef, SpoolInput};

    let mut models = Vec::new();
    for raw in file_ids {
        let Ok(id) = raw.parse::<i64>() else { continue };
        let Some(file) = db::get_file(conn, id).map_err(|e| e.to_string())? else { continue };
        if file.deleted_at.is_some() {
            continue;
        }
        let slice = file
            .slice_info_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<crate::threemf::SliceInfo>(json).ok());
        models.push((raw.clone(), slice));
    }

    let printers = db::printers::list_printers(conn).map_err(|e| e.to_string())?;
    let units = db::printers::list_units(conn).map_err(|e| e.to_string())?;
    let spools = db::list_filament_spools(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| {
            let slot = match (s.unit_id, s.slot_index) {
                (Some(unit_id), Some(slot_index)) => units.iter().find(|u| u.id == unit_id).map(|u| SlotRef {
                    printer: printers
                        .iter()
                        .find(|p| p.id == u.printer_id)
                        .map(|p| p.name.clone())
                        .unwrap_or_default(),
                    unit: u.name.clone(),
                    slot_number: slot_index + 1,
                }),
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

#[tauri::command]
pub fn check_filament(state: State<AppState>, file_ids: Vec<String>) -> CmdResult<Vec<crate::filament_check::ModelCheck>> {
    let conn = lock_db(&state)?;
    check_filament_with_conn(&conn, &file_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_filament_reads_slice_info_spools_and_slots() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id = crate::db::test_insert_minimal_file(&conn, "/tmp/check_filament.3mf", None).expect("file");
        conn.execute(
            "UPDATE files SET slice_info_json = ?1 WHERE id = ?2",
            rusqlite::params![
                r##"{"total_weight_g":50,"plates":[{"plate_index":1,"weight_g":50,"filaments":[{"filament_type":"PLA","color":"#C0392B","used_g":50,"used_m":16}]}]}"##,
                file_id
            ],
        )
        .expect("slice");
        let spool_id = crate::db::insert_filament_spool(
            &conn,
            &crate::db::models::NewFilamentSpool {
                material: "PLA".to_string(),
                manufacturer: Some("Bambu".to_string()),
                color: Some("Rot".to_string()),
                location: Some("Regal A".to_string()),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 640.0,
                price: None,
                image_png: None,
                color_hex: Some("#B03020".to_string()),
                kind: "filament".to_string(),
            },
        )
        .expect("spool");
        let printer = crate::db::printers::insert_printer(&conn, "X1C").expect("printer");
        let unit = crate::db::printers::insert_unit(&conn, printer, "bambu_ams", "AMS 1", None).expect("unit");
        crate::db::printers::load_spool(&conn, spool_id, unit, 1).expect("load");

        let result = check_filament_with_conn(&conn, &[file_id.to_string(), "999999".to_string(), "kaputt".to_string()])
            .expect("check");

        assert_eq!(result.len(), 1, "unbekannte und ungueltige IDs werden uebersprungen");
        assert_eq!(result[0].file_id, file_id.to_string());
        assert_eq!(result[0].status, crate::filament_check::CheckStatus::Ok);
        let used = &result[0].needs[0].spools[0];
        assert_eq!(used.spool_id, spool_id.to_string());
        assert_eq!(
            used.slot,
            Some(crate::filament_check::SlotRef { printer: "X1C".into(), unit: "AMS 1".into(), slot_number: 2 })
        );
        assert_eq!(used.location, None);
    }

    #[test]
    fn check_filament_reports_no_data_for_files_without_slice_info() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id = crate::db::test_insert_minimal_file(&conn, "/tmp/no_slice.stl", None).expect("file");
        let result = check_filament_with_conn(&conn, &[file_id.to_string()]).expect("check");
        assert_eq!(result[0].status, crate::filament_check::CheckStatus::NoData);
    }

    #[test]
    fn check_filament_skips_a_soft_deleted_file() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id = crate::db::test_insert_minimal_file(&conn, "/tmp/deleted.3mf", None).expect("file");
        conn.execute(
            "UPDATE files SET slice_info_json = ?1 WHERE id = ?2",
            rusqlite::params![
                r##"{"total_weight_g":50,"plates":[{"plate_index":1,"weight_g":50,"filaments":[{"filament_type":"PLA","color":"#C0392B","used_g":50,"used_m":16}]}]}"##,
                file_id
            ],
        )
        .expect("slice");
        crate::db::soft_delete_file(&conn, file_id, None, "2026-09-24T00:00:00Z").expect("soft delete");

        let result = check_filament_with_conn(&conn, &[file_id.to_string()]).expect("check");

        assert!(result.is_empty(), "eine geloeschte Datei wird komplett uebersprungen");
    }

    #[test]
    fn check_filament_reports_no_data_for_malformed_slice_info_json() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let file_id = crate::db::test_insert_minimal_file(&conn, "/tmp/kaputt.3mf", None).expect("file");
        conn.execute(
            "UPDATE files SET slice_info_json = ?1 WHERE id = ?2",
            rusqlite::params!["{das ist kein gueltiges json", file_id],
        )
        .expect("slice");

        let result = check_filament_with_conn(&conn, &[file_id.to_string()]).expect("check");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].status, crate::filament_check::CheckStatus::NoData);
    }

    fn new_spool(kind: &str) -> crate::db::models::NewFilamentSpool {
        crate::db::models::NewFilamentSpool {
            material: "PLA".into(),
            manufacturer: None,
            color: None,
            location: Some("Regal 1".into()),
            diameter_mm: 1.75,
            original_weight_g: 1000.0,
            remaining_weight_g: 1000.0,
            price: None,
            image_png: None,
            color_hex: None,
            kind: kind.into(),
        }
    }

    #[test]
    fn kind_round_trips_through_insert_list_and_dto() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let id = crate::db::insert_filament_spool(&conn, &new_spool("resin")).expect("insert");
        let record = crate::db::get_filament_spool(&conn, id).expect("get");
        assert_eq!(record.kind, "resin");
        assert_eq!(crate::db::list_filament_spools(&conn).expect("list")[0].kind, "resin");
        assert_eq!(spool_record_to_dto(record).kind, "resin");
    }

    #[test]
    fn a_dto_without_kind_defaults_to_filament() {
        let dto: FilamentSpoolDto = serde_json::from_value(serde_json::json!({
            "id": "", "material": "PLA", "manufacturer": null, "color": null, "location": null,
            "diameterMm": 1.75, "originalWeightG": 1000.0, "remainingWeightG": 1000.0,
            "price": null, "imagePng": null
        }))
        .expect("deserialize");
        assert_eq!(dto.kind, "filament");
    }

    #[test]
    fn unknown_kinds_are_rejected() {
        assert!(validate_spool_kind("filament").is_ok());
        assert!(validate_spool_kind("resin").is_ok());
        assert!(validate_spool_kind("pla").is_err());
        assert!(validate_spool_kind("").is_err());
    }

    #[test]
    fn a_spool_in_a_slot_cannot_become_resin_but_a_stored_one_can() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let loaded = crate::db::insert_filament_spool(&conn, &new_spool("filament")).expect("insert");
        let stored = crate::db::insert_filament_spool(&conn, &new_spool("filament")).expect("insert");
        let printer = crate::db::printers::insert_printer(&conn, "X1C").expect("printer");
        let unit = crate::db::printers::insert_unit(&conn, printer, "bambu_ams", "AMS 1", None).expect("unit");
        crate::db::printers::load_spool(&conn, loaded, unit, 0).expect("load");

        assert!(crate::db::update_filament_spool(&conn, loaded, &new_spool("resin")).is_err());
        assert_eq!(crate::db::get_filament_spool(&conn, loaded).expect("get").kind, "filament");

        crate::db::update_filament_spool(&conn, stored, &new_spool("resin")).expect("update");
        assert_eq!(crate::db::get_filament_spool(&conn, stored).expect("get").kind, "resin");
    }

    fn restock_template(conn: &Connection, kind: &str) -> i64 {
        crate::db::insert_filament_spool(
            conn,
            &crate::db::models::NewFilamentSpool {
                material: "ABS-T".to_string(),
                manufacturer: Some("Prusament".to_string()),
                color: Some("Orange".to_string()),
                location: Some("Technik".to_string()),
                diameter_mm: 1.75,
                original_weight_g: 1000.0,
                remaining_weight_g: 120.0,
                price: Some(29.95),
                image_png: Some(vec![1, 2, 3]),
                color_hex: Some("#f07f1e".to_string()),
                kind: kind.to_string(),
            },
        )
        .expect("template")
    }

    #[test]
    fn restock_creates_full_spools_in_storage_with_the_template_data() {
        let mut conn = crate::db::connect_in_memory().expect("connect");
        let template = restock_template(&conn, "filament");
        // Vorlage steckt im Drucker: Fach und Stammplatz duerfen NICHT kopiert werden.
        let printer = crate::db::printers::insert_printer(&conn, "X1C").expect("printer");
        let unit = crate::db::printers::insert_unit(&conn, printer, "bambu_ams", "AMS 1", None).expect("unit");
        crate::db::printers::load_spool(&conn, template, unit, 0).expect("load");

        let created = restock_filament_spool_with_conn(
            &mut conn,
            &template.to_string(),
            3,
            750.04,
            Some(24.5),
            Some("  Regal B ".to_string()),
        )
        .expect("restock");

        assert_eq!(created.len(), 3);
        for spool in &created {
            assert_ne!(spool.id, template.to_string());
            assert_eq!(spool.kind, "filament");
            assert_eq!(spool.material, "ABS-T");
            assert_eq!(spool.manufacturer.as_deref(), Some("Prusament"));
            assert_eq!(spool.color.as_deref(), Some("Orange"));
            assert_eq!(spool.color_hex.as_deref(), Some("#f07f1e"));
            assert_eq!(spool.diameter_mm, 1.75);
            assert_eq!(spool.image_png.as_deref(), Some("AQID"), "Bild wird kopiert");
            assert_eq!(spool.original_weight_g, 750.0, "auf 0,1 gerundet");
            assert_eq!(spool.remaining_weight_g, 750.0, "neue Spulen sind voll");
            assert_eq!(spool.price, Some(24.5));
            assert_eq!(spool.location.as_deref(), Some("Regal B"), "Lagerort getrimmt");
            assert_eq!(spool.home_location, None);
            assert_eq!(spool.unit_id, None);
            assert_eq!(spool.slot_index, None);
        }
        let ids: std::collections::HashSet<_> = created.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), 3, "jede Spule bekommt einen eigenen Datensatz");
        assert_eq!(crate::db::list_filament_spools(&conn).expect("list").len(), 4);
        let original = crate::db::get_filament_spool(&conn, template).expect("template");
        assert_eq!(original.remaining_weight_g, 120.0, "die Vorlage bleibt unveraendert");
        assert_eq!(original.unit_id, Some(unit));
    }

    #[test]
    fn restock_of_a_resin_bottle_creates_resin_bottles() {
        let mut conn = crate::db::connect_in_memory().expect("connect");
        let template = restock_template(&conn, "resin").to_string();

        let created = restock_filament_spool_with_conn(&mut conn, &template, 2, 500.0, None, None).expect("restock");

        assert!(created.iter().all(|s| s.kind == "resin" && s.remaining_weight_g == 500.0));
    }

    #[test]
    fn restock_accepts_the_upper_limit_and_stores_blank_location_and_missing_price_as_none() {
        let mut conn = crate::db::connect_in_memory().expect("connect");
        let template = restock_template(&conn, "filament").to_string();

        let created =
            restock_filament_spool_with_conn(&mut conn, &template, RESTOCK_MAX_COUNT, 1000.0, None, Some("   ".to_string()))
                .expect("restock");

        assert_eq!(created.len(), 20);
        assert!(created.iter().all(|s| s.location.is_none() && s.price.is_none()));
    }

    #[test]
    fn restock_rejects_invalid_input_and_creates_nothing() {
        let mut conn = crate::db::connect_in_memory().expect("connect");
        let template = restock_template(&conn, "filament").to_string();

        for (count, weight, price) in [
            (0, 1000.0, None),
            (21, 1000.0, None),
            (1, 0.0, None),
            (1, 0.04, None),
            (1, -5.0, None),
            (1, f64::NAN, None),
            (1, 1000.0, Some(-1.0)),
            (1, 1000.0, Some(f64::NAN)),
            (1, 1000.0, Some(f64::INFINITY)),
        ] {
            let result = restock_filament_spool_with_conn(&mut conn, &template, count, weight, price, None);
            assert!(result.is_err(), "({count}, {weight}, {price:?}) muss abgelehnt werden");
        }
        assert!(restock_filament_spool_with_conn(&mut conn, "kaputt", 1, 1000.0, None, None).is_err());
        assert_eq!(crate::db::list_filament_spools(&conn).expect("list").len(), 1);
    }

    #[test]
    fn restock_fails_with_a_clear_message_when_the_template_is_gone() {
        let mut conn = crate::db::connect_in_memory().expect("connect");

        let err = restock_filament_spool_with_conn(&mut conn, "999999", 2, 1000.0, None, None).unwrap_err();

        assert!(err.contains("Vorlage"), "verstaendliche Meldung, war: {err}");
        assert!(crate::db::list_filament_spools(&conn).expect("list").is_empty());
    }

    #[test]
    fn restock_rolls_back_every_insert_when_one_fails() {
        let mut conn = crate::db::connect_in_memory().expect("connect");
        let template = restock_template(&conn, "filament").to_string();
        // Laesst die dritte neue Spule scheitern (1 Vorlage + 2 neue = 3 Zeilen).
        conn.execute_batch(
            "CREATE TEMP TRIGGER restock_fail BEFORE INSERT ON filament_spools
             WHEN (SELECT COUNT(*) FROM filament_spools) >= 3
             BEGIN SELECT RAISE(ABORT, 'boom'); END;",
        )
        .expect("trigger");

        let result = restock_filament_spool_with_conn(&mut conn, &template, 5, 1000.0, None, None);

        assert!(result.is_err());
        assert_eq!(
            crate::db::list_filament_spools(&conn).expect("list").len(),
            1,
            "nichts halb angelegt: auch die zwei erfolgreichen Einfuegungen sind zurueckgerollt"
        );
    }
}
