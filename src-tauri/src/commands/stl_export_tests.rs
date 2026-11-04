use super::*;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/katalog-core/tests/fixtures").join(name)
}

fn triangles_in(path: &Path) -> u64 {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes.len() as u64, 84 + 50 * u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as u64);
    u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as u64
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

fn mesh_triangles(plate: Option<u32>) -> u64 {
    katalog_core::read::load_render_meshes(&fixture("creality-3plates.3mf")).unwrap().iter()
        .filter(|m| plate.is_none() || m.plate == plate).map(|m| m.indices.len() as u64).sum()
}

#[test]
fn a_3mf_is_converted_whole_or_by_plate() {
    let dir = unique_test_dir("stl_export_scopes");
    let source = fixture("creality-3plates.3mf");

    let all = dir.join("all.stl");
    assert_eq!(write_stl_export(&source, &StlExportScope::All, &all).unwrap(), mesh_triangles(None));
    assert_eq!(triangles_in(&all), mesh_triangles(None));

    let plate = dir.join("plate2.stl");
    assert_eq!(write_stl_export(&source, &StlExportScope::Plate { number: 2 }, &plate).unwrap(), mesh_triangles(Some(2)));
    assert_eq!(triangles_in(&plate), mesh_triangles(Some(2)));
    assert!(mesh_triangles(Some(2)) < mesh_triangles(None));

    let parts = dir.join("part0.stl");
    let first = katalog_core::read::load_render_meshes(&source).unwrap()[0].indices.len() as u64;
    assert_eq!(write_stl_export(&source, &StlExportScope::Parts { indices: vec![0] }, &parts).unwrap(), first);
    // Only the three files, no temp file left behind.
    assert_eq!(names(&dir), vec!["all.stl", "part0.stl", "plate2.stl"]);
}

#[test]
fn an_existing_target_is_never_overwritten() {
    let dir = unique_test_dir("stl_export_exists");
    let target = dir.join("taken.stl");
    std::fs::write(&target, b"do not touch").unwrap();
    let error = write_stl_export(&fixture("creality-3plates.3mf"), &StlExportScope::All, &target).unwrap_err();
    assert!(error.is(ErrorCode::StlTargetExists, &["taken.stl"]), "{error:?}");
    assert_eq!(std::fs::read(&target).unwrap(), b"do not touch");
    assert_eq!(names(&dir), vec!["taken.stl"]);
}

#[test]
fn the_source_file_is_never_the_target() {
    let dir = unique_test_dir("stl_export_source");
    let source = dir.join("copy.3mf");
    std::fs::copy(fixture("creality-3plates.3mf"), &source).unwrap();
    let before = std::fs::read(&source).unwrap();
    let error = write_stl_export(&source, &StlExportScope::All, &source).unwrap_err();
    assert!(error.is(ErrorCode::StlTargetIsSource, &[]), "{error:?}");
    assert_eq!(std::fs::read(&source).unwrap(), before);
}

#[test]
fn bad_selections_and_sources_give_registered_codes_and_create_no_file() {
    let dir = unique_test_dir("stl_export_errors");
    let source = fixture("creality-3plates.3mf");
    let target = dir.join("out.stl");

    let empty = write_stl_export(&source, &StlExportScope::Plate { number: 9 }, &target).unwrap_err();
    assert!(empty.is(ErrorCode::StlNothingToExport, &[]), "{empty:?}");
    let range = write_stl_export(&source, &StlExportScope::Parts { indices: vec![999] }, &target).unwrap_err();
    assert!(range.is(ErrorCode::ReqInvalidArguments, &[]), "{range:?}");

    let missing = dir.join("gone.3mf");
    let gone = write_stl_export(&missing, &StlExportScope::All, &target).unwrap_err();
    assert!(gone.is(ErrorCode::FileMissingOnDisk, &[missing.to_str().unwrap()]), "{gone:?}");

    let broken = dir.join("broken.3mf");
    std::fs::write(&broken, b"not a zip").unwrap();
    let unreadable = write_stl_export(&broken, &StlExportScope::All, &target).unwrap_err();
    assert!(unreadable.is(ErrorCode::StlSourceUnreadable, &[]), "{unreadable:?}");

    assert_eq!(names(&dir), vec!["broken.3mf"]);
}

