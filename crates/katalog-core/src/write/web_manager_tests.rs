use super::{web::*, WriteError};
use crate::{db, read::manager as r};
use rusqlite::Connection;
fn spool() -> r::FilamentSpoolDto {
    serde_json::from_value(serde_json::json!({"id":"", "material":"PLA", "location":"Shelf", "diameterMm":1.75,"originalWeightG":1000.0,"remainingWeightG":800.0,"imagePng":"iVBORw0KGgo=","colorHex":"#ABCDEF","kind":"filament"})).unwrap()
}
fn setup() -> (Connection, i64, i64, i64) {
    let mut c = db::connect_in_memory().unwrap();
    let p = add_printer(&mut c, "Printer", "Holder", "filament")
        .unwrap()
        .value;
    let s = add_filament_spool(&mut c, spool()).unwrap().value;
    (
        c,
        p.id.parse().unwrap(),
        p.units[0].id.parse().unwrap(),
        s.id.parse().unwrap(),
    )
}
fn invalid<T: std::fmt::Debug>(result: Result<T, WriteError>) {
    assert!(
        matches!(result, Err(WriteError::Invalid { .. })),
        "{result:?}"
    );
}
fn missing<T: std::fmt::Debug>(result: Result<T, WriteError>) {
    assert!(
        matches!(result, Err(WriteError::NotFound { .. })),
        "{result:?}"
    );
}

