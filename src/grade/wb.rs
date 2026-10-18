//! White balance: Robertson isotherm chromaticity, Bradford CAT and the log
//! LMS gains the grade shader consumes — ported from the referenced
//! implementation (`white_balance.rs` + `image_processing.rs` primaries).

pub const MIN_TEMPERATURE: f64 = 2000.0;
pub const MAX_TEMPERATURE: f64 = 50000.0;
pub const MAX_TINT: f64 = 150.0;
const MIRED_PER_RELATIVE_UNIT: f64 = 1.5;
const TINT_PER_RELATIVE_UNIT: f64 = 1.5;
const TINT_SCALE: f64 = -3000.0;
const MIN_LMS_FRACTION_OF_LOCUS: f64 = 0.25;

/// sRGB white point (D65).
const WP_D65: (f64, f64) = (0.3127, 0.3290);

const BRADFORD: [[f64; 3]; 3] = [
    [0.8951, -0.7502, 0.0389],
    [0.2664, 1.7135, -0.0685],
    [-0.1614, 0.0367, 1.0296],
];

const ROBERTSON_ISOTHERMS: [[f64; 4]; 31] = [
    [0.0, 0.18006, 0.26352, -0.24341],
    [10.0, 0.18066, 0.26589, -0.25479],
    [20.0, 0.18133, 0.26846, -0.26876],
    [30.0, 0.18208, 0.27119, -0.28539],
    [40.0, 0.18293, 0.27407, -0.30470],
    [50.0, 0.18388, 0.27709, -0.32675],
    [60.0, 0.18494, 0.28021, -0.35156],
    [70.0, 0.18611, 0.28342, -0.37915],
    [80.0, 0.18740, 0.28668, -0.40955],
    [90.0, 0.18880, 0.28997, -0.44278],
    [100.0, 0.19032, 0.29326, -0.47888],
    [125.0, 0.19462, 0.30141, -0.58204],
    [150.0, 0.19962, 0.30921, -0.70471],
    [175.0, 0.20525, 0.31647, -0.84901],
    [200.0, 0.21142, 0.32312, -1.0182],
    [225.0, 0.21807, 0.32909, -1.2168],
    [250.0, 0.22511, 0.33439, -1.4512],
    [275.0, 0.23247, 0.33904, -1.7298],
    [300.0, 0.24010, 0.34308, -2.0637],
    [325.0, 0.24792, 0.34655, -2.4681],
    [350.0, 0.25591, 0.34951, -2.9641],
    [375.0, 0.26400, 0.35200, -3.5814],
    [400.0, 0.27218, 0.35407, -4.3633],
    [425.0, 0.28039, 0.35577, -5.3762],
    [450.0, 0.28863, 0.35714, -6.7262],
    [475.0, 0.29685, 0.35823, -8.5955],
    [500.0, 0.30505, 0.35907, -11.324],
    [525.0, 0.31320, 0.35968, -15.628],
    [550.0, 0.32129, 0.36011, -23.325],
    [575.0, 0.32931, 0.36038, -40.770],
    [600.0, 0.33724, 0.36051, -116.45],
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WhiteBalance {
    pub temperature: f64,
    pub tint: f64,
}

/// sRGB primaries (r, g, b).
const PRIMARIES_SRGB: [(f64, f64); 3] = [(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)];

fn xy_to_xyz(xy: (f64, f64)) -> [f64; 3] {
    [xy.0 / xy.1, 1.0, (1.0 - xy.0 - xy.1) / xy.1]
}

/// RGB -> XYZ matrix for an sRGB-like primary set.
fn primaries_to_xyz_matrix() -> [[f64; 3]; 3] {
    // Column-major columns like glam.
    let cols = [
        xy_to_xyz(PRIMARIES_SRGB[0]),
        xy_to_xyz(PRIMARIES_SRGB[1]),
        xy_to_xyz(PRIMARIES_SRGB[2]),
    ];
    let white = xy_to_xyz(WP_D65);
    // invert3 works on row-major storage; convert the column triple.
    let rows = [[cols[0][0], cols[1][0], cols[2][0]], [cols[0][1], cols[1][1], cols[2][1]], [cols[0][2], cols[1][2], cols[2][2]]];
    let inverse = invert3(rows);
    // s = inverse * white.
    let mut s = [0.0f64; 3];
    for row in 0..3 {
        s[row] = inverse[row][0] * white[0]
            + inverse[row][1] * white[1]
            + inverse[row][2] * white[2];
    }
    let mut matrix = [[0.0f64; 3]; 3];
    for (column, scale) in s.iter().enumerate() {
        for row in 0..3 {
            matrix[row][column] = cols[column][row] * scale;
        }
    }
    matrix
}

/// sRGB -> Bradford cone response, column-major triple ((row, col) layout
/// used for shader upload is produced by [`rgb_to_lms_columns`]).
fn rgb_to_lms() -> [[f64; 3]; 3] {
    mul3(BRADFORD, primaries_to_xyz_matrix())
}

/// The same matrix as flattened columns (r, g, b) for uniform upload.
pub fn rgb_to_lms_columns() -> [f32; 9] {
    matrix_to_columns(rgb_to_lms())
}

/// Inverse of [`rgb_to_lms`], flattened the same way.
pub fn lms_to_rgb_columns() -> [f32; 9] {
    let inverse = invert3(rgb_to_lms());
    transposed_to_columns(&inverse)
}

fn matrix_to_columns(matrix: [[f64; 3]; 3]) -> [f32; 9] {
    let mut out = [0.0f32; 9];
    for column in 0..3 {
        for row in 0..3 {
            out[column * 3 + row] = matrix[row][column] as f32;
        }
    }
    out
}

/// invert3 returns the row-major inverse; re-flatten to column storage.
fn transposed_to_columns(matrix: &[[f64; 3]; 3]) -> [f32; 9] {
    // matrix here is row-major; produce columns directly.
    let mut out = [0.0f32; 9];
    for column in 0..3 {
        for row in 0..3 {
            out[column * 3 + row] = matrix[row][column] as f32;
        }
    }
    out
}

fn mul3(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    // Both are column sets: a * b in the (row-major product of columns)
    // convention used across this module.
    let mut out = [[0.0f64; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            let mut value = 0.0;
            for k in 0..3 {
                value += a[row][k] * b[k][column];
            }
            out[row][column] = value;
        }
    }
    out
}

fn invert3(matrix: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let (a, b, c, d, e, f, g, h, i2) = (
        matrix[0][0], matrix[0][1], matrix[0][2],
        matrix[1][0], matrix[1][1], matrix[1][2],
        matrix[2][0], matrix[2][1], matrix[2][2],
    );
    let det = a * (e * i2 - f * h) - b * (d * i2 - f * g) + c * (d * h - e * g);
    let inv_det = 1.0 / det;
    [
        [(e * i2 - f * h) * inv_det, (c * h - b * i2) * inv_det, (b * f - c * e) * inv_det],
        [(f * g - d * i2) * inv_det, (a * i2 - c * g) * inv_det, (c * d - a * f) * inv_det],
        [(d * h - e * g) * inv_det, (b * g - a * h) * inv_det, (a * e - b * d) * inv_det],
    ]
}

fn isotherm_direction(slope: f64) -> (f64, f64) {
    let len = (1.0 + slope * slope).sqrt();
    (1.0 / len, slope / len)
}

impl WhiteBalance {
    /// The neutral point all slider values are relative to.
    pub fn reference() -> Self {
        Self {
            temperature: 6504.0,
            tint: 0.0,
        }
    }

    pub fn clamped(self) -> Self {
        Self {
            temperature: self.temperature.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE),
            tint: self.tint.clamp(-MAX_TINT, MAX_TINT),
        }
    }

    /// A slider-space shift applied to this white balance.
    pub fn shifted(self, relative_temperature: f64, relative_tint: f64) -> Self {
        let mired = 1.0e6 / self.temperature - relative_temperature * MIRED_PER_RELATIVE_UNIT;
        Self {
            temperature: 1.0e6 / mired.max(1.0e6 / MAX_TEMPERATURE),
            tint: self.tint + relative_tint * TINT_PER_RELATIVE_UNIT,
        }
        .clamped()
    }

    fn to_xy(self) -> (f64, f64) {
        let mired = 1.0e6 / self.temperature;
        let index = ROBERTSON_ISOTHERMS
            .iter()
            .skip(1)
            .position(|row| mired < row[0])
            .map_or(ROBERTSON_ISOTHERMS.len() - 1, |position| position + 1);

        let [prev_mired, prev_u, prev_v, prev_slope] = ROBERTSON_ISOTHERMS[index - 1];
        let [next_mired, next_u, next_v, next_slope] = ROBERTSON_ISOTHERMS[index];
        let factor = (next_mired - mired) / (next_mired - prev_mired);

        let prev_direction = isotherm_direction(prev_slope);
        let next_direction = isotherm_direction(next_slope);
        let du = prev_direction.0 * factor + next_direction.0 * (1.0 - factor);
        let dv = prev_direction.1 * factor + next_direction.1 * (1.0 - factor);
        let len = (du * du + dv * dv).sqrt();

        let u = prev_u * factor + next_u * (1.0 - factor) + du / len * self.tint / TINT_SCALE;
        let v = prev_v * factor + next_v * (1.0 - factor) + dv / len * self.tint / TINT_SCALE;
        let denominator = u - 4.0 * v + 2.0;
        (1.5 * u / denominator, v / denominator)
    }

    fn chromaticity_lms(self) -> [f64; 3] {
        let (x, y) = self.to_xy();
        let xyz = [x / y, 1.0, (1.0 - x - y) / y];
        // Bradford * xyz, row-major times vector.
        let mut out = [0.0f64; 3];
        for row in 0..3 {
            out[row] = BRADFORD[row][0] * xyz[0]
                + BRADFORD[row][1] * xyz[1]
                + BRADFORD[row][2] * xyz[2];
        }
        out
    }

    fn lms(self) -> [f64; 3] {
        let locus = Self { tint: 0.0, ..self }.chromaticity_lms();
        let chroma = self.chromaticity_lms();
        let mut out = chroma;
        for channel in 0..3 {
            let floor = locus[channel] * MIN_LMS_FRACTION_OF_LOCUS;
            if out[channel] < floor {
                out[channel] = floor;
            }
        }
        out
    }
}

