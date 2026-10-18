//! Compile-time design tokens.
//!
//! This is the single place to change the app's colors, radii, spacing and
//! control sizes. Presets follow the shadcn/ui “zinc” theme; the field names
//! keep the historical palette names so widget signatures stay small, with the
//! shadcn role documented next to each one.
//!
//! To add a theme, add a `Palette::your_theme()` preset and route it from
//! [`Palette::from_mode`]. To resize the UI, edit [`Metrics::default`].

use egui::Color32;

/// Sizing, radii and spacing. Shared by every widget; changing a value here
/// resizes the whole UI without touching call sites.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // reserved tokens for upcoming widgets (large controls, thumb, etc.)
pub struct Metrics {
    /// `rounded-sm` (inputs, small chips).
    pub radius_sm: u8,
    /// `rounded-md` (buttons, cards).
    pub radius_md: u8,
    /// `rounded-lg` (panels, popovers).
    pub radius_lg: u8,
    /// 4 px step.
    pub space_xs: f32,
    /// 8 px step.
    pub space_sm: f32,
    /// 12 px step.
    pub space_md: f32,
    /// 16 px step.
    pub space_lg: f32,
    /// `h-8`.
    pub control_h_sm: f32,
    /// `h-9` (default button/input height).
    pub control_h_md: f32,
    /// `h-10`.
    pub control_h_lg: f32,
    /// `size-9` icon button.
    pub icon_button: f32,
    /// Sidebar width.
    pub sidebar_w: f32,
    /// Right edit/info panel width.
    pub panel_w: f32,
    /// Bottom info bar height.
    pub bottom_bar_h: f32,
    /// Grid spacing.
    pub grid_gap: f32,
    /// Grid outer padding.
    pub grid_pad: f32,
    /// Switch dimensions.
    pub switch_w: f32,
    pub switch_h: f32,
    /// Slider thumb diameter.
    pub slider_thumb: f32,
    /// Color-wheel pad diameter.
    pub wheel_size: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            radius_sm: 6,
            radius_md: 8,
            radius_lg: 10,
            space_xs: 4.0,
            space_sm: 8.0,
            space_md: 12.0,
            space_lg: 16.0,
            control_h_sm: 32.0,
            control_h_md: 36.0,
            control_h_lg: 40.0,
            icon_button: 36.0,
            sidebar_w: 240.0,
            panel_w: 300.0,
            bottom_bar_h: 32.0,
            grid_gap: 12.0,
            grid_pad: 18.0,
            switch_w: 40.0,
            switch_h: 22.0,
            slider_thumb: 14.0,
            wheel_size: 76.0,
        }
    }
}

/// Color tokens. Light/dark presets are the shadcn “zinc” palette; the app
/// keeps neutral primary controls (near-black on light, near-white on dark).
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Whether this is the dark preset (drives egui `Visuals`).
    pub dark: bool,
    /// shadcn `background`.
    pub bg: Color32,
    /// shadcn `sidebar` / elevated surface.
    pub sidebar: Color32,
    /// Top bar surface (shadcn `background`).
    pub header: Color32,
    /// shadcn `card`.
    pub card: Color32,
    /// shadcn `accent` (hover surface for ghost controls).
    pub card_hover: Color32,
    /// shadcn `border`.
    pub border: Color32,
    /// shadcn `foreground`.
    pub text: Color32,
    /// shadcn `muted-foreground`.
    pub dim: Color32,
    /// shadcn `primary` (filled buttons, selected rows, switches).
    pub accent: Color32,
    /// `primary` hovered.
    pub accent_hover: Color32,
    /// shadcn `primary-foreground`.
    pub on_accent: Color32,
    /// shadcn `destructive`.
    pub error: Color32,
    pub shadow: Color32,
    /// shadcn `input` (control outlines).
    pub input: Color32,
    /// shadcn `ring` (focus outlines).
    pub ring: Color32,
    pub metrics: Metrics,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(9, 9, 11),        // zinc-950
            sidebar: Color32::from_rgb(17, 17, 19), // shadcn sidebar (dark)
            header: Color32::from_rgb(9, 9, 11),
            card: Color32::from_rgb(24, 24, 27),    // zinc-900
            card_hover: Color32::from_rgb(39, 39, 42), // zinc-800 (accent)
            border: Color32::from_rgb(39, 39, 42),
            text: Color32::from_rgb(250, 250, 250), // zinc-50
            dim: Color32::from_rgb(161, 161, 170),  // zinc-400
            accent: Color32::from_rgb(250, 250, 250),
            accent_hover: Color32::from_rgb(228, 228, 231),
            on_accent: Color32::from_rgb(24, 24, 27),
            error: Color32::from_rgb(248, 113, 113), // red-400
            shadow: Color32::from_black_alpha(120),
            input: Color32::from_rgb(39, 39, 42),
            ring: Color32::from_rgb(212, 212, 216), // zinc-300
            metrics: Metrics::default(),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(255, 255, 255),
            sidebar: Color32::from_rgb(250, 250, 250), // zinc-50
            header: Color32::from_rgb(255, 255, 255),
            card: Color32::from_rgb(255, 255, 255),
            card_hover: Color32::from_rgb(244, 244, 245), // zinc-100 (accent)
            border: Color32::from_rgb(228, 228, 231),    // zinc-200
            text: Color32::from_rgb(9, 9, 11),           // zinc-950
            dim: Color32::from_rgb(113, 113, 122),       // zinc-500
            accent: Color32::from_rgb(24, 24, 27),       // zinc-900 (primary)
            accent_hover: Color32::from_rgb(39, 39, 42),
            on_accent: Color32::from_rgb(250, 250, 250),
            error: Color32::from_rgb(220, 38, 38), // red-600
            shadow: Color32::from_black_alpha(24),
            input: Color32::from_rgb(228, 228, 231),
            ring: Color32::from_rgb(161, 161, 170), // zinc-400
            metrics: Metrics::default(),
        }
    }

    pub fn from_mode(mode: crate::theme::ThemeMode) -> Self {
        let dark = match mode {
            crate::theme::ThemeMode::System => crate::theme::system_prefers_dark(),
            crate::theme::ThemeMode::Light => false,
            crate::theme::ThemeMode::Dark => true,
        };
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}
