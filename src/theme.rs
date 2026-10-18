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

pub use crate::ui::tokens::Palette;

/// Bundled Inter text faces (SIL OFL 1.1, see `assets/fonts/LICENSE.txt`). The
/// release's static text cut renders better at UI sizes than the 18 pt
/// optical-size cut.
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");

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
    let m = palette.metrics;
    let mut visuals = if palette.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.dark_mode = palette.dark;
    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.sidebar;
    visuals.extreme_bg_color = palette.card;
    visuals.text_edit_bg_color = Some(palette.card);
    visuals.faint_bg_color = palette.card;
    visuals.code_bg_color = palette.card;
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.window_corner_radius = CornerRadius::same(m.radius_lg);
    visuals.menu_corner_radius = CornerRadius::same(m.radius_md);
    visuals.hyperlink_color = palette.text;
    visuals.error_fg_color = palette.error;

    visuals.selection.bg_fill = palette.accent;
    visuals.selection.stroke = Stroke::new(1.0, palette.on_accent);

    // egui's sub-pixel glyph binning buys even kerning at the cost of blurrier
    // text; hinting stays on for crisp stems.
    visuals.text_options.subpixel_binning = false;
    visuals.text_options.font_hinting = true;

    visuals.widgets.noninteractive.bg_fill = palette.bg;
    visuals.widgets.noninteractive.weak_bg_fill = palette.bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(m.radius_md);

    visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = palette.card;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.input);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(m.radius_md);

    visuals.widgets.hovered.bg_fill = palette.card_hover;
    visuals.widgets.hovered.weak_bg_fill = palette.card_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(m.radius_md);

    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent_hover);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.on_accent);
    visuals.widgets.active.corner_radius = CornerRadius::same(m.radius_md);

    visuals.widgets.open.bg_fill = palette.card;
    visuals.widgets.open.weak_bg_fill = palette.card;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.open.corner_radius = CornerRadius::same(m.radius_md);

    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.animation_time = 0.12;
        style.spacing.item_spacing = egui::vec2(m.space_sm, m.space_sm);
        style.spacing.button_padding = egui::vec2(m.space_md, 7.0);
        style.spacing.menu_margin = Margin::same(m.space_sm as i8);
        style.spacing.indent = 16.0;
        style.spacing.interact_size = egui::vec2(m.icon_button, m.control_h_md);
        // Fixed track width so a slider's changing value text cannot reflow the
        // surrounding panel row.
        style.spacing.slider_width = 116.0;
        style.text_styles = [
            (TextStyle::Heading, bold_font(20.0)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(15.0)),
            (TextStyle::Small, FontId::proportional(12.5)),
            (TextStyle::Monospace, FontId::monospace(13.5)),
        ]
        .into();
    });
}
