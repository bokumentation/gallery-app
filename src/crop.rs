//! Crop geometry with RapidRAW's validity model: the crop rectangle lives in
//! pipeline space (orientation steps and flips applied, so the free rotation
//! is the only transform between it and the base image) and a rectangle is
//! valid when every corner, rotated back by the straighten angle, stays
//! inside the oriented source image.

use crate::grade::CropRect;

/// Nothing smaller than this is a useful crop (pixels in source space).
pub const MIN_CROP_PX: f32 = 64.0;

const EPSILON: f32 = 0.25; // keep corners on or inside the boundary

#[derive(Clone, Copy, Debug)]
pub struct CropGeom {
    /// Oriented source dimensions in pixels.
    pub oriented: [f32; 2],
    /// Free rotation in degrees, -45..=45.
    pub rotation_deg: f32,
}

impl CropGeom {
    /// Maps a pipeline-space point back into oriented image space (inverse of
    /// the rotation that sits between the rectangle and the image).
    fn unrotate(&self, point: [f32; 2]) -> [f32; 2] {
        let center = [self.oriented[0] * 0.5, self.oriented[1] * 0.5];
        if self.rotation_deg == 0.0 {
            return point;
        }
        let angle = -self.rotation_deg.to_radians();
        let (sin, cos) = angle.sin_cos();
        let delta = [point[0] - center[0], point[1] - center[1]];
        [
            center[0] + delta[0] * cos - delta[1] * sin,
            center[1] + delta[0] * sin + delta[1] * cos,
        ]
    }

    /// Every crop corner must unrotate into the oriented image.
    pub fn is_valid(&self, crop: CropRect) -> bool {
        crop.w >= MIN_CROP_PX && crop.h >= MIN_CROP_PX && self.corners_inside(crop)
    }

    /// Geometry-only check (no minimum size), used by the fit search where
    /// sub-minimum candidates are just intermediates.
    fn corners_inside(&self, crop: CropRect) -> bool {
        let x2 = crop.x + crop.w;
        let y2 = crop.y + crop.h;
        let corners = [
            [crop.x, crop.y],
            [x2, crop.y],
            [crop.x, y2],
            [x2, y2],
            [crop.x, crop.y * 0.5 + y2 * 0.5],
        ];
        let inside = |point: [f32; 2]| {
            point[0] >= -EPSILON
                && point[1] >= -EPSILON
                && point[0] <= self.oriented[0] + EPSILON
                && point[1] <= self.oriented[1] + EPSILON
        };
        corners.iter().all(|corner| inside(self.unrotate(*corner)))
    }

    /// Largest scale in (0, 1] factor applied to `crop` about its center that
    /// keeps the geometry inside the image; 1.0 when it already is.
    /// Shrinking converges to valid, so the search grows `low` while a
    /// candidate stays inside and backs `high` off when it does not.
    fn max_scale(&self, crop: CropRect) -> f32 {
        if self.corners_inside(crop) {
            return 1.0;
        }
        let center = [crop.x + crop.w * 0.5, crop.y + crop.h * 0.5];
        let mut low = 0.0f32;
        let mut high = 1.0f32;
        for _ in 0..32 {
            let mid = (low + high) * 0.5;
            if self.corners_inside(scaled_rect(crop, center, mid)) {
                low = mid;
            } else {
                high = mid;
            }
        }
        low
    }

    /// Shrink `crop` about its center (or, for axis-aligned cases, clamp to
    /// the image) until valid.
    pub fn fit(&self, crop: CropRect) -> CropRect {
        if self.rotation_deg == 0.0 {
            return self.clamp_axis_aligned(crop);
        }
        let scale = self.max_scale(crop);
        let center = [crop.x + crop.w * 0.5, crop.y + crop.h * 0.5];
        let fitted = scaled_rect(crop, center, scale);
        match self.is_valid(fitted) {
            true => fitted,
            false => {
                // Even at full scale of the fit, keep at least the minimum.
                let width = crop.w.max(MIN_CROP_PX);
                let height = crop.h.max(MIN_CROP_PX);
                CropRect {
                    x: center[0] - width * 0.5,
                    y: center[1] - height * 0.5,
                    w: width,
                    h: height,
                }
            }
        }
    }

