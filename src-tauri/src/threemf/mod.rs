pub mod container;
pub mod error;
pub mod geometry;
pub mod model_xml;
pub mod paint;
pub mod plates;
pub mod slice_info;
pub mod slicer_colors;
pub mod source;

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
const MAX_INSTANCED_TRIANGLES: usize = 60_000_000;

struct WorkBudget {
    visits_left: usize,
    vertices_left: usize,
    triangles_left: usize,
    preview_triangles: usize,
}

impl WorkBudget {
    fn new() -> Self {
        Self { visits_left: MAX_OBJECT_VISITS, vertices_left: MAX_INSTANCED_VERTICES, triangles_left: MAX_INSTANCED_TRIANGLES, preview_triangles: 0 }
    }

    fn visit(&mut self) -> Result<(), ThreeMfError> {
        self.visits_left = self.visits_left.checked_sub(1).ok_or_else(|| {
            ThreeMfError::ResourceLimitExceeded(format!("more than {MAX_OBJECT_VISITS} object instances"))
        })?;
        Ok(())
    }

    fn triangles(&mut self, count: usize) -> Result<(), ThreeMfError> {
        self.triangles_left = self.triangles_left.checked_sub(count).ok_or_else(|| {
            ThreeMfError::ResourceLimitExceeded(format!("more than {MAX_INSTANCED_TRIANGLES} instanced triangles"))
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

/// What a render mesh needs for slicer colors once all meshes are known.
struct SlicerSource<'a> {
    mesh: &'a model_xml::Mesh,
    extruder: u32,
    /// The mesh references a `<colorgroup>`, which has no palette here.
    unread_group: bool,
}

#[derive(Default)]
struct Collected<'a> {
    meshes: Vec<RenderMesh>,
    sources: Vec<SlicerSource<'a>>,
}

/// Where a mesh sits in the slicer's object/part structure.
#[derive(Clone, Copy)]
struct SlicerScope<'a> {
    /// Slicer metadata names build items of the root model only.
    object: Option<&'a str>,
    /// A direct component of the build item's object is a slicer part.
    part: Option<&'a str>,
    at_build_object: bool,
}

pub fn extract_render_meshes(package: &PackageParts) -> Result<Vec<RenderMesh>, ThreeMfError> {
    let mut collected = Collected::default();
    let mut budget = WorkBudget::new();
    for item in &package.root_model.build_items {
        let transform = item.transform.unwrap_or_else(Matrix3x4::identity);
        let mut path = Vec::new();
        let start = collected.meshes.len();
        let scope = SlicerScope {
            object: item.path.is_none().then_some(item.object_id.as_str()),
            part: None,
            at_build_object: true,
        };
        collect_render_meshes(
            package,
            item.path.as_deref(),
            &item.object_id,
            &transform,
            &mut collected,
            &mut path,
            &mut budget,
            scope,
        )?;
        if let Some(number) = package.plate_assignments.objects.get(&item.object_id) {
            for mesh in &mut collected.meshes[start..] {
                mesh.plate = Some(*number);
                mesh.plate_name = package.plate_assignments.plates.get(number).and_then(Option::as_deref).map(display_name);
            }
        }
    }
    apply_slicer_colors(&package.slicer_colors, &mut collected);
    Ok(collected.meshes)
}

/// Standard `<basematerials>` keep priority: slicer colors are only used when
/// no mesh got a standard palette, so two sources are never mixed in one view.
fn apply_slicer_colors(slicer: &slicer_colors::SlicerColors, collected: &mut Collected) {
    let standard = collected.meshes.iter().any(|m| !m.palette.is_empty());
    let has_colors = slicer.has_colors();
    for (mesh, source) in collected.meshes.iter_mut().zip(&collected.sources) {
        let painted = !source.mesh.paint.is_empty() || source.mesh.paint_split;
        let mut unresolved = source.unread_group;
        if !standard {
            unresolved |= source.mesh.paint_unreadable;
            if has_colors {
                let (indices, groups, palette, missing) = slicer_indices(source, &slicer.filaments);
                if !palette.is_empty() {
                    mesh.indices = indices;
                    mesh.groups = groups;
                    mesh.palette = palette;
                }
                unresolved |= missing;
                mesh.paint_simplified = source.mesh.paint_split;
            } else {
                // Paint or a second extruder without a color list cannot be shown.
                unresolved |= painted || source.extruder > 1;
            }
        }
        mesh.colors_unresolved = unresolved;
    }
}

/// Buckets triangles by filament: a painted triangle uses its paint state,
/// an unpainted one the extruder of its part. The last value reports
/// triangles whose filament has no usable color.
fn slicer_indices(
    source: &SlicerSource,
    filaments: &[Option<RenderColor>],
) -> (Vec<[u32; 3]>, Vec<RenderGroup>, Vec<RenderColor>, bool) {
    let mesh = source.mesh;
    let mut palette = Vec::new();
    // Resolved palette index per filament number, filled on first use. Fixed
    // size: this runs once per rendered instance, and the color list is capped.
    let mut resolved = [None::<Option<usize>>; slicer_colors::MAX_FILAMENTS + 1];
    let mut buckets: BTreeMap<Option<usize>, Vec<[u32; 3]>> = BTreeMap::new();
    let mut missing = false;
    // Paint states are u8; a larger extruder keeps unpainted area as state 0,
    // which then resolves to the extruder itself (and has no color).
    let extruder = u8::try_from(source.extruder).unwrap_or(0);
    let mut subdivided = mesh.paint_areas.iter().peekable();
    for (i, tri) in mesh.triangles.iter().enumerate() {
        let state = match subdivided.next_if(|(index, _)| *index as usize == i) {
            Some((_, areas)) => paint::dominant(areas, extruder),
            None => mesh.paint.get(i).copied().unwrap_or(0),
        };
        let filament = if state == 0 { source.extruder } else { u32::from(state) } as usize;
        let color_index = match resolved.get_mut(filament) {
            Some(Some(index)) => *index,
            Some(slot) => {
                let index = filaments.get(filament.wrapping_sub(1)).cloned().flatten().map(|color| {
                    palette.push(color);
                    palette.len() - 1
                });
                *slot = Some(index);
                index
            }
            None => None,
        };
        missing |= color_index.is_none();
        buckets.entry(color_index).or_default().push(*tri);
    }
    let mut indices = Vec::with_capacity(mesh.triangles.len());
    let mut groups = Vec::new();
    for (color_index, triangles) in buckets {
        groups.push(RenderGroup { start: indices.len() * 3, count: triangles.len() * 3, color_index });
        indices.extend(triangles);
    }
    (indices, groups, palette, missing)
}

#[allow(clippy::too_many_arguments)]
fn collect_render_meshes<'a>(
    package: &'a PackageParts,
    file: Option<&str>,
    object_id: &str,
    transform: &Matrix3x4,
    out: &mut Collected<'a>,
    path: &mut Vec<(Option<String>, String)>,
    budget: &mut WorkBudget,
    scope: SlicerScope<'a>,
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
                budget.preview_triangles = budget.preview_triangles.saturating_add(mesh.triangles.len());
                if budget.preview_triangles > model_xml::MAX_PREVIEW_TRIANGLES {
                    return Err(ThreeMfError::ResourceLimitExceeded(format!(
                        "model too large for 3D preview ({} triangles)", budget.preview_triangles
                    )));
                }
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
                out.meshes.push(RenderMesh {
                    positions, indices, groups, palette,
                    object_name: object.name.as_deref().map(display_name),
                    ..Default::default()
                });
                let in_group = |pid: Option<&str>| pid.is_some_and(|pid| model.color_groups.contains(pid));
                out.sources.push(SlicerSource {
                    mesh,
                    extruder: package.slicer_colors.extruder(scope.object, scope.part),
                    unread_group: !model.color_groups.is_empty() && (in_group(object.pid.as_deref())
                        || mesh.properties.iter().any(|(pid, _)| in_group(pid.as_deref()))),
                });
            }
        }
    }

    for component in &object.components {
        let child_transform =
            transform.compose(&component.transform.unwrap_or_else(Matrix3x4::identity));
        let child_file = component.path.as_deref().or(file);
        let child_scope = SlicerScope {
            part: if scope.at_build_object { Some(component.object_id.as_str()) } else { scope.part },
            at_build_object: false,
            ..scope
        };
        collect_render_meshes(
            package,
            child_file,
            &component.object_id,
            &child_transform,
            out,
            path,
            budget,
            child_scope,
        )?;
    }

    path.pop();
    Ok(())
}

