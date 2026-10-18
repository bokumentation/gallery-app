use crate::theme::{self, Palette};
pub use crate::ui::icons::{
    paint_folder_icon, paint_icon, paint_image_icon, paint_search_icon, Icon,
};
use egui::{
    Align, Align2, Button, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Key, Layout,
    Margin, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextEdit, TextStyle, Ui, Vec2,
};

pub const SEARCH_ID: &str = "gallery-search";

pub fn flat_button(ui: &mut Ui, label: &str, palette: Palette) -> Response {
    let m = palette.metrics;
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.bg_fill = Color32::TRANSPARENT;
        widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        widgets.inactive.bg_stroke = Stroke::NONE;
        widgets.hovered.bg_fill = palette.card_hover;
        widgets.hovered.weak_bg_fill = palette.card_hover;
        widgets.hovered.bg_stroke = Stroke::NONE;
        widgets.active.bg_fill = palette.card_hover;
        widgets.active.weak_bg_fill = palette.card_hover;
        ui.add(
            Button::new(RichText::new(label).color(palette.text))
                .frame(true)
                .corner_radius(m.radius_md)
                .min_size(Vec2::new(0.0, m.control_h_md)),
        )
    })
    .inner
}

pub fn accent_button(ui: &mut Ui, label: &str, palette: Palette) -> Response {
    let m = palette.metrics;
    ui.add(
        Button::new(RichText::new(label).color(palette.on_accent))
            .fill(palette.accent)
            .stroke(Stroke::NONE)
            .corner_radius(m.radius_md)
            .min_size(Vec2::new(0.0, m.control_h_md)),
    )
}

/// Small chip button used for preset rows: filled when selected, ghost
/// otherwise, so the active choice is always visible.
pub fn toggle_button(ui: &mut Ui, label: &str, selected: bool, palette: Palette) -> Response {
    let m = palette.metrics;
    ui.add(
        Button::new(
            RichText::new(label)
                .size(theme::TEXT_SMALL)
                .color(if selected {
                    palette.on_accent
                } else {
                    palette.text
                }),
        )
        .fill(if selected {
            palette.accent
        } else {
            Color32::TRANSPARENT
        })
        .stroke(if selected {
            Stroke::NONE
        } else {
            Stroke::new(1.0, palette.input)
        })
        .corner_radius(m.radius_sm)
        .min_size(Vec2::new(0.0, m.control_h_sm - 2.0)),
    )
}

pub fn symbol_button(ui: &mut Ui, glyph: &str, palette: Palette) -> Response {
    let m = palette.metrics;
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.style_mut().visuals.widgets;
            widgets.inactive.bg_fill = Color32::TRANSPARENT;
            widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            widgets.inactive.bg_stroke = Stroke::NONE;
            widgets.hovered.bg_fill = palette.card_hover;
            widgets.hovered.weak_bg_fill = palette.card_hover;
            widgets.hovered.bg_stroke = Stroke::NONE;
            widgets.active.bg_fill = palette.card_hover;
            widgets.active.weak_bg_fill = palette.card_hover;
            ui.add(
                Button::new(RichText::new(glyph).size(17.0).color(palette.text))
                    .frame(true)
                    .min_size(Vec2::splat(m.control_h_sm))
                    .corner_radius(m.radius_md),
            )
        })
        .inner;
    response
}

pub fn switch(ui: &mut Ui, on: &mut bool, palette: Palette) -> Response {
    let m = palette.metrics;
    let desired = Vec2::new(m.switch_w, m.switch_h);
    let (rect, mut response) = ui.allocate_exact_size(desired, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let animation = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);
    let background = if *on { palette.accent } else { palette.input };
    let radius = (m.switch_h * 0.5) as u8;
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius), background);
    let pad = 2.0;
    let knob = m.switch_h * 0.5 - pad;
    let knob_x = egui::lerp(
        (rect.left() + knob + pad)..=(rect.right() - knob - pad),
        animation,
    );
    ui.painter().circle_filled(
        egui::pos2(knob_x, rect.center().y),
        knob,
        if *on { palette.on_accent } else { palette.card },
    );
    response
}

