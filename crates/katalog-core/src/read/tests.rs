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
    assert!(error.message.contains("außerhalb"));
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
fn add_and_list_print_log_entry_roundtrips_through_dto() {
    let mut conn = crate::db::connect_in_memory().expect("connect");

    // add_print_log_entry needs a real file_id.

    let new_file = crate::db::models::NewFile {
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
