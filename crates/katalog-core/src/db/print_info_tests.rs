//! Storing and loading the print data of a 3MF: validation, savepoint guard, loaders, cleanup.
use super::models::{FileType, MaterialRecord, NewFile, PrintInfoRecord, PrintPlateRecord, ScannedMetadataUpdate};
use super::*;
use crate::threemf::print_info::{SettingsState, SliceState, MAX_PLATES};
use rusqlite::params;
use std::collections::BTreeMap;
use std::io::Write;

/// A tiny 3MF (one triangle) plus the given extra entries, built in memory.
pub(crate) fn three_mf_with(extra: &[(&str, &str)]) -> Vec<u8> {
    let xml = r#"<model unit="millimeter"><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="10" y="0" z="0"/><vertex x="0" y="10" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("3D/3dmodel.model", options).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    for (name, content) in extra {
        zip.start_file(*name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

pub(crate) fn new_file(path: &str, file_type: FileType, print_info: Option<PrintInfoRecord>) -> NewFile {
    NewFile {
        name: path.rsplit('/').next().unwrap().to_string(),
        path: path.to_string(),
        file_type,
        folder_id: None,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes: 1024,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        thumbnail_png: None,
        imported_at: "2026-09-08T12:00:00Z".to_string(),
        file_modified_at: None,
        materials: vec![MaterialRecord { name: "PLA".to_string(), display_color: None }],
        metadata: BTreeMap::new(),
        tags: Vec::new(),
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: None,
        content_hash: None,
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        license: None,
        source: Default::default(),
        queue_position: None,
        favorite: false,
        plate_count: None,
        slice_info_json: None,
        print_info,
    }
}

pub(crate) fn plate(index: i64) -> PrintPlateRecord {
    PrintPlateRecord {
        plate_index: index,
        plate_name: Some(format!("Plate {index}")),
        print_seconds: None,
        first_layer_seconds: Some(100.0 + index as f64),
        plate_bed_type: Some("supertack_plate".to_string()),
    }
}

pub(crate) fn record(slice_state: SliceState, plates: Vec<PrintPlateRecord>) -> PrintInfoRecord {
    PrintInfoRecord {
        reader_version: crate::threemf::print_info::PRINT_INFO_READER_VERSION,
        read_at: "2026-10-31T10:00:00+00:00".to_string(),
        slice_state,
        settings_state: SettingsState::Read,
        printer_model: Some("Test Printer".to_string()),
        bed_type: Some("Test Plate".to_string()),
        layer_height_mm: Some(0.2),
        nozzle_mm: Some(0.4),
        nozzle_mixed: false,
        bed_temp_c: None,
        bed_temp_mixed: false,
        nozzle_temp_c: None,
        nozzle_temp_mixed: false,
        plates,
    }
}

fn count(conn: &rusqlite::Connection, table: &str, file_id: i64) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE file_id = ?1"), params![file_id], |r| r.get(0)).unwrap()
}

fn update_with(print_info: Option<PrintInfoRecord>) -> ScannedMetadataUpdate {
    ScannedMetadataUpdate {
        file_modified_at: None,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        thumbnail_png: None,
        plate_count: None,
        slice_info_json: None,
        materials: Vec::new(),
        metadata: BTreeMap::new(),
        file_size_bytes: 2048,
        content_hash: None,
        source: Default::default(),
        print_info,
    }
}

/// Fault injection for the tests: any insert into the plate table fails.
fn break_plate_inserts(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "CREATE TRIGGER print_plates_fault BEFORE INSERT ON file_print_plates
         BEGIN SELECT RAISE(ABORT, 'injected fault'); END;",
    )
    .unwrap();
}

#[test]
fn validated_makes_every_value_table_safe() {
    let mut raw = record(SliceState::Sliced, vec![plate(1)]);
    raw.layer_height_mm = Some(0.0);
    raw.nozzle_mm = Some(f64::NAN);
    raw.bed_temp_c = Some(-5.0);
    raw.nozzle_temp_c = Some(900.0);
    raw.bed_type = Some(format!("x{}y", '\u{7}'));
    raw.printer_model = Some("p".repeat(10_000));
    raw.plates[0].print_seconds = Some(0);
    raw.plates[0].first_layer_seconds = Some(-1.0);
    let clean = raw.validated();
    assert_eq!(clean.layer_height_mm, None, "0 is never stored as a value");
    assert_eq!(clean.nozzle_mm, None);
    assert_eq!(clean.bed_temp_c, None);
    assert_eq!(clean.nozzle_temp_c, None);
    assert_eq!(clean.bed_type.as_deref(), Some("xy"));
    assert_eq!(clean.printer_model.as_deref().map(str::len), Some(128));
    assert_eq!(clean.plates[0].print_seconds, None);
    assert_eq!(clean.plates[0].first_layer_seconds, None);
    assert_eq!(clean.clone().validated(), clean, "idempotent");
}

