pub mod container;
pub mod error;
pub mod geometry;
pub mod model_xml;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::Path;

pub use error::ThreeMfError;
use geometry::{BoundingBox, Matrix3x4};
use model_xml::{Object, ParsedModel};

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

fn parse_3mf_reader<R: std::io::Read + std::io::Seek>(
    reader: R,
) -> Result<ThreeMfDocument, ThreeMfError> {
    let package = container::read_package(reader)?;
    let model = model_xml::parse_model_xml(&package.model_xml)?;

    let (bbox, volume_mm3) = resolve_geometry(&model);

    let dimensions_mm = bbox.is_valid().then(|| bbox.size());
    let volume_cm3 = bbox.is_valid().then_some(volume_mm3 / 1000.0);

    Ok(ThreeMfDocument {
        object_count: model.build_items.len(),
        dimensions_mm,
        volume_cm3,
        materials: model
            .materials
            .into_iter()
            .map(|m| ThreeMfMaterial {
                name: m.name,
                display_color: m.display_color,
            })
            .collect(),
        metadata: model.metadata,
        thumbnail_png: package.thumbnail,
    })
}

fn resolve_geometry(model: &ParsedModel) -> (BoundingBox, f64) {
    let mut bbox = BoundingBox::empty();
    let mut volume_mm3 = 0.0;

    for item in &model.build_items {
        let transform = item.transform.unwrap_or_else(Matrix3x4::identity);
        accumulate_object(
            &model.objects,
            &item.object_id,
            &transform,
            &mut bbox,
            &mut volume_mm3,
        );
    }

    (bbox, volume_mm3)
}

fn accumulate_object(
    objects: &HashMap<String, Object>,
    object_id: &str,
    transform: &Matrix3x4,
    bbox: &mut BoundingBox,
    volume_mm3: &mut f64,
) {
    let Some(object) = objects.get(object_id) else {
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
            *volume_mm3 += geometry::signed_volume(&world_vertices, &mesh.triangles).abs();
        }
    }

    for component in &object.components {
        let child_transform =
            transform.compose(&component.transform.unwrap_or_else(Matrix3x4::identity));
        accumulate_object(
            objects,
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
}