// ---------------------------------------------------------------------------
// Color wheel (hue/saturation pad) for the split-toning / grading sections
// ---------------------------------------------------------------------------

/// Convert HSV (h 0..360, s 0..1, v 0..1) to RGB. Local helper for the wheel
/// gradient; the grading math itself lives on the GPU.
fn hsv_rgb(h: f32, s: f32, v: f32) -> Color32 {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Color32::from_rgb(
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

/// Public HSV helper for gradient tracks in the edit panel.
pub fn hsv_color(h: f32, s: f32, v: f32) -> Color32 {
    hsv_rgb(h, s, v)
}

/// A hue ring with a draggable saturation handle plus a luminance slider.
/// `value.hue` is degrees, `value.saturation`/`value.luminance` are -/+100
/// slider units. Returns true when the value changed.
pub fn color_wheel(
    ui: &mut Ui,
    value: &mut crate::grade::HslRange,
    label: &str,
    palette: Palette,
) -> bool {
    let m = palette.metrics;
    let wheel = m.wheel_size;
    let gap = m.space_sm;
    // Fixed columns: the wheel and the controls never reflow while dragging.
    let control_w = (ui.available_width() - wheel - gap).max(130.0);
    let mut changed = false;
    ui.horizontal(|ui| {
        // The layout already inserts `item_spacing` between the two cells;
        // adding an explicit gap would make the row wider than the panel.
        ui.spacing_mut().item_spacing.x = gap;
        let wheel_response =
            ui.allocate_ui_with_layout(Vec2::splat(wheel), Layout::top_down(Align::Center), |ui| {
                wheel_pad(ui, value, palette)
            });
        changed |= wheel_response.inner;
        let controls = ui.allocate_ui_with_layout(
            Vec2::new(control_w, wheel),
            Layout::top_down(Align::LEFT),
            |ui| {
                ui.set_width(control_w);
                ui.label(
                    RichText::new(label)
                        .size(theme::TEXT_CAPTION)
                        .color(palette.dim),
                );
                let tint = hsv_rgb(value.hue, (value.saturation / 100.0).clamp(0.0, 1.0), 1.0);
                let (chip, _) =
                    ui.allocate_exact_size(Vec2::new(control_w.min(60.0), 5.0), Sense::hover());
                ui.painter().rect_filled(chip, CornerRadius::same(2), tint);
                ui.add_space(1.0);
                slider_compact(
                    ui,
                    &mut value.luminance,
                    -100.0..=100.0,
                    1.0,
                    "Lum",
                    palette,
                )
            },
        );
        changed |= controls.inner;
    });
    changed
}

/// Hue/saturation pad. Fixed-size so the surrounding row cannot reflow.
fn wheel_pad(ui: &mut Ui, value: &mut crate::grade::HslRange, palette: Palette) -> bool {
    let size = Vec2::splat(palette.metrics.wheel_size);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let center = rect.center();
    let outer = rect.width() * 0.5 - 1.0;
    let inner = outer * 0.62;
    let painter = ui.painter();
    // Hue ring: 48 solid-colour quads approximated as triangles.
    const SEGMENTS: u32 = 48;
    for k in 0..SEGMENTS {
        let a0 = (k as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        let a1 = ((k + 1) as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        let hue = (k as f32 / SEGMENTS as f32) * 360.0;
        let color = hsv_rgb(hue, 1.0, 1.0);
        let dir0 = egui::vec2(a0.cos(), a0.sin());
        let dir1 = egui::vec2(a1.cos(), a1.sin());
        let p0 = center + dir0 * inner;
        let p1 = center + dir0 * outer;
        let p2 = center + dir1 * outer;
        let p3 = center + dir1 * inner;
        painter.add(egui::Shape::convex_polygon(
            vec![p0, p1, p2, p3],
            color,
            Stroke::NONE,
        ));
    }
    painter.circle_filled(center, inner - 1.0, palette.card);
    painter.circle_stroke(center, inner, Stroke::new(1.0, palette.border));

    // Handle: angle from hue, radius from saturation.
    let angle = value.hue.to_radians();
    let radius = inner + (outer - inner) * (value.saturation.clamp(0.0, 100.0) / 100.0);
    let handle = center + egui::vec2(angle.cos(), angle.sin()) * radius;
    painter.circle_filled(handle, 6.0, Color32::WHITE);
    painter.circle_stroke(handle, 6.0, Stroke::new(1.5, palette.accent));

    let mut changed = false;
    if response.dragged() || response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let delta = pointer - center;
            let dist = delta.length();
            if dist > 0.5 {
                let hue = delta.y.atan2(delta.x).to_degrees();
                value.hue = hue.rem_euclid(360.0);
            }
            value.saturation = (((dist - inner) / (outer - inner)) * 100.0).clamp(0.0, 100.0);
            changed = true;
        }
    }
    changed
}

/// Inline value editor state for a slider, kept in egui's frame memory.
#[derive(Clone, Default)]
struct SliderEdit {
    editing: bool,
    buffer: String,
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn mix_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        egui::lerp(a.r() as f32..=b.r() as f32, t) as u8,
        egui::lerp(a.g() as f32..=b.g() as f32, t) as u8,
        egui::lerp(a.b() as f32..=b.b() as f32, t) as u8,
    )
}

fn sample_gradient(colors: &[Color32], t: f32) -> Color32 {
    if colors.len() <= 1 {
        return colors.first().copied().unwrap_or(Color32::TRANSPARENT);
    }
    let scaled = t.clamp(0.0, 1.0) * (colors.len() - 1) as f32;
    let index = scaled.floor() as usize;
    let fraction = scaled - index as f32;
    let a = colors[index.min(colors.len() - 1)];
    let b = colors[(index + 1).min(colors.len() - 1)];
    mix_color(a, b, fraction)
}

fn paint_gradient_rail(ui: &Ui, rect: Rect, colors: &[Color32]) {
    if colors.is_empty() {
        return;
    }
    let segments = 48;
    let mut mesh = egui::Mesh::default();
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let color = sample_gradient(colors, t);
        let x = egui::lerp(rect.left()..=rect.right(), t);
        mesh.colored_vertex(egui::pos2(x, rect.top()), color);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), color);
        if i > 0 {
            let base = (i as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// Draw one full-width track and handle all pointer/keyboard interaction.
/// Returns true when the value changed.
#[allow(clippy::too_many_arguments)] // shared preset parameters keep the slider variants explicit
fn slider_track(
    ui: &mut Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    default: f32,
    palette: Palette,
    gradient: Option<&[Color32]>,
    height: f32,
    id: Id,
) -> bool {
    let width = ui.available_width().max(60.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let rail = Rect::from_center_size(rect.center(), Vec2::new((width - 12.0).max(20.0), 3.0));
    let range = (max - min).max(f32::EPSILON);
    let value_t = ((*value - min) / range).clamp(0.0, 1.0);
    let origin_t = ((default - min) / range).clamp(0.0, 1.0);

    if let Some(colors) = gradient {
        if colors.len() > 1 {
            paint_gradient_rail(ui, rail, colors);
        } else {
            ui.painter()
                .rect_filled(rail, CornerRadius::same(2), palette.input);
        }
    } else {
        ui.painter()
            .rect_filled(rail, CornerRadius::same(2), palette.input);
        let x0 = egui::lerp(rail.left()..=rail.right(), origin_t);
        let x1 = egui::lerp(rail.left()..=rail.right(), value_t);
        let fill = Rect::from_min_max(
            egui::pos2(x0.min(x1), rail.top()),
            egui::pos2(x0.max(x1), rail.bottom()),
        );
        ui.painter()
            .rect_filled(fill, CornerRadius::same(2), with_alpha(palette.accent, 90));
    }

    let thumb_x = egui::lerp(rail.left()..=rail.right(), value_t);
    let radius = if response.hovered() || response.dragged() {
        6.5
    } else {
        5.5
    };
    ui.painter()
        .circle_filled(egui::pos2(thumb_x, rail.center().y), radius, palette.accent);
    ui.painter().circle_stroke(
        egui::pos2(thumb_x, rail.center().y),
        radius,
        Stroke::new(1.0, with_alpha(palette.on_accent, 70)),
    );
    if response.has_focus() {
        ui.painter().circle_stroke(
            egui::pos2(thumb_x, rail.center().y),
            radius + 2.5,
            Stroke::new(1.0, palette.ring),
        );
    }

    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let before = *value;
    let mut changed = false;
    if response.clicked() {
        response.request_focus();
    }
    if response.is_pointer_button_down_on() || response.dragged() {
        if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
            let t = ((pointer.x - rail.left()) / rail.width()).clamp(0.0, 1.0);
            let fine = ui.input(|input| input.modifiers.shift);
            let effective = if fine { step * 0.2 } else { step };
            let raw = min + t * range;
            let snapped = if effective > 0.0 {
                min + ((raw - min) / effective).round() * effective
            } else {
                raw
            };
            *value = snapped.clamp(min, max);
        }
    }
    if response.double_clicked() {
        *value = default;
    }
    if response.has_focus() {
        let key_step = if ui.input(|input| input.modifiers.shift) {
            step * 0.2
        } else {
            step
        };
        if ui.input(|input| input.key_pressed(Key::ArrowLeft) || input.key_pressed(Key::ArrowDown))
        {
            *value = (*value - key_step).clamp(min, max);
        }
        if ui.input(|input| input.key_pressed(Key::ArrowRight) || input.key_pressed(Key::ArrowUp)) {
            *value = (*value + key_step).clamp(min, max);
        }
    }
    changed |= (*value - before).abs() > f32::EPSILON;

    // Keep the value cell stable when this widget is used outside the stacked
    // row; the id is only used for focus bookkeeping.
    let _ = id;
    changed
}

/// RapidRAW-style slider: label and value share a fixed first row, then the
/// track uses the full content width. Double-click the track, or click the
/// label, to reset. Shift slows the drag down for fine control.
pub fn slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    label: &str,
    palette: Palette,
) -> bool {
    slider_impl(ui, value, range, step, label, palette, None)
}

/// Same slider with a painted gradient rail (temperature, tint, hue, HSL).
pub fn slider_gradient(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    label: &str,
    palette: Palette,
    colors: &[Color32],
) -> bool {
    slider_impl(ui, value, range, step, label, palette, Some(colors))
}

fn slider_impl(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    label: &str,
    palette: Palette,
    gradient: Option<&[Color32]>,
) -> bool {
    let min = *range.start();
    let max = *range.end();
    if max <= min {
        return false;
    }
    let default = if min <= 0.0 && max >= 0.0 { 0.0 } else { min };
    let id = ui.scope_id().with(("slider", label));
    let edit_id = id.with("edit");
    let mut reset = false;
    let mut changed = false;
    let previous_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 2.0;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let label_response = ui
            .add(
                egui::Label::new(
                    RichText::new(label)
                        .size(theme::TEXT_SMALL)
                        .color(palette.dim),
                )
                .sense(Sense::click()),
            )
            .on_hover_text("Click to reset · Shift-drag for fine control");
        if label_response.clicked() {
            reset = true;
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(54.0, 18.0), Sense::click());
            let mut state =
                ui.data_mut(|data| data.get_temp::<SliderEdit>(edit_id).unwrap_or_default());

            if state.editing {
                let field_id = id.with("value-field");
                let text_edit = TextEdit::singleline(&mut state.buffer)
                    .id(field_id)
                    .font(TextStyle::Body)
                    .align(egui::Align2::RIGHT_CENTER)
                    .desired_width(54.0)
                    .margin(Margin::symmetric(3, 0));
                let field = ui.place(rect, text_edit);
                let enter = ui.input(|input| input.key_pressed(Key::Enter));
                let escape = ui.input(|input| input.key_pressed(Key::Escape));
                if escape {
                    state.editing = false;
                    ui.memory_mut(|memory| memory.surrender_focus(field_id));
                } else if enter || field.lost_focus() {
                    if let Ok(parsed) = state.buffer.trim().replace(',', ".").parse::<f32>() {
                        let clamped = parsed.clamp(min, max);
                        if (clamped - *value).abs() > f32::EPSILON {
                            *value = clamped;
                            changed = true;
                        }
                    }
                    state.editing = false;
                    ui.memory_mut(|memory| memory.surrender_focus(field_id));
                } else if !field.has_focus() {
                    ui.memory_mut(|memory| memory.request_focus(field_id));
                }
            } else {
                ui.painter().text(
                    egui::pos2(rect.right(), rect.center().y),
                    Align2::RIGHT_CENTER,
                    format_slider_value(*value, step),
                    FontId::proportional(11.5),
                    palette.text,
                );
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::Text);
                }
                if response.clicked() {
                    state.editing = true;
                    state.buffer = format_slider_value(*value, step);
                    ui.memory_mut(|memory| memory.request_focus(id.with("value-field")));
                }
                if response.double_clicked() {
                    reset = true;
                }
            }
            ui.data_mut(|data| data.insert_temp(edit_id, state));
        });
    });

    if reset {
        *value = default;
    }
    changed |= slider_track(
        ui, value, min, max, step, default, palette, gradient, 14.0, id,
    );
    ui.spacing_mut().item_spacing.y = previous_spacing;
    changed || reset
}

