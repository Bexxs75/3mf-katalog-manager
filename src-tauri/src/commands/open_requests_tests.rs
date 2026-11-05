use super::*;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn touch(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"model").unwrap();
    path
}

fn uri_of(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[test]
fn args_yield_absolute_relative_and_uri_paths() {
    let dir = unique_test_dir("open_args_forms");
    let plain = touch(&dir, "a.3mf");
    let relative = touch(&dir, "b.STL");
    let spaced = touch(&dir, "mit Leerzeichen \u{e4}.3mf");
    let args = strings(&["app", plain.to_str().unwrap(), "b.STL", &uri_of(&spaced)]);
    assert_eq!(open_requests_from_args(&args, &dir), vec![plain, relative, spaced]);
}

#[test]
fn uri_with_localhost_and_windows_drive_is_read_but_foreign_hosts_are_not() {
    assert_eq!(path_from_file_uri("file://localhost/tmp/a%20b.3mf"), Some(PathBuf::from("/tmp/a b.3mf")));
    assert_eq!(path_from_file_uri("FILE:///C:/Mod%C3%A8le.3mf"), Some(PathBuf::from("C:/Mod\u{e8}le.3mf")));
    assert_eq!(path_from_file_uri("file://server/share/a.3mf"), None);
    assert_eq!(path_from_file_uri("file:///tmp/%zz.3mf"), None);
    assert_eq!(path_from_file_uri("https://example.com/a.3mf"), None);
}

#[test]
fn options_unsupported_files_directories_and_missing_files_are_ignored() {
    let dir = unique_test_dir("open_args_filter");
    let good = touch(&dir, "ok.step");
    touch(&dir, "notes.txt");
    std::fs::create_dir(dir.join("folder.3mf")).unwrap();
    let args = strings(&[
        "app", "--verbose", "-psn_0_12345", "notes.txt", "folder.3mf", "gone.3mf", "", "https://x.example/a.3mf", "ok.step",
    ]);
    assert_eq!(open_requests_from_args(&args, &dir), vec![good]);
}

#[test]
fn a_path_starting_with_a_dash_needs_the_separator() {
    let dir = unique_test_dir("open_args_dash");
    let dashed = touch(&dir, "-model.3mf");
    assert!(open_requests_from_args(&strings(&["app", "-model.3mf"]), &dir).is_empty());
    assert_eq!(open_requests_from_args(&strings(&["app", "--", "-model.3mf"]), &dir), vec![dashed]);
}

#[test]
fn duplicates_collapse_and_the_program_name_is_never_a_file() {
    let dir = unique_test_dir("open_args_dupes");
    let file = touch(&dir, "a.obj");
    let args = strings(&[file.to_str().unwrap(), "a.obj", &uri_of(&file)]);
    // args[0] is the program, so only the second and third entry count; both name the same file.
    assert_eq!(open_requests_from_args(&args, &dir), vec![file]);
}

#[test]
fn more_than_the_limit_is_dropped() {
    let dir = unique_test_dir("open_args_limit");
    let mut args = vec!["app".to_string()];
    for i in 0..MAX_OPEN_FILES + 10 {
        args.push(touch(&dir, &format!("m{i:03}.3mf")).to_string_lossy().into_owned());
    }
    assert_eq!(open_requests_from_args(&args, &dir).len(), MAX_OPEN_FILES);

    let pending = PendingOpenFiles::default();
    pending.push((0..MAX_OPEN_FILES + 5).map(|i| PathBuf::from(format!("/m{i}.3mf"))).collect());
    pending.push(vec![PathBuf::from("/late.3mf")]);
    let taken = pending.take();
    assert_eq!(taken.len(), MAX_OPEN_FILES);
    assert!(!taken.contains(&PathBuf::from("/late.3mf")));
    assert!(pending.take().is_empty(), "taking empties the queue");
}

#[test]
fn pending_keeps_each_path_once() {
    let pending = PendingOpenFiles::default();
    pending.push(vec![PathBuf::from("/a.3mf"), PathBuf::from("/a.3mf")]);
    pending.push(vec![PathBuf::from("/a.3mf"), PathBuf::from("/b.3mf")]);
    assert_eq!(pending.take(), vec![PathBuf::from("/a.3mf"), PathBuf::from("/b.3mf")]);
}

fn catalog_file(path: &Path, hash: Option<&str>) -> NewFile {
    NewFile {
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        file_type: FileType::ThreeMf,
        folder_id: None,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes: 5,
        dimensions_mm: None,
        volume_cm3: None,
        object_count: None,
        thumbnail_png: None,
        imported_at: "2026-11-04T00:00:00Z".to_string(),
        file_modified_at: None,
        materials: Vec::new(),
        metadata: BTreeMap::new(),
        tags: Vec::new(),
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: None,
        content_hash: hash.map(str::to_string),
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        license: None,
        source: Default::default(),
        queue_position: None,
        favorite: false,
        plate_count: None,
        slice_info_json: None,
        print_info: None,
    }
}

#[test]
fn catalog_files_are_found_by_path_or_hash_and_the_rest_is_external() {
    let dir = unique_test_dir("open_resolve");
    let in_catalog = touch(&dir, "known.3mf");
    let copy = touch(&dir, "copy-elsewhere.3mf");
    let stranger = touch(&dir, "stranger.stl");
    let mut conn = db::connect_in_memory().unwrap();
    let hash = compute_content_hash(&copy).unwrap();
    let by_path = db::insert_file(&mut conn, &catalog_file(&in_catalog, None)).unwrap();
    // A different path with the same bytes: found by content hash (e.g. a copy of a cataloged file).
    let other_path = dir.join("moved-away.3mf");
    let by_hash = db::insert_file(&mut conn, &catalog_file(&other_path, Some(&hash))).unwrap();

    let sources = ExternalSources::default();
    let found = resolve_open_requests(&conn, &sources, vec![in_catalog.clone(), copy.clone(), stranger.clone()]);
    // Every fixture file holds the same bytes, so "stranger" has the same hash as the cataloged copy.
    assert_eq!(found[0], OpenRequest::Catalog { file_id: by_path.to_string() });
    assert_eq!(found[1], OpenRequest::Catalog { file_id: by_hash.to_string() });
    assert!(matches!(found[2], OpenRequest::Catalog { .. }));

    std::fs::write(&stranger, b"different bytes").unwrap();
    let found = resolve_open_requests(&conn, &sources, vec![stranger.clone()]);
    match &found[0] {
        OpenRequest::External { source_id, name, size_bytes, extension } => {
            assert_eq!((name.as_str(), *size_bytes, extension.as_str()), ("stranger.stl", 15, "stl"));
            assert_eq!(sources.resolve(source_id).unwrap(), stranger);
        }
        other => panic!("expected an external source, got {other:?}"),
    }
}

#[test]
fn an_ambiguous_hash_is_external_and_a_vanished_file_is_dropped() {
    let dir = unique_test_dir("open_resolve_ambiguous");
    let file = touch(&dir, "twice.3mf");
    let mut conn = db::connect_in_memory().unwrap();
    let hash = compute_content_hash(&file).unwrap();
    for name in ["x.3mf", "y.3mf"] {
        db::insert_file(&mut conn, &catalog_file(&dir.join(name), Some(&hash))).unwrap();
    }
    let sources = ExternalSources::default();
    let found = resolve_open_requests(&conn, &sources, vec![file.clone(), dir.join("gone.3mf")]);
    assert_eq!(found.len(), 1);
    assert!(matches!(found[0], OpenRequest::External { .. }));
}

#[test]
fn only_registered_sources_resolve_and_the_store_is_bounded() {
    let sources = ExternalSources::default();
    let first = PathBuf::from("/first.3mf");
    let id = sources.register(&first).unwrap();
    assert_eq!(sources.register(&first).as_deref(), Some(id.as_str()), "the same path keeps its id");
    assert_eq!(sources.resolve(&id).unwrap(), first);
    assert!(sources.resolve("ext-forged").is_err());
    assert!(sources.resolve("/etc/passwd").is_err());

    for i in 1..MAX_EXTERNAL_SOURCES {
        assert!(sources.register(&PathBuf::from(format!("/m{i}.3mf"))).is_some());
    }
    assert!(sources.register(&PathBuf::from("/newest.3mf")).is_none(), "a full table refuses instead of pushing out");
    assert_eq!(sources.resolve(&id).unwrap(), first, "an issued id stays valid");
    assert_eq!(sources.0.lock().unwrap().len(), MAX_EXTERNAL_SOURCES);

    sources.release(&id);
    let newest = sources.register(&PathBuf::from("/newest.3mf")).expect("room again after a release");
    sources.release(&newest);
    assert!(sources.resolve(&newest).is_err());
}

#[test]
fn a_full_hand_over_keeps_every_issued_id_valid() {
    let dir = unique_test_dir("open_full_handover");
    let conn = db::connect_in_memory().unwrap();
    let files: Vec<PathBuf> = (0..MAX_OPEN_FILES).map(|i| touch(&dir, &format!("f{i}.3mf"))).collect();
    let sources = ExternalSources::default();
    let found = resolve_open_requests(&conn, &sources, files);
    assert_eq!(found.len(), MAX_OPEN_FILES);
    for request in &found {
        let OpenRequest::External { source_id, .. } = request else { panic!("external expected") };
        assert!(sources.resolve(source_id).is_ok(), "{source_id} must stay valid");
    }
    // A further hand-over neither displaces them nor hands out ids that cannot work.
    let extra = touch(&dir, "extra.3mf");
    assert!(resolve_open_requests(&conn, &sources, vec![extra]).is_empty());
    for request in &found {
        let OpenRequest::External { source_id, .. } = request else { unreachable!() };
        assert!(sources.resolve(source_id).is_ok());
    }
}

#[test]
fn the_wire_format_names_the_kind() {
    let json = serde_json::to_value(OpenRequest::Catalog { file_id: "7".into() }).unwrap();
    assert_eq!(json, serde_json::json!({ "kind": "catalog", "fileId": "7" }));
    let json = serde_json::to_value(OpenRequest::External {
        source_id: "ext-1".into(), name: "a.stl".into(), size_bytes: 3, extension: "stl".into(),
    })
    .unwrap();
    assert_eq!(json, serde_json::json!({ "kind": "external", "sourceId": "ext-1", "name": "a.stl", "sizeBytes": 3, "extension": "stl" }));
}

#[test]
fn an_external_source_is_vouched_for_by_the_backend_for_the_import() {
    let dir = unique_test_dir("open_prove");
    let file = touch(&dir, "viewed.3mf");
    let sources = ExternalSources::default();
    let jobs = ImportJobs::default();
    let id = sources.register(&file).unwrap();

    // Without the hand-over no proof exists: neither a forged id nor the plain path is accepted.
    assert!(prove_external_source(&sources, &jobs, "ext-forged").is_err());
    assert!(jobs.authorize_models(ImportSource::Dropped, std::slice::from_ref(&file), false, &[]).is_err());

    assert_eq!(prove_external_source(&sources, &jobs, &id).unwrap(), file);
    assert!(jobs.authorize_models(ImportSource::Dropped, std::slice::from_ref(&file), false, &[]).is_ok());
    // The proof is single use, like a real drop.
    assert!(jobs.authorize_models(ImportSource::Dropped, std::slice::from_ref(&file), false, &[]).is_err());
}

#[test]
fn a_vanished_or_released_source_gives_no_proof() {
    let dir = unique_test_dir("open_prove_gone");
    let file = touch(&dir, "soon-gone.stl");
    let sources = ExternalSources::default();
    let jobs = ImportJobs::default();
    let id = sources.register(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    let error = prove_external_source(&sources, &jobs, &id).unwrap_err();
    assert!(error.is(ErrorCode::FileNotFound, &[]));
    assert!(jobs.authorize_models(ImportSource::Dropped, std::slice::from_ref(&file), false, &[]).is_err());

    let again = touch(&dir, "released.stl");
    let id = sources.register(&again).unwrap();
    sources.release(&id);
    assert!(prove_external_source(&sources, &jobs, &id).is_err());
}

/// The file associations announced to the operating system (`tauri.conf.json`, inherited by
/// the Preview variant, which only overrides name and identifier) must cover exactly the model
/// extensions the start-argument handling accepts, and must never claim to be the default.
#[test]
fn file_associations_match_model_extensions_and_stay_alternate() {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
    let preview: serde_json::Value = serde_json::from_str(include_str!("../../tauri.preview.conf.json")).unwrap();
    assert!(preview.pointer("/bundle/fileAssociations").is_none(), "Preview must inherit, not redefine");
    let entries = conf.pointer("/bundle/fileAssociations").and_then(|v| v.as_array()).expect("fileAssociations");
    let mut declared: Vec<String> = Vec::new();
    for entry in entries {
        assert_eq!(entry["rank"], "Alternate", "must not take over as default program: {entry}");
        assert_eq!(entry["role"], "Viewer", "{entry}");
        assert!(entry["mimeType"].as_str().is_some_and(|m| m.starts_with("model/")), "{entry}");
        for ext in entry["ext"].as_array().unwrap() {
            declared.push(ext.as_str().unwrap().to_string());
        }
    }
    declared.sort();
    assert_eq!(declared, strings(&["3mf", "obj", "step", "stl", "stp"]));
    for ext in &declared {
        assert!(is_model_extension(Path::new(&format!("x.{ext}"))), "{ext}");
    }
}
