//! The right-side edit panel: crop and color grade, shown while the viewer is
//! open. Implemented as an inherent `GalleryApp` method so it can touch app
//! state like the other top-bar / sidebar methods, kept here to stay compact.
//!
//! Phase 1 ships the panel shell with the Light and Color slider groups; crop
//! interactions, the HSL mixer and the grading wheels land in later phases.

use crate::app::GalleryApp;
use crate::grade::EditState;
use crate::theme;
use crate::ui::widgets as w;
use egui::{CollapsingHeader, Context, Frame, Margin, RichText, Ui};

pub fn show(app: &mut GalleryApp, ctx: &Context) {
    let palette = app.palette;
    egui::SidePanel::right("edit_panel")
        .resizable(true)
        .default_width(300.0)
        .width_range(240.0..=440.0)
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .inner_margin(Margin::symmetric(8, 10)),
        )
        .show(ctx, |ui| {
            panel_content(app, ui);
        });
}

fn panel_content(app: &mut GalleryApp, ui: &mut Ui) {
    let palette = app.palette;
    let Some(path) = app.current_viewer_path() else {
        return;
    };

    ui.label(
        RichText::new("Adjust")
            .font(theme::bold_font(15.0))
            .color(palette.text),
    );
    ui.add_space(6.0);

    // Seed the per-image edit entry so values survive the viewer.
    app.edits.entry(path.clone()).or_default();

    light_section(app, &path, ui);
    color_section(app, &path, ui);
    later_sections(&path, ui, palette);

    ui.add_space(10.0);
    if w::accent_button(ui, "Reset all", palette).clicked() {
        *app.edits.entry(path.clone()).or_default() = EditState::default();
    }
}

fn light_section(app: &mut GalleryApp, path: &std::path::PathBuf, ui: &mut Ui) {    CollapsingHeader::new(RichText::new("Light").size(12.5))
        .default_open(true)
        .show(ui, |ui| {
            let adjustments =
                &mut app.edits.entry(path.clone()).or_default().adjustments;
            let _ = w::slider(ui, &mut adjustments.exposure, -5.0..=5.0, 0.01, "Exposure");
            let _ = w::slider(ui, &mut adjustments.brightness, -5.0..=5.0, 0.01, "Brightness");
            let _ = w::slider(ui, &mut adjustments.contrast, -100.0..=100.0, 1.0, "Contrast");
            let _ = w::slider(ui, &mut adjustments.highlights, -100.0..=100.0, 1.0, "Highlights");
            let _ = w::slider(ui, &mut adjustments.shadows, -100.0..=100.0, 1.0, "Shadows");
            let _ = w::slider(ui, &mut adjustments.whites, -100.0..=100.0, 1.0, "Whites");
            let _ = w::slider(ui, &mut adjustments.blacks, -100.0..=100.0, 1.0, "Blacks");
        });
}

fn color_section(app: &mut GalleryApp, path: &std::path::PathBuf, ui: &mut Ui) {
    CollapsingHeader::new(RichText::new("Color").size(12.5))
        .default_open(true)
        .show(ui, |ui| {
            let adjustments =
                &mut app.edits.entry(path.clone()).or_default().adjustments;
            let _ = w::slider(ui, &mut adjustments.temperature, -100.0..=100.0, 1.0, "Temperature");
            let _ = w::slider(ui, &mut adjustments.tint, -100.0..=100.0, 1.0, "Tint");
            let _ = w::slider(ui, &mut adjustments.saturation, -100.0..=100.0, 1.0, "Saturation");
            let _ = w::slider(ui, &mut adjustments.vibrance, -100.0..=100.0, 1.0, "Vibrance");
            let _ = w::slider(ui, &mut adjustments.hue, -180.0..=180.0, 1.0, "Hue");
        });
}

fn later_sections(path: &std::path::PathBuf, ui: &mut Ui, palette: crate::theme::Palette) {
    let sections = [
        ("HSL Color Mixer", "Eight color ranges arrive in a later phase."),
        ("Color Grading", "Four grading wheels arrive in a later phase."),
        ("Crop", "Aspect presets and straighten arrive in a later phase."),
    ];
    for (title, hint) in sections {
        CollapsingHeader::new(RichText::new(title).size(12.5))
            .default_open(false)
            .show(ui, |ui| {
                ui.label(RichText::new(hint).size(11.5).color(palette.dim));
            });
    }
    let _ = path;
}