/// Compact stacked variant for the color-wheel control column.
pub fn slider_compact(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    label: &str,
    palette: Palette,
) -> bool {
    let min = *range.start();
    let max = *range.end();
    if max <= min {
        return false;
    }
    let default = if min <= 0.0 && max >= 0.0 { 0.0 } else { min };
    let mut reset = false;
    let previous_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 1.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let label_response = ui
            .add(
                egui::Label::new(
                    RichText::new(label)
                        .size(theme::TEXT_CAPTION)
                        .color(palette.dim),
                )
                .sense(Sense::click()),
            )
            .on_hover_text("Click to reset · Shift-drag for fine control");
        if label_response.clicked() {
            reset = true;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format_slider_value(*value, step))
                    .size(theme::TEXT_CAPTION)
                    .color(palette.text),
            );
        });
    });
    if reset {
        *value = default;
    }
    let changed = slider_track(
        ui,
        value,
        min,
        max,
        step,
        default,
        palette,
        None,
        12.0,
        ui.scope_id().with(("slider-compact", label)),
    ) || reset;
    ui.spacing_mut().item_spacing.y = previous_spacing;
    changed
}

/// Fixed-width numeric label: two decimals for 0.01 steps, one for 0.1, none
/// for integers. The explicit sign keeps the value column aligned.
fn format_slider_value(value: f32, step: f32) -> String {
    if value.abs() < 0.000_5 {
        return "0".to_owned();
    }
    let off_step = step > 0.0 && (value / step).fract().abs() > 0.000_5;
    if step >= 0.1 && !off_step {
        if step >= 1.0 {
            format!("{value:+.0}")
        } else {
            format!("{value:+.1}")
        }
    } else {
        format!("{value:+.2}")
    }
}

