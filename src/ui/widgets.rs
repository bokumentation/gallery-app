use crate::theme::Palette;
pub use crate::ui::icons::{
    paint_folder_icon, paint_icon, paint_image_icon, paint_search_icon, Icon,
};
use egui::{
    Align, Button, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, Response,
    RichText, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2,
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
        Button::new(RichText::new(label).size(12.0).color(if selected {
            palette.on_accent
        } else {
            palette.text
        }))
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
        .min_size(Vec2::new(0.0, m.control_h_sm - 8.0)),
    )
}

pub fn symbol_button(ui: &mut Ui, glyph: &str, palette: Palette) -> Response {
    let m = palette.metrics;
    let response = ui.scope(|ui| {
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

pub fn segmented<T: Copy + PartialEq>(
    ui: &mut Ui,
    options: &[(T, &str)],
    value: &mut T,
    palette: Palette,
) -> bool {
    let m = palette.metrics;
    let mut changed = false;
    Frame::new()
        .fill(palette.card)
        .corner_radius(m.radius_md)
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (option, label) in options {
                let selected = *value == *option;
                let response = ui.add(
                    Button::new(
                        RichText::new(*label)
                            .size(12.0)
                            .color(if selected { palette.text } else { palette.dim }),
                    )
                    .fill(if selected {
                        palette.card_hover
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::NONE)
                    .corner_radius(m.radius_sm)
                    .min_size(Vec2::new(27.0, m.control_h_sm - 8.0)),
                );
                if response.clicked() && !selected {
                    *value = *option;
                    changed = true;
                }
            }
        });
    changed
}

pub fn switch(ui: &mut Ui, on: &mut bool, palette: Palette) -> Response {
    let m = palette.metrics;
    let desired = Vec2::new(m.switch_w, m.switch_h);
    let (rect, mut response) = ui.allocate_exact_size(desired, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let animation = ui
        .ctx()
        .animate_bool_with_time(response.id, *on, 0.12);
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
        let wheel_response = ui.allocate_ui_with_layout(
            Vec2::splat(wheel),
            Layout::top_down(Align::Center),
            |ui| wheel_pad(ui, value, palette),
        );
        changed |= wheel_response.inner;
        ui.add_space(gap);
        let controls = ui.allocate_ui_with_layout(
            Vec2::new(control_w, wheel),
            Layout::top_down(Align::LEFT),
            |ui| {
                ui.set_width(control_w);
                ui.label(RichText::new(label).size(11.0).color(palette.dim));
                let tint = hsv_rgb(value.hue, (value.saturation / 100.0).clamp(0.0, 1.0), 1.0);
                let (chip, _) = ui.allocate_exact_size(
                    Vec2::new(control_w.min(80.0), 6.0),
                    Sense::hover(),
                );
                ui.painter().rect_filled(chip, CornerRadius::same(3), tint);
                ui.spacing_mut().slider_width = 92.0;
                slider(ui, &mut value.luminance, -100.0..=100.0, 1.0, "Lum", palette)
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
            value.saturation =
                (((dist - inner) / (outer - inner)) * 100.0).clamp(0.0, 100.0);
            changed = true;
        }
    }
    changed
}

/// shadcn-outlined labelled slider; returns true when the value changed.
pub fn slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    label: &str,
    palette: Palette,
) -> bool {
    let before = *value;
    let slider = egui::Slider::new(value, range)
        .step_by(step as f64)
        .show_value(true)
        .text(label);
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.style_mut().visuals.widgets;
            widgets.inactive.bg_fill = palette.input;
            widgets.inactive.weak_bg_fill = palette.input;
            widgets.hovered.bg_fill = palette.ring;
            widgets.hovered.weak_bg_fill = palette.ring;
            widgets.active.bg_fill = palette.accent;
            widgets.active.weak_bg_fill = palette.accent;
            widgets.active.fg_stroke = Stroke::new(1.0, palette.on_accent);
            ui.add(slider)
        })
        .inner;
    response.changed() && *value != before
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
    let response = ui.put(
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
            ui.painter().circle_filled(
                clear_rect.center(),
                9.0,
                palette.card_hover,
            );
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
            .font(FontId::proportional(10.5))
            .color(palette.dim),
    );
}

/// Ghost icon button (transparent until hovered), shadcn `ghost`/`sm` style.
pub fn icon_button(ui: &mut Ui, icon: Icon, palette: Palette) -> Response {
    ghost_icon_button(ui, icon, palette, palette.text)
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
    paint_icon(ui, icon, rect.shrink(m.space_sm), palette.on_accent);
    response
}

/// Ghost icon button that shows the accent surface when `active`, used for
/// panel tabs (shadcn `ghost` with `data-state=active`).
pub fn toggle_icon_button(
    ui: &mut Ui,
    icon: Icon,
    active: bool,
    palette: Palette,
) -> Response {
    let m = palette.metrics;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(m.icon_button), Sense::click());
    if active {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(m.radius_md), palette.card_hover);
    } else if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(m.radius_md), palette.card_hover);
    }
    let color = if active { palette.text } else { palette.dim };
    paint_icon(ui, icon, rect.shrink(m.space_sm), color);
    response
}

fn ghost_icon_button(ui: &mut Ui, icon: Icon, palette: Palette, color: Color32) -> Response {
    let m = palette.metrics;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(m.icon_button), Sense::click());
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(m.radius_md), palette.card_hover);
    }
    let inner = rect.shrink(m.space_sm);
    paint_icon(ui, icon, inner, color);
    response
}
