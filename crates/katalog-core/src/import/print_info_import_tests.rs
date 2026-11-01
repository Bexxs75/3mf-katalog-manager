//! The print data travels with the file through every import path (desktop reader, web
//! reader, batch insert) and a damaged slice description never blocks the import.
use super::*;
use crate::db::print_info_tests::three_mf_with;
use crate::db::{self, models::PrintInfoRecord};
use crate::threemf::print_info::{SettingsState, SliceState, MAX_PLATES};

const BAMBU: &[u8] = include_bytes!("../../tests/fixtures/synthetic-bambu-project.3mf");
const CREALITY: &[u8] = include_bytes!("../../tests/fixtures/creality-3plates.3mf");

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn indexes(record: &PrintInfoRecord) -> Vec<i64> {
    record.plates.iter().map(|p| p.plate_index).collect()
}

fn import(conn: &mut rusqlite::Connection, file: &NewFile) -> i64 {
    let tx = conn.transaction().unwrap();
    let id = db::insert_file_within_tx(&tx, file).unwrap();
    tx.commit().unwrap();
    id
}

#[test]
fn an_unsliced_bambu_project_imports_with_its_settings_and_plate_rows() {
    let dir = crate::unique_test_dir("print_info_bambu");
    let path = write(&dir, "project.3mf", BAMBU);
    let file = read_model_file(&path, None).unwrap();
    let record = file.print_info.clone().expect("a 3MF carries print data");
    assert_eq!(record.slice_state, SliceState::NoSliceData);
    assert_eq!(record.settings_state, SettingsState::Read);
    assert_eq!(record.printer_model.as_deref(), Some("Bambu Lab X1 Carbon"));
    assert_eq!(indexes(&record), vec![1, 2, 3, 4]);

    let mut conn = db::connect_in_memory().unwrap();
    let id = import(&mut conn, &file);
    let stored = db::get_file(&conn, id).unwrap().unwrap().print_info.unwrap();
    assert_eq!(stored, record);
    assert_eq!(stored.plates[0].first_layer_seconds, Some(123.5));
    assert_eq!(stored.plates[2].first_layer_seconds, None, "plate 3 has no plate_3.json");
}

#[test]
fn the_file_name_never_decides_the_state() {
    let dir = crate::unique_test_dir("print_info_name");
    let plain = read_model_file(&write(&dir, "project.3mf", BAMBU), None).unwrap().print_info.unwrap();
    let named = read_model_file(&write(&dir, "x SLICED.3mf", BAMBU), None).unwrap().print_info.unwrap();
    assert_eq!(named.slice_state, SliceState::NoSliceData);
    assert_eq!(named.plates, plain.plates);
    assert_eq!((named.slice_state, named.settings_state), (plain.slice_state, plain.settings_state));
}