#[test]
fn manager_spool_crud_and_image_free_reads_preserve_desktop_images() {
    let (mut c, _, _, id) = setup();
    let desktop = r::list_filament_spools(&c, true).unwrap();
    assert!(desktop[0].image_png.is_some());
    assert!(!serde_json::to_value(&desktop[0])
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("hasImage"));
    let web = r::list_filament_spools_web(&c).unwrap();
    assert_eq!(web[0]["hasImage"], true);
    assert!(web[0].get("imagePng").is_none());
    assert!(r::list_filament_spools(&c, false).unwrap()[0]
        .image_png
        .is_none());
    assert_eq!(
        r::get_filament_spool_image(&c, id)
            .unwrap()
            .image_png
            .as_deref(),
        Some("iVBORw0KGgo=")
    );
    let mut s = spool();
    s.id = id.to_string();
    s.material = "PETG".into();
    s.image_png = None;
    let updated = update_filament_spool(&mut c, id, s, true).unwrap();
    assert_eq!(updated.changes, 1);
    assert_eq!(updated.value.material, "PETG");
    assert_eq!(updated.value.color_hex.as_deref(), Some("#abcdef"));
    assert_eq!(
        r::list_filament_spools_web(&c).unwrap()[0]["hasImage"],
        false
    );
    assert!(r::get_filament_spool_image(&c, id)
        .unwrap()
        .image_png
        .is_none());
    assert_eq!(delete_filament_spool(&mut c, id).unwrap().changes, 1);
    missing(delete_filament_spool(&mut c, id));
    missing(update_filament_spool(&mut c, id, spool(), true));
    assert_eq!(
        r::get_filament_spool_image(&c, id).err().unwrap().code,
        Some(crate::read::GeometryErrorCode::NotFound)
    );
}
#[test]
fn manager_spool_validation_covers_utf16_kind_color_finite_and_rounding_limits() {
    let mut c = db::connect_in_memory().unwrap();
    for (field, value) in [
        ("material", serde_json::json!(" ")),
        ("material", serde_json::json!("😀".repeat(61))),
        ("manufacturer", serde_json::json!("x".repeat(121))),
        ("color", serde_json::json!("x".repeat(121))),
        ("location", serde_json::json!("x".repeat(121))),
        ("kind", serde_json::json!("other")),
        ("colorHex", serde_json::json!("red")),
        ("diameterMm", serde_json::json!(0)),
        ("diameterMm", serde_json::json!(5.1)),
        ("originalWeightG", serde_json::json!(0.04)),
        ("originalWeightG", serde_json::json!(f64::MAX)),
        ("remainingWeightG", serde_json::json!(-1)),
        ("remainingWeightG", serde_json::json!(1001)),
        ("price", serde_json::json!(-1)),
        ("imagePng", serde_json::json!("R0lGODlh")),
    ] {
        let mut v = serde_json::to_value(spool()).unwrap();
        v[field] = value;
        invalid(add_filament_spool(
            &mut c,
            serde_json::from_value(v).unwrap(),
        ));
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut s = spool();
        s.original_weight_g = value;
        invalid(add_filament_spool(&mut c, s));
        let mut s = spool();
        s.remaining_weight_g = value;
        invalid(add_filament_spool(&mut c, s));
        let mut s = spool();
        s.price = Some(value);
        invalid(add_filament_spool(&mut c, s));
        let mut s = spool();
        s.diameter_mm = value;
        invalid(add_filament_spool(&mut c, s));
    }
    assert!(r::list_filament_spools(&c, false).unwrap().is_empty());
    let mut s = spool();
    s.material = "😀".repeat(60);
    s.diameter_mm = 5.0;
    s.original_weight_g = MAX_WEIGHT;
    s.remaining_weight_g = MAX_WEIGHT;
    assert!(add_filament_spool(&mut c, s)
        .unwrap()
        .value
        .original_weight_g
        .is_finite());
    let mut resin = spool();
    resin.kind = "resin".into();
    resin.diameter_mm = 0.0;
    assert_eq!(
        add_filament_spool(&mut c, resin).unwrap().value.diameter_mm,
        0.0
    );
}
#[test]
fn manager_printer_and_unit_contracts_and_missing_ids() {
    let (mut c, p, u, _) = setup();
    rename_printer(&mut c, p, &"😀".repeat(60)).unwrap();
    invalid(rename_printer(&mut c, p, &"😀".repeat(61)));
    invalid(rename_printer(&mut c, p, "  "));
    missing(rename_printer(&mut c, 999, "A"));
    missing(delete_printer(&mut c, 999));
    missing(update_printer_details(&mut c, 999, Default::default()));
    update_printer_details(
        &mut c,
        p,
        super::printers::PrinterDetails {
            manufacturer: Some("  Maker ".into()),
            nozzle_mm: Some(0.4),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        r::list_printers_with_conn(&c).unwrap()[0]
            .manufacturer
            .as_deref(),
        Some("Maker")
    );
    invalid(update_printer_details(
        &mut c,
        p,
        super::printers::PrinterDetails {
            manufacturer: Some("😀".repeat(61)),
            ..Default::default()
        },
    ));
    invalid(update_printer_details(
        &mut c,
        p,
        super::printers::PrinterDetails {
            bed_x_mm: Some(2001.0),
            ..Default::default()
        },
    ));
    missing(add_unit(&mut c, 999, "external", "A", None));
    invalid(add_unit(&mut c, p, "custom", "A", Some(17)));
    invalid(add_unit(&mut c, p, "custom", "A", None));
    invalid(add_unit(&mut c, p, "bad", "A", None));
    invalid(add_unit(&mut c, p, "custom", " ", Some(1)));
    let custom = add_unit(&mut c, p, "custom", "A", Some(16))
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    assert_eq!(update_unit(&mut c, custom, "B", Some(2)).unwrap().value, 0);
    invalid(update_unit(&mut c, custom, "B", Some(0)));
    invalid(update_unit(&mut c, custom, &"x".repeat(61), None));
    missing(update_unit(&mut c, 999, "A", None));
    missing(delete_unit(&mut c, 999));
    missing(reorder_units(&mut c, 999, &[]));
    reorder_units(&mut c, p, &[custom, u]).unwrap();
    reorder_printers(&mut c, &[p]).unwrap();
    delete_unit(&mut c, custom).unwrap();
    delete_printer(&mut c, p).unwrap();
}
#[test]
fn manager_load_swap_move_noop_and_unload_are_atomic() {
    let (mut c, p, u, a) = setup();
    let b = add_filament_spool(&mut c, spool())
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    let other = add_unit(&mut c, p, "external", "B", None)
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    missing(load_spool(&mut c, 999, u, 0));
    missing(load_spool(&mut c, a, 999, 0));
    missing(unload_spool(&mut c, 999, None));
    invalid(unload_spool(&mut c, a, None));
    invalid(load_spool(&mut c, a, u, i64::MAX));
    invalid(load_spool(&mut c, a, u, -1));
    invalid(load_spool(&mut c, a, u, 1));
    assert_eq!(load_spool(&mut c, a, u, 0).unwrap().changes, 1);
    assert_eq!(load_spool(&mut c, a, u, 0).unwrap().changes, 0);
    load_spool(&mut c, b, other, 0).unwrap();
    let swapped = load_spool(&mut c, b, u, 0).unwrap();
    assert_eq!(swapped.value.displaced_spool_id, Some(a.to_string()));
    assert_eq!(swapped.changes, 1);
    assert_eq!(
        db::get_filament_spool(&c, a).unwrap().location.as_deref(),
        Some("Shelf")
    );
    invalid(unload_spool(&mut c, b, Some("x".repeat(121))));
    assert_eq!(
        unload_spool(&mut c, b, None).unwrap().value.as_deref(),
        Some("Shelf")
    );
    // Fail the second UPDATE after displacing an occupant: both spools must return unchanged.
    load_spool(&mut c, a, u, 0).unwrap();
    c.execute_batch(&format!("CREATE TEMP TRIGGER reject_load BEFORE UPDATE OF unit_id ON filament_spools WHEN NEW.id={b} AND NEW.unit_id IS NOT NULL BEGIN SELECT RAISE(ABORT,'test'); END;")).unwrap();
    assert!(load_spool(&mut c, b, u, 0).is_err());
    assert_eq!(db::get_filament_spool(&c, a).unwrap().unit_id, Some(u));
    assert_eq!(db::get_filament_spool(&c, b).unwrap().unit_id, None);
}
#[test]
fn manager_resin_vat_is_fixed_and_material_kind_cannot_change_in_a_slot() {
    let (mut c, p, u, a) = setup();
    let resin = add_printer(&mut c, "Resin", "Vat", "resin").unwrap().value;
    let rp = resin.id.parse().unwrap();
    let vat = resin.units[0].id.parse().unwrap();
    invalid(add_unit(&mut c, rp, "external", "A", None));
    invalid(add_unit(&mut c, p, "resin_vat", "A", None));
    invalid(update_unit(&mut c, vat, "B", None));
    invalid(delete_unit(&mut c, vat));
    invalid(load_spool(&mut c, a, vat, 0));
    let mut s = spool();
    s.kind = "resin".into();
    let b = add_filament_spool(&mut c, s)
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    invalid(load_spool(&mut c, b, u, 0));
    load_spool(&mut c, b, vat, 0).unwrap();
    invalid(update_filament_spool(&mut c, b, spool(), true));
    assert_eq!(db::get_filament_spool(&c, b).unwrap().kind, "resin");
    delete_printer(&mut c, rp).unwrap();
    assert_eq!(
        db::get_filament_spool(&c, b).unwrap().location.as_deref(),
        Some("Shelf")
    );
}
#[test]
fn manager_reorder_invalid_membership_and_mid_write_failures_roll_back() {
    let (mut c, p, u, _) = setup();
    let p2 = add_printer(&mut c, "B", "Holder", "filament")
        .unwrap()
        .value;
    let p2id = p2.id.parse().unwrap();
    let u2 = p2.units[0].id.parse().unwrap();
    for ids in [vec![p, 999], vec![p, p], vec![p]] {
        invalid(reorder_printers(&mut c, &ids));
    }
    for ids in [vec![u, 999], vec![u, u], vec![u, u2], vec![]] {
        invalid(reorder_units(&mut c, p, &ids));
    }
    c.execute_batch(&format!("CREATE TEMP TRIGGER reject_order BEFORE UPDATE OF position ON printers WHEN NEW.id={p} BEGIN SELECT RAISE(ABORT,'test'); END;")).unwrap();
    assert!(reorder_printers(&mut c, &[p2id, p]).is_err());
    assert_eq!(r::list_printers_with_conn(&c).unwrap()[0].id, p.to_string());
    assert_eq!(
        c.query_row("SELECT position FROM printers WHERE id=?1", [p2id], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        1
    );
}
fn jobs(c: &Connection, p: i64, spool: i64, file: i64) {
    c.execute("INSERT INTO printer_connections(printer_id,kind,address,connected_since) VALUES (?1,'moonraker','example',0)",[p]).unwrap();
    for (n, state) in ["open", "confirmed", "ignored"].iter().enumerate() {
        c.execute("INSERT INTO printer_jobs(printer_id,remote_id,file_name,outcome,raw_status,ended_at,print_duration_s,used_mm,state,booked_spool_id,booked_file_id,booked_g) VALUES (?1,?2,'one','completed','completed',?2,10,20,?3,?4,?5,12.5)",rusqlite::params![p,n as i64,state,spool,file]).unwrap();
    }
}
#[test]
fn manager_delete_spool_preserves_jobs_booked_grams_and_file_associations() {
    let (mut c, p, u, s) = setup();
    let file = db::test_insert_minimal_file(&c, "/one", None).unwrap();
    jobs(&c, p, s, file);
    load_spool(&mut c, s, u, 0).unwrap();
    assert_eq!(delete_filament_spool(&mut c, s).unwrap().changes, 1);
    assert_eq!(c.query_row("SELECT count(*) FROM printer_jobs WHERE booked_spool_id IS NULL AND booked_g=12.5 AND booked_file_id=?1",[file],|r|r.get::<_,i64>(0)).unwrap(),3);
    assert!(r::get_last_printer_for_file_with_conn(&c, file)
        .unwrap()
        .is_some());
    assert!(r::list_filament_spools(&c, true).unwrap().is_empty());
}
#[test]
fn manager_delete_printer_cascades_jobs_connections_units_but_keeps_files_logs_consumption() {
    let (mut c, p, u, s) = setup();
    let file = db::test_insert_minimal_file(&c, "/one", None).unwrap();
    jobs(&c, p, s, file);
    db::insert_print_log_entry(
        &c,
        &db::models::NewPrintLogEntry {
            file_id: file,
            printed_at: "date".into(),
            note: None,
            photo_png: None,
        },
    )
    .unwrap();
    load_spool(&mut c, s, u, 0).unwrap();
    let result = delete_printer(&mut c, p).unwrap();
    assert_eq!(result.value, 1);
    assert_eq!(result.changes, 1);
    for table in [
        "printer_jobs",
        "printer_connections",
        "material_units",
        "printers",
    ] {
        assert_eq!(
            c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert!(db::get_file(&c, file).unwrap().is_some());
    assert_eq!(db::list_print_log_entries(&c, file).unwrap().len(), 1);
    let s = db::get_filament_spool(&c, s).unwrap();
    assert_eq!(s.remaining_weight_g, 800.0);
    assert_eq!(s.location.as_deref(), Some("Shelf"));
    assert!(s.unit_id.is_none());
    assert!(r::get_last_printer_for_file_with_conn(&c, file)
        .unwrap()
        .is_none());
}
#[test]
fn manager_shrink_and_delete_unit_restore_home_locations_and_failed_delete_rolls_back() {
    let (mut c, p, _, s) = setup();
    let unit = add_unit(&mut c, p, "custom", "Custom", Some(3))
        .unwrap()
        .value
        .id
        .parse()
        .unwrap();
    load_spool(&mut c, s, unit, 2).unwrap();
    let result = update_unit(&mut c, unit, "Small", Some(2)).unwrap();
    assert_eq!(result.value, 1);
    assert_eq!(result.changes, 1);
    assert_eq!(
        db::get_filament_spool(&c, s).unwrap().location.as_deref(),
        Some("Shelf")
    );
    load_spool(&mut c, s, unit, 0).unwrap();
    c.execute_batch("CREATE TEMP TRIGGER reject_delete BEFORE DELETE ON material_units BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert!(delete_unit(&mut c, unit).is_err());
    assert_eq!(db::get_filament_spool(&c, s).unwrap().unit_id, Some(unit));
    c.execute_batch("DROP TRIGGER reject_delete;").unwrap();
    let deleted = delete_unit(&mut c, unit).unwrap();
    assert_eq!(deleted.value, 1);
    assert_eq!(deleted.changes, 1);
    assert_eq!(
        db::get_filament_spool(&c, s).unwrap().location.as_deref(),
        Some("Shelf")
    );
}
#[test]
fn manager_quotas_only_restrict_creation_including_default_units() {
    let (mut c, p, u, s) = setup();
    c.execute_batch("WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n<5000) INSERT INTO filament_spools(material,diameter_mm,original_weight_g,remaining_weight_g,created_at) SELECT 'PLA',1.75,1000,1000,'' FROM seq;").unwrap();
    invalid(add_filament_spool(&mut c, spool()));
    update_filament_spool(&mut c, s, spool(), true).unwrap();
    delete_filament_spool(&mut c, s).unwrap();
    c.execute_batch("WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n<200) INSERT INTO printers(name) SELECT 'P' FROM seq;").unwrap();
    invalid(add_printer(&mut c, "New", "Holder", "filament"));
    rename_printer(&mut c, p, "Old").unwrap();
    for _ in 1..50 {
        db::printers::insert_unit(&c, p, "external", "H", None).unwrap();
    }
    invalid(add_unit(&mut c, p, "external", "New", None));
    update_unit(&mut c, u, "Old", None).unwrap();
    delete_unit(&mut c, u).unwrap();
    add_unit(&mut c, p, "external", "New", None).unwrap();
    delete_printer(&mut c, p).unwrap();
}
#[test]
fn manager_ams_limit_and_failed_default_unit_leave_no_partial_creation() {
    let (mut c, p, _, _) = setup();
    for _ in 0..4 {
        add_unit(&mut c, p, "bambu_ams", "AMS", None).unwrap();
    }
    invalid(add_unit(&mut c, p, "bambu_ams_lite", "AMS", None));
    invalid(add_printer(&mut c, "New", " ", "filament"));
    invalid(add_printer(&mut c, "New", "Vat", "other"));
    assert_eq!(r::list_printers_with_conn(&c).unwrap().len(), 1);
}
#[test]
fn manager_history_is_bounded_only_in_web_and_check_is_read_only() {
    let (c, p, _, s) = setup();
    let file = db::test_insert_minimal_file(&c, "/one", None).unwrap();
    jobs(&c, p, s, file);
    c.execute_batch(&format!("WITH RECURSIVE seq(n) AS (VALUES(10) UNION ALL SELECT n+1 FROM seq WHERE n<510) INSERT INTO printer_jobs(printer_id,remote_id,file_name,outcome,raw_status,ended_at,print_duration_s,used_mm) SELECT {p},n,'one','completed','completed',n,1,1 FROM seq;")).unwrap();
    assert_eq!(r::list_printer_history_with_conn(&c, p).unwrap().len(), 504);
    let web = r::list_printer_history_limit(&c, p, 500).unwrap();
    assert_eq!(web.len(), 500);
    assert_eq!(web[0].id, "504");
    assert!(r::list_printer_history_limit(&c, 999, 500)
        .unwrap()
        .is_empty());
    c.execute_batch("PRAGMA query_only=ON").unwrap();
    assert_eq!(
        r::check_filament_with_conn(&c, &[file.to_string(), "bad".into(), "9999".into()])
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn manager_default_unit_counts_towards_quota_and_printer_audit() {
    let mut c = db::connect_in_memory().unwrap();
    for kind in ["filament", "resin"] {
        let added = add_printer(&mut c, "P", "Default", kind).unwrap();
        assert_eq!(added.changes, 1);
        assert_eq!(added.value.units.len(), 1);
    }
    let p = 1;
    for _ in 1..50 {
        add_unit(&mut c, p, "external", "H", None).unwrap();
    }
    invalid(add_unit(&mut c, p, "external", "Overflow", None));
    assert_eq!(r::list_printers_with_conn(&c).unwrap()[0].units.len(), 50);
}

#[test]
fn manager_default_unit_quota_failure_rolls_back_new_printer() {
    let mut c = db::connect_in_memory().unwrap();
    // Simulate units created during printer insertion: default holder must still
    // pass the same quota admission, and failure must roll back the whole insert.
    c.execute_batch(
        "CREATE TEMP TRIGGER fill_units AFTER INSERT ON printers BEGIN
      INSERT INTO material_units(printer_id, kind, name, slot_count)
      SELECT NEW.id, 'external', 'H', 1 FROM json_each('[1,2,3,4,5,6,7,8,9,10]') a,
      json_each('[1,2,3,4,5]') b; END;",
    )
    .unwrap();
    invalid(add_printer(&mut c, "P", "Holder", "filament"));
    assert!(r::list_printers_with_conn(&c).unwrap().is_empty());
    assert!(db::printers::list_units(&c).unwrap().is_empty());
}

#[test]
fn manager_omitted_image_never_updates_blob_and_web_list_flags_match() {
    let (mut c, _, _, id) = setup();
    let original = db::get_filament_spool(&c, id).unwrap().image_png;
    c.execute_batch("CREATE TEMP TRIGGER reject_image_update BEFORE UPDATE OF image_png ON filament_spools BEGIN SELECT RAISE(ABORT,'image touched'); END;").unwrap();
    let mut changed = spool();
    changed.image_png = None;
    changed.material = "PETG".into();
    let updated = update_filament_spool(&mut c, id, changed, false).unwrap();
    assert_eq!(updated.value.material, "PETG");
    assert_eq!(updated.changes, 1);
    assert_eq!(db::get_filament_spool(&c, id).unwrap().image_png, original);
    let mut no_image = spool();
    no_image.image_png = None;
    let empty_id = add_filament_spool(&mut c, no_image).unwrap().value.id;
    let list = r::list_filament_spools_web(&c).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], id.to_string()); // PETG before PLA
    assert_eq!(list[0]["hasImage"], true);
    assert_eq!(list[1]["id"], empty_id);
    assert_eq!(list[1]["hasImage"], false);
    assert!(list.iter().all(|s| s.get("imagePng").is_none()));
}
fn resin_bottle(c: &mut Connection, remaining: f64) -> i64 {
    let mut s = spool();
    s.kind = "resin".into();
    s.diameter_mm = 0.0;
    s.original_weight_g = 1000.0;
    s.remaining_weight_g = remaining;
    s.image_png = None;
    add_filament_spool(c, s).unwrap().value.id.parse().unwrap()
}
fn spool_count(c: &Connection) -> i64 {
    c.query_row("SELECT count(*) FROM filament_spools", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn restock_creates_full_copies_in_storage_and_leaves_the_template_alone() {
    let (mut c, _, unit, template) = setup();
    load_spool(&mut c, template, unit, 0).unwrap();
    let out = restock_filament_spool(&mut c, template, 3, 750.04, Some(24.5), Some("  Shelf B ".into())).unwrap();
    assert_eq!(out.value.len(), 3);
    assert_eq!(out.changes, 3, "the audit counts every created row");
    for s in &out.value {
        assert_eq!(s.original_weight_g, 750.0);
        assert_eq!(s.remaining_weight_g, 750.0);
        assert_eq!(s.location.as_deref(), Some("Shelf B"));
        assert_eq!((s.unit_id.as_deref(), s.slot_index), (None, None));
        assert_eq!(s.material, "PLA");
        assert_eq!(s.image_png.as_deref(), Some("iVBORw0KGgo="));
    }
    assert_eq!(spool_count(&c), 4);
    let original = db::get_filament_spool(&c, template).unwrap();
    assert_eq!(original.remaining_weight_g, 800.0);
    assert_eq!(original.unit_id, Some(unit));
}
#[test]
fn restock_boundaries_missing_template_and_nothing_created_on_rejection() {
    let (mut c, _, _, template) = setup();
    assert_eq!(restock_filament_spool(&mut c, template, 20, 1.0, None, None).unwrap().value.len(), 20);
    for (count, weight, price) in [
        (0, 1000.0, None),
        (21, 1000.0, None),
        (-1, 1000.0, None),
        (1, 0.0, None),
        (1, 0.04, None),
        (1, -5.0, None),
        (1, f64::NAN, None),
        (1, f64::INFINITY, None),
        (1, MAX_WEIGHT * 2.0, None),
        (1, 1000.0, Some(-1.0)),
        (1, 1000.0, Some(f64::NAN)),
        (1, 1000.0, Some(f64::INFINITY)),
    ] {
        invalid(restock_filament_spool(&mut c, template, count, weight, price, None));
    }
    invalid(restock_filament_spool(&mut c, template, 1, 1000.0, None, Some("x".repeat(121))));
    missing(restock_filament_spool(&mut c, 999_999, 1, 1000.0, None, None));
    assert_eq!(spool_count(&c), 21);
}
#[test]
fn restock_respects_the_total_spool_quota_without_partial_creation() {
    let (mut c, _, _, template) = setup();
    c.execute_batch("WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n<4998) INSERT INTO filament_spools(material,diameter_mm,original_weight_g,remaining_weight_g,created_at) SELECT 'PLA',1.75,1000,1000,'' FROM seq;").unwrap();
    assert_eq!(spool_count(&c), 4999);
    invalid(restock_filament_spool(&mut c, template, 2, 1000.0, None, None));
    assert_eq!(spool_count(&c), 4999);
    restock_filament_spool(&mut c, template, 1, 1000.0, None, None).unwrap();
    assert_eq!(spool_count(&c), 5000);
}
#[test]
fn consume_resin_deducts_rounded_clamps_at_zero_and_is_not_idempotent() {
    let (mut c, _, _, _) = setup();
    let bottle = resin_bottle(&mut c, 100.0);
    let out = consume_resin(&mut c, bottle, 25.44).unwrap();
    assert_eq!(out.value.remaining_weight_g, 74.6);
    assert_eq!(out.value.original_weight_g, 1000.0);
    assert_eq!(out.changes, 1);
    let again = consume_resin(&mut c, bottle, 25.44).unwrap();
    assert_eq!(again.value.remaining_weight_g, 49.2, "a repeated request subtracts again");
    assert_eq!(consume_resin(&mut c, bottle, 500.0).unwrap().value.remaining_weight_g, 0.0);
}
#[test]
fn consume_resin_rejects_bad_amounts_filament_and_missing_bottles() {
    let (mut c, _, _, filament) = setup();
    let bottle = resin_bottle(&mut c, 100.0);
    for amount in [0.0, 0.04, -1.0, f64::NAN, f64::INFINITY, MAX_WEIGHT * 2.0] {
        invalid(consume_resin(&mut c, bottle, amount));
    }
    invalid(consume_resin(&mut c, filament, 10.0));
    missing(consume_resin(&mut c, 999_999, 10.0));
    assert_eq!(db::get_filament_spool(&c, bottle).unwrap().remaining_weight_g, 100.0);
    assert_eq!(db::get_filament_spool(&c, filament).unwrap().remaining_weight_g, 800.0);
}
