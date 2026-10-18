//! The right-side edit panel: crop and color grade, shown while the viewer is
//! open. Implemented as an inherent `GalleryApp` method so it can touch app
//! state like the other top-bar / sidebar methods, kept here to stay compact.
//!
//! Phase 1 ships the panel shell with the Light and Color slider groups; crop
//! interactions, the HSL mixer and the grading wheels land in later phases.

use crate::app::GalleryApp;
use crate::crop::{CropDrag, CropGeom};
use crate::grade::EditState;
use crate::theme;
use crate::ui::widgets as w;
use egui::{CollapsingHeader, Color32, Frame, Margin, Rect, RichText, Ui};

pub fn show(app: &mut GalleryApp, ui: &mut Ui) {
    let palette = app.palette;
    egui::Panel::right("edit_panel")
        .resizable(false)
        .exact_size(300.0)
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .inner_margin(Margin::symmetric(8, 10)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| panel_content(app, ui));
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

    lut_section(app, &path, ui);
    light_section(app, &path, ui);
    color_section(app, &path, ui);
    crop_section(app, &path, ui);
    later_sections(ui, palette, [
        ("HSL Color Mixer", "Eight color ranges arrive in a later phase."),
        ("Color Grading", "Four grading wheels arrive in a later phase."),
    ]);

    ui.add_space(10.0);
    if w::accent_button(ui, "Reset all", palette).clicked() {
        *app.edits.entry(path.clone()).or_default() = EditState::default();
    }
}

/// The LUT section sits at the top of the panel hierarchy: it is the most
/// global adjustment and applies before the per-slider color work.
fn lut_section(app: &mut GalleryApp, path: &std::path::PathBuf, ui: &mut Ui) {
    let palette = app.palette;
    CollapsingHeader::new(RichText::new("LUT").size(12.5))
        .default_open(false)
        .show(ui, |ui| {
            let state = app.edits.entry(path.clone()).or_default().clone();
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Lookup table").size(11.5).color(palette.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::flat_button(ui, "Import…", palette)
                        .on_hover_text("Import a .cube LUT file and keep it installed")
                        .clicked()
                    {
                        app.import_lut();
                    }
                });
            });

            // Attached-LUT picker over the persisted store.
            let mut selected = state.lut.clone().unwrap_or_else(|| "(none)".to_owned());
            egui::ComboBox::from_id_salt("lut-picker")
                .width(ui.available_width() - 8.0)
                .selected_text(RichText::new(&selected).size(11.5))
                .show_ui(ui, |ui| {
                    let none = ui
                        .selectable_label(selected == "(none)", RichText::new("(none)").size(11.5))
                        .clicked();
                    if none {
                        selected = "(none)".to_owned();
                    }
                    for name in &app.lut_names {
                        if ui
                            .selectable_label(selected == *name, RichText::new(name).size(11.5))
                            .clicked()
                        {
                            selected = name.clone();
                        }
                    }
                });
            let attached = (selected != "(none)").then(|| selected.clone());
            if attached != state.lut {
                let state = app.edits.get_mut(path).unwrap();
                state.lut = attached;
            }
            ui.add_space(2.0);
            ui.label(
                RichText::new("3D .cube files; imported LUTs stay installed.")
                    .size(10.5)
                    .color(palette.dim),
            );
        });
}

