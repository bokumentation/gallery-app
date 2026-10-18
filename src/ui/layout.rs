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
