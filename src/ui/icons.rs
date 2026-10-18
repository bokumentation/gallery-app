//! Lucide-style icon geometry as compile-time data.
//!
//! Every icon is a list of strokes in a 24×24 view box. Add or swap an icon by
//! editing one entry here; the renderer scales and centres the box into the
//! target rect. No runtime SVG parsing and no icon-font dependency.

use egui::{Color32, Rect, Stroke, StrokeKind, Ui};

#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(dead_code)] // Check/Sun/Moon and zoom icons are used by upcoming UI
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
    Folder,
    FolderOpen,
    Copy,
    ZoomIn,
    ZoomOut,
    Maximize,
    Restore,
    Minimize,
    Close,
    Image,
    Search,
}

/// One stroke in the 24×24 icon space.
#[derive(Clone, Copy)]
pub enum StrokeDef {
    Line([f32; 2], [f32; 2]),
    /// Open polyline; round caps and joins are added by the renderer.
    Poly(&'static [[f32; 2]]),
    Circle {
        center: [f32; 2],
        radius: f32,
    },
    /// Filled dot (used for icon details such as the info “i”).
    Dot {
        center: [f32; 2],
        radius: f32,
    },
    Rect {
        xy: [f32; 2],
        wh: [f32; 2],
        rx: f32,
    },
    /// Arc in screen degrees (0° = east, positive = clockwise).
    Arc {
        center: [f32; 2],
        radius: f32,
        start: f32,
        end: f32,
    },
}

const MENU: &[StrokeDef] = &[
    StrokeDef::Line([4.0, 6.0], [20.0, 6.0]),
    StrokeDef::Line([4.0, 12.0], [20.0, 12.0]),
    StrokeDef::Line([4.0, 18.0], [20.0, 18.0]),
];

const REFRESH_CW: &[StrokeDef] = &[
    StrokeDef::Arc {
        center: [12.0, 12.0],
        radius: 9.0,
        start: 200.0,
        end: 340.0,
    },
    StrokeDef::Poly(&[[17.5, 8.0], [20.6, 8.9], [19.6, 12.0]]),
    StrokeDef::Arc {
        center: [12.0, 12.0],
        radius: 9.0,
        start: 20.0,
        end: 160.0,
    },
    StrokeDef::Poly(&[[6.5, 16.0], [3.4, 15.1], [4.4, 12.0]]),
];

const ROTATE_RIGHT: &[StrokeDef] = &[
    StrokeDef::Arc {
        center: [12.0, 10.6],
        radius: 9.4,
        start: 115.0,
        end: 340.0,
    },
    StrokeDef::Line([21.4, 4.9], [18.4, 3.1]),
    StrokeDef::Line([21.4, 4.9], [19.0, 7.6]),
];

const ROTATE_LEFT: &[StrokeDef] = &[
    StrokeDef::Arc {
        center: [12.0, 10.6],
        radius: 9.4,
        start: -160.0,
        end: 65.0,
    },
    StrokeDef::Line([2.6, 4.9], [5.6, 3.1]),
    StrokeDef::Line([2.6, 4.9], [5.0, 7.6]),
];

const FLIP_H: &[StrokeDef] = &[
    StrokeDef::Poly(&[
        [8.0, 3.0],
        [5.0, 3.0],
        [3.0, 5.0],
        [3.0, 19.0],
        [5.0, 21.0],
        [8.0, 21.0],
    ]),
    StrokeDef::Poly(&[
        [16.0, 3.0],
        [19.0, 3.0],
        [21.0, 5.0],
        [21.0, 19.0],
        [19.0, 21.0],
        [16.0, 21.0],
    ]),
    StrokeDef::Line([12.0, 2.0], [12.0, 4.0]),
    StrokeDef::Line([12.0, 8.0], [12.0, 10.0]),
    StrokeDef::Line([12.0, 14.0], [12.0, 16.0]),
    StrokeDef::Line([12.0, 20.0], [12.0, 22.0]),
];

const FLIP_V: &[StrokeDef] = &[
    StrokeDef::Poly(&[
        [3.0, 8.0],
        [3.0, 5.0],
        [5.0, 3.0],
        [19.0, 3.0],
        [21.0, 5.0],
        [21.0, 8.0],
    ]),
    StrokeDef::Poly(&[
        [3.0, 16.0],
        [3.0, 19.0],
        [5.0, 21.0],
        [19.0, 21.0],
        [21.0, 19.0],
        [21.0, 16.0],
    ]),
    StrokeDef::Line([2.0, 12.0], [4.0, 12.0]),
    StrokeDef::Line([8.0, 12.0], [10.0, 12.0]),
    StrokeDef::Line([14.0, 12.0], [16.0, 12.0]),
    StrokeDef::Line([20.0, 12.0], [22.0, 12.0]),
];

const CROP: &[StrokeDef] = &[
    StrokeDef::Poly(&[[6.0, 2.0], [6.0, 16.0], [8.0, 18.0], [22.0, 18.0]]),
    StrokeDef::Poly(&[[18.0, 22.0], [18.0, 8.0], [16.0, 6.0], [2.0, 6.0]]),
];

const SLIDERS: &[StrokeDef] = &[
    StrokeDef::Line([4.0, 6.0], [14.0, 6.0]),
    StrokeDef::Line([18.0, 6.0], [20.0, 6.0]),
    StrokeDef::Dot {
        center: [16.0, 6.0],
        radius: 2.0,
    },
    StrokeDef::Line([4.0, 12.0], [6.0, 12.0]),
    StrokeDef::Line([10.0, 12.0], [20.0, 12.0]),
    StrokeDef::Dot {
        center: [8.0, 12.0],
        radius: 2.0,
    },
    StrokeDef::Line([4.0, 18.0], [14.0, 18.0]),
    StrokeDef::Line([18.0, 18.0], [20.0, 18.0]),
    StrokeDef::Dot {
        center: [16.0, 18.0],
        radius: 2.0,
    },
];

const DOWNLOAD: &[StrokeDef] = &[
    StrokeDef::Line([12.0, 3.0], [12.0, 15.0]),
    StrokeDef::Poly(&[[7.0, 10.0], [12.0, 15.0], [17.0, 10.0]]),
    StrokeDef::Line([5.0, 21.0], [19.0, 21.0]),
];

const CHECK: &[StrokeDef] = &[StrokeDef::Poly(&[[20.0, 6.0], [9.0, 17.0], [4.0, 12.0]])];

const SUN: &[StrokeDef] = &[
    StrokeDef::Circle {
        center: [12.0, 12.0],
        radius: 4.0,
    },
    StrokeDef::Line([12.0, 2.0], [12.0, 4.0]),
    StrokeDef::Line([12.0, 20.0], [12.0, 22.0]),
    StrokeDef::Line([2.0, 12.0], [4.0, 12.0]),
    StrokeDef::Line([20.0, 12.0], [22.0, 12.0]),
    StrokeDef::Line([8.5, 8.5], [7.0, 7.0]),
    StrokeDef::Line([15.5, 8.5], [17.0, 7.0]),
    StrokeDef::Line([8.5, 15.5], [7.0, 17.0]),
    StrokeDef::Line([15.5, 15.5], [17.0, 17.0]),
];

const MOON: &[StrokeDef] = &[
    StrokeDef::Arc {
        center: [12.0, 12.0],
        radius: 9.0,
        start: -132.0,
        end: 132.0,
    },
    StrokeDef::Arc {
        center: [15.6, 12.0],
        radius: 7.2,
        start: 115.0,
        end: 245.0,
    },
];

const INFO: &[StrokeDef] = &[
    StrokeDef::Circle {
        center: [12.0, 12.0],
        radius: 9.0,
    },
    StrokeDef::Line([12.0, 11.0], [12.0, 16.0]),
    StrokeDef::Dot {
        center: [12.0, 8.0],
        radius: 1.1,
    },
];

const FOLDER: &[StrokeDef] = &[StrokeDef::Poly(&[
    [3.0, 5.0],
    [8.0, 5.0],
    [10.0, 7.0],
    [21.0, 7.0],
    [21.0, 19.0],
    [3.0, 19.0],
    [3.0, 5.0],
])];

const FOLDER_OPEN: &[StrokeDef] = &[
    StrokeDef::Poly(&[
        [3.0, 19.0],
        [3.0, 5.0],
        [5.0, 3.0],
        [9.0, 3.0],
        [11.0, 5.0],
        [19.0, 5.0],
        [21.0, 7.0],
        [21.0, 10.0],
    ]),
    StrokeDef::Poly(&[
        [3.0, 19.0],
        [5.5, 11.0],
        [8.0, 9.0],
        [21.0, 9.0],
        [19.0, 19.0],
        [3.0, 19.0],
    ]),
];

const COPY: &[StrokeDef] = &[
    StrokeDef::Rect {
        xy: [8.0, 8.0],
        wh: [13.0, 13.0],
        rx: 2.0,
    },
    StrokeDef::Poly(&[
        [4.0, 16.0],
        [3.0, 15.0],
        [3.0, 4.0],
        [4.0, 3.0],
        [15.0, 3.0],
        [16.0, 4.0],
    ]),
];

const ZOOM_IN: &[StrokeDef] = &[
    StrokeDef::Circle {
        center: [11.0, 11.0],
        radius: 7.0,
    },
    StrokeDef::Line([16.0, 16.0], [21.0, 21.0]),
    StrokeDef::Line([11.0, 8.0], [11.0, 14.0]),
    StrokeDef::Line([8.0, 11.0], [14.0, 11.0]),
];

const ZOOM_OUT: &[StrokeDef] = &[
    StrokeDef::Circle {
        center: [11.0, 11.0],
        radius: 7.0,
    },
    StrokeDef::Line([16.0, 16.0], [21.0, 21.0]),
    StrokeDef::Line([8.0, 11.0], [14.0, 11.0]),
];

const MAXIMIZE: &[StrokeDef] = &[
    StrokeDef::Poly(&[[8.0, 3.0], [5.0, 3.0], [3.0, 5.0], [3.0, 8.0]]),
    StrokeDef::Poly(&[[21.0, 8.0], [21.0, 5.0], [19.0, 3.0], [16.0, 3.0]]),
    StrokeDef::Poly(&[[3.0, 16.0], [3.0, 19.0], [5.0, 21.0], [8.0, 21.0]]),
    StrokeDef::Poly(&[[16.0, 21.0], [19.0, 21.0], [21.0, 19.0], [21.0, 16.0]]),
];

const RESTORE: &[StrokeDef] = &[
    StrokeDef::Rect {
        xy: [8.0, 8.0],
        wh: [12.0, 12.0],
        rx: 2.0,
    },
    StrokeDef::Poly(&[[4.0, 16.0], [4.0, 5.0], [5.0, 4.0], [16.0, 4.0]]),
];

const MINIMIZE: &[StrokeDef] = &[StrokeDef::Line([5.0, 12.0], [19.0, 12.0])];

const CLOSE: &[StrokeDef] = &[
    StrokeDef::Line([6.0, 6.0], [18.0, 18.0]),
    StrokeDef::Line([18.0, 6.0], [6.0, 18.0]),
];

const IMAGE: &[StrokeDef] = &[
    StrokeDef::Rect {
        xy: [3.0, 3.0],
        wh: [18.0, 18.0],
        rx: 2.0,
    },
    StrokeDef::Circle {
        center: [9.0, 9.0],
        radius: 2.0,
    },
    StrokeDef::Poly(&[[21.0, 15.0], [16.0, 10.0], [5.0, 21.0]]),
];

const SEARCH: &[StrokeDef] = &[
    StrokeDef::Circle {
        center: [11.0, 11.0],
        radius: 7.0,
    },
    StrokeDef::Line([16.0, 16.0], [21.0, 21.0]),
];

/// Geometry for an icon. Swap icon sets here (or in the `const` tables above).
pub fn icon_def(icon: Icon) -> &'static [StrokeDef] {
    match icon {
        Icon::Menu => MENU,
        Icon::RefreshCw => REFRESH_CW,
        Icon::RotateLeft => ROTATE_LEFT,
        Icon::RotateRight => ROTATE_RIGHT,
        Icon::FlipH => FLIP_H,
        Icon::FlipV => FLIP_V,
        Icon::Crop => CROP,
        Icon::Sliders => SLIDERS,
        Icon::Download => DOWNLOAD,
        Icon::Check => CHECK,
        Icon::Sun => SUN,
        Icon::Moon => MOON,
        Icon::Info => INFO,
        Icon::Folder => FOLDER,
        Icon::FolderOpen => FOLDER_OPEN,
        Icon::Copy => COPY,
        Icon::ZoomIn => ZOOM_IN,
        Icon::ZoomOut => ZOOM_OUT,
        Icon::Maximize => MAXIMIZE,
        Icon::Restore => RESTORE,
        Icon::Minimize => MINIMIZE,
        Icon::Close => CLOSE,
        Icon::Image => IMAGE,
        Icon::Search => SEARCH,
    }
}

