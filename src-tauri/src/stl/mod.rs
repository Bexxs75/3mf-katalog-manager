mod error;
mod parser;

use std::path::Path;

pub use error::StlError;

use crate::geometry::{signed_volume, BoundingBox};

#[derive(Debug, Clone)]
pub struct StlDocument {
    pub triangle_count: usize,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
}

#[allow(dead_code)]
pub fn parse_stl_file(path: &Path) -> Result<StlDocument, StlError> {
    let bytes = std::fs::read(path)?;
    parse_stl_bytes(&bytes)
}

pub fn parse_stl_bytes(bytes: &[u8]) -> Result<StlDocument, StlError> {
    let (vertices, triangles) = parser::parse(bytes)?;

    let mut bbox = BoundingBox::empty();
    for v in &vertices {
        bbox.extend(*v);
    }
    let volume_mm3 = signed_volume(&vertices, &triangles).abs();

    let dimensions_mm = bbox.is_valid().then(|| bbox.size());
    let volume_cm3 = bbox.is_valid().then_some(volume_mm3 / 1000.0);

    Ok(StlDocument {
        triangle_count: triangles.len(),
        dimensions_mm,
        volume_cm3,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE_VERTICES: [[f64; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 0.0],
        [0.0, 10.0, 0.0],
        [0.0, 0.0, 10.0],
        [10.0, 0.0, 10.0],
        [10.0, 10.0, 10.0],
        [0.0, 10.0, 10.0],
    ];

    const CUBE_TRIANGLES: [[usize; 3]; 12] = [
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

    fn cube_facets() -> Vec<[[f64; 3]; 3]> {
        CUBE_TRIANGLES
            .iter()
            .map(|tri| {
                [
                    CUBE_VERTICES[tri[0]],
                    CUBE_VERTICES[tri[1]],
                    CUBE_VERTICES[tri[2]],
                ]
            })
            .collect()
    }

    fn build_ascii_cube() -> Vec<u8> {
        let mut text = String::from("solid cube\n");
        for facet in cube_facets() {
            text.push_str("  facet normal 0 0 0\n    outer loop\n");
            for v in facet {
                text.push_str(&format!("      vertex {} {} {}\n", v[0], v[1], v[2]));
            }
            text.push_str("    endloop\n  endfacet\n");
        }
        text.push_str("endsolid cube\n");
        text.into_bytes()
    }

    fn build_binary_cube() -> Vec<u8> {
        let facets = cube_facets();
        let mut buf = vec![0u8; 80];
        buf.extend_from_slice(&(facets.len() as u32).to_le_bytes());
        for facet in facets {
            buf.extend_from_slice(&[0u8; 12]); // normal
            for v in facet {
                for &c in &v {
                    buf.extend_from_slice(&(c as f32).to_le_bytes());
                }
            }
            buf.extend_from_slice(&[0u8; 2]); // attribute byte count
        }
        buf
    }

    #[test]
    fn parses_ascii_cube_dimensions_and_volume() {
        let bytes = build_ascii_cube();
        let doc = parse_stl_bytes(&bytes).expect("ascii parse should succeed");

        assert_eq!(doc.triangle_count, 12);
        let dims = doc.dimensions_mm.expect("dimensions present");
        for d in dims {
            assert!((d - 10.0).abs() < 1e-6, "unexpected dimension: {d}");
        }
        let volume = doc.volume_cm3.expect("volume present");
        assert!((volume - 1.0).abs() < 1e-6, "unexpected volume: {volume}");
    }

    #[test]
    fn parses_binary_cube_dimensions_and_volume() {
        let bytes = build_binary_cube();
        let doc = parse_stl_bytes(&bytes).expect("binary parse should succeed");

        assert_eq!(doc.triangle_count, 12);
        let dims = doc.dimensions_mm.expect("dimensions present");
        for d in dims {
            assert!((d - 10.0).abs() < 1e-4, "unexpected dimension: {d}");
        }
        let volume = doc.volume_cm3.expect("volume present");
        assert!((volume - 1.0).abs() < 1e-4, "unexpected volume: {volume}");
    }

    #[test]
    fn rejects_malformed_ascii() {
        let bytes = b"solid broken\n  facet normal 0 0 0\n    outer loop\n      vertex 0 0\n    endloop\n  endfacet\nendsolid broken\n".to_vec();
        let result = parse_stl_bytes(&bytes);
        assert!(matches!(result, Err(StlError::Parse(_))));
    }
}
