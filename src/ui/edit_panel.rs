//! The right-side edit panel: crop and color grade, shown while the viewer is
//! open. Implemented as an inherent `GalleryApp` method so it can touch app
//! state like the other top-bar / sidebar methods, kept here to stay compact.
//!
//! Phase 1 ships the panel shell with the Light and Color slider groups; crop
//! interactions, the HSL mixer and the grading wheels land in later phases.

use crate::app::GalleryApp;
use crate::crop::{CropDrag, CropGeom};
use crate::grade::{CropRect, EditState};
use crate::theme;
use crate::ui::layout::{RightTab, Section, EDIT_SECTIONS};
use crate::ui::widgets as w;
use egui::{Color32, Frame, Margin, Rect, RichText, Ui, Vec2};

pub fn show(app: &mut GalleryApp, ui: &mut Ui) {
    let palette = app.palette;
    let metrics = palette.metrics;
    egui::Panel::right("right_panel")
        // Same recipe as the left sidebar: a persisted, user-resizable width
        // with a safe range, so a narrow window or a long LUT name can never
        // squeeze the slider rows.
        .resizable(true)
        .default_size(metrics.panel_w)
        .size_range(260.0..=420.0)
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .inner_margin(Margin::symmetric(8, 6)),
        )
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            let edit = app.right_tab == RightTab::Edit;
            ui.horizontal(|ui| {
                let tab = w::tabs(
                    ui,
                    &[
                        ("Adjust", app.right_tab == RightTab::Edit),
                        ("Info", app.right_tab == RightTab::Info),
                    ],
                    palette,
                );
                if let Some(index) = tab {
                    app.right_tab = if index == 0 {
                        RightTab::Edit
                    } else {
                        RightTab::Info
                    };
                }
                if edit {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::flat_button(ui, "Reset all", palette).clicked() {
                            if let Some(path) = app.current_viewer_path() {
                                *app.edits.entry(path).or_default() = EditState::default();
                            }
                        }
                    });
                }
            });
            ui.add_space(2.0);
            w::adaptive_scroll(ui, ui.available_rect_before_wrap());

            crate::theme::with_scroll_handle(ui, palette, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match app.right_tab {
                            RightTab::Edit => panel_content(app, ui),
                            RightTab::Info => crate::ui::info_panel::panel_content(app, ui),
                        }
                    });
            });
        });
}

fn panel_content(app: &mut GalleryApp, ui: &mut Ui) {
    let palette = app.palette;
    let Some(path) = app.current_viewer_path() else {
        return;
    };

    // Seed the per-image edit entry so values survive the viewer.
    app.edits.entry(path.to_path_buf()).or_default();

    for &section in EDIT_SECTIONS {
        let mut open = app.edit_open[section.index()];
        w::section_card(
            ui,
            section.title(),
            &mut open,
            palette,
            |ui| match section {
                Section::Lut => lut_section(app, &path, ui),
                Section::Light => light_section(app, &path, ui),
                Section::Color => color_section(app, &path, ui),
                Section::Hsl => hsl_section(app, &path, ui),
                Section::Grading => grading_section(app, &path, ui),
                Section::Crop => crop_section(app, &path, ui),
            },
        );
        app.edit_open[section.index()] = open;
    }
}