pub fn search_field(ui: &mut Ui, text: &mut String, width: f32, palette: Palette) -> bool {
    let m = palette.metrics;
    // `width` is the outer box width; the icon and clear affordance live inside.
    let total = Vec2::new(width.max(140.0), m.control_h_md);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(m.radius_md), palette.card);
    painter.rect_stroke(
        rect,
        CornerRadius::same(m.radius_md),
        Stroke::new(1.0, palette.input),
        StrokeKind::Inside,
    );

    let icon_size = 15.0;
    let icon_rect = Rect::from_min_size(
        egui::pos2(rect.left() + 11.0, rect.center().y - icon_size * 0.5),
        Vec2::splat(icon_size),
    );
    paint_search_icon(ui, icon_rect, palette.dim);

    let mut changed = false;
    // Leave room for the clear button only while it is visible.
    let clear_space = if text.is_empty() { 14.0 } else { 30.0 };
    let text_rect = Rect::from_min_max(
        egui::pos2(icon_rect.right() + 8.0, rect.top()),
        egui::pos2(rect.right() - clear_space, rect.bottom()),
    );
    // `place` (not `put`) adds the edit box without moving the layout cursor.
    // `put` advances the cursor after the inner text rect, which in a
    // right-to-left toolbar lands *inside* the painted search box and makes the
    // next widget (the grid-size dropdown) overlap it.
    let response = ui.place(
        text_rect,
        TextEdit::singleline(text)
            .id(Id::unique(SEARCH_ID))
            .frame(Frame::NONE)
            .hint_text(RichText::new("Search images").color(palette.dim))
            .desired_width(text_rect.width())
            .margin(Margin::ZERO)
            .align(egui::Align2::LEFT_CENTER),
    );
    changed |= response.changed();

    if !text.is_empty() {
        let clear_rect = Rect::from_center_size(
            egui::pos2(rect.right() - 17.0, rect.center().y),
            Vec2::splat(20.0),
        );
        let response = ui.interact(clear_rect, Id::unique("search-clear"), Sense::click());
        if response.hovered() {
            ui.painter()
                .circle_filled(clear_rect.center(), 9.0, palette.card_hover);
        }
        ui.painter().text(
            clear_rect.center(),
            egui::Align2::CENTER_CENTER,
            "×",
            FontId::proportional(14.0),
            palette.dim,
        );
        if response.clicked() {
            text.clear();
            changed = true;
        }
    }

    changed
}

