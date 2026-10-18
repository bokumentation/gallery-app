//! Layout seams: right-panel tabs and edit-section order.
//!
//! Sizes (panel width, bar heights, spacing) live in
//! [`crate::ui::tokens::Metrics`]; this module holds the structural choices.

/// Right-panel content tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RightTab {
    #[default]
    Edit,
    Info,
}

/// Edit-panel sections, top to bottom.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Lut,
    Light,
    Color,
    Hsl,
    Grading,
    Crop,
}

/// Reorder or remove sections here; the panel renders this list in order.
pub const EDIT_SECTIONS: &[Section] = &[
    Section::Lut,
    Section::Light,
    Section::Color,
    Section::Hsl,
    Section::Grading,
    Section::Crop,
];

impl Section {
    pub const COUNT: usize = 6;

    pub fn index(self) -> usize {
        match self {
            Self::Lut => 0,
            Self::Light => 1,
            Self::Color => 2,
            Self::Hsl => 3,
            Self::Grading => 4,
            Self::Crop => 5,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Lut => "LUT",
            Self::Light => "Light",
            Self::Color => "Color",
            Self::Hsl => "HSL Mixer",
            Self::Grading => "Color Grading",
            Self::Crop => "Crop",
        }
    }

    pub fn default_open(self) -> bool {
        matches!(self, Self::Light | Self::Color)
    }
}