/// The LUT section sits at the top of the panel hierarchy: it is the most
/// global adjustment and applies before the per-slider color work.
fn lut_section(app: &mut GalleryApp, path: &std::path::Path, ui: &mut Ui) {
    let palette = app.palette;
    let state = app.edits.entry(path.to_path_buf()).or_default().clone();
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Lookup table")
                .size(theme::TEXT_SMALL)
                .color(palette.dim),
        );
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
        .width(ui.available_width())
        .selected_text(RichText::new(&selected).size(theme::TEXT_SMALL))
        .show_ui(ui, |ui| {
            let none = ui
                .selectable_label(
                    selected == "(none)",
                    RichText::new("(none)").size(theme::TEXT_SMALL),
                )
                .clicked();
            if none {
                selected = "(none)".to_owned();
            }
            for name in &app.lut_names {
                if ui
                    .selectable_label(
                        selected == *name,
                        RichText::new(name).size(theme::TEXT_SMALL),
                    )
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
    ui.add_space(4.0);
    ui.label(
        RichText::new("3D .cube files; imported LUTs stay installed.")
            .size(theme::TEXT_CAPTION)
            .color(palette.dim),
    );
}

/// The crop tool: mode switch, 90-degree steps, flips, aspect presets and the
/// straighten slider. The on-screen frame lives in `crop_overlay`.
fn crop_section(app: &mut GalleryApp, path: &std::path::Path, ui: &mut Ui) {
    let palette = app.palette;
    let state = app.edits.entry(path.to_path_buf()).or_default().clone();
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
        use w::Icon;
        if w::icon_button(ui, Icon::RotateRight, palette)
            .on_hover_text("Rotate 90 degrees")
            .clicked()
        {
            app.crop_drag = None;
            let state = app.edits.get_mut(path).unwrap();
            state.orientation_steps = (state.orientation_steps + 1) % 4;
            state.crop = None;
        }
        if w::icon_button(ui, Icon::RotateLeft, palette)
            .on_hover_text("Rotate back 90 degrees")
            .clicked()
        {
            app.crop_drag = None;
            let state = app.edits.get_mut(path).unwrap();
            state.orientation_steps = (state.orientation_steps + 3) % 4;
            state.crop = None;
        }
        if w::icon_button(ui, Icon::FlipH, palette)
            .on_hover_text("Flip horizontally")
            .clicked()
        {
            app.crop_drag = None;
            let state = app.edits.get_mut(path).unwrap();
            state.flip_h = !state.flip_h;
            state.crop = None;
        }
        if w::icon_button(ui, Icon::FlipV, palette)
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
    ui.label(
        RichText::new("Aspect")
            .size(theme::TEXT_SMALL)
            .color(palette.dim),
    );
    let presets: [(&str, Option<f32>); 6] = [
        ("Free", None),
        ("1:1", Some(1.0)),
        ("4:3", Some(4.0 / 3.0)),
        ("3:2", Some(3.0 / 2.0)),
        ("16:9", Some(16.0 / 9.0)),
        ("9:16", Some(9.0 / 16.0)),
    ];
    let cell = ((ui.available_width() - 12.0) / 3.0).max(48.0);
    egui::Grid::new("crop-aspect-grid")
        .num_columns(3)
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            for (index, (label, ratio)) in presets.iter().enumerate() {
                let selected = state.aspect == *ratio;
                let button =
                    egui::Button::new(RichText::new(*label).size(theme::TEXT_SMALL).color(
                        if selected {
                            palette.on_accent
                        } else {
                            palette.text
                        },
                    ))
                    .fill(if selected {
                        palette.accent
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(if selected {
                        egui::Stroke::NONE
                    } else {
                        egui::Stroke::new(1.0, palette.input)
                    })
                    .corner_radius(egui::CornerRadius::same(palette.metrics.radius_sm))
                    .min_size(Vec2::new(cell, 28.0));
                if ui.add_sized([cell, 28.0], button).clicked() {
                    app.crop_drag = None;
                    let state = app.edits.get_mut(path).unwrap();
                    let geometry = CropGeom {
                        oriented: oriented_f32,
                        rotation_deg: state.rotation,
                    };
                    // Preserve the current frame instead of resetting
                    // it: re-fit an aspect change and re-validate for
                    // straighten.
                    let base = state.crop.unwrap_or_else(|| geometry.centered(None));
                    state.aspect = *ratio;
                    let next = match ratio {
                        Some(ratio) => geometry.apply_aspect(base, *ratio),
                        None => geometry.fit(base),
                    };
                    normalize_crop(state, &geometry, next);
                }
                if index % 3 == 2 {
                    ui.end_row();
                }
            }
        });

    ui.add_space(4.0);
    let mut rotation = state.rotation;
    if w::slider(ui, &mut rotation, -45.0..=45.0, 0.5, "Straighten", palette) {
        let geometry = CropGeom {
            oriented: oriented_f32,
            rotation_deg: rotation,
        };
        let state = app.edits.get_mut(path).unwrap();
        state.rotation = rotation;
        let base = state
            .crop
            .unwrap_or_else(|| geometry.centered(state.aspect));
        normalize_crop(state, &geometry, geometry.fit(base));
    }

    ui.add_space(4.0);
    if w::flat_button(ui, "Reset crop", palette).clicked() {
        app.crop_drag = None;
        let state = app.edits.get_mut(path).unwrap();
        state.crop = None;
        state.rotation = 0.0;
        state.aspect = None;
    }
}

fn light_section(app: &mut GalleryApp, path: &std::path::Path, ui: &mut Ui) {
    let palette = app.palette;
    let adjustments = &mut app.edits.entry(path.to_path_buf()).or_default().adjustments;
    let _ = w::slider(
        ui,
        &mut adjustments.exposure,
        -5.0..=5.0,
        0.01,
        "Exposure",
        palette,
    );
    let _ = w::slider(
        ui,
        &mut adjustments.brightness,
        -5.0..=5.0,
        0.01,
        "Brightness",
        palette,
    );
    let _ = w::slider(
        ui,
        &mut adjustments.contrast,
        -100.0..=100.0,
        1.0,
        "Contrast",
        palette,
    );
    let _ = w::slider(
        ui,
        &mut adjustments.highlights,
        -100.0..=100.0,
        1.0,
        "Highlights",
        palette,
    );
    let _ = w::slider(
        ui,
        &mut adjustments.shadows,
        -100.0..=100.0,
        1.0,
        "Shadows",
        palette,
    );
    let _ = w::slider(
        ui,
        &mut adjustments.whites,
        -100.0..=100.0,
        1.0,
        "Whites",
        palette,
    );
    let _ = w::slider(
        ui,
        &mut adjustments.blacks,
        -100.0..=100.0,
        1.0,
        "Blacks",
        palette,
    );
}

fn color_section(app: &mut GalleryApp, path: &std::path::Path, ui: &mut Ui) {
    let palette = app.palette;
    let adjustments = &mut app.edits.entry(path.to_path_buf()).or_default().adjustments;
    let _ = w::slider_gradient(
        ui,
        &mut adjustments.temperature,
        -100.0..=100.0,
        1.0,
        "Temperature",
        palette,
        &TEMPERATURE_GRADIENT,
    );
    let _ = w::slider_gradient(
        ui,
        &mut adjustments.tint,
        -100.0..=100.0,
        1.0,
        "Tint",
        palette,
        &TINT_GRADIENT,
    );
    let _ = w::slider(
        ui,
        &mut adjustments.saturation,
        -100.0..=100.0,
        1.0,
        "Saturation",
        palette,
    );
    let _ = w::slider(
        ui,
        &mut adjustments.vibrance,
        -100.0..=100.0,
        1.0,
        "Vibrance",
        palette,
    );
    let _ = w::slider_gradient(
        ui,
        &mut adjustments.hue,
        -180.0..=180.0,
        1.0,
        "Hue",
        palette,
        &HUE_GRADIENT,
    );
}

const TEMPERATURE_GRADIENT: [Color32; 5] = [
    Color32::from_rgb(45, 74, 116),
    Color32::from_rgb(63, 148, 182),
    Color32::from_rgb(138, 138, 138),
    Color32::from_rgb(199, 197, 73),
    Color32::from_rgb(199, 134, 60),
];

const TINT_GRADIENT: [Color32; 5] = [
    Color32::from_rgb(68, 141, 65),
    Color32::from_rgb(89, 212, 89),
    Color32::from_rgb(138, 138, 138),
    Color32::from_rgb(156, 84, 138),
    Color32::from_rgb(190, 64, 159),
];

const HUE_GRADIENT: [Color32; 13] = [
    Color32::from_rgb(130, 80, 255),
    Color32::from_rgb(255, 80, 180),
    Color32::from_rgb(255, 80, 80),
    Color32::from_rgb(255, 160, 80),
    Color32::from_rgb(255, 230, 80),
    Color32::from_rgb(160, 240, 80),
    Color32::from_rgb(80, 230, 120),
    Color32::from_rgb(80, 220, 220),
    Color32::from_rgb(80, 150, 255),
    Color32::from_rgb(110, 100, 255),
    Color32::from_rgb(170, 90, 255),
    Color32::from_rgb(230, 80, 230),
    Color32::from_rgb(130, 80, 255),
];

/// The on-screen crop frame over the drawn image: dimmed surround, thirds
/// grid, grips and drag handling. `rect` is where the image is drawn and
/// `source` its full-resolution oriented size.
pub fn crop_overlay(app: &mut GalleryApp, ui: &mut Ui, rect: Rect, source: [f32; 2]) {
    let Some(path) = app.current_viewer_path() else {
        return;
    };
    let palette = app.palette;

    // Work on a local snapshot; write the mutated state back at the end.
    let mut state = app.edits.entry(path.to_path_buf()).or_default().clone();
    let oriented = state.oriented_size([source[0] as u32, source[1] as u32]);
    let geometry = CropGeom {
        oriented: oriented.map(|axis| axis as f32),
        rotation_deg: state.rotation,
    };
    let mut crop = state
        .crop
        .unwrap_or_else(|| geometry.fit(geometry.centered(state.aspect)));

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
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(rect.min.x, ui_crop.max.y),
            egui::vec2(rect.width(), (rect.max.y - ui_crop.max.y).max(0.0)),
        ),
        0,
        dim,
    );
    let left_top = egui::pos2(rect.min.x, ui_crop.min.y.max(rect.min.y));
    let left_bottom = egui::pos2(ui_crop.min.x.max(rect.min.x), ui_crop.max.y.min(rect.max.y));
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
            [
                egui::pos2(x, ui_crop.top()),
                egui::pos2(x, ui_crop.bottom()),
            ],
            egui::Stroke::new(0.75, Color32::from_white_alpha(160)),
        );
        painter.line_segment(
            [
                egui::pos2(ui_crop.left(), y),
                egui::pos2(ui_crop.right(), y),
            ],
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

    normalize_crop(&mut state, &geometry, crop);
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
    crop.contains(pointer).then_some(crate::crop::Handle::Move)
}

