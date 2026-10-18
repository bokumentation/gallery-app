use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle, Visuals,
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

/// One type ramp for the whole app. Keep every new label on one of these
/// roles instead of picking a size at the call site.
pub const TEXT_CAPTION: f32 = 10.5;
pub const TEXT_SMALL: f32 = 11.5;
pub const TEXT_BODY: f32 = 12.5;
pub const TEXT_SECTION: f32 = 12.5;
pub const TEXT_TITLE: f32 = 15.0;
pub const TEXT_HEADING: f32 = 17.0;

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
    // Panels paint their own surfaces; the window background is drawn once in
    // `GalleryApp::ui` so the transparent corners stay transparent.
    visuals.panel_fill = Color32::TRANSPARENT;
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

    visuals.widgets.inactive.bg_fill = palette.input;
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
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.menu_margin = Margin::same(m.space_xs as i8);
        style.spacing.indent = 12.0;
        style.spacing.interact_size = egui::vec2(m.icon_button, m.control_h_md);
        // Floating scrollbars: hidden at rest, fade in over the panel, and
        // expand when the bar itself is hovered. Because they overlay the
        // content they reserve no width, so panel/grid layout never jumps.
        style.spacing.scroll.floating = true;
        style.spacing.scroll.bar_width = 8.0;
        style.spacing.scroll.floating_width = 4.0;
        style.spacing.scroll.floating_allocated_width = 0.0;
        style.spacing.scroll.handle_min_length = 24.0;
        style.spacing.scroll.bar_inner_margin = 2.0;
        style.spacing.scroll.bar_outer_margin = 2.0;
        style.spacing.scroll.content_margin = Margin::ZERO;
        style.spacing.scroll.foreground_color = false;
        style.spacing.scroll.dormant_handle_opacity = 0.0;
        style.spacing.scroll.active_handle_opacity = if palette.dark { 0.55 } else { 0.7 };
        style.spacing.scroll.interact_handle_opacity = if palette.dark { 0.9 } else { 1.0 };
        style.spacing.scroll.dormant_background_opacity = 0.0;
        style.spacing.scroll.active_background_opacity = 0.0;
        style.spacing.scroll.interact_background_opacity = if palette.dark { 0.12 } else { 0.08 };
        // Fixed track width so a slider's changing value text cannot reflow the
        // surrounding panel row.
        style.spacing.slider_width = 120.0;
        style.text_styles = [
            (TextStyle::Heading, bold_font(TEXT_HEADING)),
            (TextStyle::Body, FontId::proportional(13.0)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Small, FontId::proportional(TEXT_SMALL)),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
    });

    // egui defaults to 40 px per wheel line, which feels much slower than
    // Chrome/GTK (~100 px). Keep egui's smooth wheel easing, but move ~100 px
    // per notch so sensitivity matches other desktop apps.
    ctx.options_mut(|options| options.input_options.line_scroll_speed = 100.0);
}

/// Paint scrollbar handles with the muted scroll color for the duration of one
/// scroll area. `ScrollStyle` only exposes opacity, not a color, so the handle
/// color comes from the temporary widget visuals. The previous visuals are
/// restored immediately, keeping ordinary controls untouched.
pub fn with_scroll_handle<R>(
    ui: &mut egui::Ui,
    palette: Palette,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let old = {
        let widgets = &ui.style().visuals.widgets;
        (
            widgets.inactive.bg_fill,
            widgets.hovered.bg_fill,
            widgets.active.bg_fill,
        )
    };
    {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.bg_fill = palette.scroll_handle;
        widgets.hovered.bg_fill = palette.scroll_handle;
        widgets.active.bg_fill = palette.scroll_handle;
    }

    let result = add(ui);

    {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.bg_fill = old.0;
        widgets.hovered.bg_fill = old.1;
        widgets.active.bg_fill = old.2;
    }
    result
}