    /// Axis-aligned case: intersect with the image and enforce the minimum.
    fn clamp_axis_aligned(&self, crop: CropRect) -> CropRect {
        let min_w = MIN_CROP_PX.min(self.oriented[0]);
        let min_h = MIN_CROP_PX.min(self.oriented[1]);
        let x = crop.x.max(0.0).min((self.oriented[0] - min_w).max(0.0));
        let y = crop.y.max(0.0).min((self.oriented[1] - min_h).max(0.0));
        let w = ((crop.x + crop.w).min(self.oriented[0]) - x)
            .max(min_w)
            .min(self.oriented[0] - x);
        let h = ((crop.y + crop.h).min(self.oriented[1]) - y)
            .max(min_h)
            .min(self.oriented[1] - y);
        CropRect { x, y, w, h }
    }

    /// Largest axis-aligned rectangle centered in the image; `aspect` is
    /// width / height when given.
    pub fn centered(&self, aspect: Option<f32>) -> CropRect {
        let [width, height] = self.oriented;
        let (width, height) = match aspect {
            Some(ratio) if width / height > ratio => (height * ratio, height),
            Some(ratio) => (width, width / ratio),
            None => (width, height),
        };
        let width = width
            .min(self.oriented[0])
            .max(MIN_CROP_PX.min(self.oriented[0]));
        let height = height
            .min(self.oriented[1])
            .max(MIN_CROP_PX.min(self.oriented[1]));
        CropRect {
            x: (self.oriented[0] - width) * 0.5,
            y: (self.oriented[1] - height) * 0.5,
            w: width,
            h: height,
        }
    }

    /// Adjust the sides of `crop` to `aspect` (width/height) about its center,
    /// keeping it valid and at least MIN_CROP_PX on the long side. Used by the
    /// aspect preset buttons and tested from the unit suite.
    pub fn apply_aspect(&self, crop: CropRect, aspect: f32) -> CropRect {
        let center = [crop.x + crop.w * 0.5, crop.y + crop.h * 0.5];
        let mut candidate = if crop.w / crop.h > aspect {
            let width = crop.h * aspect;
            let height = crop.h;
            rect_about(center, width, height)
        } else {
            let width = crop.w;
            let height = crop.w / aspect;
            rect_about(center, width, height)
        };
        // Grow the short side instead of shrinking the long one.
        if candidate.w < MIN_CROP_PX || candidate.h < MIN_CROP_PX {
            let height = (candidate.h.max(MIN_CROP_PX) * aspect.max(1.0))
                .min(self.oriented[0] / 2.0)
                .max(MIN_CROP_PX);
            let width = height * aspect;
            candidate = rect_about(center, width, height);
        }
        self.fit(candidate)
    }
}

fn scaled_rect(crop: CropRect, center: [f32; 2], scale: f32) -> CropRect {
    rect_about(center, crop.w * scale, crop.h * scale)
}

fn rect_about(center: [f32; 2], width: f32, height: f32) -> CropRect {
    CropRect {
        x: center[0] - width * 0.5,
        y: center[1] - height * 0.5,
        w: width,
        h: height,
    }
}

/// State of an in-progress crop drag. Constructed when a grip picks up a
/// drag; `apply` maps a pointer delta into the updated source-space rect.
/// Pure numbers, so the UI layer only converts between screen and source
/// pixels.
#[derive(Clone, Copy, Debug)]
pub struct CropDrag {
    pub handle: Handle,
    /// Pointer position in source pixels at drag start.
    pub start_pointer: [f32; 2],
    pub start_rect: CropRect,
    pub geometry: CropGeom,
    /// Aspect (width/height) enforced while dragging, when set.
    pub aspect: Option<f32>,
}

