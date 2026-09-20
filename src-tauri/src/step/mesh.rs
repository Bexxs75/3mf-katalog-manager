//! TopoDS_Shape -> RenderMesh.

use opencascade_sys::{
    b_rep::BRep_Tool_Triangulation,
    b_rep_mesh::IncrementalMesh_new,
    poly::{Handle_Poly_Triangulation_Get, Poly_Triangulation_Node},
    top_abs::{TopAbs_Orientation, TopAbs_ShapeEnum},
    top_exp::TopExp_Explorer_new,
    top_loc::{Location_new, TopLoc_Location_Transformation},
    topo_ds::{TopoDS, TopoDS_Shape},
};

use crate::geometry::{compute_flat_normals, RenderMesh};

use super::{StepError, DEFLECTION_DIVISOR, DEFLECTION_MAX_MM, DEFLECTION_MIN_MM};

/// Toleranz aus der Bounding-Box-Diagonale: fein genug fuer kleine Teile,
/// sparsam genug fuer grosse. Planare Flaechen sind davon unberuehrt.
pub fn deflection_for(diagonal_mm: f64) -> f64 {
    (diagonal_mm / DEFLECTION_DIVISOR).clamp(DEFLECTION_MIN_MM, DEFLECTION_MAX_MM)
}

/// Tesselliert die Shape und liefert eine flache Dreiecksliste: je Dreieck drei
/// Eckpunkte in Weltkoordinaten. Die Liste ist absichtlich nicht indiziert,
/// damit jede Flaeche ihre eigene flache Normale behalten kann.
pub fn raw_triangles(shape: &TopoDS_Shape, deflection: f64) -> Result<Vec<[f64; 3]>, StepError> {
    raw_triangles_limited(shape, deflection, usize::MAX)
}

/// Wie [`raw_triangles`], bricht aber schon beim Lesen des ersten Dreiecks ueber
/// der Grenze ab. So muss eine pathologische STEP-Datei nicht erst vollstaendig
/// in einen riesigen Rust-Vektor kopiert werden, bevor die Begrenzung greift.
pub(crate) fn raw_triangles_limited(
    shape: &TopoDS_Shape,
    deflection: f64,
    max_triangles: usize,
) -> Result<Vec<[f64; 3]>, StepError> {
    // Bewusst nur EIN Mesh fuer die ganze Datei, auch bei mehreren Koerpern.
    let mesh = IncrementalMesh_new(shape, deflection);
    if !mesh.IsDone() {
        return Err(StepError::TessellationFailed);
    }

    let mut positions = Vec::new();
    let mut explorer = TopExp_Explorer_new(shape, TopAbs_ShapeEnum::TopAbs_FACE);

    while explorer.More() {
        let face = TopoDS::Face(explorer.Current());
        let mut location = Location_new();
        let triangulation_handle = BRep_Tool_Triangulation(face, location.pin_mut());

        if let Ok(triangulation) = Handle_Poly_Triangulation_Get(&triangulation_handle) {
            let transform = TopLoc_Location_Transformation(&location);
            let reversed = face.Orientation() == TopAbs_Orientation::TopAbs_REVERSED;

            for index in 1..=triangulation.NbTriangles() {
                let triangles = positions.len() / 3;
                if triangles >= max_triangles {
                    return Err(StepError::TooComplex {
                        triangles: triangles + 1,
                        limit: max_triangles,
                    });
                }

                let triangle = triangulation.Triangle(index);
                let mut corners = [[0.0; 3]; 3];

                for corner_index in 1..=3 {
                    let mut point =
                        Poly_Triangulation_Node(triangulation, triangle.Value(corner_index));
                    point.pin_mut().Transform(&transform);
                    corners[(corner_index - 1) as usize] = [point.X(), point.Y(), point.Z()];
                }

                // Umgekehrte Flaechen brauchen die umgekehrte Windung, damit die
                // daraus berechneten Normalen nach aussen zeigen.
                if reversed {
                    corners.swap(1, 2);
                }
                positions.extend_from_slice(&corners);
            }
        }

        explorer.pin_mut().Next();
    }

    if positions.is_empty() {
        return Err(StepError::NoGeometry);
    }

    Ok(positions)
}