/// The crop tool: mode switch, 90-degree steps, flips, aspect presets and the
/// straighten slider. The on-screen frame lives in `crop_overlay`.
fn crop_section(app: &mut GalleryApp, path: &std::path::PathBuf, ui: &mut Ui) {
    let palette = app.palette;
    CollapsingHeader::new(RichText::new("Crop").size(12.5))
        .default_open(false)
        .show(ui, |ui| {
            let state = app.edits.entry(path.clone()).or_default().clone();
            let source = app
                .viewer
                .source_dimensions(path)
                .or_else(|| app.thumbs.get(path).map(|thumb| thumb.size))
                .unwrap_or([0, 0]);
            let oriented_f32 = state.oriented_size(source).map(|axis| axis as f32);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Frame").size(11.5).color(palette.dim));
                let mut active = app.crop_active;
                if w::switch(ui, &mut active, palette).changed() {
                    app.crop_drag = None;
                    app.crop_active = active;
                }
            });

            ui.horizontal(|ui| {
                if w::flat_button(ui, "↻", palette)
                    .on_hover_text("Rotate 90 degrees")
                    .clicked()
                {
                    app.crop_drag = None;
                    let state = app.edits.get_mut(path).unwrap();
                    state.orientation_steps = (state.orientation_steps + 1) % 4;
                    state.crop = None;
                }
                if w::flat_button(ui, "⟲", palette)
                    .on_hover_text("Rotate back 90 degrees")
                    .clicked()
                {
                    app.crop_drag = None;
                    let state = app.edits.get_mut(path).unwrap();
                    state.orientation_steps = (state.orientation_steps + 3) % 4;
                    state.crop = None;
                }
                if w::flat_button(ui, "⇄", palette)
                    .on_hover_text("Flip horizontally")
                    .clicked()
                {
                    app.crop_drag = None;
                    let state = app.edits.get_mut(path).unwrap();
                    state.flip_h = !state.flip_h;
                    state.crop = None;
                }
                if w::flat_button(ui, "⇅", palette)
                    .on_hover_text("Flip vertically")
                    .clicked()
                {
                    app.crop_drag = None;
                    let state = app.edits.get_mut(path).unwrap();
                    state.flip_v = !state.flip_v;
                    state.crop = None;
                }
            });

            ui.add_space(4.0);
            ui.label(RichText::new("Aspect").size(11.5).color(palette.dim));
            ui.horizontal_wrapped(|ui| {
                let presets: [(&str, Option<f32>); 6] = [
                    ("Free", None),
                    ("1:1", Some(1.0)),
                    ("4:3", Some(4.0 / 3.0)),
                    ("3:2", Some(3.0 / 2.0)),
                    ("16:9", Some(16.0 / 9.0)),
                    ("9:16", Some(9.0 / 16.0)),
                ];
                for (label, ratio) in presets {
                    let selected = state.aspect == ratio;
                    if w::flat_button(ui, label, palette).clicked() {
                        app.crop_drag = None;
                        let state = app.edits.get_mut(path).unwrap();
                        state.aspect = ratio;
                        let geom = crate::crop::CropGeom {
                            oriented: oriented_f32,
                            rotation_deg: state.rotation,
                        };
                        state.crop = Some(match ratio {
                            Some(ratio) => geom.centered(Some(ratio)),
                            None => geom.centered(None),
                        });
                    }
                    let _ = selected;
                }
            });

            ui.add_space(4.0);
            let mut rotation = state.rotation;
            if w::slider(ui, &mut rotation, -45.0..=45.0, 0.5, "Straighten") {
                let geometry = CropGeom {
                    oriented: oriented_f32,
                    rotation_deg: rotation,
                };
                let state = app.edits.get_mut(path).unwrap();
                state.rotation = rotation;
                let base = state.crop.unwrap_or_else(|| {
                    geometry.centered(state.aspect)
                });
                state.crop = Some(geometry.fit(base));
            }

            ui.add_space(4.0);
            if w::flat_button(ui, "Reset crop", palette).clicked() {
                app.crop_drag = None;
                let state = app.edits.get_mut(path).unwrap();
                state.crop = None;
                state.rotation = 0.0;
                state.aspect = None;
            }
        });
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

