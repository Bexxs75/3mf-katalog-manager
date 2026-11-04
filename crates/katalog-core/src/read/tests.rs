use super::*;
use crate::db::models::FileType;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

struct Catalog {
    dir: PathBuf,
    path: PathBuf,
    file_id: i64,
    trash_id: i64,
    collection_id: i64,
    parent: i64,
    child: i64,
}
impl Drop for Catalog {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn one_stl() -> Vec<u8> {
    let mut bytes = vec![0u8; 80];
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 12]);
    for v in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
        for c in v {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&[0u8; 2]);
    bytes
}
fn catalog() -> Catalog {
    let dir = crate::unique_test_dir("core_read_catalog");
    let path = dir.join("catalog.db");
    let conn = db::connect(&path).unwrap();
    let parent = db::insert_folder_with_parent(&conn, "root", None, dir.to_str().unwrap()).unwrap();
    let child = db::insert_folder_with_parent(
        &conn,
        "child",
        Some(parent),
        dir.join("child").to_str().unwrap(),
    )
    .unwrap();
    let model = dir.join("one.stl");
    std::fs::write(&model, one_stl()).unwrap();
    let file_id =
        db::test_insert_minimal_file(&conn, model.to_str().unwrap(), Some(child)).unwrap();
    conn.execute("UPDATE files SET file_type='stl', thumbnail_png=x'010203', render_snapshot_png=x'0405', custom_image_png=x'06', volume_cm3=10, slice_info_json=?1 WHERE id=?2",
        rusqlite::params![r#"{"total_weight_g":50,"plates":[{"plate_index":1,"weight_g":50,"filaments":[{"filament_type":"PLA","color":null,"used_g":50,"used_m":1}]}]}"#, file_id]).unwrap();
    db::add_tag_to_file(&conn, file_id, "Beispiel").unwrap();
    let trash_id =
        db::test_insert_minimal_file(&conn, "/missing/original.stl", Some(child)).unwrap();
    let trash = dir.join("trash.stl");
    std::fs::write(&trash, one_stl()).unwrap();
    db::soft_delete_file(&conn, trash_id, Some(trash.to_str().unwrap()), "2026-10-01").unwrap();
    let collection_id = db::create_collection(&conn, "Sammlung", "now").unwrap();
    db::add_file_to_collection(&conn, collection_id, file_id, 0).unwrap();
    db::insert_print_log_entry(
        &conn,
        &db::models::NewPrintLogEntry {
            file_id,
            printed_at: "2026-10-16".into(),
            note: Some("Druck".into()),
            photo_png: Some(vec![7, 8]),
        },
    )
    .unwrap();
    db::insert_filament_spool(
        &conn,
        &db::models::NewFilamentSpool {
            color_hexes: Vec::new(),
            material: "PLA".into(),
            manufacturer: None,
            color: None,
            location: None,
            diameter_mm: 1.75,
            original_weight_g: 1000.,
            remaining_weight_g: 900.,
            price: Some(20.),
            image_png: None,
            color_hex: None,
            kind: "filament".into(),
        },
    )
    .unwrap();
    drop(conn);
    Catalog {
        dir,
        path,
        file_id,
        trash_id,
        collection_id,
        parent,
        child,
    }
}
fn directory_entries(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut entries = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

#[test]
fn all_read_services_preserve_database_hash_and_directory_end_state() {
    // All files and rows are fully prepared before the first measurement.
    let c = catalog();
    let before = Sha256::digest(std::fs::read(&c.path).unwrap());
    let entries = directory_entries(&c.dir);
    let conn = db::open_read_only(&c.path).unwrap();
    let summaries = list_file_summaries(&conn).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].estimated_weight_g, Some(50.));
    assert_eq!(summaries[0].weight_source, "slicer");
    let images = load_file_images(&conn, &[c.file_id]).unwrap();
    let images = encode_file_images(images);
    assert_eq!(images[0].id, c.file_id.to_string());
    assert_eq!(
        images[0].thumbnail_image.as_deref(),
        Some("data:image/png;base64,AQID")
    );
    assert_eq!(
        images[0].render_snapshot_image.as_deref(),
        Some("data:image/png;base64,BAU=")
    );
    assert_eq!(
        images[0].custom_image.as_deref(),
        Some("data:image/png;base64,Bg==")
    );
    assert_eq!(
        list_all_file_tags(&conn).unwrap()[&c.file_id.to_string()],
        ["Beispiel"]
    );
    let details = list_files_by_ids(&conn, vec![c.file_id.to_string()]).unwrap();
    assert_eq!(
        details[0].cost_estimate.as_ref().unwrap().total_cost,
        Some(1.)
    );
    let tags = list_tag_counts(&conn).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(
        (&tags[0].label, tags[0].count),
        (&"Beispiel".to_string(), 1)
    );
    let log = list_print_log_entries(&conn, c.file_id.to_string()).unwrap();
    assert_eq!(log[0].note.as_deref(), Some("Druck"));
    assert_eq!(
        log[0].photo_image.as_deref(),
        Some("data:image/png;base64,Bwg=")
    );
    let folders = list_folders(&conn).unwrap();
    for id in [c.parent, c.child] {
        assert_eq!(
            folders
                .iter()
                .find(|f| f.id == id.to_string())
                .unwrap()
                .count,
            1
        );
    }
    let collections = list_collections(&conn).unwrap();
    assert_eq!(collections[0].name, "Sammlung");
    assert_eq!(collections[0].model_count, 1);
    assert_eq!(
        list_collection_files(&conn, c.collection_id.to_string()).unwrap()[0].id,
        c.file_id.to_string()
    );
    assert_eq!(list_trash(&conn).unwrap()[0].id, c.trash_id.to_string());
    let source = load_geometry_source(&conn, &c.file_id.to_string()).unwrap();
    assert_eq!(source.file_type, db::models::FileType::Stl);
    // Fixed bytes generated by the old desktop loader/encoder, before extraction.
    let golden = include_bytes!("../../tests/fixtures/geometry-one-stl.bin");
    assert_eq!(get_model_geometry(&source.path, None).unwrap(), golden);
    let trashed = load_geometry_source(&conn, &c.trash_id.to_string()).unwrap();
    assert_eq!(trashed.path, c.dir.join("trash.stl"));
    assert_eq!(
        get_model_geometry(&trashed.path, Some(std::slice::from_ref(&c.dir))).unwrap(),
        golden
    );
    drop(conn);
    assert_eq!(Sha256::digest(std::fs::read(&c.path).unwrap()), before);
    assert_eq!(directory_entries(&c.dir), entries);
}

#[test]
fn geometry_roots_reject_outside_and_allow_none() {
    let c = catalog();
    let other = c.dir.join("allowed");
    std::fs::create_dir(&other).unwrap();
    let model = c.dir.join("one.stl");
    let error = get_model_geometry(&model, Some(&[other])).unwrap_err();
    assert!(error.expected);
    assert_eq!(error.code, Some(GeometryErrorCode::Unreadable));
    assert_eq!(error.message, "dlPathNotAllowed");
    assert!(error.params.is_empty());
    assert!(get_model_geometry(&model, Some(&[])).is_err());
    assert_eq!(
        get_model_geometry(&model, None).unwrap(),
        include_bytes!("../../tests/fixtures/geometry-one-stl.bin")
    );
}

#[cfg(unix)]
#[test]
fn geometry_roots_resolve_symlinks_and_parent_traversal() {
    let c = catalog();
    let allowed = c.dir.join("allowed");
    std::fs::create_dir(&allowed).unwrap();
    std::os::unix::fs::symlink(c.dir.join("one.stl"), allowed.join("escape.stl")).unwrap();
    for path in [allowed.join("escape.stl"), allowed.join("../one.stl")] {
        assert!(get_model_geometry(&path, Some(std::slice::from_ref(&allowed))).is_err());
        assert!(get_model_geometry(&path, None).is_ok());
    }
}

#[test]
fn read_validation_and_geometry_classification_are_preserved() {
    let c = catalog();
    let conn = db::open_read_only(&c.path).unwrap();
    assert!(list_files_by_ids(&conn, vec!["invalid".into()]).is_err());
    assert!(list_collection_files(&conn, "invalid".into()).is_err());
    assert!(list_print_log_entries(&conn, "invalid".into()).is_err());
    assert!(load_file_images(&conn, &[c.file_id; 201]).is_err());
    assert!(load_geometry_source(&conn, "invalid").is_err());
    let missing = load_geometry_source(&conn, "99999").unwrap_err();
    assert!(missing.expected);
    assert_eq!(missing.code, Some(GeometryErrorCode::NotFound));
    let unsupported = get_model_geometry(&c.dir.join("x.step"), None).unwrap_err();
    assert!(unsupported.expected);
    assert_eq!(unsupported.code, Some(GeometryErrorCode::Unsupported));
    let missing = get_model_geometry(&c.dir.join("missing.stl"), None).unwrap_err();
    assert!(missing.expected);
    assert_eq!(missing.code, Some(GeometryErrorCode::NotFound));
    std::fs::write(c.dir.join("invalid.stl"), b"invalid").unwrap();
    let invalid = get_model_geometry(&c.dir.join("invalid.stl"), None).unwrap_err();
    assert!(!invalid.expected);
    assert_eq!(invalid.code, Some(GeometryErrorCode::Unreadable));
}

#[test]
fn summary_keeps_card_values_after_rename_move_restore_and_metadata_reload() {
    let conn = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&conn, "/tmp/card.3mf", None).unwrap();
    conn.execute("INSERT INTO file_materials (file_id, name, display_color) VALUES (?1, 'Kirschrot', '#ff0000')", [id]).unwrap();
    conn.execute(
        "UPDATE files SET volume_cm3 = 10, slice_info_json = ?1 WHERE id = ?2",
        rusqlite::params![r#"{"total_weight_g":30.46,"plates":[]}"#, id],
    )
    .unwrap();
    // Persisted metadata is authoritative after every refresh, even for an unseen model.
    for update in [
        "UPDATE files SET name = 'renamed.3mf'",
        "UPDATE files SET path = '/tmp/moved/card.3mf'",
        "UPDATE files SET deleted_at = '2026-01-01'",
        "UPDATE files SET deleted_at = NULL",
    ] {
        conn.execute(update, []).unwrap();
        let summaries = db::list_file_summaries(&conn).unwrap();
        if update.contains("deleted_at = '") {
            assert!(summaries.is_empty());
            continue;
        }
        let dto = summary_to_dto(summaries.into_iter().next().unwrap());
        assert_eq!(dto.materials[0].name, "Kirschrot");
        assert_eq!(dto.estimated_weight_g, Some(30.46));
        assert_eq!(dto.weight_source, "slicer");
    }
    conn.execute("UPDATE files SET slice_info_json = NULL", [])
        .unwrap();
    let dto = summary_to_dto(db::list_file_summaries(&conn).unwrap().remove(0));
    assert_eq!(dto.weight_source, "estimated");
    assert_eq!(
        dto.estimated_weight_g,
        estimate_weight_g(Some(10.0), Some("Kirschrot"))
    );
}

#[test]
fn summary_serialization_has_presence_flags_and_no_image_fields() {
    let summary = FileSummaryDto {
        source_platform: None,
        source_model_id: None,
        source_profile_id: None,
        license: None,
        materials: Vec::new(),
        estimated_weight_g: None,
        weight_source: "estimated".into(),
        id: "1".into(),
        name: "test".into(),
        path: "/tmp/test.stl".into(),
        file_type: "stl".into(),
        folder_id: String::new(),
        file_size_bytes: 10,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        imported_at: String::new(),
        file_modified_at: Some("2023-11-14T22:13:20+00:00".into()),
        print_status: "not_printed".into(),
        favorite: false,
        queue_position: None,
        has_thumbnail: true,
        has_render_snapshot: false,
        creator: None,
        last_viewed_at: None,
        content_hash: None,
        print_data_state: "none".into(),
        print_facts: None,
    };
    let json = serde_json::to_value(summary).unwrap();
    assert_eq!(json["fileModifiedAt"], "2023-11-14T22:13:20+00:00");
    assert_eq!(json["hasThumbnail"], true);
    assert_eq!(json["hasRenderSnapshot"], false);
    assert!(json.get("thumbnailImage").is_none());
    assert!(json.get("renderSnapshotImage").is_none());
    assert!(json.get("customImage").is_none());
}

#[test]
fn summary_print_data_state_follows_the_reader_version_of_each_file_type() {
    use crate::db::print_info_tests::{new_file, plate, record};
    use crate::threemf::print_info::{SliceState, PRINT_INFO_READER_VERSION};

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let with_version = |version: u32| {
        let mut r = record(SliceState::Sliced, vec![plate(1)]);
        r.reader_version = version;
        Some(r)
    };
    let rows = [
        ("current.gcode", FileType::Gcode, with_version(crate::gcode::GCODE_READER_VERSION)),
        // The number of the 3MF reader means nothing for a G-code row.
        ("threemf-version.gcode", FileType::Gcode, with_version(PRINT_INFO_READER_VERSION)),
        ("never-read.gcode", FileType::Gcode, None),
        ("current.3mf", FileType::ThreeMf, with_version(PRINT_INFO_READER_VERSION)),
        // Likewise the other way round: the G-code number does not make a 3MF row current.
        ("gcode-version.3mf", FileType::ThreeMf, with_version(crate::gcode::GCODE_READER_VERSION)),
    ];
    for (name, file_type, info) in rows {
        crate::db::insert_file(&mut conn, &new_file(&format!("/tmp/{name}"), file_type, info)).unwrap();
    }
    let summaries = list_file_summaries(&conn).unwrap();
    let state = |name: &str| summaries.iter().find(|s| s.name == name).unwrap().print_data_state.clone();
    assert_eq!(state("current.gcode"), "sliced");
    assert_eq!(state("threemf-version.gcode"), "unread");
    assert_eq!(state("never-read.gcode"), "unread");
    assert_eq!(state("current.3mf"), "sliced");
    assert_eq!(state("gcode-version.3mf"), "unread");
    let facts = |name: &str| summaries.iter().find(|s| s.name == name).unwrap().print_facts.is_some();
    assert!(facts("current.gcode") && !facts("threemf-version.gcode"));
}

#[test]
fn summary_print_data_state_tells_unread_from_the_stored_state() {
    use crate::db::print_info_tests::{new_file, plate, record};
    use crate::threemf::print_info::SliceState;

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let mut stale = record(SliceState::Sliced, vec![plate(1)]);
    stale.reader_version = 99;
    let rows = [
        ("sliced.3mf", FileType::ThreeMf, Some(record(SliceState::Sliced, vec![plate(1)]))),
        ("source.3mf", FileType::ThreeMf, Some(record(SliceState::NoSliceData, vec![plate(1)]))),
        ("unknown.3mf", FileType::ThreeMf, Some(record(SliceState::NotRecognized, vec![]))),
        ("broken.3mf", FileType::ThreeMf, Some(record(SliceState::Unreadable, vec![]))),
        ("stale.3mf", FileType::ThreeMf, Some(stale)),
        ("never-read.3mf", FileType::ThreeMf, None),
        ("model.stl", FileType::Stl, None),
    ];
    for (name, file_type, info) in rows {
        crate::db::insert_file(&mut conn, &new_file(&format!("/tmp/{name}"), file_type, info)).unwrap();
    }
    let states: std::collections::BTreeMap<String, String> = list_file_summaries(&conn)
        .unwrap()
        .into_iter()
        .map(|s| (s.name, s.print_data_state))
        .collect();
    let expected = [
        ("sliced.3mf", "sliced"),
        ("source.3mf", "no_slice_data"),
        ("unknown.3mf", "not_recognized"),
        ("broken.3mf", "unreadable"),
        ("stale.3mf", "unread"),
        ("never-read.3mf", "unread"),
        ("model.stl", "none"),
    ];
    for (name, state) in expected {
        assert_eq!(states[name], state, "{name}");
    }
    let json = serde_json::to_value(list_file_summaries(&conn).unwrap().remove(0)).unwrap();
    assert!(json["printDataState"].is_string(), "camelCase key");
}

#[test]
fn add_and_list_print_log_entry_roundtrips_through_dto() {
    let mut conn = crate::db::connect_in_memory().expect("connect");

    // add_print_log_entry needs a real file_id.

    let new_file = crate::db::models::NewFile {
        source: Default::default(),
        name: "cube.3mf".to_string(),
        path: "/tmp/print-log-test-cube.3mf".to_string(),
        file_type: FileType::ThreeMf,
        folder_id: None,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes: 0,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        thumbnail_png: None,
        imported_at: "2026-09-13T00:00:00Z".to_string(),
        file_modified_at: None,
        materials: Vec::new(),
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
        queue_position: None,
        favorite: false,
        plate_count: None,
        slice_info_json: None,
        print_info: None,
    };
    let file_id = crate::db::insert_file(&mut conn, &new_file).expect("insert file");

    let entry = crate::db::models::NewPrintLogEntry {
        file_id,
        printed_at: "2026-09-13T10:00:00Z".to_string(),
        note: Some("Testdruck".to_string()),
        photo_png: None,
    };
    let id = crate::db::insert_print_log_entry(&conn, &entry).expect("insert entry");

    let entries = crate::db::list_print_log_entries(&conn, file_id).expect("list entries");
    let dtos: Vec<PrintLogEntryDto> = entries.into_iter().map(print_log_entry_to_dto).collect();

    assert_eq!(dtos.len(), 1);
    assert_eq!(dtos[0].id, id.to_string());
    assert_eq!(dtos[0].note.as_deref(), Some("Testdruck"));
    assert_eq!(dtos[0].photo_image, None);
}

#[test]
fn suggested_auto_tags_match_the_tagging_rules_for_the_stored_file() {
    let conn = db::connect_in_memory().unwrap();
    let id = db::test_insert_minimal_file(&conn, "/Kabelhalter_v3_final.3mf", None).unwrap();
    let file = db::get_file(&conn, id).unwrap().unwrap();
    let expected = crate::tagging::suggest_tags(&crate::tagging::TaggingContext {
        file_name: &file.name,
        dimensions_mm: file.dimensions_mm,
        object_count: file.object_count,
        materials: &file.materials,
    });
    assert_eq!(expected, vec!["kabelhalter".to_string()]);
    assert_eq!(suggested_auto_tags_with_conn(&conn, id).unwrap(), expected);
    // Tags added by hand are not part of the suggestion.
    db::add_tag_to_file(&conn, id, "mein-projekt").unwrap();
    assert_eq!(suggested_auto_tags_with_conn(&conn, id).unwrap(), expected);
}

#[test]
fn suggested_auto_tags_for_an_unknown_file_is_an_expected_error() {
    let conn = db::connect_in_memory().unwrap();
    let e = suggested_auto_tags_with_conn(&conn, 999).unwrap_err();
    assert!(e.expected);
    assert_eq!(e.message, "fileNotFound");
}

#[test]
fn blocked_auto_tags_are_listed_sorted_as_stored() {
    let conn = db::connect_in_memory().unwrap();
    assert!(list_blocked_auto_tags(&conn).unwrap().is_empty());
    db::blocked_tags::block_auto_tag(&conn, "Zahnrad").unwrap();
    db::blocked_tags::block_auto_tag(&conn, "Multicolor").unwrap();
    assert_eq!(list_blocked_auto_tags(&conn).unwrap(), vec!["mehrfarbig", "zahnrad"]);
}

#[test]
fn image_budget_defers_later_entries_but_always_serves_the_first() {
    let entry = |id: i64, size: usize| db::FileImages {
        id,
        thumbnail_png: None,
        render_snapshot_png: None,
        custom_image_png: Some(vec![1; size]),
    };
    let served = encode_file_images_budgeted(vec![entry(1, 300), entry(2, 300), entry(3, 10), entry(4, 300)], 500);
    let deferred: Vec<_> = served.iter().map(|e| e.deferred).collect();
    assert_eq!(deferred, [false, true, true, true]);
    assert!(served[0].custom_image.is_some());
    assert!(served[1].custom_image.is_none());
    assert_eq!(served.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["1", "2", "3", "4"]);
    // The first entry is served even when it alone exceeds the budget.
    assert!(!encode_file_images_budgeted(vec![entry(1, 900)], 10)[0].deferred);
    let json = serde_json::to_value(&served[0]).unwrap();
    assert!(json.get("deferred").is_none());
}

#[test]
fn a_row_of_the_first_reader_version_counts_as_unread_so_prusa_projects_can_be_read_again() {
    use crate::db::print_info_tests::{new_file, record};
    use crate::threemf::print_info::{SliceState, PRINT_INFO_READER_VERSION};

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let mut old = record(SliceState::NotRecognized, vec![]);
    old.reader_version = 1;
    let mut current = record(SliceState::NotRecognized, vec![]);
    current.reader_version = PRINT_INFO_READER_VERSION;
    for (name, info) in [("old.3mf", old), ("current.3mf", current)] {
        crate::db::insert_file(&mut conn, &new_file(&format!("/tmp/{name}"), FileType::ThreeMf, Some(info))).unwrap();
    }
    let states: std::collections::BTreeMap<String, String> =
        list_file_summaries(&conn).unwrap().into_iter().map(|s| (s.name, s.print_data_state)).collect();
    assert_eq!(states["old.3mf"], "unread");
    assert_eq!(states["current.3mf"], "not_recognized");
}

// Print facts of the list

const PRINT_FACTS_GOLDEN: &str = include_str!("../../tests/fixtures/print-facts-golden.json");

fn golden_cases() -> Vec<serde_json::Value> {
    let doc: serde_json::Value = serde_json::from_str(PRINT_FACTS_GOLDEN).expect("the golden file is JSON");
    doc["cases"].as_array().expect("cases").clone()
}

/// The catalog record a golden case describes.
fn golden_record(case: &serde_json::Value) -> crate::db::models::PrintInfoRecord {
    use crate::db::models::{PrintInfoRecord, PrintPlateRecord};
    use crate::threemf::print_info::{SettingsState, SliceState};
    let r = &case["record"];
    let text = |v: &serde_json::Value| v.as_str().map(str::to_string);
    PrintInfoRecord {
        reader_version: r["readerVersion"].as_u64().unwrap() as u32,
        read_at: "2026-10-31T10:00:00+00:00".to_string(),
        slice_state: SliceState::parse(r["sliceState"].as_str().unwrap()).unwrap(),
        settings_state: SettingsState::parse(r["settingsState"].as_str().unwrap()).unwrap(),
        printer_model: text(&r["printerModel"]),
        bed_type: text(&r["bedType"]),
        layer_height_mm: r["layerHeightMm"].as_f64(),
        nozzle_mm: r["nozzleMm"].as_f64(),
        nozzle_mixed: r["nozzleMixed"].as_bool().unwrap(),
        bed_temp_c: None,
        bed_temp_mixed: false,
        nozzle_temp_c: None,
        nozzle_temp_mixed: false,
        plates: r["plates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PrintPlateRecord {
                plate_index: p["plateIndex"].as_i64().unwrap(),
                plate_name: text(&p["plateName"]),
                print_seconds: p["printSeconds"].as_i64(),
                first_layer_seconds: p["firstLayerSeconds"].as_f64(),
                plate_bed_type: text(&p["plateBedType"]),
            })
            .collect(),
        filament_colors: r["filamentColors"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect(),
    }
}

#[test]
fn the_golden_file_matches_the_list_facts_and_the_detail_print_info() {
    let mut conn = crate::db::connect_in_memory().expect("connect");
    let cases = golden_cases();
    assert!(cases.len() >= 8, "the golden file holds its cases");
    let mut ids = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let path = format!("/tmp/golden-{index}.3mf");
        let file = crate::db::print_info_tests::new_file(&path, FileType::ThreeMf, Some(golden_record(case)));
        ids.push(crate::db::insert_file(&mut conn, &file).unwrap());
    }
    let summaries = list_file_summaries(&conn).unwrap();
    for (case, id) in cases.iter().zip(&ids) {
        let name = case["name"].as_str().unwrap();
        let summary = summaries.iter().find(|s| s.id == id.to_string()).expect("listed");
        let json = serde_json::to_value(summary).unwrap();
        let expected = &case["expected"]["printFacts"];
        if expected.is_null() {
            assert!(json.get("printFacts").is_none(), "{name}: the block is left out, not null");
        } else {
            assert_eq!(&json["printFacts"], expected, "{name}: list facts");
        }
        let detail = crate::db::get_file(&conn, *id).unwrap().unwrap().print_info.expect("stored");
        assert_eq!(serde_json::to_value(PrintInfoDto::from(detail)).unwrap(), case["expected"]["printInfo"], "{name}: detail print info");
    }
}

