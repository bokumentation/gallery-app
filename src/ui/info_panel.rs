//! Right-panel “Image Info” tab: identity, path and metadata for the current
//! image, plus the session edit summary.

use crate::app::GalleryApp;
use crate::theme;
use crate::thumbs::human_size;
use egui::{Align, Layout, RichText, Ui};
use std::path::Path;
use std::time::SystemTime;

pub fn panel_content(app: &mut GalleryApp, ui: &mut Ui) {
    let palette = app.palette;
    let Some(item) = app.current_item() else {
        return;
    };
    let path = item.path.clone();

    ui.label(
        RichText::new(&item.name)
            .font(theme::bold_font(theme::TEXT_TITLE))
            .color(palette.text),
    );
    ui.add_space(2.0);
    ui.add(
        egui::Label::new(
            RichText::new(item.path.display().to_string())
                .size(theme::TEXT_CAPTION)
                .monospace()
                .color(palette.dim),
        )
        .selectable(true)
        .wrap(),
    );
    ui.add_space(10.0);

    let dimensions = app
        .viewer
        .source_dimensions(&path)
        .or_else(|| app.thumbs.get(&path).map(|thumb| thumb.size));
    let edit = app.edits.get(&path).cloned().unwrap_or_default();

    card(ui, "File", palette, |ui| {
        row(
            ui,
            "Size",
            format!("{} ({} bytes)", human_size(item.size), item.size),
            palette,
        );
        row(ui, "Format", file_format(&path), palette);
        row(ui, "Created", format_timestamp(item.created), palette);
        row(ui, "Modified", format_timestamp(item.modified), palette);
    });
    ui.add_space(4.0);

    card(ui, "Image", palette, |ui| {
        if let Some(source) = dimensions {
            let oriented = edit.oriented_size(source);
            let export = edit.display_size(source);
            row(
                ui,
                "Dimensions",
                format!("{} × {}", source[0], source[1]),
                palette,
            );
            if oriented != source {
                row(
                    ui,
                    "Oriented",
                    format!("{} × {}", oriented[0], oriented[1]),
                    palette,
                );
            }
            if export != source {
                row(
                    ui,
                    "Export size",
                    format!("{} × {}", export[0], export[1]),
                    palette,
                );
            }
            row(
                ui,
                "Megapixels",
                format!(
                    "{:.1} MP",
                    source[0] as f64 * source[1] as f64 / 1_000_000.0
                ),
                palette,
            );
        } else {
            row(ui, "Dimensions", "Loading…".to_owned(), palette);
        }
    });
    ui.add_space(4.0);

    card(ui, "Edit Summary", palette, |ui| {
        if edit.is_identity() {
            ui.label(
                RichText::new("No edits in this session")
                    .size(theme::TEXT_SMALL)
                    .color(palette.dim),
            );
        } else {
            let mut lines = Vec::new();
            if edit.orientation_steps != 0 {
                lines.push(format!("Rotate {}°", edit.orientation_steps * 90));
            }
            if edit.rotation != 0.0 {
                lines.push(format!("Straighten {:.1}°", edit.rotation));
            }
            if edit.flip_h {
                lines.push("Flip horizontally".to_owned());
            }
            if edit.flip_v {
                lines.push("Flip vertically".to_owned());
            }
            if let Some(crop) = edit.crop {
                lines.push(format!("Crop {} × {}", crop.w.round(), crop.h.round()));
            }
            if !edit.adjustments.is_identity() {
                lines.push("Color adjustments".to_owned());
            }
            if let Some(lut) = &edit.lut {
                lines.push(format!("LUT: {lut}"));
            }
            for line in lines {
                ui.label(
                    RichText::new(line)
                        .size(theme::TEXT_SMALL)
                        .color(palette.text),
                );
            }
        }
    });
}

fn card(ui: &mut Ui, title: &str, palette: crate::theme::Palette, add: impl FnOnce(&mut Ui)) {
    let top = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [top.left_top(), egui::pos2(top.right(), top.top())],
        egui::Stroke::new(1.0, palette.border),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(title)
            .font(theme::bold_font(theme::TEXT_SECTION))
            .color(palette.text),
    );
    ui.add_space(2.0);
    add(ui);
    ui.add_space(6.0);
}

fn row(ui: &mut Ui, label: &str, value: String, palette: crate::theme::Palette) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .size(theme::TEXT_SMALL)
                .color(palette.dim),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(value)
                        .size(theme::TEXT_SMALL)
                        .color(palette.text),
                )
                .truncate()
                .sense(egui::Sense::hover()),
            );
        });
    });
}

fn file_format(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_uppercase())
        .unwrap_or_else(|| "Unknown".to_owned())
}

/// Format a filesystem timestamp as `YYYY-MM-DD HH:MM UTC`. The standard
/// library exposes no calendar formatting, and pulling in a date crate for this
/// one row is not worth it; the result is UTC-labelled so it is unambiguous.
fn format_timestamp(time: Option<SystemTime>) -> String {
    let Some(time) = time else {
        return "—".to_owned();
    };
    let Ok(since_epoch) = time.duration_since(std::time::UNIX_EPOCH) else {
        return "—".to_owned();
    };
    let secs = since_epoch.as_secs();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3_600,
        (rem % 3_600) / 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a calendar date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (y + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn unix_epoch_formats() {
        assert_eq!(
            format_timestamp(Some(std::time::UNIX_EPOCH)),
            "1970-01-01 00:00 UTC"
        );
    }

    #[test]
    fn known_timestamp_formats() {
        let time = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(format_timestamp(Some(time)), "2023-11-14 22:13 UTC");
    }

    #[test]
    fn missing_timestamp_is_a_dash() {
        assert_eq!(format_timestamp(None), "—");
    }
}
