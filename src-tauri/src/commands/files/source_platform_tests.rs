//! Download platform and IDs read from the model metadata of a 3MF: every way a
//! file reaches the catalog (single import, job/folder/archive/disk-sync reader,
//! rescan) must fill or clear the same columns.
//! Synthetic archives only: the files hold nothing but `3D/3dmodel.model`.
use super::*;
use std::io::Write;

/// Metadata of a file downloaded from Makerworld, as written by its export.
const MAKERWORLD: &[(&str, &str)] = &[
    ("Application", "BambuStudio-02.01.00.00"),
    ("Designer", "Jane"),
    ("Origin", "original"),
    ("DesignRegion", "US"),
    ("DesignModelId", "USfa963547e39de4"),
    ("DesignProfileId", "1067399466"),
];

fn write_3mf(path: &Path, metadata: &[(&str, &str)]) {
    let entries: String = metadata
        .iter()
        .map(|(name, value)| format!(r#"<metadata name="{name}">{value}</metadata>"#))
        .collect();
    let model = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">{entries}<resources></resources><build></build></model>"#
    );
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
        zip.start_file("3D/3dmodel.model", options).unwrap();
        zip.write_all(model.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    std::fs::write(path, buf).unwrap();
}

fn model_in(dir: &Path, name: &str, metadata: &[(&str, &str)]) -> PathBuf {
    let path = dir.join(name);
    write_3mf(&path, metadata);
    path
}

fn source_of(file: &db::models::NewFile) -> (Option<&str>, Option<&str>, Option<&str>) {
    (file.source.platform.as_deref(), file.source.model_id.as_deref(), file.source.profile_id.as_deref())
}

fn stored(conn: &Connection, id: i64) -> (Option<String>, Option<String>, Option<String>) {
    let file = db::get_file(conn, id).unwrap().unwrap();
    (file.source_platform, file.source_model_id, file.source_profile_id)
}

const MAKERWORLD_SOURCE: (Option<&str>, Option<&str>, Option<&str>) =
    (Some("makerworld"), Some("USfa963547e39de4"), Some("1067399466"));

#[test]
fn every_reader_recognises_a_makerworld_file() {
    let dir = unique_test_dir("source_platform_readers");
    let path = model_in(&dir, "gingerbread.3mf", MAKERWORLD);
    assert_eq!(source_of(&read_model_file(&path, None).unwrap()), MAKERWORLD_SOURCE);
    // Job, folder, archive and disk-sync imports all use this reader.
    assert_eq!(source_of(&read_import_model_file(&path, None).unwrap()), MAKERWORLD_SOURCE);
    // creator keeps coming from `Designer`.
    assert_eq!(read_model_file(&path, None).unwrap().creator.as_deref(), Some("Jane"));
}

#[test]
fn import_stores_the_source_and_the_dto_carries_it_in_camel_case() {
    let dir = unique_test_dir("source_platform_import");
    let path = model_in(&dir, "gingerbread.3mf", MAKERWORLD);
    let conn = crate::db::connect_in_memory().unwrap();

    let dto = import_one(&conn, &path, None, None, None).unwrap();

    let json = serde_json::to_value(&dto).unwrap();
    assert_eq!(json["sourcePlatform"], "makerworld");
    assert_eq!(json["sourceModelId"], "USfa963547e39de4");
    assert_eq!(json["sourceProfileId"], "1067399466");
    let id: i64 = dto.id.parse().unwrap();
    assert_eq!(stored(&conn, id), (Some("makerworld".into()), Some("USfa963547e39de4".into()), Some("1067399466".into())));

    let summary = db::list_file_summaries(&conn).unwrap().into_iter().next().unwrap();
    let json = serde_json::to_value(summary_to_dto(summary)).unwrap();
    assert_eq!(json["sourcePlatform"], "makerworld");
    assert_eq!(json["sourceModelId"], "USfa963547e39de4");
    assert_eq!(json["sourceProfileId"], "1067399466");
}

#[test]
fn a_file_without_proof_gets_null_fields_in_the_dto() {
    let dir = unique_test_dir("source_platform_none_dto");
    let path = model_in(&dir, "plain.3mf", &[("Application", "PrusaSlicer-2.8.0")]);
    let conn = crate::db::connect_in_memory().unwrap();
    let dto = import_one(&conn, &path, None, None, None).unwrap();
    let json = serde_json::to_value(&dto).unwrap();
    for key in ["sourcePlatform", "sourceModelId", "sourceProfileId"] {
        assert!(json.get(key).is_some_and(serde_json::Value::is_null), "{key} must be present and null");
    }
}

#[test]
fn nothing_is_recognised_without_both_ids() {
    let dir = unique_test_dir("source_platform_partial");
    let only_model = model_in(&dir, "only_model.3mf", &[("DesignModelId", "USfa963547e39de4"), ("Designer", "Jane")]);
    let only_profile = model_in(&dir, "only_profile.3mf", &[("DesignProfileId", "1067399466")]);
    let none = model_in(&dir, "none.3mf", &[("Application", "PrusaSlicer-2.8.0")]);
    for path in [only_model, only_profile, none] {
        assert_eq!(source_of(&read_model_file(&path, None).unwrap()), (None, None, None), "{}", path.display());
    }
}

#[test]
fn a_file_saved_again_in_a_slicer_with_empty_fields_is_not_recognised() {
    let dir = unique_test_dir("source_platform_resaved");
    // Bambu Studio keeps the keys but writes them empty after "save as".
    let path = model_in(&dir, "luigi.3mf", &[
        ("Application", "BambuStudio-02.01.00.00"), ("Designer", ""), ("DesignerUserId", ""),
        ("Origin", ""), ("DesignRegion", ""), ("DesignModelId", ""), ("DesignProfileId", ""),
    ]);
    assert_eq!(source_of(&read_model_file(&path, None).unwrap()), (None, None, None));
}

#[test]
fn values_are_cleaned_before_they_are_stored() {
    let dir = unique_test_dir("source_platform_sanitize");
    let long = "7".repeat(400);
    let path = model_in(&dir, "dirty.3mf", &[
        // tab, line feed and a right-to-left override between the digits
        ("DesignModelId", "US&#9;fa&#10;96&#8238;35"),
        ("DesignProfileId", &long),
    ]);
    let file = read_model_file(&path, None).unwrap();
    assert_eq!(file.source.model_id.as_deref(), Some("USfa9635"));
    assert_eq!(file.source.profile_id.as_deref(), Some(&long[..128]));
    assert_eq!(file.source.platform.as_deref(), Some("makerworld"));
}

#[test]
fn rescan_fills_the_fields_of_an_entry_imported_before_the_feature() {
    let dir = unique_test_dir("source_platform_rescan_set");
    let path = model_in(&dir, "old.3mf", MAKERWORLD);
    let mut conn = crate::db::connect_in_memory().unwrap();
    let id: i64 = import_one(&conn, &path, None, None, None).unwrap().id.parse().unwrap();
    // The state of a catalog from before the columns existed.
    conn.execute("UPDATE files SET source_platform = NULL, source_model_id = NULL, source_profile_id = NULL", []).unwrap();
    assert_eq!(stored(&conn, id), (None, None, None));

    let dto = rescan_file(&mut conn, id).unwrap();

    assert_eq!(stored(&conn, id), (Some("makerworld".into()), Some("USfa963547e39de4".into()), Some("1067399466".into())));
    assert_eq!(dto.source_platform.as_deref(), Some("makerworld"));
    assert_eq!(dto.source_model_id.as_deref(), Some("USfa963547e39de4"));
    assert_eq!(dto.source_profile_id.as_deref(), Some("1067399466"));
}

#[test]
fn rescan_clears_the_fields_when_the_file_no_longer_carries_them() {
    let dir = unique_test_dir("source_platform_rescan_clear");
    let path = model_in(&dir, "model.3mf", MAKERWORLD);
    let mut conn = crate::db::connect_in_memory().unwrap();
    let id: i64 = import_one(&conn, &path, None, None, None).unwrap().id.parse().unwrap();
    assert!(stored(&conn, id).0.is_some());

    write_3mf(&path, &[("Application", "BambuStudio-02.01.00.00"), ("DesignModelId", ""), ("DesignProfileId", "")]);
    rescan_file(&mut conn, id).unwrap();

    assert_eq!(stored(&conn, id), (None, None, None));
}

#[test]
fn rescan_of_other_formats_leaves_the_fields_empty() {
    let dir = unique_test_dir("source_platform_rescan_stl");
    let path = dir.join("part.obj");
    std::fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let mut conn = crate::db::connect_in_memory().unwrap();
    let id: i64 = import_one(&conn, &path, None, None, None).unwrap().id.parse().unwrap();
    conn.execute("UPDATE files SET source_platform = 'makerworld', source_model_id = 'x', source_profile_id = 'y'", []).unwrap();
    rescan_file(&mut conn, id).unwrap();
    assert_eq!(stored(&conn, id), (None, None, None));
}
