use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use super::writer::{copy_into_new_file, replace_file_atomically, BLOCK_TRIANGLES};
use super::*;
use crate::geometry::{signed_volume, BoundingBox, RenderMesh};
use crate::unique_test_dir;

const CUBE_TRIANGLES: [[u32; 3]; 12] = [
    [0, 2, 1],
    [0, 3, 2],
    [4, 5, 6],
    [4, 6, 7],
    [0, 1, 5],
    [0, 5, 4],
    [3, 6, 2],
    [3, 7, 6],
    [0, 7, 3],
    [0, 4, 7],
    [1, 2, 6],
    [1, 6, 5],
];

/// 10 mm cube, outward facing, no normals (like every 3MF mesh).
fn cube(plate: Option<u32>) -> RenderMesh {
    let positions = vec![
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 0.0],
        [0.0, 10.0, 0.0],
        [0.0, 0.0, 10.0],
        [10.0, 0.0, 10.0],
        [10.0, 10.0, 10.0],
        [0.0, 10.0, 10.0],
    ];
    RenderMesh { positions, indices: CUBE_TRIANGLES.to_vec(), plate, ..Default::default() }
}

fn written(meshes: &[&RenderMesh]) -> Vec<u8> {
    let mut out = Vec::new();
    write_stl(&mut out, meshes).unwrap();
    out
}

fn facet_normal_at(bytes: &[u8], facet: usize) -> [f32; 3] {
    let at = 84 + facet * 50;
    let f = |o: usize| f32::from_le_bytes(bytes[at + o..at + o + 4].try_into().unwrap());
    [f(0), f(4), f(8)]
}

/// Every triangle as three corner positions, in order.
fn triangles_of(mesh: &RenderMesh) -> Vec<[[f32; 3]; 3]> {
    mesh.indices.iter().map(|t| t.map(|i| mesh.positions[i as usize])).collect()
}

fn bbox_of(meshes: &[RenderMesh]) -> BoundingBox {
    let mut bbox = BoundingBox::empty();
    for p in meshes.iter().flat_map(|m| m.indices.iter().flatten().map(|&i| &m.positions[i as usize])) {
        bbox.extend(p.map(f64::from));
    }
    bbox
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn read_back(bytes: &[u8]) -> RenderMesh {
    parse_stl_geometry(bytes).expect("written STL must be readable")
}

#[test]
fn writes_80_byte_header_count_and_50_bytes_per_triangle() {
    let mesh = cube(None);
    let bytes = written(&[&mesh]);
    assert_eq!(bytes.len(), 80 + 4 + 12 * 50);
    assert_eq!(u32::from_le_bytes(bytes[80..84].try_into().unwrap()), 12);
    assert!(!bytes.starts_with(b"solid"), "a header starting with 'solid' looks like ASCII STL");
    // Attribute bytes are zero.
    assert!(bytes[84..].chunks(50).all(|facet| facet[48..] == [0, 0]));
}

#[test]
fn roundtrip_keeps_triangles_order_orientation_and_bounding_box() {
    let mesh = cube(None);
    let back = read_back(&written(&[&mesh]));
    assert_eq!(back.indices.len(), 12);
    let expected: Vec<_> = triangles_of(&mesh);
    let actual: Vec<_> = triangles_of(&back);
    assert_eq!(actual, expected, "vertex order (orientation) must survive");
    let (a, b) = (bbox_of(std::slice::from_ref(&mesh)), bbox_of(std::slice::from_ref(&back)));
    assert_eq!(a.min, b.min);
    assert_eq!(a.max, b.max);
    let vertices: Vec<[f64; 3]> = back.positions.iter().map(|p| p.map(f64::from)).collect();
    let volume = signed_volume(&vertices, &back.indices);
    assert!((volume - 1000.0).abs() < 1e-3, "outward cube has positive volume, got {volume}");
}

#[test]
fn normals_come_from_the_triangles_not_from_the_mesh() {
    let mesh = cube(None);
    assert!(mesh.normals.is_none());
    let bytes = written(&[&mesh]);
    for facet in 0..12 {
        let n = facet_normal_at(&bytes, facet);
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-5, "facet {facet}: not a unit normal {n:?}");
        // Pointing away from the cube center (5, 5, 5).
        let tri = mesh.indices[facet].map(|i| mesh.positions[i as usize]);
        let centroid: Vec<f32> = (0..3).map(|k| (tri[0][k] + tri[1][k] + tri[2][k]) / 3.0 - 5.0).collect();
        let dot: f32 = (0..3).map(|k| n[k] * centroid[k]).sum();
        assert!(dot > 0.0, "facet {facet}: normal {n:?} points inwards");
    }
    // Facet 2 is the top face: (0, 0, 1).
    assert_eq!(facet_normal_at(&bytes, 2), [0.0, 0.0, 1.0]);
}

