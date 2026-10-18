//! Edit state shared by the right panel, the preview grade pass and export.

pub mod gpu;

/// A crop rectangle in pipeline-space pixels (after orientation steps and flips).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Per-image edit recipe. Matches the semantics of the referenced RapidRAW
/// pipeline: geometry is applied first (orientation steps, flips, free
/// rotation, crop), then color grading, all in one compute pass.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EditState {
    pub crop: Option<CropRect>,
    /// Free rotation in degrees, -45..=45.
    pub rotation: f32,
    /// User 90-degree steps, 0..=3 (clockwise).
    pub orientation_steps: u32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub adjustments: Adjustments,
}

impl EditState {
    /// Dimensions of the image after orientation steps and flips, given the
    /// base (decoded) texture size.
    pub fn oriented_size(&self, base: [u32; 2]) -> [u32; 2] {
        if self.orientation_steps % 2 == 1 {
            [base[1], base[0]]
        } else {
            base
        }
    }

    /// Dimensions of the on-screen result: the crop rectangle when set,
    /// otherwise the oriented size.
    pub fn display_size(&self, base: [u32; 2]) -> [u32; 2] {
        let oriented = self.oriented_size(base);
        match self.crop {
            Some(crop) => [crop.w as u32, crop.h as u32],
            None => oriented,
        }
    }

    /// The pipeline-space crop rectangle used even when identity.
    pub fn crop_rect(&self, base: [u32; 2]) -> CropRect {
        let [w, h] = self.oriented_size(base);
        self.crop.unwrap_or(CropRect {
            x: 0.0,
            y: 0.0,
            w: w as f32,
            h: h as f32,
        })
    }

    /// True when no transform would change the image, so the base texture can
    /// be drawn directly without a compute pass.
    pub fn is_identity(&self) -> bool {
        self.crop.is_none()
            && self.rotation == 0.0
            && self.orientation_steps == 0
            && !self.flip_h
            && !self.flip_v
            && self.adjustments.is_identity()
    }
}

/// HSL color-mixer hue centres: red, orange, yellow, green, aqua, blue,
/// purple, magenta (used by the mixer UI in a later phase).
#[allow(dead_code)]
pub const HSL_HUES: [f32; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];
#[allow(dead_code)]
pub const HSL_NAMES: [&str; 8] = [
    "Red",
    "Orange",
    "Yellow",
    "Green",
    "Aqua",
    "Blue",
    "Purple",
    "Magenta",
];

/// Per-range HSL mixer values. Slider space is -100..=100 for hue, saturation
/// and luminance, like the referenced app.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct HslRange {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}

/// Color-grading wheels and master blending/balance.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ColorGrading {
    /// Shadow, midtone, highlight, global wheels. Hue 0..=360 (180 neutral),
    /// saturation 0..=100, luminance -100..=100.
    pub wheels: [HslRange; 4],
    /// Crossover mask sharpness, 0..=100 (50 default in the reference app).
    pub blending: f32,
    /// Crossover position, -100..=100.
    pub balance: f32,
}

impl ColorGrading {
    /// The reference app starts blending at 50; the rest neutral.
    pub fn initial() -> Self {
        Self {
            blending: 50.0,
            ..Self::default()
        }
    }
}

/// Core color-grade controls, in slider space. Ranges mirror the referenced
/// implementation's UI (see `grade/gpu.rs` for the shader-unit scaling).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjustments {
    pub exposure: f32,   // -5..=5, 0.01 steps
    pub brightness: f32, // -5..=5
    pub contrast: f32,   // -100..=100
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    /// Relative white balance, -100..=100 (mired-based shift in the shader).
    pub temperature: f32,
    pub tint: f32,
    pub saturation: f32,
    pub vibrance: f32,
    /// Global hue rotation in degrees, -180..=180.
    pub hue: f32,
    /// HSL mixer order: red, orange, yellow, green, aqua, blue, purple, magenta.
    pub hsl: [HslRange; 8],
    pub color_grading: ColorGrading,
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            temperature: 0.0,
            tint: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
            hue: 0.0,
            hsl: [HslRange::default(); 8],
            color_grading: ColorGrading::initial(),
        }
    }
}

impl Adjustments {
    pub fn is_identity(&self) -> bool {
        *self == Adjustments::default()
    }
}