#[test]
fn validated_applies_the_state_rules() {
    // Temperatures and print time exist only for a sliced file; a mixed value has no number.
    let mut raw = record(SliceState::NoSliceData, vec![plate(1)]);
    raw.bed_temp_c = Some(60.0);
    raw.nozzle_temp_c = Some(210.0);
    raw.nozzle_temp_mixed = true;
    raw.plates[0].print_seconds = Some(600);
    let clean = raw.validated();
    assert_eq!((clean.bed_temp_c, clean.nozzle_temp_c, clean.nozzle_temp_mixed), (None, None, false));
    assert_eq!(clean.plates[0].print_seconds, None);

    let mut sliced = record(SliceState::Sliced, vec![plate(1)]);
    sliced.nozzle_temp_c = Some(210.0);
    sliced.nozzle_temp_mixed = true;
    sliced.bed_temp_c = Some(60.0);
    sliced.plates[0].print_seconds = Some(600);
    let clean = sliced.validated();
    assert_eq!(clean.nozzle_temp_c, None, "mixed means no single number");
    assert!(clean.nozzle_temp_mixed);
    assert_eq!(clean.bed_temp_c, Some(60.0));
    assert_eq!(clean.plates[0].print_seconds, Some(600));

    // Not recognised / unreadable files have no plate rows.
    for state in [SliceState::NotRecognized, SliceState::Unreadable] {
        assert!(record(state, vec![plate(1), plate(2)]).validated().plates.is_empty());
    }
}

#[test]
fn validated_applies_the_plate_rules() {
    // Index 0 and negative get no row; the first plate with an index wins.
    let mut first = plate(2);
    first.plate_name = Some("first".to_string());
    let mut second = plate(2);
    second.plate_name = Some("second".to_string());
    let clean = record(SliceState::NoSliceData, vec![plate(0), plate(-3), first, second, plate(5)]).validated();
    let kept: Vec<(i64, Option<&str>)> = clean.plates.iter().map(|p| (p.plate_index, p.plate_name.as_deref())).collect();
    assert_eq!(kept, vec![(2, Some("first")), (5, Some("Plate 5"))]);

    // At most MAX_PLATES rows.
    let many: Vec<_> = (1..=(MAX_PLATES as i64 + 100)).map(plate).collect();
    let clean = record(SliceState::NoSliceData, many).validated();
    assert_eq!(clean.plates.len(), MAX_PLATES);
    assert_eq!(clean.plates.last().unwrap().plate_index, MAX_PLATES as i64);
}

#[test]
fn insert_file_writes_print_info_and_the_loaders_return_it() {
    let mut conn = connect_in_memory().unwrap();
    let mut written = record(SliceState::NoSliceData, vec![plate(1), plate(2), plate(4)]);
    written.nozzle_mixed = true;
    written.nozzle_mm = None;
    let id = insert_file(&mut conn, &new_file("/tmp/a.3mf", FileType::ThreeMf, Some(written.clone()))).unwrap();
    let plain = insert_file(&mut conn, &new_file("/tmp/b.3mf", FileType::ThreeMf, None)).unwrap();

    let loaded = get_file(&conn, id).unwrap().unwrap().print_info.expect("print data loaded");
    assert_eq!(loaded, written.clone().validated());
    assert_eq!(loaded.plates.len(), 3);
    assert!(get_file(&conn, plain).unwrap().unwrap().print_info.is_none());

    let by_ids = list_files_by_ids(&conn, &[plain, id]).unwrap();
    assert!(by_ids[0].print_info.is_none());
    assert_eq!(by_ids[1].print_info.as_ref(), Some(&loaded));
    let all = list_files(&conn).unwrap();
    assert_eq!(all.iter().filter(|f| f.print_info.is_some()).count(), 1);
    // The file row itself is untouched by the new rows (materials are still stored).
    assert_eq!(get_file(&conn, id).unwrap().unwrap().materials.len(), 1);
}

