use crate::theme::Palette;
use egui::{
    Button, Color32, CornerRadius, FontId, Frame, Id, Margin, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, TextEdit, Ui, Vec2,
};

pub const SEARCH_ID: &str = "gallery-search";

pub fn flat_button(ui: &mut Ui, label: &str, palette: Palette) -> Response {
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
                .corner_radius(8),
        )
    })
    .inner
}

pub fn accent_button(ui: &mut Ui, label: &str, palette: Palette) -> Response {
    ui.add(
        Button::new(RichText::new(label).color(palette.on_accent))
            .fill(palette.accent)
            .stroke(Stroke::NONE)
            .corner_radius(9)
            .min_size(Vec2::new(0.0, 32.0)),
    )
}

/// Small chip button used for preset rows: filled when selected, ghost
/// otherwise, so the active choice is always visible.
pub fn toggle_button(ui: &mut Ui, label: &str, selected: bool, palette: Palette) -> Response {
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
            Stroke::new(1.0, palette.border)
        })
        .corner_radius(8)
        .min_size(Vec2::new(0.0, 24.0)),
    )
}

pub fn symbol_button(ui: &mut Ui, glyph: &str, palette: Palette) -> Response {
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
                .min_size(Vec2::new(32.0, 32.0))
                .corner_radius(8),
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
    let mut changed = false;
    Frame::new()
        .fill(palette.card)
        .corner_radius(9)
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
                    .corner_radius(7)
                    .min_size(Vec2::new(27.0, 23.0)),
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
    let desired = Vec2::new(40.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(desired, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let animation = ui
        .ctx()
        .animate_bool_with_time(response.id, *on, 0.12);
    let background = if *on {
        palette.accent
    } else {
        palette.card_hover
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(11), background);
    let knob_x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), animation);
    ui.painter().circle_filled(
        egui::pos2(knob_x, rect.center().y),
        8.0,
        Color32::WHITE,
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
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            let size = Vec2::splat(76.0);
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
                // Slightly overlap segments to avoid seams.
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
        });
        ui.vertical(|ui| {
            ui.label(RichText::new(label).size(11.0).color(palette.dim));
            let tint = hsv_rgb(value.hue, (value.saturation / 100.0).clamp(0.0, 1.0), 1.0);
            let (chip, _) = ui
                .allocate_exact_size(Vec2::new(ui.available_width().min(16.0), 6.0), Sense::hover());
            ui.painter().rect_filled(chip, CornerRadius::same(3), tint);
            changed |= slider(ui, &mut value.luminance, -100.0..=100.0, 1.0, "Lum");
        });
    });
    changed
}

/// Adwaita-styled labelled slider; returns true when the value changed.
pub fn slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    label: &str,
) -> bool {
    let before = *value;
    let slider = egui::Slider::new(value, range)
        .step_by(step as f64)
        .show_value(true)
        .text(label);
    let response = ui.add(slider);
    response.changed() && *value != before
}

pub fn search_field(ui: &mut Ui, text: &mut String, width: f32, palette: Palette) -> bool {
    let total = Vec2::new(width + 68.0, 32.0);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(16),
        if palette.dark {
            Color32::from_rgb(34, 34, 34)
        } else {
            Color32::from_rgb(255, 255, 255)
        },
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(16),
        Stroke::new(1.0, palette.border),
        StrokeKind::Inside,
    );

    let icon_rect = Rect::from_min_size(
        rect.min + Vec2::new(13.0, (rect.height() - 15.0) * 0.5),
        Vec2::splat(15.0),
    );
    paint_search_icon(ui, icon_rect, palette.dim);

    let mut changed = false;
    let text_rect = Rect::from_min_max(
        egui::pos2(icon_rect.right() + 8.0, rect.top() + 4.0),
        egui::pos2(rect.right() - 14.0, rect.bottom() - 4.0),
    );
    let response = ui.put(
        text_rect,
        TextEdit::singleline(text)
            .id(Id::unique(SEARCH_ID))
            .frame(Frame::NONE)
            .hint_text(RichText::new("Search images").color(palette.dim))
            .desired_width(text_rect.width())
            .margin(egui::vec2(0.0, 3.0)),
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

pub fn paint_folder_icon(ui: &Ui, rect: egui::Rect, color: Color32) {
    let painter = ui.painter();
    let tab = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(0.0, rect.height() * 0.12),
        egui::vec2(rect.width() * 0.46, rect.height() * 0.24),
    );
    painter.rect_filled(tab, CornerRadius::same(2), color);
    let body = egui::Rect::from_min_max(
        rect.left_top() + egui::vec2(0.0, rect.height() * 0.28),
        rect.right_bottom(),
    );
    painter.rect_filled(body, CornerRadius::same(3), color);
}