#[test]
fn print_facts_are_left_out_unless_a_current_row_says_sliced_or_no_slice_data() {
    use crate::db::print_info_tests::{new_file, plate, record};
    use crate::threemf::print_info::SliceState;

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let mut older = record(SliceState::Sliced, vec![plate(1)]);
    older.reader_version = 2;
    let rows = [
        ("sliced.3mf", FileType::ThreeMf, Some(record(SliceState::Sliced, vec![plate(1)]))),
        ("nodata.3mf", FileType::ThreeMf, Some(record(SliceState::NoSliceData, vec![]))),
        ("not-recognised.3mf", FileType::ThreeMf, Some(record(SliceState::NotRecognized, vec![]))),
        ("unreadable.3mf", FileType::ThreeMf, Some(record(SliceState::Unreadable, vec![]))),
        ("older.3mf", FileType::ThreeMf, Some(older)),
        ("unread.3mf", FileType::ThreeMf, None),
        ("model.stl", FileType::Stl, None),
    ];
    for (name, file_type, info) in rows {
        crate::db::insert_file(&mut conn, &new_file(&format!("/tmp/{name}"), file_type, info)).unwrap();
    }
    let facts: std::collections::BTreeMap<String, bool> = list_file_summaries(&conn)
        .unwrap()
        .into_iter()
        .map(|s| (s.name, s.print_facts.is_some()))
        .collect();
    let expected: std::collections::BTreeMap<String, bool> = [
        ("sliced.3mf", true), ("nodata.3mf", true), ("not-recognised.3mf", false), ("unreadable.3mf", false),
        ("older.3mf", false), ("unread.3mf", false), ("model.stl", false),
    ].into_iter().map(|(n, v)| (n.to_string(), v)).collect();
    assert_eq!(facts, expected);
}