#[test]
fn update_scanned_metadata_replaces_the_rows() {
    let mut conn = connect_in_memory().unwrap();
    let id = insert_file(&mut conn, &new_file("/tmp/a.3mf", FileType::ThreeMf, Some(record(SliceState::NoSliceData, vec![plate(1), plate(2), plate(3)])))).unwrap();
    assert_eq!(count(&conn, "file_print_plates", id), 3);

    // Plate count drops from 3 to 2 and the state changes.
    let mut newer = record(SliceState::Sliced, vec![plate(1), plate(2)]);
    newer.printer_model = Some("Other".to_string());
    update_scanned_metadata(&mut conn, id, &update_with(Some(newer))).unwrap();
    assert_eq!(count(&conn, "file_print_plates", id), 2);
    assert_eq!(count(&conn, "file_print_info", id), 1);
    let loaded = get_file(&conn, id).unwrap().unwrap().print_info.unwrap();
    assert_eq!(loaded.slice_state, SliceState::Sliced);
    assert_eq!(loaded.printer_model.as_deref(), Some("Other"));

    // A file that is no longer recognised as a slicer project clears the plate rows.
    update_scanned_metadata(&mut conn, id, &update_with(Some(record(SliceState::NotRecognized, vec![plate(1)])))).unwrap();
    assert_eq!(count(&conn, "file_print_plates", id), 0);
    assert_eq!(get_file(&conn, id).unwrap().unwrap().print_info.unwrap().slice_state, SliceState::NotRecognized);

    // `None` (non-3MF) leaves the stored state alone.
    update_scanned_metadata(&mut conn, id, &update_with(None)).unwrap();
    assert_eq!(count(&conn, "file_print_info", id), 1);
}

#[test]
fn summary_state_is_none_for_a_missing_row_else_the_stored_state() {
    let mut conn = connect_in_memory().unwrap();
    let sliced = insert_file(&mut conn, &new_file("/tmp/a.3mf", FileType::ThreeMf, Some(record(SliceState::Sliced, vec![])))).unwrap();
    let source = insert_file(&mut conn, &new_file("/tmp/b.3mf", FileType::ThreeMf, Some(record(SliceState::NoSliceData, vec![plate(1)])))).unwrap();
    let unread = insert_file(&mut conn, &new_file("/tmp/c.3mf", FileType::ThreeMf, None)).unwrap();
    let stl = insert_file(&mut conn, &new_file("/tmp/d.stl", FileType::Stl, None)).unwrap();
    let summaries = list_file_summaries(&conn).unwrap();
    let state = |id: i64| summaries.iter().find(|s| s.id == id).unwrap().print_info.as_ref().map(|p| (p.reader_version, p.slice_state.clone()));
    assert_eq!(state(sliced), Some((crate::threemf::print_info::PRINT_INFO_READER_VERSION, "sliced".to_string())));
    assert_eq!(state(source), Some((crate::threemf::print_info::PRINT_INFO_READER_VERSION, "no_slice_data".to_string())));
    assert_eq!(state(unread), None);
    assert_eq!(state(stl), None);
}

#[test]
fn deleting_a_file_leaves_no_orphan_rows_and_the_trash_keeps_them() {
    let mut conn = connect_in_memory().unwrap();
    let id = insert_file(&mut conn, &new_file("/tmp/a.3mf", FileType::ThreeMf, Some(record(SliceState::NoSliceData, vec![plate(1), plate(2)])))).unwrap();

    // Trash and restore keep the rows.
    soft_delete_file(&conn, id, Some("/tmp/trash/a.3mf"), "2026-10-01T00:00:00Z").unwrap();
    assert_eq!(count(&conn, "file_print_plates", id), 2);
    restore_file(&conn, id, None).unwrap();
    assert_eq!(count(&conn, "file_print_info", id), 1);
    assert_eq!(count(&conn, "file_print_plates", id), 2);

    // The final delete (what purging the trash ends in) cascades.
    soft_delete_file(&conn, id, None, "2026-01-01T00:00:00Z").unwrap();
    let expired = purge_expired_trash(&conn, "2026-06-01T00:00:00Z").unwrap();
    assert_eq!(expired.len(), 1);
    delete_file(&conn, id).unwrap();
    assert_eq!(count(&conn, "file_print_info", id), 0);
    assert_eq!(count(&conn, "file_print_plates", id), 0);
    let orphans: i64 = conn.query_row("SELECT (SELECT COUNT(*) FROM file_print_info) + (SELECT COUNT(*) FROM file_print_plates)", [], |r| r.get(0)).unwrap();
    assert_eq!(orphans, 0);
}

