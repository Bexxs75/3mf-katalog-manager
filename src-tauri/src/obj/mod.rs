mod error;
mod parser;

use std::path::Path;

pub use error::ObjError;

use crate::geometry::{signed_volume, BoundingBox};

#[derive(Debug, Clone)]
pub struct ObjDocument {
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
}

pub fn parse_obj_file(path: &Path) -> Result<ObjDocument, ObjError> {
    let bytes = std::fs::read(path)?;
    parse_obj_bytes(&bytes)
}

pub fn parse_obj_bytes(bytes: &[u8]) -> Result<ObjDocument, ObjError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ObjError::Parse("not valid ASCII/UTF-8 OBJ text".to_string()))?;
    let (vertices, triangles) = parser::parse(text)?;

    let mut bbox = BoundingBox::empty();
    for v in &vertices {
        bbox.extend(*v);
    }
    let volume_mm3 = signed_volume(&vertices, &triangles).abs();

    let dimensions_mm = bbox.is_valid().then(|| bbox.size());
    let volume_cm3 = bbox.is_valid().then_some(volume_mm3 / 1000.0);

    Ok(ObjDocument {
        dimensions_mm,
        volume_cm3,
    })
}

pub fn parse_obj_geometry(bytes: &[u8]) -> Result<crate::geometry::RenderMesh, ObjError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ObjError::Parse("not valid ASCII/UTF-8 OBJ text".to_string()))?;
    let (vertices, triangles) = parser::parse(text)?;
    let normals = crate::geometry::compute_flat_normals(&vertices, &triangles);
    let positions: Vec<[f32; 3]> = vertices
        .iter()
        .map(|v| [v[0] as f32, v[1] as f32, v[2] as f32])
        .collect();
    Ok(crate::geometry::RenderMesh {
        positions,
        indices: triangles,
        normals: Some(normals),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE_OBJ: &str = "\
v 0.0 0.0 0.0
v 10.0 0.0 0.0
v 10.0 10.0 0.0
v 0.0 10.0 0.0
v 0.0 0.0 10.0
v 10.0 0.0 10.0
v 10.0 10.0 10.0
v 0.0 10.0 10.0
f 1 3 2
f 1 4 3
f 5 6 7
f 5 7 8
f 1 2 6
f 1 6 5
f 4 7 3
f 4 8 7
f 1 8 4
f 1 5 8
f 2 3 7
f 2 7 6
";

    #[test]
    fn parses_a_10mm_cube_dimensions_and_volume() {
        let doc = parse_obj_bytes(CUBE_OBJ.as_bytes()).unwrap();
        assert_eq!(doc.dimensions_mm, Some([10.0, 10.0, 10.0]));
        assert!((doc.volume_cm3.unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_obj_geometry_returns_a_render_mesh_with_normals() {
        let mesh = parse_obj_geometry(CUBE_OBJ.as_bytes()).unwrap();
        assert_eq!(mesh.positions.len(), 8);
        assert_eq!(mesh.indices.len(), 12);
        assert!(mesh.normals.is_some());
    }

    #[test]
    fn parse_obj_bytes_rejects_non_utf8_content() {
        let invalid = vec![0xFF, 0xFE, 0xFD];
        assert!(parse_obj_bytes(&invalid).is_err());
    }
}