/// Longest name the legend shows. Names are copied into every rendered
/// instance, so a huge name in a file must not multiply into gigabytes.
const MAX_DISPLAY_NAME_CHARS: usize = 128;

/// Name cut to `MAX_DISPLAY_NAME_CHARS` characters (never inside a UTF-8 character).
fn display_name(name: &str) -> String {
    match name.char_indices().nth(MAX_DISPLAY_NAME_CHARS) {
        Some((end, _)) => name[..end].to_string(),
        None => name.to_string(),
    }
}

/// `#RRGGBB` or `#RRGGBBAA` as lowercase `#rrggbb`; the preview ignores alpha.
fn normalize_color(c: &str) -> Option<String> {
    if !matches!(c.len(), 7 | 9) || !c.starts_with('#') || !c[1..].bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(c[..7].to_ascii_lowercase())
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
            let color = normalize_color(material.display_color.as_deref()?)?;
            let color = RenderColor { name: display_name(&material.name), color, filament: None };
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
            budget.triangles(mesh.triangles.len())?;
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

    /// Run locally with `cargo test -- --ignored large_`.
    /// Fixed-width XML records make a 300 MB grid without a large source String.
    #[test]
    #[ignore]
    fn large_grid_imports_three_hundred_megabytes_of_xml() {
        struct TempDir(std::path::PathBuf);
        impl Drop for TempDir {
            fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
        }
        let temp = TempDir(std::env::temp_dir().join(format!("large-3mf-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())));
        std::fs::create_dir(&temp.0).unwrap();
        let path = temp.0.join("large-grid.3mf");
        let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file("3D/3dmodel.model", SimpleFileOptions::default()).unwrap();
        zip.write_all(b"<model><resources><object id='1'><mesh><vertices>").unwrap();
        let corners = [[0,0,0], [10,0,0], [10,10,0], [0,10,0], [0,0,10], [10,0,10], [10,10,10], [0,10,10]];
        let faces = [[0,2,1], [0,3,2], [4,5,6], [4,6,7], [0,1,5], [0,5,4], [3,6,2], [3,7,6], [0,7,3], [0,4,7], [1,2,6], [1,6,5]];
        let write_record = |zip: &mut ZipWriter<std::fs::File>, mut record: String| {
            record.extend(std::iter::repeat_n(' ', 148 - record.len()));
            record.push_str("/>");
            zip.write_all(record.as_bytes()).unwrap();
        };
        for cube in 0..100_000 {
            let offset = [(cube % 100) * 10, ((cube / 100) % 100) * 10, (cube / 10_000) * 10];
            for [x,y,z] in corners {
                write_record(&mut zip, format!("<vertex x='{}' y='{}' z='{}'", x + offset[0], y + offset[1], z + offset[2]));
            }
        }
        zip.write_all(b"</vertices><triangles>").unwrap();
        for cube in 0..100_000 {
            for [a,b,c] in faces {
                write_record(&mut zip, format!("<triangle v1='{}' v2='{}' v3='{}'", cube * 8 + a, cube * 8 + b, cube * 8 + c));
            }
        }
        zip.write_all(b"</triangles></mesh></object></resources><build><item objectid='1'/></build></model>").unwrap();
        zip.finish().unwrap();
        let package = container::read_package(std::fs::File::open(&path).unwrap()).unwrap();
        let (bbox, volume, _) = resolve_geometry(&package).unwrap();
        assert_eq!(bbox.size(), [1000.0, 1000.0, 100.0]);
        assert!((volume - 100_000_000.0).abs() < 0.01);
    }

    #[test]
    fn preview_triangle_budget_counts_repeated_instances_but_import_is_unchanged() {
        let mut package = container::read_package(std::io::Cursor::new(build_test_3mf())).unwrap();
        let mesh = package.root_model.objects.get_mut("1").unwrap().mesh.as_mut().unwrap();
        mesh.triangles = vec![[0, 1, 2]; model_xml::MAX_PREVIEW_TRIANGLES / 2 + 1];
        package.root_model.build_items.push(package.root_model.build_items[0].clone());
        assert!(resolve_geometry(&package).is_ok());
        assert!(matches!(extract_render_meshes(&package), Err(ThreeMfError::ResourceLimitExceeded(_))));
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
            referenced_models: Default::default(), thumbnail: None, plate_count: None, plate_assignments: Default::default(), slice_info: None,
            slicer_colors: Default::default() };
        extract_render_meshes(&package).unwrap()
    }

    #[test]
    fn resolves_object_color_ignoring_alpha() {
        let meshes = render(r#"pid="1" pindex="0""#, r#"<triangle v1="0" v2="1" v3="2"/>"#);
        assert_eq!(meshes[0].palette, vec![RenderColor { name: "Red".into(), color: "#ff0000".into(), filament: None }]);
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

#[cfg(test)]
mod instanced_triangle_tests {
    use super::*;

    #[test]
    fn import_limits_triangles_across_component_instances() {
        let triangles = "<triangle v1=\"0\" v2=\"0\" v3=\"0\"/>".repeat(10);
        let components = "<component objectid=\"1\"/>".repeat(3);
        let xml = format!(r#"<model><resources>
            <object id="1"><mesh><vertices><vertex x="1" y="2" z="3"/>
            </vertices><triangles>{triangles}</triangles></mesh></object>
            <object id="2"><components>{components}</components></object>
            </resources><build><item objectid="2"/></build></model>"#);
        let package = PackageParts {
            root_model: model_xml::parse_model_xml(&xml).unwrap(),
            referenced_models: Default::default(), thumbnail: None, plate_count: None,
            plate_assignments: Default::default(), slice_info: None,
            slicer_colors: Default::default(),
        };
        let mut budget = WorkBudget::new();
        // Two complete instances fit; the third must fail before volume work.
        budget.triangles_left = 20;
        let result = accumulate_object(&package, None, "2", &Matrix3x4::identity(),
            &mut BoundingBox::empty(), &mut 0.0, &mut Vec::new(), &mut budget);
        assert!(matches!(result, Err(ThreeMfError::ResourceLimitExceeded(_))));
        assert_eq!(budget.triangles_left, 0);
        let mut budget = WorkBudget::new();
        budget.triangles_left = 30;
        accumulate_object(&package, None, "2", &Matrix3x4::identity(),
            &mut BoundingBox::empty(), &mut 0.0, &mut Vec::new(), &mut budget).unwrap();
        assert_eq!(budget.triangles_left, 0);
    }
}

#[cfg(test)]
mod slicer_color_tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    const NS: &str = r#"xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06""#;
    const FILAMENTS: &str = r##"{"filament_colour": ["#FF9016", "#FFFFFF", "#000000", "#00AE42"], "filament_type": ["PLA", "PLA", "PETG", "PLA"]}"##;

    fn mesh(triangles: &[&str]) -> String {
        let triangles: String = triangles.iter().map(|attrs| format!(r#"<triangle v1="0" v2="1" v3="2" {attrs}/>"#)).collect();
        format!(r#"<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles>{triangles}</triangles></mesh>"#)
    }

    fn package(entries: &[(&str, &str)]) -> PackageParts {
        let mut bytes = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut bytes));
            for (name, content) in entries {
                zip.start_file(*name, SimpleFileOptions::default()).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        container::read_geometry_package(std::io::Cursor::new(bytes)).unwrap()
    }

    fn render(entries: &[(&str, &str)]) -> Vec<RenderMesh> {
        extract_render_meshes(&package(entries)).unwrap()
    }

    /// (color, triangle count) per group, so tests read like the legend.
    fn colors(mesh: &RenderMesh) -> Vec<(Option<&str>, usize)> {
        mesh.groups.iter().map(|g| (g.color_index.map(|i| mesh.palette[i].color.as_str()), g.count / 3)).collect()
    }

    /// Bambu Studio keeps the parts of an object in a separate model file and
    /// names each part by the object id of its component there.
    fn parts_in_object_file(settings: &str, filaments: Option<&str>) -> Vec<RenderMesh> {
        let root = format!(r#"<model {NS}><resources>
            <object id="5" type="model"><components>
              <component p:path="/3D/Objects/object_1.model" objectid="1"/>
              <component p:path="/3D/Objects/object_1.model" objectid="2" transform="1 0 0 0 1 0 0 0 1 5 0 0"/>
              <component p:path="/3D/Objects/object_1.model" objectid="3"/></components></object>
            <object id="6" type="model"><components>
              <component p:path="/3D/Objects/object_1.model" objectid="1"/></components></object>
            </resources><build><item objectid="5"/><item objectid="6" transform="1 0 0 0 1 0 0 0 1 20 0 0"/></build></model>"#);
        let parts = format!(r#"<model {NS}><resources>
            <object id="1" type="model" name="Beak">{}</object>
            <object id="2" type="model" name="Body">{}</object>
            <object id="3" type="model" name="Eye">{}</object></resources><build/></model>"#,
            mesh(&[""]), mesh(&["", ""]), mesh(&[""]));
        let mut entries = vec![("3D/3dmodel.model", root.as_str()), ("3D/Objects/object_1.model", parts.as_str()),
            ("Metadata/model_settings.config", settings)];
        if let Some(filaments) = filaments { entries.push(("Metadata/project_settings.config", filaments)); }
        render(&entries)
    }

    const DUCK_SETTINGS: &str = r#"<config>
        <object id="5"><metadata key="extruder" value="1"/>
          <part id="1" subtype="normal_part"><metadata key="extruder" value="1"/></part>
          <part id="2" subtype="normal_part"><metadata key="extruder" value="2"/></part>
          <part id="3" subtype="normal_part"><metadata key="extruder" value="3"/></part></object>
        <object id="6"><metadata key="extruder" value="4"/>
          <part id="1"><metadata key="extruder" value="0"/></part></object>
        <plate><metadata key="plater_id" value="1"/><model_instance><metadata key="object_id" value="5"/></model_instance></plate>
      </config>"#;

    #[test]
    fn part_extruders_pick_filament_colors() {
        let meshes = parts_in_object_file(DUCK_SETTINGS, Some(FILAMENTS));
        let found: Vec<_> = meshes.iter().map(|m| (m.object_name.as_deref().unwrap(), colors(m))).collect();
        assert_eq!(found, vec![
            ("Beak", vec![(Some("#ff9016"), 1)]),
            ("Body", vec![(Some("#ffffff"), 2)]),
            ("Eye", vec![(Some("#000000"), 1)]),
            // Part extruder 0 inherits the object's extruder.
            ("Beak", vec![(Some("#00ae42"), 1)]),
        ]);
        assert_eq!(meshes[2].palette[0], RenderColor { name: "PETG".into(), color: "#000000".into(), filament: Some(3) });
        assert!(meshes.iter().all(|m| !m.colors_unresolved && !m.paint_simplified));
        assert_eq!(meshes[1].positions[0], [5.0, 0.0, 0.0]);
    }

    #[test]
    fn extruders_without_filament_colors_raise_the_note() {
        let meshes = parts_in_object_file(DUCK_SETTINGS, None);
        assert!(meshes.iter().all(|m| m.palette.is_empty() && m.groups.is_empty()));
        assert_eq!(meshes.iter().map(|m| m.colors_unresolved).collect::<Vec<_>>(), vec![false, true, true, true]);
    }

    #[test]
    fn extruder_beyond_the_color_list_stays_uncolored_with_the_note() {
        let meshes = parts_in_object_file(DUCK_SETTINGS, Some(r##"{"filament_colour": ["#FF9016", "#FFFFFF"]}"##));
        assert!(meshes[2].palette.is_empty() && meshes[2].groups.is_empty());
        assert!(meshes[2].colors_unresolved && meshes[3].colors_unresolved);
        assert_eq!(colors(&meshes[1]), vec![(Some("#ffffff"), 2)]);
        assert!(!meshes[0].colors_unresolved && !meshes[1].colors_unresolved);
    }

    /// Painted project as Bambu Studio writes it: inline part meshes, paint
    /// states on triangles, a support blocker part that is not rendered.
    fn painted(triangles: &[&str], filaments: Option<&str>) -> Vec<RenderMesh> {
        let root = format!(r#"<model {NS}><resources>
            <object id="1" type="model">{}</object>
            <object id="2" type="model">{}</object>
            <object id="3" type="other">{}</object>
            <object id="4" type="model"><components><component objectid="1"/><component objectid="2"/><component objectid="3"/></components></object>
            </resources><build><item objectid="4"/></build></model>"#,
            mesh(&["", ""]), mesh(triangles), mesh(&[r#"paint_color="8""#]));
        let settings = r#"<config><object id="4"><metadata key="extruder" value="1"/>
            <part id="1"><metadata key="extruder" value="1"/></part><part id="2"><metadata key="extruder" value="2"/></part>
            <part id="3" subtype="support_blocker"><metadata key="extruder" value="1"/></part></object></config>"#;
        let mut entries = vec![("3D/3dmodel.model", root.as_str()), ("Metadata/model_settings.config", settings)];
        if let Some(filaments) = filaments { entries.push(("Metadata/project_settings.config", filaments)); }
        render(&entries)
    }

    #[test]
    fn paint_states_override_the_part_extruder_per_triangle() {
        let meshes = painted(&[r#"paint_color="0C""#, "", r#"paint_color="1C""#, r#"paint_color="8""#, r#"paint_color="4""#,
            r#"paint_color="0C""#], Some(FILAMENTS));
        assert_eq!(meshes.len(), 2);
        assert_eq!(colors(&meshes[0]), vec![(Some("#ff9016"), 2)]);
        // Unpainted and state 2 both use filament 2, the part's extruder.
        let mut found = colors(&meshes[1]);
        found.sort();
        assert_eq!(found, vec![(Some("#000000"), 2), (Some("#00ae42"), 1), (Some("#ff9016"), 1), (Some("#ffffff"), 2)]);
        assert_eq!(meshes[1].indices.len(), 6);
        assert!(meshes.iter().all(|m| !m.colors_unresolved && !m.paint_simplified));
    }

    #[test]
    fn subdivided_paint_shows_the_main_color_and_says_so() {
        // Three-side split: three quarters state 4, one quarter unpainted.
        let meshes = painted(&[r#"paint_color="1C1C1C03""#, ""], Some(FILAMENTS));
        assert_eq!(colors(&meshes[1]), vec![(Some("#00ae42"), 1), (Some("#ffffff"), 1)]);
        assert!(meshes[1].paint_simplified && !meshes[0].paint_simplified);
        assert!(!meshes[1].colors_unresolved);
    }

    #[test]
    fn unpainted_part_of_a_subdivided_triangle_counts_for_each_instance_extruder() {
        // Shares: state 2 = 3/8, state 3 = 3/8, unpainted = 1/4. The same
        // mesh is used by two objects whose parts have different extruders.
        let root = format!(r#"<model {NS}><resources>
            <object id="1" type="model">{}</object>
            <object id="5" type="model"><components><component objectid="1"/></components></object>
            <object id="6" type="model"><components><component objectid="1"/></components></object>
            </resources><build><item objectid="5"/><item objectid="6"/></build></model>"#,
            mesh(&[r#"paint_color="0C8100C83""#, ""]));
        let settings = r#"<config><object id="5"><part id="1"><metadata key="extruder" value="3"/></part></object>
            <object id="6"><part id="1"><metadata key="extruder" value="1"/></part></object></config>"#;
        let meshes = render(&[("3D/3dmodel.model", &root), ("Metadata/model_settings.config", settings),
            ("Metadata/project_settings.config", FILAMENTS)]);
        // Extruder 3: filament 3 covers 5/8 of the painted triangle.
        assert_eq!(colors(&meshes[0]), vec![(Some("#000000"), 2)]);
        // Extruder 1: filaments 2 and 3 tie at 3/8, the lower number wins.
        assert_eq!(colors(&meshes[1]), vec![(Some("#ffffff"), 1), (Some("#ff9016"), 1)]);
        assert!(meshes.iter().all(|m| m.paint_simplified && !m.colors_unresolved));
    }

    #[test]
    fn long_names_are_cut_before_they_are_copied_into_every_instance() {
        let long = "Ä".repeat(1 << 20);
        let components = r#"<component objectid="1"/>"#.repeat(1000);
        let root = format!(r##"<model {NS}><resources>
            <basematerials id="9"><base name="{long}" displaycolor="#112233"/></basematerials>
            <object id="1" type="model" name="{long}">{}</object>
            <object id="2" type="model" pid="9" pindex="0">{}</object>
            <object id="5" type="model"><components>{components}</components></object>
            </resources><build><item objectid="5"/><item objectid="2"/></build></model>"##, mesh(&[""]), mesh(&[""]));
        let filaments = format!(r##"{{"filament_colour": ["#FF9016"], "filament_type": ["{long}"]}}"##);
        let plates = format!(r#"<config><plate><metadata key="plater_id" value="1"/><metadata key="plater_name" value="{long}"/>
            <model_instance><metadata key="object_id" value="5"/></model_instance></plate></config>"#);
        let meshes = render(&[("3D/3dmodel.model", &root), ("Metadata/project_settings.config", &filaments),
            ("Metadata/model_settings.config", &plates)]);
        assert_eq!(meshes.len(), 1001);
        assert!(meshes[..1000].iter().all(|m| m.plate_name.as_deref() == Some(&*"Ä".repeat(MAX_DISPLAY_NAME_CHARS))));
        let expected = "Ä".repeat(MAX_DISPLAY_NAME_CHARS);
        // Standard basematerials win, so the slicer name is checked separately below.
        assert!(meshes.iter().all(|m| m.object_name.as_deref().is_none_or(|n| n == expected)));
        assert_eq!(meshes[1000].palette[0].name, expected);
        let slicer = render(&[("3D/3dmodel.model", &root.replace(r#" pid="9" pindex="0""#, "")),
            ("Metadata/project_settings.config", &filaments)]);
        assert!(slicer.iter().all(|m| m.palette.len() == 1 && m.palette[0].name == expected));
        assert_eq!(display_name("PLA"), "PLA");
    }

    #[test]
    fn paint_without_colors_or_damaged_paint_raises_the_note() {
        let meshes = painted(&[r#"paint_color="8""#], None);
        assert!(meshes[1].colors_unresolved && meshes[1].palette.is_empty());
        let meshes = painted(&[r#"paint_color="ZZ""#, ""], Some(FILAMENTS));
        assert_eq!(colors(&meshes[1]), vec![(Some("#ffffff"), 2)]);
        assert!(meshes[1].colors_unresolved);
    }

    #[test]
    fn single_filament_projects_get_their_color_without_the_note() {
        let root = format!(r#"<model {NS}><resources><object id="1" type="model">{}</object></resources><build><item objectid="1"/></build></model>"#, mesh(&[""]));
        let meshes = render(&[("3D/3dmodel.model", &root), ("Metadata/project_settings.config", r##"{"filament_colour": ["#1E90FF"]}"##)]);
        assert_eq!(colors(&meshes[0]), vec![(Some("#1e90ff"), 1)]);
        assert!(!meshes[0].colors_unresolved);
        let meshes = render(&[("3D/3dmodel.model", &root)]);
        assert!(meshes[0].palette.is_empty() && !meshes[0].colors_unresolved);
    }

    #[test]
    fn standard_basematerials_win_over_slicer_colors_unchanged() {
        let root = format!(r##"<model {NS}><resources>
            <basematerials id="10"><base name="Orange" displaycolor="#FF6A13FF"/><base name="Blau" displaycolor="#1E90FFFF"/></basematerials>
            <object id="1" type="model" pid="10" pindex="0">{}</object>
            <object id="2" type="model">{}</object></resources>
            <build><item objectid="1"/><item objectid="2"/></build></model>"##,
            mesh(&["", r#"pid="10" p1="1" paint_color="8""#]), mesh(&[r#"paint_color="0C""#]));
        let settings = r#"<config><object id="1"><metadata key="extruder" value="2"/></object><object id="2"><metadata key="extruder" value="3"/></object></config>"#;
        let plain = render(&[("3D/3dmodel.model", &root)]);
        let with_slicer = render(&[("3D/3dmodel.model", &root), ("Metadata/model_settings.config", settings),
            ("Metadata/project_settings.config", FILAMENTS)]);
        assert_eq!(plain, with_slicer);
        assert_eq!(colors(&plain[0]), vec![(Some("#ff6a13"), 1), (Some("#1e90ff"), 1)]);
        assert!(plain[1].palette.is_empty() && plain.iter().all(|m| !m.colors_unresolved));
    }

    #[test]
    fn color_groups_raise_the_note_without_a_palette() {
        let root = format!(r##"<model {NS} xmlns:m="http://schemas.microsoft.com/3dmanufacturing/material/2015/02"><resources>
            <m:colorgroup id="20"><m:color color="#FF0000FF"/></m:colorgroup>
            <object id="1" type="model">{}</object><object id="2" type="model">{}</object></resources>
            <build><item objectid="1"/><item objectid="2"/></build></model>"##,
            mesh(&[r#"pid="20" p1="0""#]), mesh(&[""]));
        let meshes = render(&[("3D/3dmodel.model", &root)]);
        assert!(meshes.iter().all(|m| m.palette.is_empty()));
        assert_eq!(meshes.iter().map(|m| m.colors_unresolved).collect::<Vec<_>>(), vec![true, false]);
    }

    #[test]
    fn import_skips_paint_strings() {
        let root = format!(r#"<model {NS}><resources><object id="1" type="model">{}</object></resources><build><item objectid="1"/></build></model>"#,
            mesh(&[r#"paint_color="8""#]));
        let mut bytes = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut bytes));
            zip.start_file("3D/3dmodel.model", SimpleFileOptions::default()).unwrap();
            zip.write_all(root.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let imported = container::read_package(std::io::Cursor::new(bytes)).unwrap();
        assert!(imported.root_model.objects["1"].mesh.as_ref().unwrap().paint.is_empty());
        assert!(!imported.slicer_colors.has_colors());
    }

    /// Manual check against real slicer projects that cannot live in the
    /// repository: `THREEMF_COLOR_FILES=/path/a.3mf:/path/b.3mf cargo test
    /// --no-default-features -- --ignored --nocapture real_slicer_projects`.
    #[test]
    #[ignore]
    fn real_slicer_projects_report_their_colors() {
        let Ok(files) = std::env::var("THREEMF_COLOR_FILES") else { return };
        for file in files.split(':').filter(|f| !f.is_empty()) {
            let started = std::time::Instant::now();
            let meshes = extract_render_meshes_from_path(std::path::Path::new(file)).unwrap();
            let elapsed = started.elapsed();
            let mut per_color: BTreeMap<String, usize> = BTreeMap::new();
            for mesh in &meshes {
                for group in &mesh.groups {
                    let key = group.color_index.map_or("(none)".to_string(), |i| {
                        let c = &mesh.palette[i];
                        format!("filament {:?} {} {}", c.filament, c.name, c.color)
                    });
                    *per_color.entry(key).or_default() += group.count / 3;
                }
                if mesh.groups.is_empty() { *per_color.entry("(no palette)".into()).or_default() += mesh.indices.len(); }
            }
            let peak = std::fs::read_to_string("/proc/self/status").ok()
                .and_then(|s| s.lines().find(|l| l.starts_with("VmHWM")).map(str::to_string));
            println!("{file}: {} meshes in {elapsed:?}, {peak:?}, unresolved={}, simplified={}",
                meshes.len(), meshes.iter().any(|m| m.colors_unresolved), meshes.iter().any(|m| m.paint_simplified));
            for (color, triangles) in per_color { println!("  {color}: {triangles} triangles"); }
        }
    }
}