#[test]
fn print_time_sum_is_complete_only_when_every_plate_row_has_a_time() {
    use crate::db::print_info_tests::{new_file, plate, record};
    use crate::threemf::print_info::SliceState;

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let timed = |index: i64, seconds: Option<i64>| {
        let mut p = plate(index);
        p.print_seconds = seconds;
        p
    };
    let cases = [
        ("all.3mf", vec![timed(1, Some(600)), timed(2, Some(900))]),
        ("some.3mf", vec![timed(1, Some(600)), timed(2, None)]),
        ("none.3mf", vec![timed(1, None), timed(2, None)]),
        ("rows-none.3mf", vec![]),
    ];
    for (name, plates) in &cases {
        crate::db::insert_file(&mut conn, &new_file(&format!("/tmp/{name}"), FileType::ThreeMf, Some(record(SliceState::Sliced, plates.clone())))).unwrap();
    }
    let facts: std::collections::BTreeMap<String, PrintFactsDto> = list_file_summaries(&conn)
        .unwrap()
        .into_iter()
        .map(|s| (s.name, s.print_facts.expect("facts")))
        .collect();
    let seen = |name: &str| (facts[name].plate_count, facts[name].print_seconds, facts[name].print_seconds_complete);
    assert_eq!(seen("all.3mf"), (2, Some(1500), true));
    assert_eq!(seen("some.3mf"), (2, Some(600), false), "a partial sum is not a total");
    assert_eq!(seen("none.3mf"), (2, None, false), "no time at all is unknown, never 0");
    assert_eq!(seen("rows-none.3mf"), (0, None, false), "no plate rows is not complete");
}