pub fn paint_search_icon(ui: &Ui, rect: egui::Rect, color: Color32) {
    let painter = ui.painter();
    let radius = rect.width() * 0.3;
    let center = rect.left_top() + egui::vec2(radius + 1.0, radius + 1.0);
    painter.circle_stroke(center, radius, Stroke::new(1.3, color));
    painter.line_segment(
        [
            center + egui::vec2(radius * 0.72, radius * 0.72),
            rect.right_bottom(),
        ],
        Stroke::new(1.4, color),
    );
}

pub fn paint_image_icon(ui: &Ui, rect: egui::Rect, color: Color32) {
    let painter = ui.painter();
    painter.rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(1.4, color),
        StrokeKind::Inside,
    );
    let sun = egui::pos2(rect.left() + rect.width() * 0.3, rect.top() + rect.height() * 0.3);
    painter.circle_filled(sun, rect.width() * 0.07, color);
    painter.line_segment(
        [
            egui::pos2(rect.left() + rect.width() * 0.12, rect.bottom() - rect.height() * 0.15),
            egui::pos2(rect.left() + rect.width() * 0.45, rect.top() + rect.height() * 0.5),
        ],
        Stroke::new(1.4, color),
    );
    painter.line_segment(
        [
            egui::pos2(rect.left() + rect.width() * 0.35, rect.top() + rect.height() * 0.62),
            egui::pos2(rect.right() - rect.width() * 0.1, rect.bottom() - rect.height() * 0.15),
        ],
        Stroke::new(1.4, color),
    );
}

pub fn small_caps(ui: &mut Ui, text: &str, palette: Palette) {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(FontId::proportional(10.5))
            .color(palette.dim),
    );
}

// ---------------------------------------------------------------------------
// Lucide-style icons, drawn as painter geometry (24x24 viewBox mapped to rect)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(dead_code)] // Check/Sun/Moon kept for upcoming panel polish
pub enum Icon {
    Menu,
    RefreshCw,
    RotateLeft,
    RotateRight,
    FlipH,
    FlipV,
    Crop,
    Sliders,
    Download,
    Check,
    Sun,
    Moon,
    Info,
}

/// Ghost icon button (transparent until hovered), shadcn `ghost`/`sm` style.
pub fn icon_button(ui: &mut Ui, icon: Icon, palette: Palette) -> Response {
    ghost_icon_button(ui, icon, palette, palette.text)
}

/// Accent-filled icon button (shadcn `primary`/`sm`).
pub fn accent_icon_button(ui: &mut Ui, icon: Icon, palette: Palette) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let fill = if response.hovered() || response.is_pointer_button_down_on() {
        palette.accent.gamma_multiply(1.12)
    } else {
        palette.accent
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), fill);
    paint_icon(ui, icon, rect.shrink(6.0), palette.on_accent);
    response
}

fn ghost_icon_button(ui: &mut Ui, icon: Icon, palette: Palette, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), palette.card_hover);
    }
    let inner = rect.shrink(6.0);
    paint_icon(ui, icon, inner, color);
    response
}

/// Filled arc drawn as a line polyline (degrees 0 = east, positive clockwise
/// screen space).
pub fn arc(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    from_deg: f32,
    to_deg: f32,
    stroke: Stroke,
) {
    let steps = 24;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|k| {
            let t = k as f32 / steps as f32;
            let a_deg = egui::lerp(from_deg..=to_deg, t);
            let a = a_deg.to_radians();
            center + egui::vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, stroke));
}

