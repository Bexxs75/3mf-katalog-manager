#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl BoundingBox {
    pub fn empty() -> Self {
        BoundingBox {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }

    pub fn extend(&mut self, p: [f64; 3]) {
        for ((min, max), value) in self.min.iter_mut().zip(self.max.iter_mut()).zip(p) {
            if value < *min {
                *min = value;
            }
            if value > *max {
                *max = value;
            }
        }
    }

    pub fn is_valid(&self) -> bool {
        self.min[0] <= self.max[0]
    }

    pub fn size(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

/// Signed-tetrahedron volume (mm^3) of a closed, consistently-oriented
/// triangle mesh, computed via the divergence theorem.
pub fn signed_volume(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> f64 {
    let mut sum = 0.0;
    for tri in triangles {
        let v1 = vertices[tri[0] as usize];
        let v2 = vertices[tri[1] as usize];
        let v3 = vertices[tri[2] as usize];
        sum += v1[0] * (v2[1] * v3[2] - v2[2] * v3[1])
            - v1[1] * (v2[0] * v3[2] - v2[2] * v3[0])
            + v1[2] * (v2[0] * v3[1] - v2[1] * v3[0]);
    }
    sum / 6.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<[u32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
}

/// Flat normal per triangle for all three corners, like three.js'
/// `computeVertexNormals()` for non-indexed geometry (the previous STL shading).
pub fn compute_flat_normals(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0f32; 3]; vertices.len()];
    for tri in triangles {
        let a = vertices[tri[0] as usize];
        let b = vertices[tri[1] as usize];
        let c = vertices[tri[2] as usize];

        let cb = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
        let ab = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cross = [
            cb[1] * ab[2] - cb[2] * ab[1],
            cb[2] * ab[0] - cb[0] * ab[2],
            cb[0] * ab[1] - cb[1] * ab[0],
        ];
        let len = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let n = if len > 0.0 {
            [
                (cross[0] / len) as f32,
                (cross[1] / len) as f32,
                (cross[2] / len) as f32,
            ]
        } else {
            [0.0, 0.0, 0.0]
        };

        for &idx in tri {
            normals[idx as usize] = n;
        }
    }
    normals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_flat_normals_returns_outward_unit_normal_for_single_triangle() {
        // Triangle in the xy plane (z=0); expected normal per the (C-B) x (A-B)
        // convention (see compute_flat_normals).
        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let triangles = [[0u32, 1, 2]];

        let normals = compute_flat_normals(&vertices, &triangles);

        assert_eq!(normals.len(), 3);
        for n in &normals {
            assert!(n[0].abs() < 1e-6, "unexpected x: {n:?}");
            assert!(n[1].abs() < 1e-6, "unexpected y: {n:?}");
            assert!((n[2] - 1.0).abs() < 1e-6, "unexpected z: {n:?}");
        }
    }

    #[test]
    fn compute_flat_normals_returns_zero_for_degenerate_triangle() {
        let vertices = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        let triangles = [[0u32, 1, 2]];

        let normals = compute_flat_normals(&vertices, &triangles);

        assert_eq!(normals[0], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn render_mesh_is_constructible_and_comparable() {
        let a = RenderMesh {
            positions: vec![[0.0, 0.0, 0.0]],
            indices: vec![],
            normals: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
