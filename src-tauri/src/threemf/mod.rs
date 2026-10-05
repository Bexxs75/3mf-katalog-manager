pub mod container;
pub mod error;
pub mod geometry;
pub mod model_xml;
pub mod plates;
pub mod slice_info;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub use error::ThreeMfError;
pub use slice_info::SliceInfo;
use crate::geometry::{BoundingBox, RenderMesh, RenderColor, RenderGroup};
use container::PackageParts;
use geometry::Matrix3x4;

#[derive(Debug, Clone)]
pub struct ThreeMfMaterial {
    pub name: String,
    pub display_color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ThreeMfDocument {
    pub object_count: usize,
    pub dimensions_mm: Option<[f64; 3]>,
    pub tagging_extent_mm: Option<f64>,
    pub volume_cm3: Option<f64>,
    pub materials: Vec<ThreeMfMaterial>,
    pub metadata: BTreeMap<String, String>,
    pub thumbnail_png: Option<Vec<u8>>,
    pub plate_count: Option<u32>,
    pub slice_info: Option<SliceInfo>,
}

pub fn parse_3mf_file(path: &Path) -> Result<ThreeMfDocument, ThreeMfError> {
    let file = crate::safe_file::open_regular(path)?;
    parse_3mf_reader(file)
}

// Only used by tests (production code only reads from file paths via
// parse_3mf_file), but handy for in-memory test fixtures.
#[cfg(test)]
pub fn parse_3mf_bytes(bytes: &[u8]) -> Result<ThreeMfDocument, ThreeMfError> {
    parse_3mf_reader(std::io::Cursor::new(bytes))
}

/// Reads and fully resolves the 3MF package at `path` (including object files
/// referenced via p:path) and extracts render-ready mesh data from it - positions
/// already world-transformed, normals stay `None` (matching the previous behavior
/// of three.js' ThreeMFLoader, which never set normals for 3MF).
pub fn extract_render_meshes_from_path(path: &Path) -> Result<Vec<RenderMesh>, ThreeMfError> {
    let file = crate::safe_file::open_regular(path)?;
    let package = container::read_geometry_package(file)?;
    extract_render_meshes(&package)
}

/// Maximum nesting depth of the component chain, against stack overflow with extremely deep (acyclic) graphs.
const MAX_COMPONENT_DEPTH: usize = 256;

/// Work allowed for resolving one package's component graph. The depth limit
/// alone doesn't bound the work: an object that references the same child
/// twice on each of 30 levels is tiny but expands to 2^30 instances.
/// Instancing the same object on several branches is legitimate, so this is a
/// budget and not a "visited" set.
const MAX_OBJECT_VISITS: usize = 100_000;
const MAX_INSTANCED_VERTICES: usize = 30_000_000;

struct WorkBudget {
    visits_left: usize,
    vertices_left: usize,
}

impl WorkBudget {
    fn new() -> Self {
        Self { visits_left: MAX_OBJECT_VISITS, vertices_left: MAX_INSTANCED_VERTICES }
    }

    fn visit(&mut self) -> Result<(), ThreeMfError> {
        self.visits_left = self.visits_left.checked_sub(1).ok_or_else(|| {
            ThreeMfError::ResourceLimitExceeded(format!("more than {MAX_OBJECT_VISITS} object instances"))
        })?;
        Ok(())
    }