#[test]
fn the_list_json_has_camel_case_names_and_leaves_missing_fields_out() {
    use crate::db::print_info_tests::{new_file, record};
    use crate::threemf::print_info::SliceState;

    let mut conn = crate::db::connect_in_memory().expect("connect");
    let mut bare = record(SliceState::NoSliceData, vec![]);
    bare.printer_model = None;
    bare.layer_height_mm = None;
    bare.nozzle_mm = None;
    crate::db::insert_file(&mut conn, &new_file("/tmp/bare.3mf", FileType::ThreeMf, Some(bare))).unwrap();
    let mut full = record(SliceState::NoSliceData, vec![]);
    full.nozzle_mm = None;
    full.nozzle_mixed = true;
    full.filament_colors = vec!["#00ff00".into()];
    crate::db::insert_file(&mut conn, &new_file("/tmp/full.3mf", FileType::ThreeMf, Some(full))).unwrap();

    let by_name: std::collections::BTreeMap<String, serde_json::Value> = list_file_summaries(&conn)
        .unwrap()
        .into_iter()
        .map(|s| (s.name.clone(), serde_json::to_value(s).unwrap()))
        .collect();
    assert_eq!(by_name["bare.3mf"]["printFacts"], serde_json::json!({"plateCount": 0}), "every missing value is left out");
    assert_eq!(
        by_name["full.3mf"]["printFacts"],
        serde_json::json!({"printerModel": "Test Printer", "layerHeightMm": 0.2, "nozzleMixed": true, "plateCount": 0, "colors": ["#00ff00"]})
    );
}