#[test]
fn triangles_without_area_are_kept_with_a_zero_normal() {
    let mut mesh = cube(None);
    mesh.indices.push([0, 0, 1]);
    mesh.indices.push([2, 2, 2]);
    let bytes = written(&[&mesh]);
    assert_eq!(u32::from_le_bytes(bytes[80..84].try_into().unwrap()), 14, "count matches the mesh list");
    assert_eq!(facet_normal_at(&bytes, 12), [0.0; 3]);
    assert_eq!(facet_normal_at(&bytes, 13), [0.0; 3]);
    assert_eq!(read_back(&bytes).indices.len(), 14);
}

#[test]
fn non_finite_coordinates_are_an_error_and_write_nothing() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut mesh = cube(None);
        mesh.positions[3][1] = bad;
        let mut out = Vec::new();
        let result = write_stl(&mut out, &[&mesh]);
        assert!(matches!(result, Err(StlWriteError::NonFinite)), "{bad}");
        assert!(out.is_empty(), "nothing may be written before validation");
    }
}

#[test]
fn a_missing_vertex_is_an_error_instead_of_a_panic() {
    let mut mesh = cube(None);
    mesh.indices.push([0, 1, 8]);
    let mut out = Vec::new();
    assert!(matches!(write_stl(&mut out, &[&mesh]), Err(StlWriteError::InvalidIndex)));
    assert!(out.is_empty());
}

#[test]
fn more_triangles_than_the_limit_is_an_error_at_the_helper_parameter() {
    let mesh = cube(None);
    let mut out = Vec::new();
    let result = write_stl_with_limit(&mut out, &[&mesh], 11);
    assert!(matches!(result, Err(StlWriteError::TooManyTriangles { count: 12, max: 11 })), "{result:?}");
    assert!(out.is_empty());
    // The limit counts over all meshes together.
    let other = cube(None);
    let result = write_stl_with_limit(&mut out, &[&mesh, &other], 23);
    assert!(matches!(result, Err(StlWriteError::TooManyTriangles { .. })));
    assert_eq!(write_stl_with_limit(&mut out, &[&mesh, &other], 24).unwrap(), 24);
    assert_eq!(MAX_STL_TRIANGLES, u32::MAX as u64);
}

/// Records the size of every write call.
struct Recorder(Vec<usize>);
impl Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.push(buf.len());
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn large_meshes_are_written_in_blocks_not_in_one_buffer() {
    let mut mesh = cube(None);
    let triangles = BLOCK_TRIANGLES * 2 + 100;
    mesh.indices = (0..triangles).map(|i| CUBE_TRIANGLES[i % 12]).collect();
    let mut recorder = Recorder(Vec::new());
    assert_eq!(write_stl(&mut recorder, &[&mesh]).unwrap(), triangles as u64);
    let writes = recorder.0;
    assert_eq!(writes.iter().sum::<usize>(), 84 + triangles * 50);
    assert!(writes.iter().all(|&n| n <= BLOCK_TRIANGLES * 50 + 50), "a write call was larger than a block: {writes:?}");
    assert!(writes.len() >= 4, "header, count and at least two blocks expected: {writes:?}");
}

