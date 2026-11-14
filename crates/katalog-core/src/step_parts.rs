//! Splitting a STEP file into one mesh per body - the pure part.
//!
//! The OCCT-dependent code (`step`, behind the `step-preview` feature) only
//! delivers the triangle list of each body. Everything that decides how those
//! lists become parts (empty bodies, the 256 limit, the collective part, the
//! mesh format) lives here without the feature, so the standard test run covers it.

use crate::geometry::{compute_flat_normals, RenderMesh};

/// Most parts one STEP file is shown as. More bodies are merged, so the scene does not
/// get thousands of draw calls (draft figure from the specification, to be measured).
pub const MAX_PARTS: usize = 256;

/// `object_name` of the collective part ("further parts"). The interface replaces it
/// with its own translated text; no real object name looks like this.
pub const REST_PART_NAME: &str = "step:rest";

/// Triangle corners (three per triangle, not indexed) -> RenderMesh with flat normals.
pub fn triangles_to_mesh(positions: &[[f64; 3]]) -> RenderMesh {
    let indices: Vec<[u32; 3]> = (0..positions.len() / 3)
        .map(|i| {
            let base = (i * 3) as u32;
            [base, base + 1, base + 2]
        })
        .collect();

    RenderMesh {
        positions: positions.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect(),
        normals: Some(compute_flat_normals(positions, &indices)),
        indices,
        ..Default::default()
    }
}

/// One mesh per body, in file order. Bodies without triangles are dropped. With more
/// than `max_parts` bodies the first `max_parts - 1` stay single and the rest becomes
/// one collective part named [`REST_PART_NAME`], so the result never exceeds `max_parts`.
pub fn bodies_to_meshes(bodies: Vec<Vec<[f64; 3]>>, max_parts: usize) -> Vec<RenderMesh> {
    let max_parts = max_parts.max(1);
    let mut bodies: Vec<Vec<[f64; 3]>> = bodies.into_iter().filter(|b| !b.is_empty()).collect();
    let rest = if bodies.len() > max_parts {
        let tail = bodies.split_off(max_parts - 1);
        Some(tail.into_iter().flatten().collect::<Vec<_>>())
    } else {
        None
    };

    let mut meshes: Vec<RenderMesh> = bodies.iter().map(|b| triangles_to_mesh(b)).collect();
    if let Some(rest) = rest {
        let mut mesh = triangles_to_mesh(&rest);
        mesh.object_name = Some(REST_PART_NAME.to_string());
        meshes.push(mesh);
    }
    meshes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body of `n` triangles that all start at x = `tag`, so the order stays visible.
    fn body(tag: f64, n: usize) -> Vec<[f64; 3]> {
        (0..n).flat_map(|_| [[tag, 0.0, 0.0], [tag + 1.0, 0.0, 0.0], [tag, 1.0, 0.0]]).collect()
    }

    #[test]
    fn one_body_gives_one_mesh_in_the_known_format() {
        let meshes = bodies_to_meshes(vec![body(0.0, 12)], MAX_PARTS);
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].positions.len(), 36);
        assert_eq!(meshes[0].indices.len(), 12);
        assert_eq!(meshes[0].indices[1], [3, 4, 5]);
        assert_eq!(meshes[0].normals.as_ref().map(Vec::len), Some(36));
        assert_eq!(meshes[0].object_name, None);
    }

    #[test]
    fn bodies_keep_their_order_and_their_own_positions() {
        let meshes = bodies_to_meshes(vec![body(0.0, 2), body(30.0, 3)], MAX_PARTS);
        assert_eq!(meshes.len(), 2);
        assert_eq!(meshes[0].positions[0][0], 0.0);
        assert_eq!(meshes[1].positions[0][0], 30.0);
        assert_eq!(meshes[0].indices.len() + meshes[1].indices.len(), 5);
    }

    #[test]
    fn empty_bodies_are_dropped_without_shifting_the_others() {
        let meshes = bodies_to_meshes(vec![Vec::new(), body(7.0, 1), Vec::new()], MAX_PARTS);
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].positions[0][0], 7.0);
        assert!(bodies_to_meshes(vec![Vec::new()], MAX_PARTS).is_empty());
    }

    #[test]
    fn exactly_the_limit_stays_single_parts() {
        let bodies: Vec<_> = (0..MAX_PARTS).map(|i| body(i as f64, 1)).collect();
        let meshes = bodies_to_meshes(bodies, MAX_PARTS);
        assert_eq!(meshes.len(), MAX_PARTS);
        assert!(meshes.iter().all(|m| m.object_name.is_none()));
    }

    #[test]
    fn one_body_over_the_limit_makes_a_collective_part_and_keeps_all_triangles() {
        let bodies: Vec<_> = (0..MAX_PARTS + 1).map(|i| body(i as f64, 2)).collect();
        let total: usize = bodies.iter().map(|b| b.len() / 3).sum();
        let meshes = bodies_to_meshes(bodies, MAX_PARTS);
        assert_eq!(meshes.len(), MAX_PARTS, "never more than the limit");
        let rest = meshes.last().unwrap();
        assert_eq!(rest.object_name.as_deref(), Some(REST_PART_NAME));
        // Bodies MAX_PARTS-1 and MAX_PARTS (two triangles each) are in the collective part.
        assert_eq!(rest.indices.len(), 4);
        assert_eq!(rest.positions[0][0], (MAX_PARTS - 1) as f64 as f32);
        assert_eq!(meshes.iter().map(|m| m.indices.len()).sum::<usize>(), total);
        assert!(meshes[..MAX_PARTS - 1].iter().all(|m| m.object_name.is_none()));
    }

    #[test]
    fn many_more_bodies_end_in_one_collective_part() {
        let bodies: Vec<_> = (0..1000).map(|i| body(i as f64, 1)).collect();
        let meshes = bodies_to_meshes(bodies, 10);
        assert_eq!(meshes.len(), 10);
        assert_eq!(meshes[9].indices.len(), 1000 - 9);
    }

    #[test]
    fn a_limit_of_zero_is_treated_as_one() {
        let meshes = bodies_to_meshes(vec![body(0.0, 1), body(1.0, 1)], 0);
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].indices.len(), 2);
    }
}