pub fn small_caps(ui: &mut Ui, text: &str, palette: Palette) {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(FontId::proportional(theme::TEXT_CAPTION))
            .color(palette.dim),
    );
}

/// Replace egui's fixed low-pass wheel smoothing with the raw delta for the
/// scroll area under the pointer. Fast wheel input then moves fast and slow
/// input moves slowly, instead of every gesture easing in at the same rate.
/// The scroll area still owns scrollbar dragging and programmatic scrolling.
pub fn adaptive_scroll(ui: &mut Ui, viewport: Rect) {
    if !ui.rect_contains_pointer(viewport) {
        return;
    }
    let line_speed = ui
        .ctx()
        .options(|options| options.input_options.line_scroll_speed);
    let page_height = viewport.height();
    let raw_y = ui.input(|input| {
        input
            .raw
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel {
                    unit, delta, phase, ..
                } if matches!(*phase, egui::TouchPhase::Move) => Some(match unit {
                    egui::MouseWheelUnit::Point => delta.y,
                    egui::MouseWheelUnit::Line => delta.y * line_speed,
                    egui::MouseWheelUnit::Page => delta.y * page_height,
                }),
                _ => None,
            })
            .sum::<f32>()
    });
    // Even when this frame has no wheel event, clearing the smoothed delta
    // prevents egui's residual low-pass tail from adding lag after a notch.
    ui.input_mut(|input| input.smooth_scroll_delta.y = raw_y);
}

