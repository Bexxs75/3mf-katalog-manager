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
        for i in 0..3 {
            if p[i] < self.min[i] {
                self.min[i] = p[i];
            }
            if p[i] > self.max[i] {
                self.max[i] = p[i];
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
