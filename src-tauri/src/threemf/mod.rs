pub mod container;
pub mod error;
pub mod geometry;
pub mod model_xml;

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

pub use error::ThreeMfError;
use crate::geometry::{BoundingBox, RenderMesh};
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
    pub volume_cm3: Option<f64>,
    pub materials: Vec<ThreeMfMaterial>,
    pub metadata: BTreeMap<String, String>,
    pub thumbnail_png: Option<Vec<u8>>,
}

#[allow(dead_code)]
pub fn parse_3mf_file(path: &Path) -> Result<ThreeMfDocument, ThreeMfError> {
    let file = File::open(path)?;
    parse_3mf_reader(file)
}

pub fn parse_3mf_bytes(bytes: &[u8]) -> Result<ThreeMfDocument, ThreeMfError> {
    parse_3mf_reader(std::io::Cursor::new(bytes))
}

/// Liest und loest das 3MF-Paket unter `path` vollstaendig auf (inklusive
/// per p:path referenzierter Objekt-Dateien) und extrahiert daraus
/// render-fertige Mesh-Daten - Positionen bereits weltraum-transformiert,
/// Normalen bleiben `None` (entspricht dem bisherigen Verhalten von
/// three.js' ThreeMFLoader, der fuer 3MF nie Normalen gesetzt hat).
pub fn extract_render_meshes_from_path(path: &Path) -> Result<Vec<RenderMesh>, ThreeMfError> {
    let file = File::open(path)?;
    let package = container::read_package(file)?;
    Ok(extract_render_meshes(&package))
}

pub fn extract_render_meshes(package: &PackageParts) -> Vec<RenderMesh> {
    let mut meshes = Vec::new();
    for item in &package.root_model.build_items {
        let transform = item.transform.unwrap_or_else(Matrix3x4::identity);
        collect_render_meshes(
            package,
            item.path.as_deref(),
            &item.object_id,
            &transform,
            &mut meshes,
        );
    }
    meshes
}

fn collect_render_meshes(
    package: &PackageParts,
    file: Option<&str>,
    object_id: &str,
    transform: &Matrix3x4,
    out: &mut Vec<RenderMesh>,
) {
    let Some(object) = package.lookup_object(file, object_id) else {
        return;
    };

    let is_model_geometry = object.object_type.as_deref().is_none_or(|t| t == "model");

    if is_model_geometry {
        if let Some(mesh) = &object.mesh {
            if !mesh.triangles.is_empty() {
                let positions: Vec<[f32; 3]> = mesh
                    .vertices
                    .iter()
                    .map(|v| {
                        let [x, y, z] = transform.transform_point(*v);
                        [x as f32, y as f32, z as f32]
                    })
                    .collect();
                out.push(RenderMesh {
                    positions,
                    indices: mesh.triangles.clone(),
                    normals: None,
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
        );
    }
}

fn parse_3mf_reader<R: std::io::Read + std::io::Seek>(
    reader: R,
) -> Result<ThreeMfDocument, ThreeMfError> {
    let package = container::read_package(reader)?;

    let (bbox, volume_mm3) = resolve_geometry(&package);

    let dimensions_mm = bbox.is_valid().then(|| bbox.size());
    let volume_cm3 = bbox.is_valid().then_some(volume_mm3 / 1000.0);

    Ok(ThreeMfDocument {
        object_count: package.root_model.build_items.len(),
        dimensions_mm,
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
    })
}

fn resolve_geometry(package: &PackageParts) -> (BoundingBox, f64) {
    let mut bbox = BoundingBox::empty();
    let mut volume_mm3 = 0.0;

    for item in &package.root_model.build_items {
        let transform = item.transform.unwrap_or_else(Matrix3x4::identity);
        accumulate_object(
            package,
            item.path.as_deref(),
            &item.object_id,
            &transform,
            &mut bbox,
            &mut volume_mm3,
        );
    }

    (bbox, volume_mm3)
}

fn accumulate_object(
    package: &PackageParts,
    file: Option<&str>,
    object_id: &str,
    transform: &Matrix3x4,
    bbox: &mut BoundingBox,
    volume_mm3: &mut f64,
) {
    let Some(object) = package.lookup_object(file, object_id) else {
        return;
    };

    // Only "model" objects (the default when unspecified) count toward the
    // catalog's reported size/volume — support/solidsupport/surface objects
    // aren't part of the printed part itself.
    let is_model_geometry = object.object_type.as_deref().is_none_or(|t| t == "model");

    if is_model_geometry {
        if let Some(mesh) = &object.mesh {
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
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

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

        let meshes = extract_render_meshes(&package);

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

        let meshes = extract_render_meshes(&package);

        assert_eq!(meshes.len(), 1);
    }
}