impl CropDrag {
    /// The updated rectangle after the pointer moved by `delta` (in source
    /// pixels). Result may still be invalid when rotated; the caller
    /// finalises with [`CropGeom::fit`].
    pub fn apply(&self, delta: [f32; 2]) -> CropRect {
        let [max_x, max_y] = self.geometry.oriented;
        let r = self.start_rect;
        let mut left = r.x;
        let mut top = r.y;
        let mut right = r.x + r.w;
        let mut bottom = r.y + r.h;
        let [dx, dy] = delta;

        match self.handle {
            Handle::Move => {
                let width = r.w;
                let height = r.h;
                let max_left = (max_x - width).max(0.0);
                let max_top = (max_y - height).max(0.0);
                let mut candidate = self.start_rect;
                candidate.x = (candidate.x + dx).clamp(0.0, max_left);
                candidate.y = (candidate.y + dy).clamp(0.0, max_top);
                if self.geometry.rotation_deg != 0.0 {
                    // Step sizes shrink until the rect is still valid.
                    for step in 1..=8u32 {
                        let t = 1.0 / (1 << step) as f32;
                        let attempt = CropRect {
                            x: r.x + dx * t,
                            y: r.y + dy * t,
                            w: width,
                            h: height,
                        };
                        if self.geometry.is_valid(attempt) {
                            candidate = attempt;
                            break;
                        }
                    }
                }
                return candidate;
            }
            Handle::North => {
                top = (top + dy).clamp(0.0, bottom - MIN_CROP_PX);
            }
            Handle::South => {
                bottom = (bottom + dy).clamp(top + MIN_CROP_PX, max_y);
            }
            Handle::West => {
                left = (left + dx).clamp(0.0, right - MIN_CROP_PX);
            }
            Handle::East => {
                right = (right + dx).clamp(left + MIN_CROP_PX, max_x);
            }
            corner => {
                // Corner grips: the diagonally opposite corner stays fixed.
                let sign_x = match corner {
                    Handle::NorthWest | Handle::SouthWest => -1.0,
                    _ => 1.0,
                };
                let sign_y = match corner {
                    Handle::NorthWest | Handle::NorthEast => -1.0,
                    _ => 1.0,
                };
                let anchor = (
                    if sign_x > 0.0 { left } else { right },
                    if sign_y > 0.0 { top } else { bottom },
                );
                let max_w = if sign_x > 0.0 {
                    max_x - anchor.0
                } else {
                    anchor.0
                };
                let max_h = if sign_y > 0.0 {
                    max_y - anchor.1
                } else {
                    anchor.1
                };
                let mut width = (r.w + dx * sign_x).max(MIN_CROP_PX).min(max_w);
                let mut height = (r.h + dy * sign_y).max(MIN_CROP_PX).min(max_h);
                if let Some(aspect) = self.aspect {
                    // The axis with the larger effective change drives; the
                    // other follows and keeps the rect inside the image.
                    if dx.abs() * aspect > dy.abs() {
                        height = (width / aspect).min(max_h).max(MIN_CROP_PX.min(max_h));
                        width = (height * aspect).min(max_w).max(MIN_CROP_PX.min(max_w));
                    } else {
                        width = (height * aspect).min(max_w).max(MIN_CROP_PX.min(max_w));
                        height = (width / aspect).min(max_h).max(MIN_CROP_PX.min(max_h));
                    }
                }
                left = if sign_x > 0.0 {
                    anchor.0
                } else {
                    anchor.0 - width
                };
                right = left + width;
                top = if sign_y > 0.0 {
                    anchor.1
                } else {
                    anchor.1 - height
                };
                bottom = top + height;
            }
        }

        CropRect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        }
    }
}