#[test]
fn selection_by_scope_keeps_mesh_order_and_rejects_bad_input() {
    let meshes = vec![cube(Some(1)), cube(Some(2)), cube(Some(2)), cube(None)];
    let ptrs = |sel: Vec<&RenderMesh>| sel.iter().map(|m| *m as *const RenderMesh).collect::<Vec<_>>();
    let at = |i: usize| &meshes[i] as *const RenderMesh;

    assert_eq!(ptrs(select_meshes(&meshes, StlScope::All).unwrap()), vec![at(0), at(1), at(2), at(3)]);
    assert_eq!(ptrs(select_meshes(&meshes, StlScope::Plate(2)).unwrap()), vec![at(1), at(2)]);
    // Click order and duplicates do not matter.
    assert_eq!(ptrs(select_meshes(&meshes, StlScope::Parts(&[3, 1, 3, 0])).unwrap()), vec![at(0), at(1), at(3)]);

    assert!(matches!(select_meshes(&meshes, StlScope::Plate(7)), Err(StlWriteError::NothingToExport)));
    assert!(matches!(select_meshes(&meshes, StlScope::Parts(&[])), Err(StlWriteError::NothingToExport)));
    assert!(matches!(
        select_meshes(&meshes, StlScope::Parts(&[0, 4])),
        Err(StlWriteError::PartOutOfRange { index: 4, count: 4 })
    ));
    assert!(matches!(select_meshes(&[], StlScope::All), Err(StlWriteError::NothingToExport)));

    // A part without triangles is not an export.
    let empty = vec![RenderMesh::default()];
    assert!(matches!(select_meshes(&empty, StlScope::Parts(&[0])), Err(StlWriteError::NothingToExport)));
}

#[test]
fn creality_three_plates_all_equals_sum_and_plate_two_only_its_meshes() {
    let meshes = crate::threemf::extract_render_meshes_from_path(&fixture("creality-3plates.3mf")).unwrap();
    assert_eq!(meshes.iter().map(|m| m.plate).collect::<Vec<_>>(), vec![Some(1), Some(2), Some(2), Some(2), Some(3)]);
    let triangles = |plate: Option<u32>| -> usize {
        meshes.iter().filter(|m| plate.is_none() || m.plate == plate).map(|m| m.indices.len()).sum()
    };

    let all = written(&select_meshes(&meshes, StlScope::All).unwrap());
    let all_back = read_back(&all);
    assert_eq!(all_back.indices.len(), triangles(None));
    assert_eq!(all_back.indices.len(), triangles(Some(1)) + triangles(Some(2)) + triangles(Some(3)));

    let plate2 = written(&select_meshes(&meshes, StlScope::Plate(2)).unwrap());
    let plate2_back = read_back(&plate2);
    assert_eq!(plate2_back.indices.len(), triangles(Some(2)));
    assert!(triangles(Some(2)) < triangles(None));
    let expected: Vec<RenderMesh> = meshes.iter().filter(|m| m.plate == Some(2)).cloned().collect();
    let (a, b) = (bbox_of(&expected), bbox_of(&[plate2_back]));
    assert_eq!((a.min, a.max), (b.min, b.max));

    assert!(matches!(select_meshes(&meshes, StlScope::Plate(4)), Err(StlWriteError::NothingToExport)));
}

#[test]
fn synthetic_fixtures_roundtrip_with_equal_triangle_count_and_bounding_box() {
    for name in ["synthetic-bambu-project.3mf", "synthetic-bambu-colors.3mf", "synthetic-prusa-project.3mf"] {
        let meshes = crate::threemf::extract_render_meshes_from_path(&fixture(name)).unwrap();
        let back = read_back(&written(&select_meshes(&meshes, StlScope::All).unwrap()));
        assert_eq!(back.indices.len(), meshes.iter().map(|m| m.indices.len()).sum::<usize>(), "{name}");
        let (a, b) = (bbox_of(&meshes), bbox_of(&[back]));
        assert_eq!((a.min, a.max), (b.min, b.max), "{name}");
    }
}