fn screen_to_source(pointer: egui::Pos2, rect: Rect, scale: f32) -> [f32; 2] {
    [
        (pointer.x - rect.min.x) / scale,
        (pointer.y - rect.min.y) / scale,
    ]
}

/// Store a crop, collapsing an unrotated full-frame rectangle back to `None`
/// so merely opening the crop tool does not register as an edit. When the
/// straighten angle is non-zero a fitted rectangle must be kept so the grade
/// pass never samples outside the rotated frame.
fn normalize_crop(state: &mut EditState, geometry: &CropGeom, crop: CropRect) {
    let default_full = state.rotation == 0.0 && crop == geometry.centered(state.aspect);
    state.crop = if default_full { None } else { Some(crop) };
}

/// Eight-range HSL mixer: a hue-range chip row plus hue/saturation/luminance
/// sliders for the selected range.
fn hsl_section(app: &mut GalleryApp, path: &std::path::Path, ui: &mut Ui) {
    let palette = app.palette;
    ui.horizontal_wrapped(|ui| {
        for (index, name) in crate::grade::HSL_NAMES.iter().enumerate() {
            let selected = app.hsl_range == index;
            if w::toggle_button(ui, name, selected, palette).clicked() {
                app.hsl_range = index;
            }
        }
    });
    let index = app.hsl_range.min(crate::grade::HSL_NAMES.len() - 1);
    ui.add_space(4.0);
    let base_hue = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 300.0, 340.0][index];
    let hue_colors: Vec<Color32> = [-60.0, -30.0, 0.0, 30.0, 60.0]
        .iter()
        .map(|offset| w::hsv_color(base_hue + offset, 0.75, 0.95))
        .collect();
    let saturation_colors = [
        w::hsv_color(base_hue, 0.0, 0.85),
        w::hsv_color(base_hue, 1.0, 0.9),
    ];
    let luminance_colors = [
        Color32::BLACK,
        w::hsv_color(base_hue, 0.8, 0.8),
        Color32::WHITE,
    ];
    let range = &mut app
        .edits
        .entry(path.to_path_buf())
        .or_default()
        .adjustments
        .hsl[index];
    let _ = w::slider_gradient(
        ui,
        &mut range.hue,
        -100.0..=100.0,
        1.0,
        "Hue",
        palette,
        &hue_colors,
    );
    let _ = w::slider_gradient(
        ui,
        &mut range.saturation,
        -100.0..=100.0,
        1.0,
        "Saturation",
        palette,
        &saturation_colors,
    );
    let _ = w::slider_gradient(
        ui,
        &mut range.luminance,
        -100.0..=100.0,
        1.0,
        "Luminance",
        palette,
        &luminance_colors,
    );
}

/// Four-way split toning / color grading: shadow, midtone, highlight and global
/// wheels with master blending and balance.
fn grading_section(app: &mut GalleryApp, path: &std::path::Path, ui: &mut Ui) {
    let palette = app.palette;
    let grading = &mut app
        .edits
        .entry(path.to_path_buf())
        .or_default()
        .adjustments
        .color_grading;
    const LABELS: [&str; 4] = ["Shadows", "Midtones", "Highlights", "Global"];
    for (index, label) in LABELS.iter().enumerate() {
        let _ = w::color_wheel(ui, &mut grading.wheels[index], label, palette);
        ui.add_space(2.0);
    }
    ui.add_space(4.0);
    let _ = w::slider(
        ui,
        &mut grading.blending,
        0.0..=100.0,
        1.0,
        "Blending",
        palette,
    );
    let _ = w::slider(
        ui,
        &mut grading.balance,
        -100.0..=100.0,
        1.0,
        "Balance",
        palette,
    );
}