/// The on-screen crop frame over the drawn image: dimmed surround, thirds
/// grid, grips and drag handling. `rect` is where the image is drawn and
/// `source` its full-resolution oriented size.
pub fn crop_overlay(app: &mut GalleryApp, ui: &mut Ui, rect: Rect, source: [f32; 2]) {
    let Some(path) = app.current_viewer_path() else {
        return;
    };
    let palette = app.palette;

    // Work on a local snapshot; write the mutated state back at the end.
    let mut state = app.edits.entry(path.clone()).or_default().clone();
    let oriented = state.oriented_size([source[0] as u32, source[1] as u32]);
    let geometry = CropGeom {
        oriented: oriented.map(|axis| axis as f32),
        rotation_deg: state.rotation,
    };
    let mut crop = state.crop.unwrap_or_else(|| geometry.centered(None));

    // The drawn texture is the full oriented frame (crop preview mode), so
    // the same scale applies on both axes because the fit preserves aspect.
    let display_w = oriented[0] as f32;
    let scale = rect.width() / display_w.max(1.0);

    let ui_crop = Rect::from_min_size(
        rect.min + egui::vec2(crop.x, crop.y) * scale,
        egui::vec2(crop.w, crop.h) * scale,
    );

    // Dimmed surround.
    let dim = Color32::from_black_alpha(120);
    let painter = ui.painter();
    painter.rect_filled(
        Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width(), ui_crop.min.y - rect.min.y).max(egui::Vec2::ZERO),
        ),
        0,
        dim,
    );
    painter.rect_filled(Rect::from_min_size(
        egui::pos2(rect.min.x, ui_crop.max.y),
        egui::vec2(
            rect.width(),
            (rect.max.y - ui_crop.max.y).max(0.0),
        ),
    ), 0, dim);
    let left_top = egui::pos2(rect.min.x, ui_crop.min.y.max(rect.min.y));
    let left_bottom = egui::pos2(
        ui_crop.min.x.max(rect.min.x),
        ui_crop.max.y.min(rect.max.y),
    );
    painter.rect_filled(Rect::from_two_pos(left_top, left_bottom), 0, dim);
    painter.rect_filled(
        Rect::from_two_pos(
            egui::pos2(ui_crop.max.x, left_top.y),
            egui::pos2(rect.max.x, left_bottom.y),
        ),
        0,
        dim,
    );

    // Frame, thirds and grips.
    painter.rect_stroke(
        ui_crop,
        0.0,
        egui::Stroke::new(1.5, palette.accent),
        egui::StrokeKind::Middle,
    );
    for thirds in 1..3u8 {
        let x = ui_crop.left() + ui_crop.width() * (thirds as f32 / 3.0);
        let y = ui_crop.top() + ui_crop.height() * (thirds as f32 / 3.0);
        painter.line_segment(
            [egui::pos2(x, ui_crop.top()), egui::pos2(x, ui_crop.bottom())],
            egui::Stroke::new(0.75, Color32::from_white_alpha(160)),
        );
        painter.line_segment(
            [egui::pos2(ui_crop.left(), y), egui::pos2(ui_crop.right(), y)],
            egui::Stroke::new(0.75, Color32::from_white_alpha(160)),
        );
    }
    let grip = crate::crop::Handle::GRIP_RADIUS;
    for (_handle, fx, fy) in crate::crop::Handle::grips() {
        let pos = egui::pos2(
            ui_crop.left() + ui_crop.width() * fx,
            ui_crop.top() + ui_crop.height() * fy,
        );
        let square = Rect::from_center_size(pos, egui::vec2(grip, grip) * 0.9);
        painter.rect_filled(square, 2, Color32::WHITE);
        painter.rect_stroke(
            square,
            2.0,
            egui::Stroke::new(1.0, palette.accent),
            egui::StrokeKind::Outside,
        );
    }

    // Drag handling over the whole image area.
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let pointer = response.interact_pointer_pos();
    let mut drag = app.crop_drag;

    if response.drag_started() {
        if let Some(pointer) = pointer {
            if let Some(handle) = pick_handle(ui_crop, pointer) {
                let start_pointer = screen_to_source(pointer, rect, scale);
                drag = Some(CropDrag {
                    handle,
                    start_pointer,
                    start_rect: crop,
                    geometry,
                    aspect: state.aspect,
                });
            }
        }
    }

    if let Some(pending) = &mut drag {
        if let Some(pointer) = pointer {
            // Cumulative delta from drag start: apply() re-derives the whole
            // rect from start_rect, so the pointer must stay fixed.
            let start = screen_to_source(pointer, rect, scale);
            let delta = [
                start[0] - pending.start_pointer[0],
                start[1] - pending.start_pointer[1],
            ];
            crop = geometry.fit(pending.apply(delta));
        }
    }
    if response.drag_stopped() {
        app.crop_drag = None;
    }

    state.crop = Some(crop);
    *app.edits.entry(path).or_default() = state;
    if let Some(pending) = drag {
        if response.dragged() {
            app.crop_drag = Some(pending);
        }
    }
}

fn pick_handle(crop: Rect, pointer: egui::Pos2) -> Option<crate::crop::Handle> {
    let grip = crate::crop::Handle::GRIP_RADIUS;
    for (handle, fx, fy) in crate::crop::Handle::grips() {
        let pos = egui::pos2(
            crop.left() + crop.width() * fx,
            crop.top() + crop.height() * fy,
        );
        if Rect::from_center_size(pos, egui::vec2(grip, grip) * 2.0).contains(pointer) {
            return Some(handle);
        }
    }
    crop.contains(pointer)
        .then_some(crate::crop::Handle::Move)
}

fn screen_to_source(pointer: egui::Pos2, rect: Rect, scale: f32) -> [f32; 2] {
    [
        (pointer.x - rect.min.x) / scale,
        (pointer.y - rect.min.y) / scale,
    ]
}

fn later_sections(
    ui: &mut Ui,
    palette: crate::theme::Palette,
    sections: [(&str, &str); 2],
) {    for (title, hint) in sections {
        CollapsingHeader::new(RichText::new(title).size(12.5))
            .default_open(false)
            .show(ui, |ui| {
                ui.label(RichText::new(hint).size(11.5).color(palette.dim));
            });
    }
}