#[test]
fn the_creality_project_and_other_types_import_correctly() {
    let dir = crate::unique_test_dir("print_info_creality");
    let creality = read_model_file(&write(&dir, "c.3mf", CREALITY), None).unwrap();
    let record = creality.print_info.clone().unwrap();
    assert_eq!(record.slice_state, SliceState::NoSliceData);
    assert_eq!(record.printer_model.as_deref(), Some("Creality K1C"));
    assert_eq!(record.plates.len(), 3);
    assert!(record.plates.iter().all(|p| p.print_seconds.is_none() && p.first_layer_seconds.is_none()));

    let stl = write(&dir, "t.stl", b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 10 0 0\nvertex 0 20 0\nendloop\nendfacet\nendsolid x\n");
    assert!(read_model_file(&stl, None).unwrap().print_info.is_none(), "non-3MF files have no print data");
    let mut conn = db::connect_in_memory().unwrap();
    let id = import(&mut conn, &read_model_file(&stl, None).unwrap());
    assert!(db::get_file(&conn, id).unwrap().unwrap().print_info.is_none());
}

#[test]
fn the_web_reader_delivers_the_same_print_data() {
    let dir = crate::unique_test_dir("print_info_web");
    let path = write(&dir, "project.3mf", BAMBU);
    let desktop = read_model_file(&path, None).unwrap().print_info.unwrap();
    let handle = std::fs::File::open(&path).unwrap();
    let web = read_web_model(handle, &path, "project.3mf", WebImportLimits::default()).unwrap().print_info.unwrap();
    assert_eq!(web.plates, desktop.plates);
    assert_eq!((web.slice_state, web.printer_model.clone()), (desktop.slice_state, desktop.printer_model.clone()));
}

#[test]
fn odd_plate_indexes_in_the_slice_info_never_block_the_import() {
    let dir = crate::unique_test_dir("print_info_odd");
    // Index 0, a repeated index, a plate without an index (takes its position 4).
    let odd = r#"<config>
<plate><metadata key="index" value="0"/></plate>
<plate><metadata key="index" value="2"/></plate>
<plate><metadata key="index" value="2"/></plate>
<plate><metadata key="weight" value="1.5"/></plate>
</config>"#;
    let bytes = three_mf_with(&[("Metadata/slice_info.config", odd)]);
    let file = read_model_file(&write(&dir, "odd.3mf", &bytes), None).unwrap();
    let record = file.print_info.clone().unwrap();
    assert_eq!(record.slice_state, SliceState::Sliced);
    assert_eq!(indexes(&record), vec![2, 4]);
    let mut conn = db::connect_in_memory().unwrap();
    let id = import(&mut conn, &file);
    assert_eq!(db::get_file(&conn, id).unwrap().unwrap().print_info.unwrap().plates.len(), 2);

    // 5000 plates: the import succeeds and keeps at most MAX_PLATES rows.
    let mut many = String::from("<config>");
    for _ in 0..5000 {
        many.push_str("<plate><metadata key=\"weight\" value=\"1\"/></plate>");
    }
    many.push_str("</config>");
    let bytes = three_mf_with(&[("Metadata/slice_info.config", &many)]);
    let file = read_model_file(&write(&dir, "many.3mf", &bytes), None).unwrap();
    let id = import(&mut conn, &file);
    let stored = db::get_file(&conn, id).unwrap().unwrap().print_info.unwrap();
    assert_eq!(stored.plates.len(), MAX_PLATES);
}

#[test]
fn a_print_data_fault_in_the_batch_keeps_every_file() {
    // Fault injection: the plate table refuses inserts. Both 3MF files are still imported.
    let dir = crate::unique_test_dir("print_info_fault");
    let first = read_model_file(&write(&dir, "a.3mf", BAMBU), None).unwrap();
    let second = read_model_file(&write(&dir, "b.3mf", CREALITY), None).unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    conn.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON file_print_plates BEGIN SELECT RAISE(ABORT, 'injected fault'); END;").unwrap();
    let tx = conn.transaction().unwrap();
    let a = db::insert_file_within_tx(&tx, &first).unwrap();
    let b = db::insert_file_within_tx(&tx, &second).unwrap();
    tx.commit().unwrap();
    for id in [a, b] {
        let stored = db::get_file(&conn, id).unwrap().unwrap();
        assert!(stored.print_info.is_none(), "not read yet, no half row");
    }
}

const PRUSA: &[u8] = include_bytes!("../../tests/fixtures/synthetic-prusa-project.3mf");

#[test]
fn a_prusa_project_imports_with_its_settings_and_without_plates_or_temperatures() {
    let dir = crate::unique_test_dir("print_info_prusa");
    let path = write(&dir, "prusa.3mf", PRUSA);
    let file = read_model_file(&path, None).unwrap();
    let record = file.print_info.clone().expect("a 3MF carries print data");
    assert_eq!(record.slice_state, SliceState::NoSliceData);
    assert_eq!(record.settings_state, SettingsState::Read);
    assert_eq!(record.printer_model.as_deref(), Some("MK4S"));
    assert_eq!(record.layer_height_mm, Some(0.15));
    assert_eq!(record.nozzle_mm, Some(0.4));
    assert_eq!(record.bed_type, None);
    assert_eq!((record.bed_temp_c, record.nozzle_temp_c), (None, None));
    assert!(record.plates.is_empty());

    // The row is stored (SAVEPOINT path of the import) and read back unchanged.
    let mut conn = db::connect_in_memory().unwrap();
    let id = import(&mut conn, &file);
    assert_eq!(db::get_file(&conn, id).unwrap().unwrap().print_info.unwrap(), record);

    // The web reader delivers the same.
    let handle = std::fs::File::open(&path).unwrap();
    let web = read_web_model(handle, &path, "prusa.3mf", WebImportLimits::default()).unwrap().print_info.unwrap();
    assert_eq!(PrintInfoRecord { read_at: record.read_at.clone(), ..web }, record, "same data, only the read time differs");
}

#[test]
fn a_damaged_prusa_config_still_imports_the_file() {
    let dir = crate::unique_test_dir("print_info_prusa_bad");
    let bytes = three_mf_with(&[("Metadata/Slic3r_PE.config", "not a config at all")]);
    let file = read_model_file(&write(&dir, "bad.3mf", &bytes), None).unwrap();
    let record = file.print_info.clone().unwrap();
    assert_eq!((record.slice_state, record.settings_state), (SliceState::NoSliceData, SettingsState::Unreadable));
    let mut conn = db::connect_in_memory().unwrap();
    let id = import(&mut conn, &file);
    assert_eq!(db::get_file(&conn, id).unwrap().unwrap().print_info.unwrap(), record);
}