pub fn paint_icon(ui: &Ui, icon: Icon, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.6, color);
    let l = rect.left();
    let r = rect.right();
    let t = rect.top();
    let b = rect.bottom();
    let m = (l + r) * 0.5;
    let cy = (t + b) * 0.5;

    match icon {
        Icon::Menu => {
            for k in 0..3 {
                let y = t + rect.height() * (0.18 + 0.32 * k as f32);
                painter.line_segment([egui::pos2(l, y), egui::pos2(r, y)], stroke);
            }
        }
        Icon::RefreshCw => {
            // open circular arrows (two half arcs with arrowheads)
            let radius = rect.width() * 0.38;
            let center = egui::pos2(m, cy);
            arc(painter, center, radius, 170.0, 350.0, stroke);
            arc(painter, center, radius, -10.0, 170.0, stroke);
            // arrowheads at arc ends
            painter.line_segment(
                [egui::pos2(m + radius, cy - radius * 0.5), egui::pos2(m + radius, cy + 1.0)],
                stroke,
            );
        }
        Icon::RotateLeft | Icon::RotateRight => {
            let ccw = icon == Icon::RotateLeft;
            let radius = rect.width() * 0.40;
            let center = egui::pos2(m, cy - rect.height() * 0.06);
            let (from, to): (f32, f32) = if ccw {
                (-170.0, 60.0)
            } else {
                (110.0, 340.0)
            };
            arc(painter, center, radius, from, to, stroke);
            // arrowhead at the start of the arc (top edge)
            let head = egui::pos2(m + if ccw { -radius } else { radius }, cy - rect.height() * 0.30);
            let dir = if ccw { 0.55 } else { -0.55 };
            painter.line_segment(
                [head, head + egui::vec2(-dir * 4.0, -3.4)],
                stroke,
            );
            painter.line_segment([head, head + egui::vec2(dir * 4.0, -3.4)], stroke);
        }
        Icon::FlipH => {
            // mirrored triangles split by vertical edge
            let inner = 2.0;
            let bl = egui::pos2(l + inner, b - inner);
            let tl = egui::pos2(l + inner, t + inner);
            let mr = egui::pos2(m - 1.0, cy);
            painter.line_segment([tl, mr], stroke);
            painter.line_segment([mr, bl], stroke);
            let br = egui::pos2(r - inner, b - inner);
            let tr = egui::pos2(r - inner, t + inner);
            let ml = egui::pos2(m + 1.0, cy);
            painter.line_segment([tr, ml], stroke);
            painter.line_segment([ml, br], stroke);
            painter.line_segment([egui::pos2(m, t + 1.0), egui::pos2(m, b - 1.0)], stroke);
        }
        Icon::FlipV => {
            let inner = 2.0;
            let tl = egui::pos2(l + inner, t + inner);
            let tr = egui::pos2(r - inner, t + inner);
            let mc = egui::pos2(m, (t + inner + cy) * 0.5);
            painter.line_segment([tl, mc], stroke);
            painter.line_segment([mc, tr], stroke);
            let bl = egui::pos2(l + inner, b - inner);
            let br = egui::pos2(r - inner, b - inner);
            let mb = egui::pos2(m, (cy + 1.0 + b - inner) * 0.5);
            painter.line_segment([bl, mb], stroke);
            painter.line_segment([mb, br], stroke);
            painter.line_segment([egui::pos2(l + 1.0, cy), egui::pos2(r - 1.0, cy)], stroke);
        }
        Icon::Crop => {
            // M6 2 V16 a2 2 0 0 0 2 2 H22  +  M22 8 V8H8 a2 2 0 0 0 -2 2 V22 ... simplified lucide crop
            let x0 = l + rect.width() * 0.28;
            let y0 = t + rect.height() * 0.28;
            let x1 = r - rect.width() * 0.24;
            let y1 = b - rect.height() * 0.24;
            painter
                .line_segment([egui::pos2(x0, t + 2.0), egui::pos2(x0, y1 - 4.0)], stroke);
            arc(painter, egui::pos2(x0 + 4.0, y1 - 4.0), 4.0, 90.0, 180.0, stroke);
            painter.line_segment([egui::pos2(x0 + 4.0, y1), egui::pos2(r - 2.0, y1)], stroke);
            painter.line_segment([egui::pos2(r - 2.0, y1 - 6.0), egui::pos2(r - 2.0, y1)], stroke);
            painter.line_segment(
                [egui::pos2(x1, b - 2.0), egui::pos2(x1, y0)],
                stroke,
            );
            arc(painter, egui::pos2(x1 - 4.0, y0), 4.0, 270.0, 360.0, stroke);
            painter.line_segment([egui::pos2(x1 - 4.0, y0 - 4.0), egui::pos2(l + 2.0, y0 - 4.0)], stroke);
            painter.line_segment([egui::pos2(l + 2.0, y0 - 4.0), egui::pos2(l + 2.0, y0 + 2.0)], stroke);
        }
        Icon::Sliders => {
            // three horizontal lines with knobs at staggered heights
            for k in 0..3 {
                let y = t + rect.height() * (0.14 + 0.36 * k as f32);
                let knob_x = match k {
                    0 => l + rect.width() * 0.62,
                    1 => l + rect.width() * 0.30,
                    _ => l + rect.width() * 0.72,
                };
                painter.line_segment(
                    [egui::pos2(l, y), egui::pos2(r, y)],
                    stroke,
                );
                painter.circle_filled(egui::pos2(knob_x, y), 2.4, color);
            }
        }
        Icon::Download => {
            let top = t + rect.height() * 0.20;
            let bottom = b - rect.height() * 0.32;
            painter.line_segment([egui::pos2(m, top), egui::pos2(m, bottom)], stroke);
            painter.line_segment(
                [egui::pos2(m - 3.6, bottom - 4.0), egui::pos2(m, bottom)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(m + 3.6, bottom - 4.0), egui::pos2(m, bottom)],
                stroke,
            );
            painter.line_segment([egui::pos2(l, b - 3.0), egui::pos2(l, b)], stroke);
            painter.line_segment([egui::pos2(l, b), egui::pos2(r, b)], stroke);
            painter.line_segment([egui::pos2(r, b), egui::pos2(r, b - 3.0)], stroke);
        }
        Icon::Check => {
            painter.line_segment(
                [
                    egui::pos2(l + rect.width() * 0.15, cy + 0.5),
                    egui::pos2(l + rect.width() * 0.42, b - rect.height() * 0.18),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(l + rect.width() * 0.42, b - rect.height() * 0.18),
                    egui::pos2(r - rect.width() * 0.12, t + rect.height() * 0.18),
                ],
                stroke,
            );
        }
        Icon::Sun => {
            let radius = rect.width() * 0.26;
            painter.circle_stroke(egui::pos2(m, cy), radius, stroke);
            for k in 0..8 {
                let ang = k as f32 * std::f32::consts::TAU / 8.0;
                let dir = egui::vec2(ang.cos(), ang.sin());
                painter.line_segment(
                    [
                        egui::pos2(m, cy) + dir * (radius + 1.5),
                        egui::pos2(m, cy) + dir * (radius + 4.5),
                    ],
                    stroke,
                );
            }
        }
        Icon::Moon => {
            let radius = rect.width() * 0.67;
            let center = egui::pos2(m, cy);
            // crescent: big arc + cut-back arc
            arc(painter, center, radius, -138.0, 138.0, stroke);
            arc(
                painter,
                center + egui::vec2(radius * 0.55, 0.0),
                radius * 0.78,
                115.0,
                245.0,
                stroke,
            );
        }
        Icon::Info => {
            let radius = rect.width() * 0.42;
            painter.circle_stroke(egui::pos2(m, cy), radius, stroke);
            painter.circle_filled(egui::pos2(m, t + rect.height() * 0.34), 1.4, color);
            painter.line_segment(
                [egui::pos2(m, cy + 0.5), egui::pos2(m, b - rect.height() * 0.24)],
                stroke,
            );
        }
    }
}