#[test]
fn bambu_style_project_with_several_objects_on_two_plates() {
    let mesh = |dx: u32| {
        format!(
            r#"<mesh><vertices><vertex x="{dx}" y="0" z="0"/><vertex x="{}" y="0" z="0"/><vertex x="{dx}" y="5" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>"#,
            dx + 5
        )
    };
    let model = format!(
        r#"<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>
        <object id="1" type="model">{}</object><object id="2" type="model">{}</object><object id="3" type="model">{}</object>
        </resources><build><item objectid="1"/><item objectid="2"/><item objectid="3"/></build></model>"#,
        mesh(0),
        mesh(100),
        mesh(200)
    );
    let config = r#"<config><plate><metadata key="plater_id" value="1"/><model_instance object_id="1"/></plate>
        <plate><metadata key="plater_id" value="2"/><model_instance object_id="2"/><model_instance object_id="3"/></plate></config>"#;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in [("3D/3dmodel.model", model.as_str()), ("Metadata/model_settings.config", config)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    let meshes = meshes_of(zip.finish().unwrap().into_inner());
    assert_eq!(meshes.iter().map(|m| m.plate).collect::<Vec<_>>(), vec![Some(1), Some(2), Some(2)]);

    let all = read_back(&written(&select_meshes(&meshes, StlScope::All).unwrap()));
    assert_eq!(all.indices.len(), 3);
    let plate2 = read_back(&written(&select_meshes(&meshes, StlScope::Plate(2)).unwrap()));
    assert_eq!(plate2.indices.len(), 2);
    let bbox = bbox_of(&[plate2]);
    assert_eq!((bbox.min[0], bbox.max[0]), (100.0, 205.0));
    let one_part = read_back(&written(&select_meshes(&meshes, StlScope::Parts(&[2])).unwrap()));
    assert_eq!(bbox_of(&[one_part]).min[0], 200.0);
}