const CREALITY_3PLATES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/creality-3plates.3mf");

fn plate_overview_of(path: &std::path::Path, roots: Option<&[PathBuf]>, previews: bool) -> ReadResult<PlateOverviewDto> {
    read_plate_overview(path, FileType::ThreeMf, roots, previews)
}

#[test]
fn plate_overview_of_the_creality_file_has_objects_one_three_one_and_three_pictures() {
    let path = std::path::Path::new(CREALITY_3PLATES);
    let with = plate_overview_of(path, None, true).unwrap();
    assert_eq!(with.plates.iter().map(|p| (p.plate_index, p.object_count)).collect::<Vec<_>>(), [(1, Some(1)), (2, Some(3)), (3, Some(1))]);
    for plate in &with.plates {
        assert!(plate.preview.as_deref().unwrap().starts_with("data:image/png;base64,iVBORw0KGgo"), "plate {}", plate.plate_index);
    }
    let without = plate_overview_of(path, None, false).unwrap();
    assert!(without.plates.iter().all(|p| p.preview.is_none()));
    assert_eq!(without.plates.len(), 3);
    // Left-out fields stay left out on the wire.
    let json = serde_json::to_value(&without).unwrap();
    assert_eq!(json, serde_json::json!({"plates":[{"plateIndex":1,"objectCount":1},{"plateIndex":2,"objectCount":3},{"plateIndex":3,"objectCount":1}]}));
}

