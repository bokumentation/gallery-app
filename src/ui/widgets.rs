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