/// Compact track without a label/value row, used by the bottom zoom control.
pub fn rail_slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    palette: Palette,
) -> bool {
    let min = *range.start();
    let max = *range.end();
    let default = if min <= 0.0 && max >= 0.0 { 0.0 } else { min };
    slider_track(
        ui,
        value,
        min,
        max,
        step,
        default,
        palette,
        None,
        12.0,
        ui.scope_id(),
    )
}

/// One collapsible section in a flush panel: a compact header row with a top
/// separator, a right-hand chevron, and no card surface. This is the VS Code /
/// Zed panel rhythm rather than a floating card.
pub fn section_card(
    ui: &mut Ui,
    title: &str,
    open: &mut bool,
    palette: Palette,
    add_contents: impl FnOnce(&mut Ui),
) {
    let m = palette.metrics;
    ui.push_id(("section", title), |ui| {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), m.section_header_h),
            Sense::click(),
        );
        ui.painter().line_segment(
            [rect.left_top(), rect.right_top()],
            Stroke::new(1.0, palette.border),
        );
        if response.hovered() || response.is_pointer_button_down_on() {
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, palette.card_hover);
        }
        ui.painter().text(
            egui::pos2(rect.left() + 2.0, rect.center().y),
            Align2::LEFT_CENTER,
            title,
            theme::bold_font(theme::TEXT_SECTION),
            palette.text,
        );

        let t =
            ui.ctx()
                .animate_bool_with_time(ui.make_persistent_id("section-chevron"), *open, 0.14);
        let center = egui::pos2(rect.right() - 10.0, rect.center().y);
        let closed = [(-1.5, -3.5), (1.5, 0.0), (-1.5, 3.5)];
        let opened = [(-3.5, -1.5), (0.0, 1.5), (3.5, -1.5)];
        let mut points = [egui::Pos2::ZERO; 3];
        for i in 0..3 {
            points[i] = center
                + egui::vec2(
                    egui::lerp(closed[i].0..=opened[i].0, t),
                    egui::lerp(closed[i].1..=opened[i].1, t),
                );
        }
        ui.painter().add(egui::Shape::line(
            points.to_vec(),
            Stroke::new(1.3, palette.dim),
        ));

        if response.clicked() {
            *open = !*open;
        }
        if *open {
            Frame::new()
                .inner_margin(Margin {
                    left: 0,
                    right: 0,
                    top: 2,
                    bottom: 6,
                })
                .show(ui, add_contents);
        }
    });
}