fn single_triangle_3mf(unit: &str, item_attrs: &str) -> Vec<u8> {
    let model = format!(
        r#"<model unit="{unit}" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="2" y="0" z="0"/><vertex x="0" y="3" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object></resources><build><item objectid="1" {item_attrs}/></build></model>"#
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(model.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn meshes_of(bytes: Vec<u8>) -> Vec<RenderMesh> {
    let package = crate::threemf::container::read_geometry_package(Cursor::new(bytes)).unwrap();
    crate::threemf::extract_render_meshes(&package).unwrap()
}

#[test]
fn build_item_transform_reaches_the_stl_like_in_the_viewer() {
    // 3MF row-vector matrix: rotation 90 degrees about z, then move by (10, 20, 30).
    let meshes = meshes_of(single_triangle_3mf("millimeter", r#"transform="0 1 0 -1 0 0 0 0 1 10 20 30""#));
    let expected = [[10.0, 20.0, 30.0], [10.0, 22.0, 30.0], [7.0, 20.0, 30.0]];
    // The viewer path (the mesh list) and the STL agree, and both are the transformed triangle.
    assert_eq!(triangles_of(&meshes[0]), vec![expected]);
    let back = read_back(&written(&select_meshes(&meshes, StlScope::All).unwrap()));
    assert_eq!(triangles_of(&back), vec![expected]);
}

#[test]
fn inch_unit_is_not_evaluated_today_and_the_stl_is_written_unscaled() {
    // Documents the current behavior: the reader ignores the `unit` attribute, so an
    // inch model keeps its numbers (the preview shows it 25.4 times too small). When
    // the reader learns units, this test must change to the scaled numbers.
    let meshes = meshes_of(single_triangle_3mf("inch", ""));
    let back = read_back(&written(&select_meshes(&meshes, StlScope::All).unwrap()));
    assert_eq!(triangles_of(&back), vec![[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 3.0, 0.0]]]);
}

fn dir(name: &str) -> PathBuf {
    unique_test_dir(name)
}

fn leftovers(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn target_equal_to_source_is_refused_and_the_source_stays_intact() {
    let d = dir("stl_target_is_source");
    let source = d.join("model.3mf");
    std::fs::write(&source, b"original").unwrap();
    let mesh = cube(None);

    let result = write_stl_file(&source, Some(&source), &[&mesh]);
    assert!(matches!(result, Err(StlWriteError::TargetIsSource)), "{result:?}");
    // The same file under another spelling.
    std::fs::create_dir(d.join("sub")).unwrap();
    let spelled = d.join("sub").join("..").join("model.3mf");
    assert!(matches!(write_stl_file(&spelled, Some(&source), &[&mesh]), Err(StlWriteError::TargetIsSource)));
    #[cfg(unix)]
    {
        let link = d.join("link.stl");
        std::os::unix::fs::symlink(&source, &link).unwrap();
        assert!(matches!(write_stl_file(&link, Some(&source), &[&mesh]), Err(StlWriteError::TargetIsSource)));
        let hard = d.join("hard.stl");
        std::fs::hard_link(&source, &hard).unwrap();
        assert!(matches!(write_stl_file(&hard, Some(&source), &[&mesh]), Err(StlWriteError::TargetIsSource)));
    }
    assert_eq!(std::fs::read(&source).unwrap(), b"original");
    // A different target next to it works.
    write_stl_file(&d.join("model.stl"), Some(&source), &[&mesh]).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), b"original");
}

#[test]
fn copy_fallback_for_filesystems_without_hard_links_never_replaces() {
    let d = dir("stl_copy_fallback");
    let temp = d.join("temp.tmp");
    std::fs::write(&temp, b"fresh").unwrap();

    let taken = d.join("taken.stl");
    std::fs::write(&taken, b"precious").unwrap();
    let err = copy_into_new_file(&temp, &taken).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&taken).unwrap(), b"precious");

    let free = d.join("free.stl");
    copy_into_new_file(&temp, &free).unwrap();
    assert_eq!(std::fs::read(&free).unwrap(), b"fresh");

    // A temp file that cannot be read leaves no half-written target behind.
    let broken = d.join("broken.stl");
    assert!(copy_into_new_file(&d.join("missing.tmp"), &broken).is_err());
    assert!(!broken.exists());
}

#[test]
fn write_new_never_replaces_an_existing_target_and_leaves_no_temp_file() {
    let d = dir("stl_write_new_exists");
    let target = d.join("out.stl");
    std::fs::write(&target, b"precious").unwrap();
    let mesh = cube(None);

    let result = write_stl_file_new(&target, None, &[&mesh]);
    assert!(matches!(result, Err(StlWriteError::TargetExists)), "{result:?}");
    assert_eq!(std::fs::read(&target).unwrap(), b"precious");
    assert_eq!(leftovers(&d), vec!["out.stl".to_string()]);
    // A dangling symlink under the target name counts as taken, too.
    #[cfg(unix)]
    {
        let link = d.join("dangling.stl");
        std::os::unix::fs::symlink(d.join("nowhere"), &link).unwrap();
        assert!(matches!(write_stl_file_new(&link, None, &[&mesh]), Err(StlWriteError::TargetExists)));
        assert!(!d.join("nowhere").exists());
    }
}

#[test]
fn write_new_creates_a_complete_file_and_refuses_the_source_and_bad_input() {
    let d = dir("stl_write_new_ok");
    let mesh = cube(None);
    let target = d.join("fresh.stl");
    assert_eq!(write_stl_file_new(&target, None, &[&mesh]).unwrap(), 12);
    assert_eq!(std::fs::read(&target).unwrap(), written(&[&mesh]));
    assert_eq!(leftovers(&d), vec!["fresh.stl".to_string()]);

    let source = d.join("model.3mf");
    std::fs::write(&source, b"original").unwrap();
    assert!(matches!(write_stl_file_new(&source, Some(&source), &[&mesh]), Err(StlWriteError::TargetIsSource)));
    assert_eq!(std::fs::read(&source).unwrap(), b"original");

    let empty = RenderMesh::default();
    assert!(matches!(write_stl_file_new(&d.join("none.stl"), None, &[&empty]), Err(StlWriteError::NothingToExport)));
    assert!(!d.join("none.stl").exists());
}

#[test]
fn a_failed_write_leaves_no_partial_file_and_keeps_an_existing_target() {
    let d = dir("stl_atomic_failure");
    let target = d.join("out.stl");
    std::fs::write(&target, b"old content").unwrap();

    let result = replace_file_atomically(&target, |file| {
        file.write_all(b"half of the new")?;
        Err(std::io::Error::other("disk full"))
    });
    assert!(result.is_err());
    assert_eq!(leftovers(&d), vec!["out.stl"], "no temp file may remain");
    assert_eq!(std::fs::read(&target).unwrap(), b"old content");

    // No target yet: nothing is left either.
    let fresh = d.join("fresh.stl");
    let result = replace_file_atomically(&fresh, |file| {
        file.write_all(b"partial")?;
        Err(std::io::Error::other("disk full"))
    });
    assert!(result.is_err());
    assert_eq!(leftovers(&d), vec!["out.stl"]);

    // Success replaces the target completely.
    replace_file_atomically(&target, |file| file.write_all(b"new")).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert_eq!(leftovers(&d), vec!["out.stl"]);
}

#[test]
fn an_invalid_selection_creates_no_file_and_the_written_file_is_the_complete_stl() {
    let d = dir("stl_file_write");
    let mut bad = cube(None);
    bad.positions[0][0] = f32::NAN;
    let target = d.join("out.stl");
    assert!(matches!(write_stl_file(&target, None, &[&bad]), Err(StlWriteError::NonFinite)));
    assert!(matches!(write_stl_file_with_limit(&target, None, &[&cube(None)], 3), Err(StlWriteError::TooManyTriangles { .. })));
    assert!(leftovers(&d).is_empty(), "no file after a refused export: {:?}", leftovers(&d));

    let mesh = cube(None);
    assert_eq!(write_stl_file(&target, None, &[&mesh]).unwrap(), 12);
    assert_eq!(leftovers(&d), vec!["out.stl"]);
    assert_eq!(std::fs::read(&target).unwrap().len(), 84 + 12 * 50);
}

#[cfg(unix)]
#[test]
fn the_target_keeps_the_default_permissions_and_is_not_hardened() {
    use std::os::unix::fs::PermissionsExt;
    let d = dir("stl_permissions");
    // What the user's umask gives to any new file.
    let reference = d.join("reference");
    std::fs::File::create(&reference).unwrap();
    let default_mode = std::fs::metadata(&reference).unwrap().permissions().mode() & 0o777;

    let target = d.join("out.stl");
    write_stl_file(&target, None, &[&cube(None)]).unwrap();
    assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, default_mode);
}