/// Paint `icon` centred in `rect`, scaling the 24×24 box to fit.
pub fn paint_icon(ui: &Ui, icon: Icon, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let side = rect.width().min(rect.height());
    if side <= 0.0 {
        return;
    }
    let scale = side / 24.0;
    let origin = rect.center() - egui::vec2(side * 0.5, side * 0.5);
    let map = |p: [f32; 2]| origin + egui::vec2(p[0], p[1]) * scale;
    let stroke = Stroke::new((2.0 * scale).max(1.0), color);
    let cap = stroke.width * 0.5;

    for def in icon_def(icon) {
        match *def {
            StrokeDef::Line(a, b) => {
                let (a, b) = (map(a), map(b));
                painter.line_segment([a, b], stroke);
                painter.circle_filled(a, cap, color);
                painter.circle_filled(b, cap, color);
            }
            StrokeDef::Poly(points) => {
                let pts: Vec<egui::Pos2> = points.iter().map(|p| map(*p)).collect();
                for point in &pts {
                    painter.circle_filled(*point, cap, color);
                }
                painter.add(egui::Shape::line(pts, stroke));
            }
            StrokeDef::Circle { center, radius } => {
                painter.circle_stroke(map(center), radius * scale, stroke);
            }
            StrokeDef::Dot { center, radius } => {
                painter.circle_filled(map(center), radius * scale, color);
            }
            StrokeDef::Rect { xy, wh, rx } => {
                let rect = Rect::from_min_size(map(xy), egui::vec2(wh[0], wh[1]) * scale);
                painter.rect_stroke(
                    rect,
                    egui::CornerRadius::same((rx * scale) as u8),
                    stroke,
                    StrokeKind::Inside,
                );
            }
            StrokeDef::Arc {
                center,
                radius,
                start,
                end,
            } => {
                const STEPS: u32 = 24;
                let pts: Vec<egui::Pos2> = (0..=STEPS)
                    .map(|k| {
                        let t = k as f32 / STEPS as f32;
                        let angle = egui::lerp(start..=end, t).to_radians();
                        map(center) + egui::vec2(angle.cos(), angle.sin()) * (radius * scale)
                    })
                    .collect();
                painter.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

pub fn paint_folder_icon(ui: &Ui, rect: Rect, color: Color32) {
    paint_icon(ui, Icon::Folder, rect, color);
}

pub fn paint_search_icon(ui: &Ui, rect: Rect, color: Color32) {
    paint_icon(ui, Icon::Search, rect, color);
}

pub fn paint_image_icon(ui: &Ui, rect: Rect, color: Color32) {
    paint_icon(ui, Icon::Image, rect, color);
}