/// `current` is the white point of the image, `target` the one we adapt to;
/// the returned log gains plug directly into the shader's white balance.
pub fn adaptation_log_gains(current: WhiteBalance, target: WhiteBalance) -> [f32; 3] {
    let current_lms = current.lms();
    let target_lms = target.lms();
    let mut gains = [0.0f32; 3];
    for channel in 0..3 {
        gains[channel] = (current_lms[channel] / target_lms[channel]).ln() as f32;
    }
    gains
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f64) -> bool {
        ((a as f64 - b).abs()) < 1e-4
    }

    #[test]
    fn rgb_to_lms_maps_white_to_bradford_d65() {
        let columns = rgb_to_lms_columns();
        // Multiply by (1, 1, 1): the sum per row must match Bradford*D65.
        let white = xy_to_xyz(WP_D65);
        let mut expected = [0.0f64; 3];
        for row in 0..3 {
            expected[row] = BRADFORD[row][0] * white[0]
                + BRADFORD[row][1] * white[1]
                + BRADFORD[row][2] * white[2];
        }
        for row in 0..3 {
            let value = columns[row] as f64
                + columns[3 + row] as f64
                + columns[6 + row] as f64;
            assert!(((value - expected[row]).abs()) < 1e-3, "row {row}");
        }
    }

    #[test]
    fn inverse_round_trips() {
        let forward = rgb_to_lms();
        let back = invert3(forward);
        // back * forward = identity (row-major product). Compute directly.
        for row in 0..3 {
            for column in 0..3 {
                let mut value = 0.0;
                for k in 0..3 {
                    // Multiply row of `back` (row-major) by column of `forward`.
                    value += back[row][k] * forward[k][column];
                }
                assert!((value - if row == column { 1.0 } else { 0.0 }).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn neutral_shift_gives_zero_gains() {
        let gains = adaptation_log_gains(WhiteBalance::reference(), WhiteBalance::reference());
        assert!(gains.iter().all(|gain| close(*gain, 0.0)));
    }

    #[test]
    fn warming_shifts_gains() {
        let target = WhiteBalance::reference().shifted(50.0, 0.0);
        let gains = adaptation_log_gains(WhiteBalance::reference(), target);
        // Red gain should be positive-ish and the blue strongly negative when
        // the slider shifts warm (this maps current -> target).
        assert!(gains[0].abs() < 1.0);
        assert!(gains.iter().any(|gain| gain.abs() > 0.01));
    }

    #[test]
    fn shifted_is_clamped() {
        let target = WhiteBalance::reference().shifted(-1000.0, 0.0);
        assert!(target.temperature >= MIN_TEMPERATURE);
        assert!(target.temperature <= MAX_TEMPERATURE);
    }
}

#[cfg(test)]
mod debug_dump {
    #[test]
    fn dump() {
        // Temporary diagnostic; removed with the fix.
    }
}