/// Compact text tabs with an accent underline (VS Code / Zed panel tabs).
pub fn tabs(ui: &mut Ui, options: &[(&str, bool)], palette: Palette) -> Option<usize> {
    let mut clicked = None;
    ui.spacing_mut().item_spacing.x = 2.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (index, (label, active)) in options.iter().enumerate() {
            let width = 62.0;
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
            if *active {
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - 2.0),
                        rect.right_bottom(),
                    ),
                    CornerRadius::ZERO,
                    palette.accent,
                );
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(3), palette.card_hover);
            }
            ui.painter().text(
                egui::pos2(rect.left() + 8.0, rect.center().y - 1.0),
                Align2::LEFT_CENTER,
                *label,
                FontId::proportional(theme::TEXT_BODY),
                if *active { palette.text } else { palette.dim },
            );
            if response.clicked() {
                clicked = Some(index);
            }
        }
    });
    clicked
}

/// Ghost icon button (transparent until hovered), shadcn `ghost`/`sm` style.
pub fn icon_button(ui: &mut Ui, icon: Icon, palette: Palette) -> Response {
    ghost_icon_button(ui, icon, palette, palette.dim)
}

/// 20 px status-bar button. Keeps the bottom bar compact without shrinking the
/// normal toolbar hit targets.
pub fn status_icon_button(ui: &mut Ui, icon: Icon, active: bool, palette: Palette) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    if active || response.hovered() || response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(3), palette.card_hover);
    }
    paint_icon(
        ui,
        icon,
        rect.shrink(4.0),
        if active { palette.text } else { palette.dim },
    );
    response
}

/// Accent-filled icon button (shadcn `primary`/`sm`).
pub fn accent_icon_button(ui: &mut Ui, icon: Icon, palette: Palette) -> Response {
    let m = palette.metrics;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(m.icon_button), Sense::click());
    let fill = if response.hovered() || response.is_pointer_button_down_on() {
        palette.accent_hover
    } else {
        palette.accent
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(m.radius_md), fill);
    paint_icon(ui, icon, rect.shrink(6.5), palette.on_accent);
    response
}

