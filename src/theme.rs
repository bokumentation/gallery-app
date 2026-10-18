use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle,
    Visuals,
};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Auto",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }
}

pub fn system_prefers_dark() -> bool {
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        let value = String::from_utf8_lossy(&output.stdout).to_lowercase();
        if value.contains("prefer-dark") {
            return true;
        }
        if value.contains("prefer-light") {
            return false;
        }
    }

    if let Ok(theme) = std::env::var("GTK_THEME") {
        if theme.to_lowercase().contains("dark") {
            return true;
        }
    }

    true
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub sidebar: Color32,
    pub header: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub error: Color32,
    pub shadow: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(30, 30, 30),
            sidebar: Color32::from_rgb(48, 48, 48),
            header: Color32::from_rgb(48, 48, 48),
            card: Color32::from_rgb(43, 43, 43),
            card_hover: Color32::from_rgb(56, 56, 56),
            border: Color32::from_rgba_unmultiplied(255, 255, 255, 30),
            text: Color32::from_rgb(255, 255, 255),
            dim: Color32::from_rgb(181, 181, 181),
            accent: Color32::from_rgb(53, 132, 228),
            accent_hover: Color32::from_rgb(74, 148, 240),
            on_accent: Color32::from_rgb(255, 255, 255),
            error: Color32::from_rgb(255, 98, 98),
            shadow: Color32::from_black_alpha(70),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(255, 255, 255),
            sidebar: Color32::from_rgb(235, 235, 235),
            header: Color32::from_rgb(246, 246, 246),
            card: Color32::from_rgb(242, 242, 242),
            card_hover: Color32::from_rgb(230, 230, 230),
            border: Color32::from_rgba_unmultiplied(0, 0, 0, 25),
            text: Color32::from_rgb(29, 29, 29),
            dim: Color32::from_rgb(110, 110, 110),
            accent: Color32::from_rgb(53, 132, 228),
            accent_hover: Color32::from_rgb(42, 118, 210),
            on_accent: Color32::from_rgb(255, 255, 255),
            error: Color32::from_rgb(192, 28, 40),
            shadow: Color32::from_black_alpha(30),
        }
    }

    pub fn from_mode(mode: ThemeMode) -> Self {
        let dark = match mode {
            ThemeMode::System => system_prefers_dark(),
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
        };
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

/// Bundled Inter faces (SIL OFL 1.1) — the UI renders the same everywhere,
/// without a runtime scan of the user's font folders.
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter_18pt-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter_18pt-SemiBold.ttf");

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "gallery-ui".to_owned(),
        Arc::new(FontData::from_owned(INTER_REGULAR.to_vec())),
    );
    fonts.font_data.insert(
        "gallery-bold".to_owned(),
        Arc::new(FontData::from_owned(INTER_SEMIBOLD.to_vec())),
    );
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "gallery-ui".to_owned());
    }
    fonts.families.insert(
        FontFamily::Name("gallery-bold".into()),
        vec!["gallery-bold".to_owned(), "gallery-ui".to_owned()],
    );

    ctx.set_fonts(fonts);
}

pub fn bold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("gallery-bold".into()))
}

pub fn apply(ctx: &egui::Context, palette: Palette) {
    let mut visuals = if palette.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.dark_mode = palette.dark;
    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.sidebar;
    visuals.extreme_bg_color = if palette.dark {
        Color32::from_rgb(36, 36, 36)
    } else {
        Color32::from_rgb(255, 255, 255)
    };
    visuals.text_edit_bg_color = Some(visuals.extreme_bg_color);
    visuals.faint_bg_color = palette.card;
    visuals.code_bg_color = palette.card;
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.hyperlink_color = palette.accent;
    visuals.error_fg_color = palette.error;

    visuals.selection.bg_fill = palette.accent;
    visuals.selection.stroke = Stroke::new(1.0, palette.on_accent);

    visuals.widgets.noninteractive.bg_fill = palette.sidebar;
    visuals.widgets.noninteractive.weak_bg_fill = palette.sidebar;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);

    visuals.widgets.inactive.bg_fill = palette.card;
    visuals.widgets.inactive.weak_bg_fill = palette.card;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::TRANSPARENT);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);

    visuals.widgets.hovered.bg_fill = palette.card_hover;
    visuals.widgets.hovered.weak_bg_fill = palette.card_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);

    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent_hover);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.on_accent);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);

    visuals.widgets.open.bg_fill = palette.card;
    visuals.widgets.open.weak_bg_fill = palette.card;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.open.corner_radius = CornerRadius::same(8);

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.animation_time = 0.12;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.indent = 16.0;
    style.spacing.interact_size = egui::vec2(32.0, 30.0);
    style.text_styles = [
        (TextStyle::Heading, bold_font(19.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();

    ctx.set_style(style);
}