    fn vertices(&mut self, count: usize) -> Result<(), ThreeMfError> {
        self.vertices_left = self.vertices_left.checked_sub(count).ok_or_else(|| {
            ThreeMfError::ResourceLimitExceeded(format!("more than {MAX_INSTANCED_VERTICES} instanced vertices"))
        })?;
        Ok(())
    }
}

pub fn extract_render_meshes(package: &PackageParts) -> Result<Vec<RenderMesh>, ThreeMfError> {
    let mut meshes = Vec::new();
    let mut budget = WorkBudget::new();
    for item in &package.root_model.build_items {
        let transform = item.transform.unwrap_or_else(Matrix3x4::identity);
        let mut path = Vec::new();
        let start = meshes.len();
        collect_render_meshes(
            package,
            item.path.as_deref(),
            &item.object_id,
            &transform,
            &mut meshes,
            &mut path,
            &mut budget,
        )?;
        if let Some(number) = package.plate_assignments.objects.get(&item.object_id) {
            for mesh in &mut meshes[start..] {
                mesh.plate = Some(*number);
                mesh.plate_name = package.plate_assignments.plates.get(number).cloned().flatten();
            }
        }
    }
    Ok(meshes)
}

#[allow(clippy::too_many_arguments)]
fn collect_render_meshes(
    package: &PackageParts,
    file: Option<&str>,
    object_id: &str,
    transform: &Matrix3x4,
    out: &mut Vec<RenderMesh>,
    path: &mut Vec<(Option<String>, String)>,
    budget: &mut WorkBudget,
) -> Result<(), ThreeMfError> {
    budget.visit()?;
    let key = (file.map(str::to_string), object_id.to_string());
    if path.contains(&key) {
        return Err(ThreeMfError::ComponentCycle);
    }
    if path.len() >= MAX_COMPONENT_DEPTH {
        return Err(ThreeMfError::MaxDepthExceeded);
    }
    path.push(key);

    let Some(object) = package.lookup_object(file, object_id) else {
        path.pop();
        return Ok(());
    };

    let is_model_geometry = object.object_type.as_deref().is_none_or(|t| t == "model");

    if is_model_geometry {
        if let Some(mesh) = &object.mesh {
            if !mesh.triangles.is_empty() {
                budget.vertices(mesh.vertices.len())?;
                let positions: Vec<[f32; 3]> = mesh
                    .vertices
                    .iter()
                    .map(|v| {
                        let [x, y, z] = transform.transform_point(*v);
                        [x as f32, y as f32, z as f32]
                    })
                    .collect();
                let model = match file {
                    None => &package.root_model,
                    Some(path) => &package.referenced_models[path],
                };
                let (indices, groups, palette) = material_indices(model, object, mesh);
                out.push(RenderMesh {
                    positions, indices, groups, palette,
                    plate: None, plate_name: None,
                    normals: None,
                    object_name: object.name.clone(),
                });
            }
        }
    }

    for component in &object.components {
        let child_transform =
            transform.compose(&component.transform.unwrap_or_else(Matrix3x4::identity));
        let child_file = component.path.as_deref().or(file);
        collect_render_meshes(
            package,
            child_file,
            &component.object_id,
            &child_transform,
            out,
            path,
            budget,
        )?;
    }

    path.pop();
    Ok(())
}

fn material_indices(
    model: &model_xml::ParsedModel,
    object: &model_xml::Object,
    mesh: &model_xml::Mesh,
) -> (Vec<[u32; 3]>, Vec<RenderGroup>, Vec<RenderColor>) {
    let mut palette = Vec::new();
    let mut palette_indices = HashMap::new();
    let mut resolved = HashMap::new();
    let mut buckets: BTreeMap<Option<usize>, Vec<[u32; 3]>> = BTreeMap::new();
    for (i, tri) in mesh.triangles.iter().enumerate() {
        let property = mesh.properties.get(i);
        // A triangle can inherit the property group while overriding its index.
        let pid = property.and_then(|p| p.0.as_deref()).or(object.pid.as_deref());
        let index = property.and_then(|p| p.1).or(object.pindex);
        // Resolve each property once, rather than allocating a color per triangle.
        let color_index = *resolved.entry((pid, index)).or_insert_with(|| {
            let material = pid.zip(index)
                .and_then(|(pid, index)| model.material_groups.get(pid)?.get(index))?;
            let c = material.display_color.as_deref()?;
            if !matches!(c.len(), 7 | 9) || !c.starts_with('#')
                || !c[1..].bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let color = RenderColor { name: material.name.clone(), color: c[..7].to_ascii_lowercase() };
            if let Some(index) = palette_indices.get(&color) { return Some(*index); }
            let index = palette.len();
            palette_indices.insert(color.clone(), index);
            palette.push(color);
            Some(index)
        });
        buckets.entry(color_index).or_default().push(*tri);
    }
    if palette.is_empty() { return (mesh.triangles.clone(), Vec::new(), palette); }
    let mut indices = Vec::with_capacity(mesh.triangles.len());
    let mut groups = Vec::new();
    for (color_index, triangles) in buckets {
        groups.push(RenderGroup { start: indices.len() * 3, count: triangles.len() * 3, color_index });
        indices.extend(triangles);
    }
    (indices, groups, palette)
}

fn parse_3mf_reader<R: std::io::Read + std::io::Seek>(
    reader: R,
) -> Result<ThreeMfDocument, ThreeMfError> {
    let package = container::read_package(reader)?;

    let (bbox, volume_mm3, tagging_extent_mm) = resolve_geometry(&package)?;

    let dimensions_mm = bbox.is_valid().then(|| bbox.size());
    let volume_cm3 = bbox.is_valid().then_some(volume_mm3 / 1000.0);

    Ok(ThreeMfDocument {
        object_count: package.root_model.build_items.len(),
        dimensions_mm,
        tagging_extent_mm,
        volume_cm3,
        materials: package
            .root_model
            .materials
            .iter()
            .map(|m| ThreeMfMaterial {
                name: m.name.clone(),
                display_color: m.display_color.clone(),
            })
            .collect(),
        metadata: package.root_model.metadata.clone(),
        thumbnail_png: package.thumbnail.clone(),
        plate_count: package.plate_count,
        slice_info: package.slice_info.clone(),
    })
}

fn resolve_geometry(package: &PackageParts) -> Result<(BoundingBox, f64, Option<f64>), ThreeMfError> {
    let mut bbox = BoundingBox::empty();
    let mut volume_mm3 = 0.0;
    let mut plate_boxes = BTreeMap::new();
    let mut complete_assignments = true;

    let mut budget = WorkBudget::new();
    for item in &package.root_model.build_items {
        let transform = item.transform.unwrap_or_else(Matrix3x4::identity);
        let mut path = Vec::new();
        let mut item_box = BoundingBox::empty();
        accumulate_object(
            package,
            item.path.as_deref(),
            &item.object_id,
            &transform,
            &mut item_box,
            &mut volume_mm3,
            &mut path,
            &mut budget,
        )?;
        if item_box.is_valid() {
            bbox.extend(item_box.min);
            bbox.extend(item_box.max);
            if let Some(plate) = package.plate_assignments.objects.get(&item.object_id) {
                let plate_box = plate_boxes.entry(*plate).or_insert_with(BoundingBox::empty);
                plate_box.extend(item_box.min);
                plate_box.extend(item_box.max);
            } else {
                complete_assignments = false;
            }
        }
    }

    // Incomplete optional metadata must not silently exclude unassigned geometry.
    let tagging_extent_mm = complete_assignments.then(|| plate_boxes.values()
        .flat_map(|bbox: &BoundingBox| bbox.size()).fold(0.0_f64, f64::max))
        .filter(|_| !plate_boxes.is_empty());
    Ok((bbox, volume_mm3, tagging_extent_mm))
}

#[allow(clippy::too_many_arguments)]
fn accumulate_object(
    package: &PackageParts,
    file: Option<&str>,
    object_id: &str,
    transform: &Matrix3x4,
    bbox: &mut BoundingBox,
    volume_mm3: &mut f64,
    path: &mut Vec<(Option<String>, String)>,
    budget: &mut WorkBudget,
) -> Result<(), ThreeMfError> {
    budget.visit()?;
    let key = (file.map(str::to_string), object_id.to_string());
    if path.contains(&key) {
        return Err(ThreeMfError::ComponentCycle);
    }
    if path.len() >= MAX_COMPONENT_DEPTH {
        return Err(ThreeMfError::MaxDepthExceeded);
    }
    path.push(key);

    let Some(object) = package.lookup_object(file, object_id) else {
        path.pop();
        return Ok(());
    };

    // Only "model" objects (the default when unspecified) count toward the
    // catalog's reported size/volume — support/solidsupport/surface objects
    // aren't part of the printed part itself.
    let is_model_geometry = object.object_type.as_deref().is_none_or(|t| t == "model");

    if is_model_geometry {
        if let Some(mesh) = &object.mesh {
            budget.vertices(mesh.vertices.len())?;
            let world_vertices: Vec<[f64; 3]> = mesh
                .vertices
                .iter()
                .map(|v| transform.transform_point(*v))
                .collect();
            for v in &world_vertices {
                bbox.extend(*v);
            }
            *volume_mm3 += crate::geometry::signed_volume(&world_vertices, &mesh.triangles).abs();
        }
    }

    for component in &object.components {
        let child_transform =
            transform.compose(&component.transform.unwrap_or_else(Matrix3x4::identity));
        let child_file = component.path.as_deref().or(file);
        accumulate_object(
            package,
            child_file,
            &component.object_id,
            &child_transform,
            bbox,
            volume_mm3,
            path,
            budget,
        )?;
    }

    path.pop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn size_tags(doc: &ThreeMfDocument) -> Vec<String> {
        crate::tagging::suggest_tags(&crate::tagging::TaggingContext {
            file_name: "test.3mf",
            dimensions_mm: doc.tagging_extent_mm.map(|extent| [extent; 3]).or(doc.dimensions_mm),
            object_count: Some(doc.object_count as i64),
            materials: &[],
        })
    }

    fn plate_size_fixture(length: f64, second: bool, assignments: bool) -> Vec<u8> {
        let object = |id| format!(r#"<object id="{id}"><mesh><vertices>
            <vertex x="0" y="0" z="0"/><vertex x="{length}" y="0" z="0"/>
            <vertex x="0" y="10" z="10"/></vertices><triangles>
            <triangle v1="0" v2="1" v3="2"/></triangles></mesh></object>"#);
        let model = format!(r#"<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
            <resources>{}{}</resources><build><item objectid="1"/>{}</build></model>"#,
            object(1), object(2), if second { r#"<item objectid="2" transform="1 0 0 0 1 0 0 0 1 1000 500 0"/>"# } else { "" });
        let mut bytes = build_zip_with_model_xml(&model);
        if assignments {
            let mut zip = ZipWriter::new_append(std::io::Cursor::new(&mut bytes)).unwrap();
            zip.start_file("Metadata/model_settings.config", SimpleFileOptions::default()).unwrap();
            zip.write_all(br#"<config><plate><metadata key="plater_id" value="1"/>
                <model_instance object_id="1"/></plate><plate><metadata key="plater_id" value="2"/>
                <model_instance object_id="2"/></plate></config>"#).unwrap();
            zip.finish().unwrap();
        }
        bytes
    }

    #[test]
    fn tagging_extent_separates_small_plates_and_preserves_overall_box() {
        let doc = parse_3mf_bytes(&plate_size_fixture(80.0, true, true)).unwrap();
        assert_eq!(doc.tagging_extent_mm, Some(80.0));
        assert_eq!(doc.dimensions_mm, Some([1080.0, 510.0, 10.0]));
        assert!(!size_tags(&doc).iter().any(|tag| tag == "grossformat"));
    }

    #[test]
    fn tagging_extent_keeps_large_single_plate() {
        let doc = parse_3mf_bytes(&plate_size_fixture(250.0, false, true)).unwrap();
        assert_eq!(doc.tagging_extent_mm, Some(250.0));
        assert!(size_tags(&doc).iter().any(|tag| tag == "grossformat"));
    }

    #[test]
    fn tagging_extent_without_assignments_uses_overall_box() {
        let doc = parse_3mf_bytes(&plate_size_fixture(20.0, true, false)).unwrap();
        assert_eq!(doc.tagging_extent_mm, None);
        assert!(size_tags(&doc).iter().any(|tag| tag == "grossformat"));
    }

    #[test]
    fn tagging_extent_miniature_on_distant_plates() {
        let doc = parse_3mf_bytes(&plate_size_fixture(20.0, true, true)).unwrap();
        assert_eq!(doc.tagging_extent_mm, Some(20.0));
        assert!(size_tags(&doc).iter().any(|tag| tag == "miniatur"));
    }

    #[test]
    fn tagging_extent_combines_all_objects_assigned_to_one_plate() {
        let mut package = container::read_package(std::io::Cursor::new(
            plate_size_fixture(20.0, true, true))).unwrap();
        package.plate_assignments.objects.insert("2".into(), 1);
        let (_, _, extent) = resolve_geometry(&package).unwrap();
        assert_eq!(extent, Some(1020.0));
    }

    #[test]
    fn tagging_extent_incomplete_assignments_falls_back_to_overall_box() {
        let mut package = container::read_package(std::io::Cursor::new(
            plate_size_fixture(20.0, true, true))).unwrap();
        package.plate_assignments.objects.remove("2");
        let (bbox, _, extent) = resolve_geometry(&package).unwrap();
        assert_eq!(extent, None);
        assert_eq!(bbox.size(), [1020.0, 510.0, 10.0]);
    }

    #[test]
    fn tagging_extent_creality_fixture() {
        let doc = parse_3mf_bytes(include_bytes!("../../tests/fixtures/creality-3plates.3mf")).unwrap();
        assert!((doc.dimensions_mm.unwrap()[0] - 334.0).abs() < 0.01);
        assert_eq!(doc.tagging_extent_mm, Some(120.0));
        assert!(!size_tags(&doc).iter().any(|tag| tag == "grossformat"));
    }

    fn build_test_3mf() -> Vec<u8> {
        let model_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <metadata name="Title">Test Cube</metadata>
  <metadata name="Designer">Katalog Manager Tests</metadata>
  <resources>
    <basematerials id="1">
      <base name="PLA" displaycolor="#FF8800FF"/>
    </basematerials>
    <object id="1" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="10" y="0" z="0"/>
          <vertex x="10" y="10" z="0"/>
          <vertex x="0" y="10" z="0"/>
          <vertex x="0" y="0" z="10"/>
          <vertex x="10" y="0" z="10"/>
          <vertex x="10" y="10" z="10"/>
          <vertex x="0" y="10" z="10"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="2" v3="1"/>
          <triangle v1="0" v2="3" v3="2"/>
          <triangle v1="4" v2="5" v3="6"/>
          <triangle v1="4" v2="6" v3="7"/>
          <triangle v1="0" v2="1" v3="5"/>
          <triangle v1="0" v2="5" v3="4"/>
          <triangle v1="3" v2="6" v3="2"/>
          <triangle v1="3" v2="7" v3="6"/>
          <triangle v1="0" v2="7" v3="3"/>
          <triangle v1="0" v2="4" v3="7"/>
          <triangle v1="1" v2="2" v3="6"/>
          <triangle v1="1" v2="6" v3="5"/>
        </triangles>
      </mesh>
    </object>
  </resources>
  <build>
    <item objectid="1"/>
  </build>
</model>"##;

        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/>
  <Relationship Id="rel2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail" Target="/Metadata/thumbnail.png"/>
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
  <Default Extension="png" ContentType="image/png"/>
</Types>"#;

        let fake_png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();

            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(content_types_xml.as_bytes()).unwrap();

            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(rels_xml.as_bytes()).unwrap();

            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(model_xml.as_bytes()).unwrap();

            zip.start_file("Metadata/thumbnail.png", options).unwrap();
            zip.write_all(&fake_png).unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn parses_cube_dimensions_and_volume() {
        let bytes = build_test_3mf();
        let doc = parse_3mf_bytes(&bytes).expect("parse should succeed");

        assert_eq!(doc.object_count, 1);
        let dims = doc.dimensions_mm.expect("dimensions present");
        for d in dims {
            assert!((d - 10.0).abs() < 1e-6, "unexpected dimension: {d}");
        }
        let volume = doc.volume_cm3.expect("volume present");
        assert!((volume - 1.0).abs() < 1e-6, "unexpected volume: {volume}");
    }

    #[test]
    fn parses_metadata_and_materials() {
        let bytes = build_test_3mf();
        let doc = parse_3mf_bytes(&bytes).expect("parse should succeed");

        assert_eq!(
            doc.metadata.get("Title").map(String::as_str),
            Some("Test Cube")
        );
        assert_eq!(
            doc.metadata.get("Designer").map(String::as_str),
            Some("Katalog Manager Tests")
        );
        assert_eq!(doc.materials.len(), 1);
        assert_eq!(doc.materials[0].name, "PLA");
        assert_eq!(doc.materials[0].display_color.as_deref(), Some("#FF8800FF"));
    }

    #[test]
    fn extracts_thumbnail_bytes() {
        let bytes = build_test_3mf();
        let doc = parse_3mf_bytes(&bytes).expect("parse should succeed");

        let thumb = doc.thumbnail_png.expect("thumbnail present");
        assert_eq!(&thumb[0..4], b"\x89PNG");
    }

    #[test]
    fn rejects_non_3mf_zip() {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            zip.start_file("readme.txt", options).unwrap();
            zip.write_all(b"not a 3mf file").unwrap();
            zip.finish().unwrap();
        }

        let result = parse_3mf_bytes(&buf);
        assert!(matches!(result, Err(ThreeMfError::MissingRootModel)));
    }

    const ROOT_MODEL_XML_MULTI: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <resources>
    <object id="1" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="10" y="0" z="0"/>
          <vertex x="10" y="10" z="0"/>
          <vertex x="0" y="10" z="0"/>
          <vertex x="0" y="0" z="10"/>
          <vertex x="10" y="0" z="10"/>
          <vertex x="10" y="10" z="10"/>
          <vertex x="0" y="10" z="10"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="2" v3="1"/>
          <triangle v1="0" v2="3" v3="2"/>
          <triangle v1="4" v2="5" v3="6"/>
          <triangle v1="4" v2="6" v3="7"/>
          <triangle v1="0" v2="1" v3="5"/>
          <triangle v1="0" v2="5" v3="4"/>
          <triangle v1="3" v2="6" v3="2"/>
          <triangle v1="3" v2="7" v3="6"/>
          <triangle v1="0" v2="7" v3="3"/>
          <triangle v1="0" v2="4" v3="7"/>
          <triangle v1="1" v2="2" v3="6"/>
          <triangle v1="1" v2="6" v3="5"/>
        </triangles>
      </mesh>
    </object>
  </resources>
  <build>
    <item objectid="1"/>
    <item p:path="/3D/Objects/object_2.model" objectid="1" transform="1 0 0 0 1 0 0 0 1 20 0 0"/>
  </build>
</model>"##;

    const CHILD_MODEL_XML_MULTI: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="10" y="0" z="0"/>
          <vertex x="10" y="10" z="0"/>
          <vertex x="0" y="10" z="0"/>
          <vertex x="0" y="0" z="10"/>
          <vertex x="10" y="0" z="10"/>
          <vertex x="10" y="10" z="10"/>
          <vertex x="0" y="10" z="10"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="2" v3="1"/>
          <triangle v1="0" v2="3" v3="2"/>
          <triangle v1="4" v2="5" v3="6"/>
          <triangle v1="4" v2="6" v3="7"/>
          <triangle v1="0" v2="1" v3="5"/>
          <triangle v1="0" v2="5" v3="4"/>
          <triangle v1="3" v2="6" v3="2"/>
          <triangle v1="3" v2="7" v3="6"/>
          <triangle v1="0" v2="7" v3="3"/>
          <triangle v1="0" v2="4" v3="7"/>
          <triangle v1="1" v2="2" v3="6"/>
          <triangle v1="1" v2="6" v3="5"/>
        </triangles>
      </mesh>
    </object>
  </resources>
</model>"##;

    fn build_multi_file_test_3mf(include_referenced_file: bool) -> Vec<u8> {
        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/>
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();

            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(content_types_xml.as_bytes()).unwrap();

            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(rels_xml.as_bytes()).unwrap();

            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(ROOT_MODEL_XML_MULTI.as_bytes()).unwrap();

            if include_referenced_file {
                zip.start_file("3D/Objects/object_2.model", options)
                    .unwrap();
                zip.write_all(CHILD_MODEL_XML_MULTI.as_bytes()).unwrap();
            }

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn multi_part_3mf_dimensions_and_volume_include_referenced_file_objects() {
        let bytes = build_multi_file_test_3mf(true);
        let doc = parse_3mf_bytes(&bytes).expect("parse should succeed");

        assert_eq!(doc.object_count, 2);
        let dims = doc.dimensions_mm.expect("dimensions present");
        assert!((dims[0] - 30.0).abs() < 1e-6, "unexpected x size: {}", dims[0]);
        assert!((dims[1] - 10.0).abs() < 1e-6, "unexpected y size: {}", dims[1]);
        assert!((dims[2] - 10.0).abs() < 1e-6, "unexpected z size: {}", dims[2]);

        let volume = doc.volume_cm3.expect("volume present");
        assert!((volume - 2.0).abs() < 1e-6, "unexpected volume: {volume}");
    }

    #[test]
    fn extract_render_meshes_returns_world_transformed_geometry_from_both_files() {
        let bytes = build_multi_file_test_3mf(true);
        let package =
            container::read_package(std::io::Cursor::new(bytes)).expect("read should succeed");

        let meshes = extract_render_meshes(&package).expect("resolve should succeed");

        assert_eq!(meshes.len(), 2);
        for mesh in &meshes {
            assert_eq!(mesh.positions.len(), 8);
            assert_eq!(mesh.indices.len(), 12);
            assert!(mesh.normals.is_none());
        }

        assert!(meshes[0].positions.contains(&[0.0, 0.0, 0.0]));
        assert!(meshes[1].positions.contains(&[20.0, 0.0, 0.0]));
    }

    #[test]
    fn extract_render_meshes_skips_missing_referenced_object_without_failing() {
        let bytes = build_multi_file_test_3mf(false);
        let package =
            container::read_package(std::io::Cursor::new(bytes)).expect("read should succeed");

        let meshes = extract_render_meshes(&package).expect("resolve should succeed");

        assert_eq!(meshes.len(), 1);
    }

    #[test]
    fn parses_slice_info_when_present() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let slice_info_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="weight" value="12.40"/>
    <filament id="1" type="PLA" color="#FF8800FF" used_m="5.0" used_g="12.40"/>
  </plate>
</config>"##;

        let mut buf = build_test_3mf();
        // Own archive with a slice info entry; finished zip bytes can't be extended.
        buf.clear();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
            zip.start_file("Metadata/slice_info.config", options).unwrap();
            zip.write_all(slice_info_xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let doc = parse_3mf_bytes(&buf).expect("parse should succeed");
        let slice_info = doc.slice_info.expect("slice info present");
        assert!((slice_info.total_weight_g - 12.40).abs() < 1e-6);
    }

    #[test]
    fn slice_info_is_none_when_absent() {
        let bytes = build_test_3mf();
        let doc = parse_3mf_bytes(&bytes).expect("parse should succeed");
        assert!(doc.slice_info.is_none());
    }

    // --- Component cycles and depth ---

    /// Builds a minimal 3MF zip archive (Content_Types, _rels/.rels, a single
    /// 3D/3dmodel.model) around the given `<model>` XML, like `build_test_3mf()` but
    /// without thumbnail/materials - for tests that only care about the
    /// object/component structure.
    /// Every level references the level below twice: tiny file, 2^depth leaves.
    fn doubling_graph_model(depth: u32) -> String {
        let mut objects = String::from(r#"<object id="1" type="model"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object>"#);
        for id in 2..=depth + 1 {
            objects.push_str(&format!(
                r#"<object id="{id}" type="model"><components><component objectid="{c}"/><component objectid="{c}"/></components></object>"#,
                c = id - 1
            ));
        }
        format!(
            r#"<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>{objects}</resources><build><item objectid="{}"/></build></model>"#,
            depth + 1
        )
    }

    #[test]
    fn exponential_component_graph_hits_the_work_budget_quickly() {
        let bytes = build_zip_with_model_xml(&doubling_graph_model(30));
        let started = std::time::Instant::now();
        assert!(matches!(parse_3mf_bytes(&bytes), Err(ThreeMfError::ResourceLimitExceeded(_))));
        let package = container::read_package(std::io::Cursor::new(bytes)).unwrap();
        assert!(matches!(extract_render_meshes(&package), Err(ThreeMfError::ResourceLimitExceeded(_))));
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "took {:?}", started.elapsed());
    }

    #[test]
    fn many_legitimate_instances_below_the_budget_still_work() {
        let bytes = build_zip_with_model_xml(&doubling_graph_model(10));
        assert!(parse_3mf_bytes(&bytes).is_ok());
        let package = container::read_package(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(extract_render_meshes(&package).unwrap().len(), 1024);
    }

    #[test]
    fn invalid_triangle_index_is_an_error_not_a_crash() {
        let model = r#"<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model"><mesh><vertices><vertex x="0" y="0" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="0"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#;
        let bytes = build_zip_with_model_xml(model);
        assert!(matches!(parse_3mf_bytes(&bytes), Err(ThreeMfError::InvalidGeometry(_))));
        let package = container::read_package(std::io::Cursor::new(bytes));
        assert!(package.is_err() || extract_render_meshes(&package.unwrap()).is_err());
    }

    fn build_zip_with_model_xml(model_xml: &str) -> Vec<u8> {
        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/>
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>"#;

        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();

            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(content_types_xml.as_bytes()).unwrap();

            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(rels_xml.as_bytes()).unwrap();

            zip.start_file("3D/3dmodel.model", options).unwrap();
            zip.write_all(model_xml.as_bytes()).unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    /// Builds a zip archive with `count` chained objects (object 1 references
    /// object 2, object 2 references object 3, ..., the last object has no components)
    /// - acyclic, but possibly deeper than `MAX_COMPONENT_DEPTH`.
    fn build_zip_with_deeply_chained_objects(count: usize) -> Vec<u8> {
        let mut resources = String::new();
        for i in 1..=count {
            if i < count {
                resources.push_str(&format!(
                    r#"<object id="{i}" type="model"><components><component objectid="{next}"/></components></object>"#,
                    i = i,
                    next = i + 1
                ));
            } else {
                resources.push_str(&format!(r#"<object id="{i}" type="model"/>"#, i = i));
            }
        }

        let model_xml = format!(
            r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>{resources}</resources>
  <build><item objectid="1"/></build>
</model>"##,
            resources = resources
        );

        build_zip_with_model_xml(&model_xml)
    }

    const SELF_REFERENCING_MODEL_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model">
      <components><component objectid="1"/></components>
    </object>
  </resources>
  <build><item objectid="1"/></build>
</model>"##;

    const THREE_LEVEL_CHAIN_MODEL_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model">
      <components><component objectid="2"/></components>
    </object>
    <object id="2" type="model">
      <components><component objectid="3"/></components>
    </object>
    <object id="3" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="1" y="0" z="0"/>
          <vertex x="1" y="1" z="0"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2"/>
        </triangles>
      </mesh>
    </object>
  </resources>
  <build><item objectid="1"/></build>
</model>"##;

    const DIAMOND_SHAPED_MODEL_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model">
      <components>
        <component objectid="2"/>
        <component objectid="3"/>
      </components>
    </object>
    <object id="2" type="model">
      <components><component objectid="4"/></components>
    </object>
    <object id="3" type="model">
      <components><component objectid="4"/></components>
    </object>
    <object id="4" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/>
          <vertex x="1" y="0" z="0"/>
          <vertex x="1" y="1" z="0"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2"/>
        </triangles>
      </mesh>
    </object>
  </resources>
  <build><item objectid="1"/></build>
</model>"##;

    fn build_test_3mf_with_component_cycle() -> Vec<u8> {
        let model_xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model">
      <components><component objectid="2"/></components>
    </object>
    <object id="2" type="model">
      <components><component objectid="1"/></components>
    </object>
  </resources>
  <build><item objectid="1"/></build>
</model>"##;
        build_zip_with_model_xml(model_xml)
    }

    #[test]
    fn direct_self_reference_is_rejected_as_a_cycle() {
        let bytes = build_zip_with_model_xml(SELF_REFERENCING_MODEL_XML);
        let result = parse_3mf_bytes(&bytes);
        assert!(matches!(result, Err(ThreeMfError::ComponentCycle)));
    }

    #[test]
    fn a_b_a_cycle_is_rejected() {
        let bytes = build_test_3mf_with_component_cycle();
        let result = parse_3mf_bytes(&bytes);
        assert!(matches!(result, Err(ThreeMfError::ComponentCycle)));
    }

    #[test]
    fn a_b_c_normal_chain_still_works() {
        let bytes = build_zip_with_model_xml(THREE_LEVEL_CHAIN_MODEL_XML);
        assert!(parse_3mf_bytes(&bytes).is_ok());
    }

    #[test]
    fn same_child_object_legitimately_instanced_on_two_branches_still_works() {
        let bytes = build_zip_with_model_xml(DIAMOND_SHAPED_MODEL_XML);
        assert!(parse_3mf_bytes(&bytes).is_ok());
    }

    #[test]
    fn extreme_depth_beyond_max_component_depth_is_rejected() {
        let bytes = build_zip_with_deeply_chained_objects(300);
        let result = parse_3mf_bytes(&bytes);
        assert!(matches!(
            result,
            Err(ThreeMfError::ComponentCycle) | Err(ThreeMfError::MaxDepthExceeded)
        ));
    }
}

#[cfg(test)]
mod render_material_tests {
    use super::*;

    fn render(properties: &str, triangles: &str) -> Vec<RenderMesh> {
        let xml = format!(r##"<model><resources>
        <basematerials id="1"><base name="Red" displaycolor="#FF000080"/>
        <base name="Blue" displaycolor="#0000FF"/><base name="Bad" displaycolor="oops"/>
        <base name="Missing"/></basematerials>
        <colorgroup id="2"><color color="#00FF00"/></colorgroup>
        <object id="3" name="Body" {properties}><mesh><vertices>
        <vertex x="0"/><vertex x="1"/><vertex y="1"/><vertex z="1"/>
        </vertices><triangles>{triangles}</triangles></mesh></object></resources>
        <build><item objectid="3"/></build></model>"##);
        let package = PackageParts { root_model: model_xml::parse_model_xml(&xml).unwrap(),
            referenced_models: Default::default(), thumbnail: None, plate_count: None, plate_assignments: Default::default(), slice_info: None };
        extract_render_meshes(&package).unwrap()
    }

    #[test]
    fn resolves_object_color_ignoring_alpha() {
        let meshes = render(r#"pid="1" pindex="0""#, r#"<triangle v1="0" v2="1" v3="2"/>"#);
        assert_eq!(meshes[0].palette, vec![RenderColor { name: "Red".into(), color: "#ff0000".into() }]);
        assert_eq!(meshes[0].groups, vec![RenderGroup { start: 0, count: 3, color_index: Some(0) }]);
        assert_eq!(meshes[0].object_name.as_deref(), Some("Body"));
    }

    #[test]
    fn triangle_override_buckets_indices_without_duplicating_vertices() {
        let meshes = render(r#"pid="1" pindex="0""#, r#"
            <triangle v1="0" v2="1" v3="2"/>
            <triangle v1="0" v2="2" v3="3" p1="1"/>
            <triangle v1="0" v2="3" v3="1"/>
            <triangle v1="1" v2="2" v3="3" pid="2" p1="0"/>"#);
        let mesh = &meshes[0];
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.palette[1].color, "#0000ff");
        assert_eq!(mesh.groups, vec![RenderGroup { start: 0, count: 3, color_index: None },
            RenderGroup { start: 3, count: 6, color_index: Some(0) },
            RenderGroup { start: 9, count: 3, color_index: Some(1) }]);
        assert_eq!(mesh.indices, vec![[1,2,3], [0,1,2], [0,3,1], [0,2,3]]);
    }

    #[test]
    fn invalid_missing_and_unknown_property_groups_have_no_color() {
        for properties in ["", r#"pid="1" pindex="2""#, r#"pid="1" pindex="3""#,
            r#"pid="1" pindex="99""#, r#"pid="2" pindex="0""#] {
            let meshes = render(properties, r#"<triangle v1="0" v2="1" v3="2"/>"#);
            assert!(meshes[0].palette.is_empty(), "{properties}");
            assert!(meshes[0].groups.is_empty());
        }
    }
}