/// Dreiecksliste -> RenderMesh: Positionen sind bereits die Dreiecksecken,
/// die Indizes sind deshalb trivial, die Normalen kommen aus der vorhandenen
/// Flachnormalenberechnung.
pub fn to_render_mesh(positions: &[[f64; 3]]) -> RenderMesh {
    let indices: Vec<[u32; 3]> = (0..positions.len() / 3)
        .map(|i| {
            let base = (i * 3) as u32;
            [base, base + 1, base + 2]
        })
        .collect();

    RenderMesh {
        positions: positions
            .iter()
            .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
            .collect(),
        normals: Some(compute_flat_normals(positions, &indices)),
        indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    #[test]
    fn deflection_scales_with_the_bounding_box() {
        let d = deflection_for(173.205);
        assert!((d - 0.346_41).abs() < 1e-3, "unerwartet: {d}");
    }

    #[test]
    fn deflection_is_clamped_for_tiny_and_huge_parts() {
        assert_eq!(deflection_for(1.0), DEFLECTION_MIN_MM);
        assert_eq!(deflection_for(100_000.0), DEFLECTION_MAX_MM);
    }

    #[test]
    fn cube_yields_twelve_triangles_of_thirty_six_corners() {
        let shape = super::super::reader::read_shape(&fixture("wuerfel-10mm.step")).expect("read");
        let triangles = raw_triangles(&shape, 0.01).expect("tessellate");

        assert_eq!(triangles.len(), 12 * 3, "ein Wuerfel hat 12 Dreiecke");
    }

    #[test]
    fn cube_corners_span_exactly_ten_millimetres() {
        let shape = super::super::reader::read_shape(&fixture("wuerfel-10mm.step")).expect("read");
        let triangles = raw_triangles(&shape, 0.01).expect("tessellate");

        let mut bb = crate::geometry::BoundingBox::empty();
        for p in &triangles {
            bb.extend(*p);
        }
        let size = bb.size();
        for (axis, value) in size.iter().enumerate() {
            assert!((value - 10.0).abs() < 1e-6, "Achse {axis}: {value}");
        }
    }

    #[test]
    fn all_cube_triangle_normals_point_outwards() {
        let shape = super::super::reader::read_shape(&fixture("wuerfel-10mm.step")).expect("read");
        let triangles = raw_triangles(&shape, 0.01).expect("tessellate");
        let center = [5.0f64, 5.0, 5.0];

        for tri in triangles.chunks_exact(3) {
            let normal = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
            let centroid = [
                (tri[0][0] + tri[1][0] + tri[2][0]) / 3.0,
                (tri[0][1] + tri[1][1] + tri[2][1]) / 3.0,
                (tri[0][2] + tri[1][2] + tri[2][2]) / 3.0,
            ];
            let outward = sub(centroid, center);
            assert!(dot(normal, outward) > 0.0, "Normale zeigt nach innen");
        }
    }

    #[test]
    fn two_body_fixture_keeps_the_second_body_offset() {
        let shape = super::super::reader::read_shape(&fixture("zwei-koerper.step")).expect("read");
        let triangles = raw_triangles(&shape, 0.01).expect("tessellate");

        let mut bb = crate::geometry::BoundingBox::empty();
        for p in &triangles {
            bb.extend(*p);
        }
        let size = bb.size();
        assert!((size[0] - 40.0).abs() < 1e-6, "X-Ausdehnung: {}", size[0]);
        assert!((size[1] - 10.0).abs() < 1e-6, "Y-Ausdehnung: {}", size[1]);
    }

    #[test]
    fn raw_triangles_stops_before_allocating_past_the_limit() {
        let shape = super::super::reader::read_shape(&fixture("wuerfel-10mm.step")).expect("read");
        let err = raw_triangles_limited(&shape, 0.01, 5).expect_err("must stop at limit");

        assert!(matches!(
            err,
            StepError::TooComplex {
                triangles: 6,
                limit: 5
            }
        ));
    }

    #[test]
    fn to_render_mesh_produces_a_non_indexed_mesh_with_flat_normals() {
        let triangles = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let mesh = to_render_mesh(&triangles);

        assert_eq!(mesh.positions.len(), 3);
        assert_eq!(mesh.indices, vec![[0u32, 1, 2]]);
        let normals = mesh.normals.expect("Normale muss gesetzt sein");
        assert_eq!(normals.len(), 3);
        for n in &normals {
            assert!((n[2] - 1.0).abs() < 1e-6, "unerwartet: {n:?}");
        }
    }
}