#[test]
fn the_table_checks_refuse_what_the_validation_never_produces() {
    let mut conn = connect_in_memory().unwrap();
    let id = insert_file(&mut conn, &new_file("/tmp/a.3mf", FileType::ThreeMf, None)).unwrap();
    let head = |extra_col: &str, value: &str| {
        format!(
            "INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state, {extra_col}) VALUES ({id}, 1, 'x', 'sliced', 'read', {value})"
        )
    };
    // Table protection: each of these must fail on its own.
    assert!(conn.execute(&head("layer_height_mm", "-1"), []).is_err());
    assert!(conn.execute(&head("nozzle_mm", "0"), []).is_err());
    assert!(conn
        .execute(&format!("INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state) VALUES ({id}, 1, 'x', 'x', 'read')"), [])
        .is_err());
    conn.execute(&format!("INSERT INTO file_print_info (file_id, reader_version, read_at, slice_state, settings_state) VALUES ({id}, 1, 'x', 'sliced', 'read')"), []).unwrap();
    let plate_insert = |index: i64, seconds: &str| format!("INSERT INTO file_print_plates (file_id, plate_index, print_seconds) VALUES ({id}, {index}, {seconds})");
    assert!(conn.execute(&plate_insert(0, "NULL"), []).is_err(), "plate_index 0");
    assert!(conn.execute(&plate_insert(1, "0"), []).is_err(), "print_seconds 0");
    conn.execute(&plate_insert(1, "NULL"), []).unwrap();
    assert!(conn.execute(&plate_insert(1, "NULL"), []).is_err(), "duplicate (file_id, plate_index)");

    // And the writer never produces such values, whatever the record holds.
    let mut hostile = record(SliceState::Sliced, vec![plate(0), plate(7), plate(7), plate(-1)]);
    hostile.layer_height_mm = Some(-1.0);
    hostile.nozzle_mm = Some(0.0);
    hostile.plates[1].print_seconds = Some(0);
    let other = insert_file(&mut conn, &new_file("/tmp/b.3mf", FileType::ThreeMf, Some(hostile))).unwrap();
    assert_eq!(count(&conn, "file_print_info", other), 1, "written, not rolled back");
    assert_eq!(count(&conn, "file_print_plates", other), 1);
}

#[test]
fn a_failing_print_data_write_never_stops_the_import() {
    let mut conn = connect_in_memory().unwrap();
    break_plate_inserts(&conn);
    let tx = conn.transaction().unwrap();
    // The plate insert fails: the file must still be stored, without any print data.
    let id = insert_file_within_tx(&tx, &new_file("/tmp/a.3mf", FileType::ThreeMf, Some(record(SliceState::NoSliceData, vec![plate(1)])))).unwrap();
    let next = insert_file_within_tx(&tx, &new_file("/tmp/b.3mf", FileType::ThreeMf, None)).unwrap();
    tx.commit().unwrap();
    assert!(get_file(&conn, id).unwrap().is_some(), "the file is saved");
    assert!(get_file(&conn, next).unwrap().is_some(), "the batch goes on");
    assert_eq!(count(&conn, "file_print_info", id), 0, "state reads as not read yet: no half-written row");
    assert_eq!(count(&conn, "file_print_plates", id), 0);
    assert_eq!(list_file_summaries(&conn).unwrap().iter().find(|s| s.id == id).unwrap().print_info, None);
}

#[test]
fn a_failing_rescan_write_keeps_the_old_print_data() {
    let mut conn = connect_in_memory().unwrap();
    let id = insert_file(&mut conn, &new_file("/tmp/a.3mf", FileType::ThreeMf, Some(record(SliceState::NoSliceData, vec![plate(1), plate(2)])))).unwrap();
    let before = get_file(&conn, id).unwrap().unwrap().print_info.unwrap();
    break_plate_inserts(&conn);

    // The rest of the rescan (file size here) still lands; the print data stays as it was.
    update_scanned_metadata(&mut conn, id, &update_with(Some(record(SliceState::Sliced, vec![plate(1), plate(2), plate(3)])))).unwrap();
    let after = get_file(&conn, id).unwrap().unwrap();
    assert_eq!(after.file_size_bytes, 2048);
    assert_eq!(after.print_info.as_ref(), Some(&before), "old rows survive, never half replaced");
    assert_eq!(count(&conn, "file_print_plates", id), 2);
}