/// Handles of the on-screen crop frame. Screen-space drag maps through these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    Move,
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl Handle {
    /// All eight grips around the rectangle, clockwise from the top-left.
    pub fn grips() -> [(Handle, f32, f32); 8] {
        [
            (Handle::NorthWest, 0.0, 0.0),
            (Handle::North, 0.5, 0.0),
            (Handle::NorthEast, 1.0, 0.0),
            (Handle::East, 1.0, 0.5),
            (Handle::SouthEast, 1.0, 1.0),
            (Handle::South, 0.5, 1.0),
            (Handle::SouthWest, 0.0, 1.0),
            (Handle::West, 0.0, 0.5),
        ]
    }

    /// Half-width of the grip hit-test squares on screen (pixels).
    pub const GRIP_RADIUS: f32 = 12.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(rotation: f32) -> CropGeom {
        CropGeom {
            oriented: [6000.0, 4000.0],
            rotation_deg: rotation,
        }
    }

    #[test]
    fn full_rect_is_valid_without_rotation() {
        let g = geom(0.0);
        assert!(g.is_valid(CropRect {
            x: 0.0,
            y: 0.0,
            w: 6000.0,
            h: 4000.0
        }));
    }

    #[test]
    fn full_rect_needs_shrinking_when_rotated() {
        let g = geom(10.0);
        let full = CropRect {
            x: 0.0,
            y: 0.0,
            w: 6000.0,
            h: 4000.0,
        };
        assert!(!g.is_valid(full));
        let fitted = g.fit(full);
        assert!(g.is_valid(fitted));
        // Comparable to the analytic largest inscribed rect for 10 degrees.
        assert!(fitted.w > 4400.0, "w = {} h = {}", fitted.w, fitted.h);
        assert!(fitted.h > 2900.0, "h = {} w = {}", fitted.h, fitted.w);
    }

    #[test]
    fn rotation_magnitude_45_reduces_more_than_10() {
        let full = CropRect {
            x: 0.0,
            y: 0.0,
            w: 6000.0,
            h: 4000.0,
        };
        let shallow = geom(10.0).fit(full);
        let steep = geom(45.0).fit(full);
        // Corner constraint binds around 0.566 at 45 degrees.
        assert!(steep.w < 3500.0, "steep w = {}", steep.w);
        assert!(steep.h < 2400.0, "steep h = {}", steep.h);
        assert!(shallow.w > steep.w && shallow.h > steep.h);
    }

    #[test]
    fn centered_respects_aspect() {
        let g = geom(0.0);
        let crop = g.centered(Some(1.0));
        assert_eq!(crop.w, 4000.0);
        assert_eq!(crop.h, 4000.0);
        let crop = g.centered(Some(16.0 / 9.0));
        assert_eq!(crop.w, 6000.0);
        assert!((crop.h - 6000.0 * 9.0 / 16.0).abs() < 0.5);
    }

    #[test]
    fn aspect_adjust_keeps_center_and_ratio() {
        let g = geom(0.0);
        let crop = g.centered(Some(1.0));
        let adjusted = g.apply_aspect(crop, 2.0);
        assert!((adjusted.w / adjusted.h - 2.0).abs() < 0.02);
        assert!((adjusted.x + adjusted.w * 0.5 - 3000.0).abs() < 1.0);
        assert!(g.is_valid(adjusted));
    }

    #[test]
    fn clamped_to_minimum() {
        let g = geom(0.0);
        let tiny = g.clamp_axis_aligned(CropRect {
            x: 10.0,
            y: 10.0,
            w: 10.0,
            h: 10.0,
        });
        assert!(tiny.w >= MIN_CROP_PX.min(6000.0));
        assert!(tiny.h >= MIN_CROP_PX.min(4000.0));
    }

    #[test]
    fn drag_translation_stays_inside() {
        let g = geom(0.0);
        let crop = g.centered(Some(4.0 / 3.0));
        // Simulate moving far beyond the image edge.
        let moved = g.clamp_axis_aligned(CropRect {
            x: 9999.0,
            y: 9999.0,
            ..crop
        });
        assert!(g.is_valid(moved));
    }
}