/// Ghost icon button that shows the accent surface when `active`, used for
/// panel tabs (shadcn `ghost` with `data-state=active`).
pub fn toggle_icon_button(ui: &mut Ui, icon: Icon, active: bool, palette: Palette) -> Response {
    let m = palette.metrics;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(m.icon_button), Sense::click());
    if active || response.hovered() || response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(m.radius_md), palette.card_hover);
    }
    let color = if active { palette.text } else { palette.dim };
    paint_icon(ui, icon, rect.shrink(6.5), color);
    response
}

fn ghost_icon_button(ui: &mut Ui, icon: Icon, palette: Palette, color: Color32) -> Response {
    let m = palette.metrics;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(m.icon_button), Sense::click());
    let hovered = response.hovered() || response.is_pointer_button_down_on();
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(m.radius_md), palette.card_hover);
    }
    let inner = rect.shrink(6.5);
    paint_icon(ui, icon, inner, if hovered { palette.text } else { color });
    response
}

#[cfg(test)]
mod tests {
    use super::{color_wheel, format_slider_value, search_field, section_card, slider, tabs};
    use crate::ui::tokens::Palette;
    use egui::Layout;

    #[test]
    fn slider_values_have_a_stable_column() {
        assert_eq!(format_slider_value(-5.0, 0.01), "-5.00");
        assert_eq!(format_slider_value(5.0, 0.01), "+5.00");
        assert_eq!(format_slider_value(0.0, 0.01), "0");
        assert_eq!(format_slider_value(-2.5, 0.5), "-2.5");
        assert_eq!(format_slider_value(5.0, 1.0), "+5");
        assert_eq!(format_slider_value(-100.0, 1.0), "-100");
        assert_eq!(format_slider_value(0.2, 1.0), "+0.20");
        assert_eq!(format_slider_value(0.15, 0.1), "+0.15");
    }

    #[test]
    fn slider_row_fits_the_panel_width() {
        let palette = Palette::dark();
        egui::__run_test_ui(|ui| {
            ui.set_width(254.0);
            let mut value = 0.0;
            slider(ui, &mut value, -5.0..=5.0, 0.01, "Exposure", palette);
            let width = ui.min_rect().width();
            assert!(width <= 254.5, "slider row is {width} wide, panel is 254");
        });
    }

    #[test]
    fn color_wheel_row_fits_the_panel_width() {
        let palette = Palette::dark();
        egui::__run_test_ui(|ui| {
            ui.set_width(254.0);
            let mut wheel = crate::grade::HslRange::default();
            color_wheel(ui, &mut wheel, "Shadows", palette);
            let width = ui.min_rect().width();
            assert!(width <= 254.5, "wheel row is {width} wide, panel is 254");
        });
    }

    #[test]
    fn search_field_advances_only_its_box() {
        let palette = Palette::dark();
        egui::__run_test_ui(|ui| {
            ui.set_width(600.0);
            let mut search = String::new();
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                let right = ui.available_rect_before_wrap().right();
                let spacing = ui.spacing().item_spacing.x;
                search_field(ui, &mut search, 220.0, palette);
                let next = ui.next_widget_position().x;
                let expected = right - 220.0 - spacing;
                assert!(
                    (next - expected).abs() < 1.0,
                    "next widget starts at {next}, expected {expected} (box left)"
                );
            });
        });
    }

    #[test]
    fn section_card_fits_the_panel_width() {
        let palette = Palette::dark();
        egui::__run_test_ui(|ui| {
            ui.set_width(254.0);
            let mut open = true;
            section_card(ui, "Light", &mut open, palette, |ui| {
                ui.label("contents");
            });
            let width = ui.min_rect().width();
            assert!(width <= 254.5, "section card is {width} wide, panel is 254");
        });
    }

    #[test]
    fn tabs_fit_the_panel_width() {
        let palette = Palette::dark();
        egui::__run_test_ui(|ui| {
            ui.set_width(254.0);
            let _ = tabs(ui, &[("Adjust", true), ("Info", false)], palette);
            let width = ui.min_rect().width();
            assert!(width <= 254.5, "tabs are {width} wide, panel is 254");
        });
    }
}
