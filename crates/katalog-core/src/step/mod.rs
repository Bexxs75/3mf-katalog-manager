//! STEP preview via Open CASCADE.
//!
//! Reads a STEP file into a TopoDS_Shape, tessellates it and builds one RenderMesh
//! per body from it in the existing format (non-indexed, flat normals). The OCCT part
//! is deliberately small: it only delivers one triangle list per body; how the lists
//! become parts (limit, collective part, mesh format) is decided in `crate::step_parts`,
//! which is tested without the feature. Everything here lives behind the `step-preview`
//! feature; without it, STEP files are only cataloged.

pub mod mesh;
pub mod props;
pub mod reader;

use std::{fmt, path::Path};

use opencascade_sys::{top_abs::TopAbs_ShapeEnum, top_exp::TopExp_Explorer_new, topo_ds::TopoDS_Shape};

use crate::geometry::RenderMesh;
use crate::step_parts::{bodies_to_meshes, MAX_PARTS};

/// File size limit above which parsing isn't even attempted. Applies to
/// metadata AND geometry, so an import over a very large file doesn't hang.
pub const MAX_PARSE_BYTES: u64 = 64 * 1024 * 1024;

/// Tessellation tolerance, derived from the bounding box diagonal.
pub const DEFLECTION_DIVISOR: f64 = 500.0;
pub const DEFLECTION_MIN_MM: f64 = 0.01;

/// Limit for the triangle count of a mesh.
pub const MAX_TRIANGLES: usize = 1_500_000;

/// Factor by which the tolerance is increased in the second, coarser attempt.
pub const RETRY_DEFLECTION_FACTOR: f64 = 4.0;

#[derive(Debug)]
pub enum StepError {
    TooLarge { bytes: u64, limit: u64 },
    Io(std::io::Error),
    ReadFailed,
    NoGeometry,
    TessellationFailed,
    TooComplex { triangles: usize, limit: usize },
    TooManyFaces { faces: usize, limit: usize },
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepError::TooLarge { bytes, limit } => write!(
                f,
                "STEP-Datei zu gross fuer die Vorschau: {bytes} Bytes (Grenze {limit})"
            ),
            StepError::Io(err) => write!(f, "STEP-Datei nicht lesbar: {err}"),
            StepError::ReadFailed => write!(f, "STEP-Datei konnte nicht gelesen werden"),
            StepError::NoGeometry => write!(f, "STEP-Datei enthaelt keine Geometrie"),
            StepError::TessellationFailed => {
                write!(f, "STEP-Geometrie konnte nicht vernetzt werden")
            }
            StepError::TooManyFaces { faces, limit } => write!(
                f, "STEP-Geometrie zu komplex: {faces} Flächen (Grenze {limit})"
            ),
            StepError::TooComplex { triangles, limit } => write!(
                f,
                "STEP-Geometrie zu komplex: {triangles} Dreiecke (Grenze {limit})"
            ),
        }
    }
}

impl std::error::Error for StepError {}

/// Pure size check, so the 64 MB rule can be tested without a 64 MB file.
pub fn check_size(bytes: u64, limit: u64) -> Result<(), StepError> {
    if bytes > limit {
        Err(StepError::TooLarge { bytes, limit })
    } else {
        Ok(())
    }
}

/// Metadata of a STEP file for the existing import/rescan paths.
#[derive(Debug, Clone, PartialEq)]
pub struct StepDocument {
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: usize,
}

/// Reads the metadata. The caller deliberately downgrades an error to empty
/// metadata, so the file still stays in the catalog.
pub fn parse_step_file(path: &Path) -> Result<StepDocument, StepError> {
    let bytes = std::fs::metadata(path).map_err(StepError::Io)?.len();
    check_size(bytes, MAX_PARSE_BYTES)?;

    let shape = reader::read_shape(path)?;

    Ok(StepDocument {
        dimensions_mm: props::bounding_box(&shape).map(|bb| bb.size()),
        volume_cm3: props::volume_cm3(&shape),
        object_count: props::body_count(&shape).max(1),
    })
}

fn count_faces(shape: &TopoDS_Shape) -> usize {
    let mut faces = TopExp_Explorer_new(shape, TopAbs_ShapeEnum::TopAbs_FACE);
    let mut count = 0usize;
    while faces.More() {
        count += 1;
        faces.pin_mut().Next();
    }
    count
}

/// One triangle list per SOLID body, in file order. Faces that belong to no solid
/// (surface models, or loose faces next to solids) cannot be told apart per body here;
/// then, and for files without solids, the whole shape stays ONE list as before.
fn body_triangles(shape: &TopoDS_Shape, deflection: f64) -> Result<Vec<Vec<[f64; 3]>>, StepError> {
    let mut bodies: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut triangles = 0usize;
    let mut solid_faces = 0usize;
    let mut solids = TopExp_Explorer_new(shape, TopAbs_ShapeEnum::TopAbs_SOLID);
    while solids.More() {
        let solid = solids.Current();
        solid_faces += count_faces(solid);
        // The limit applies to the whole file, so each body gets what is left.
        match mesh::raw_triangles_limited(solid, deflection, MAX_TRIANGLES - triangles) {
            Ok(list) => {
                triangles += list.len() / 3;
                bodies.push(list);
            }
            // An empty body (no face got a triangulation) is simply not a part.
            Err(StepError::NoGeometry) => {}
            Err(StepError::TooComplex { triangles: more, .. }) => {
                return Err(StepError::TooComplex { triangles: triangles + more, limit: MAX_TRIANGLES })
            }
            Err(err) => return Err(err),
        }
        solids.pin_mut().Next();
    }

    if bodies.is_empty() || solid_faces != count_faces(shape) {
        return Ok(vec![mesh::raw_triangles_limited(shape, deflection, MAX_TRIANGLES)?]);
    }
    Ok(bodies)
}

