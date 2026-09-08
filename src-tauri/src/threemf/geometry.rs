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
