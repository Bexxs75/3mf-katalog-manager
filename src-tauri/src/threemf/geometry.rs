use super::error::ThreeMfError;

#[derive(Debug, Clone, Copy)]
pub struct Matrix3x4 {
    // Row-major, 4 rows x 3 columns: rows 0-2 are the linear part, row 3 is
    // the translation, matching the 3MF core spec's 12-value transform
    // attribute and its row-vector convention (p' = p * M).
    m: [[f64; 3]; 4],
}

impl Matrix3x4 {
    pub fn identity() -> Self {
        Matrix3x4 {
            m: [
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
            ],
        }
    }

    pub fn parse(s: &str) -> Result<Self, ThreeMfError> {
        let values: Vec<f64> = s
            .split_whitespace()
            .map(|v| v.parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| ThreeMfError::InvalidTransform(s.to_string()))?;
        if values.len() != 12 {
            return Err(ThreeMfError::InvalidTransform(s.to_string()));
        }
        Ok(Matrix3x4 {
            m: [
                [values[0], values[1], values[2]],
                [values[3], values[4], values[5]],
                [values[6], values[7], values[8]],
                [values[9], values[10], values[11]],
            ],
        })
    }

    pub fn transform_point(&self, p: [f64; 3]) -> [f64; 3] {
        let [x, y, z] = p;
        [
            x * self.m[0][0] + y * self.m[1][0] + z * self.m[2][0] + self.m[3][0],
            x * self.m[0][1] + y * self.m[1][1] + z * self.m[2][1] + self.m[3][1],
            x * self.m[0][2] + y * self.m[1][2] + z * self.m[2][2] + self.m[3][2],
        ]
    }

    /// Composes `self` (outer/parent transform) with `inner` (child/component
    /// transform) so that `outer.compose(&inner).transform_point(p)` equals
    /// `outer.transform_point(inner.transform_point(p))`.
    pub fn compose(&self, inner: &Matrix3x4) -> Matrix3x4 {
        let a = self.as_4x4();
        let b = inner.as_4x4();
        let mut full = [[0.0f64; 4]; 4];
        for row in 0..4 {
            for col in 0..4 {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += b[row][k] * a[k][col];
                }
                full[row][col] = sum;
            }
        }
        let mut m = [[0.0; 3]; 4];
        for row in 0..4 {
            m[row] = [full[row][0], full[row][1], full[row][2]];
        }
        Matrix3x4 { m }
    }

    fn as_4x4(&self) -> [[f64; 4]; 4] {
        [
            [self.m[0][0], self.m[0][1], self.m[0][2], 0.0],
            [self.m[1][0], self.m[1][1], self.m[1][2], 0.0],
            [self.m[2][0], self.m[2][1], self.m[2][2], 0.0],
            [self.m[3][0], self.m[3][1], self.m[3][2], 1.0],
        ]
    }
}

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