#[test]
fn a_changed_source_file_is_detected() {
    let d = dir("stl_source_stamp");
    let source = d.join("model.3mf");
    std::fs::write(&source, b"12345").unwrap();
    let stamp = SourceStamp::of(&source).unwrap();
    stamp.ensure_unchanged(&source).unwrap();

    // Same size, different modification time.
    let file = std::fs::OpenOptions::new().write(true).open(&source).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)).unwrap();
    drop(file);
    assert!(matches!(stamp.ensure_unchanged(&source), Err(StlWriteError::SourceChanged)));

    // Different size.
    let stamp = SourceStamp::of(&source).unwrap();
    std::fs::write(&source, b"1234567").unwrap();
    assert!(matches!(stamp.ensure_unchanged(&source), Err(StlWriteError::SourceChanged)));

    // Gone: an I/O error, not "unchanged".
    std::fs::remove_file(&source).unwrap();
    assert!(matches!(stamp.ensure_unchanged(&source), Err(StlWriteError::Io(_))));
}

/// Measurement for the work log, not part of the normal run:
/// `cargo test -p katalog-core --no-default-features --release stl::writer_tests::measure -- --ignored --nocapture`
#[test]
#[ignore]
fn measure_one_million_triangles() {
    fn peak_kib() -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| s.lines().find(|l| l.starts_with("VmHWM:")).map(str::to_owned))
            .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
            .unwrap_or(0)
    }
    // Indexed grid, 708 x 708 quads = 1 002 528 triangles.
    let n = 708usize;
    let mut positions = Vec::with_capacity((n + 1) * (n + 1));
    for y in 0..=n {
        for x in 0..=n {
            positions.push([x as f32 * 0.1, y as f32 * 0.1, ((x * y) % 7) as f32 * 0.01]);
        }
    }
    let mut indices = Vec::with_capacity(n * n * 2);
    for y in 0..n {
        for x in 0..n {
            let a = (y * (n + 1) + x) as u32;
            let (b, c, d) = (a + 1, a + (n as u32 + 1), a + (n as u32 + 2));
            indices.push([a, b, d]);
            indices.push([a, d, c]);
        }
    }
    let mesh = RenderMesh { positions, indices, ..Default::default() };
    let mesh_kib = ((mesh.positions.len() * 12 + mesh.indices.len() * 12) / 1024) as u64;
    let before = peak_kib();
    let d = dir("stl_measure");
    let target = d.join("big.stl");
    let start = std::time::Instant::now();
    let count = write_stl_file(&target, None, &[&mesh]).unwrap();
    let elapsed = start.elapsed();
    let after = peak_kib();
    let size = std::fs::metadata(&target).unwrap().len();
    println!(
        "MEASURE triangles={count} file_bytes={size} write_ms={} mesh_kib={mesh_kib} peak_before_kib={before} peak_after_kib={after} extra_peak_kib={}",
        elapsed.as_millis(),
        after.saturating_sub(before)
    );
    assert_eq!(size, 84 + count * 50);
    std::fs::remove_dir_all(d).ok();
}