#[test]
fn plate_overview_without_plate_list_or_without_3mf_gives_an_empty_list() {
    let c = catalog();
    // An STL is not even opened; a Prusa project and a text file masquerading as 3MF have no plate list.
    assert_eq!(read_plate_overview(&c.dir.join("missing.stl"), FileType::Stl, None, true).unwrap(), PlateOverviewDto::default());
    let prusa = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic-prusa-project.3mf");
    assert!(plate_overview_of(&prusa, None, true).unwrap().plates.is_empty());
    let text = c.dir.join("not-a-zip.3mf");
    std::fs::write(&text, b"hello").unwrap();
    assert!(plate_overview_of(&text, None, true).unwrap().plates.is_empty());
}

#[test]
fn plate_overview_reports_missing_files_and_refuses_paths_outside_the_roots() {
    let c = catalog();
    let missing = plate_overview_of(&c.dir.join("missing.3mf"), None, true).unwrap_err();
    assert_eq!(missing.code, Some(GeometryErrorCode::NotFound));
    let missing_in_root = plate_overview_of(&c.dir.join("missing.3mf"), Some(std::slice::from_ref(&c.dir)), true).unwrap_err();
    assert_eq!(missing_in_root.code, Some(GeometryErrorCode::NotFound));

    let elsewhere = c.dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let inside = c.dir.join("inside.3mf");
    std::fs::copy(CREALITY_3PLATES, &inside).unwrap();
    assert_eq!(plate_overview_of(&inside, Some(std::slice::from_ref(&c.dir)), false).unwrap().plates.len(), 3);
    let outside = plate_overview_of(&inside, Some(std::slice::from_ref(&elsewhere)), false).unwrap_err();
    assert_eq!(outside.code, Some(GeometryErrorCode::Unreadable));
    // `..` is resolved before the check.
    let traversal = elsewhere.join("../inside.3mf");
    assert!(plate_overview_of(&traversal, Some(std::slice::from_ref(&elsewhere)), false).is_err());
}