/// One mesh per body (at most [`MAX_PARTS`]; further bodies form one collective
/// part). A file with a single body or without solids gives one mesh as before.
pub fn parse_step_geometry(path: &Path) -> Result<Vec<RenderMesh>, StepError> {
    let bytes = std::fs::metadata(path).map_err(StepError::Io)?.len();
    check_size(bytes, MAX_PARSE_BYTES)?;

    let shape = reader::read_shape(path)?;
    let diagonal = props::bounding_box(&shape)
        .map(|bb| {
            let size = bb.size();
            (size[0] * size[0] + size[1] * size[1] + size[2] * size[2]).sqrt()
        })
        .ok_or(StepError::NoGeometry)?;

    let deflection = mesh::deflection_for(diagonal);
    let bodies = match body_triangles(&shape, deflection) {
        Ok(bodies) => bodies,
        // A coarser second attempt is better for a preview than a complete failure on
        // a detailed assembly. OCCT only replaces an existing finer triangulation if the
        // requested precision needs it; so we read the shape fresh for the second attempt.
        Err(StepError::TooComplex { .. }) => {
            let shape = reader::read_shape(path)?;
            body_triangles(&shape, deflection * RETRY_DEFLECTION_FACTOR)?
        }
        Err(err) => return Err(err),
    };

    let meshes = bodies_to_meshes(bodies, MAX_PARTS);
    if meshes.is_empty() {
        return Err(StepError::NoGeometry);
    }
    Ok(meshes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_size_accepts_a_file_at_the_limit() {
        assert!(check_size(MAX_PARSE_BYTES, MAX_PARSE_BYTES).is_ok());
    }

    #[test]
    fn check_size_rejects_one_byte_over_the_limit() {
        let err = check_size(MAX_PARSE_BYTES + 1, MAX_PARSE_BYTES).expect_err("must reject");
        assert!(matches!(err, StepError::TooLarge { .. }));
        assert!(err.to_string().contains("zu gross"));
    }

    #[test]
    fn check_size_accepts_a_small_file() {
        assert!(check_size(1024, MAX_PARSE_BYTES).is_ok());
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn parse_step_file_reports_the_cube_metadata() {
        let doc = parse_step_file(&fixture("wuerfel-10mm.step")).expect("metadata");
        let dimensions = doc.dimensions_mm.expect("Masse");
        for value in dimensions {
            assert!((value - 10.0).abs() < 1e-6, "unerwartet: {dimensions:?}");
        }
        assert!((doc.volume_cm3.expect("Volumen") - 1.0).abs() < 1e-6);
        assert_eq!(doc.object_count, 1);
    }

    #[test]
    fn parse_step_geometry_returns_one_usable_mesh_for_the_cube() {
        let meshes = parse_step_geometry(&fixture("wuerfel-10mm.step")).expect("geometry");

        assert_eq!(meshes.len(), 1, "ein Koerper, ein Mesh");
        let mesh = &meshes[0];
        assert_eq!(mesh.positions.len(), 36, "12 Dreiecke, nicht indiziert");
        assert_eq!(mesh.indices.len(), 12);
        assert!(mesh.normals.is_some());
    }

    #[test]
    fn parse_step_geometry_returns_one_mesh_per_body_with_the_offset_kept() {
        let meshes = parse_step_geometry(&fixture("zwei-koerper.step")).expect("geometry");

        assert_eq!(meshes.len(), 2, "zwei Koerper, zwei Meshes");
        let min_x = |m: &RenderMesh| m.positions.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        assert!(
            (min_x(&meshes[0]) - min_x(&meshes[1])).abs() > 1.0,
            "die Koerper liegen auseinander"
        );
        // Same triangles as the old single list, only split.
        let whole = mesh::raw_triangles(&reader::read_shape(&fixture("zwei-koerper.step")).unwrap(), 0.01)
            .unwrap()
            .len()
            / 3;
        assert_eq!(meshes.iter().map(|m| m.indices.len()).sum::<usize>(), whole);
    }

    #[test]
    fn huge_sphere_uses_a_bounded_relative_mesh() {
        let meshes = parse_step_geometry(&fixture("sphere-1e6.step")).expect("geometry");
        assert_eq!(meshes.len(), 1);
        assert!(meshes[0].indices.len() < 10_000);
    }

    #[test]
    fn parse_step_file_reports_both_bodies() {
        let doc = parse_step_file(&fixture("zwei-koerper.step")).expect("metadata");
        assert_eq!(doc.object_count, 2);
    }

    #[test]
    fn parse_step_file_degrades_instead_of_panicking_on_a_non_step_file() {
        let dir = std::env::temp_dir().join("mf_kat_step_api_muell");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("kein-step.stp");
        std::fs::write(&path, b"das ist keine STEP-Datei").expect("write");

        let result = parse_step_file(&path);
        assert!(matches!(
            result,
            Err(StepError::ReadFailed | StepError::NoGeometry)
        ));
    }
}