#[cfg(not(feature = "step-preview"))]
#[test]
fn step_without_the_preview_build_is_reported_not_written() {
    let dir = unique_test_dir("stl_export_step");
    let error = write_stl_export(&fixture("wuerfel-10mm.step"), &StlExportScope::All, &dir.join("cube.stl")).unwrap_err();
    assert!(error.is(ErrorCode::StlStepUnavailable, &[]), "{error:?}");
    assert!(names(&dir).is_empty());
}

#[cfg(feature = "step-preview")]
#[test]
fn a_step_cube_becomes_twelve_triangles() {
    let dir = unique_test_dir("stl_export_step");
    let target = dir.join("cube.stl");
    assert_eq!(write_stl_export(&fixture("wuerfel-10mm.step"), &StlExportScope::All, &target).unwrap(), 12);
    assert_eq!(triangles_in(&target), 12);
}

#[test]
fn an_stl_source_is_refused() {
    assert!(ensure_convertible(db::models::FileType::Stl).unwrap_err().is(ErrorCode::StlSourceIsStl, &[]));
    for kind in [db::models::FileType::ThreeMf, db::models::FileType::Obj, db::models::FileType::Stp] {
        assert!(ensure_convertible(kind).is_ok());
    }
}

#[test]
fn names_extensions_and_catalog_membership() {
    let source = Path::new("/models/Gehaeuse_Set.3mf");
    assert_eq!(suggested_name(source, &StlExportScope::All), "Gehaeuse_Set.stl");
    assert_eq!(suggested_name(source, &StlExportScope::Plate { number: 2 }), "Gehaeuse_Set_Platte2.stl");
    assert_eq!(suggested_name(Path::new("/m/v1.2.step"), &StlExportScope::All), "v1.2.stl");

    assert_eq!(with_stl_extension(PathBuf::from("/x/a.stl")), PathBuf::from("/x/a.stl"));
    assert_eq!(with_stl_extension(PathBuf::from("/x/a.STL")), PathBuf::from("/x/a.STL"));
    assert_eq!(with_stl_extension(PathBuf::from("/x/a")), PathBuf::from("/x/a.stl"));
    assert_eq!(with_stl_extension(PathBuf::from("/x/a.v2")), PathBuf::from("/x/a.v2.stl"));

    let base = unique_test_dir("stl_export_catalog");
    let inside = base.join("Gehaeuse");
    std::fs::create_dir_all(&inside).unwrap();
    let outside = unique_test_dir("stl_export_outside");
    assert!(is_inside_catalog(&base, &base));
    assert!(is_inside_catalog(&base, &inside));
    assert!(!is_inside_catalog(&base, &outside));
    // `..` cannot be used to claim a folder outside the catalog.
    assert!(!is_inside_catalog(&base, &inside.join("..").join("..")));
    assert!(!is_inside_catalog(&base, &base.join("missing")));
}

/// The dialog offers the plate numbers of the plate overview; the writer selects by the
/// `plate` of the meshes. Both must count the same way, or "plate 2" saves another plate.
#[test]
fn plate_numbers_of_the_overview_select_the_same_meshes() {
    let source = fixture("creality-3plates.3mf");
    let overview = katalog_core::read::read_plate_overview(&source, db::models::FileType::ThreeMf, None, false).unwrap();
    let offered: Vec<u32> = overview.plates.iter().map(|p| p.plate_index).collect();
    let mut in_meshes: Vec<u32> = katalog_core::read::load_render_meshes(&source).unwrap().iter().filter_map(|m| m.plate).collect();
    in_meshes.sort_unstable();
    in_meshes.dedup();
    assert_eq!(offered, in_meshes);
    assert!(offered.len() > 1);
}
