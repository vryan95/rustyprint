//! Rounded rectangles: boxes with `border-radius` (CSS Backgrounds 3 § 5).

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::CornerRadius;

use crate::display::{Path, PathSegment, Point, Rect};

/// How far along the tangent the control points of a quarter ellipse sit, as a fraction of
/// the radius.
const KAPPA: f32 = 0.552_284_8;

/// A rectangle with rounded or otherwise shaped corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoundedRect {
    pub rect: Rect,
    /// Horizontal and vertical radii of the top-left, top-right, bottom-right and bottom-left
    /// corners.
    pub radii: [(f32, f32); 4],
    /// The superellipse parameter of each corner's curve (CSS Borders 4 `corner-shape`): 1
    /// for an elliptical arc, 0 for a straight bevel, infinity for a square corner and
    /// negative values for concave ones.
    pub shapes: [f32; 4],
}

impl RoundedRect {
    /// A rectangle with elliptical corners.
    pub fn new(rect: Rect, radii: [(f32, f32); 4]) -> Self {
        Self {
            rect,
            radii,
            shapes: [1.0; 4],
        }
    }

    /// The border box `rect` with the corners `style` gives it: percentages resolve against
    /// the box's width and height, and radii that do not fit scale down together (§ 5.5).
    pub fn border_box(style: &ComputedValues, rect: Rect) -> Self {
        let resolve = |(h, v): &CornerRadius| {
            let (h, v) = (
                h.resolve(rect.width).max(0.0),
                v.resolve(rect.height).max(0.0),
            );
            // A corner with either radius zero is square.
            if h > 0.0 && v > 0.0 {
                (h, v)
            } else {
                (0.0, 0.0)
            }
        };
        let radii = [
            resolve(&style.border_top_left_radius),
            resolve(&style.border_top_right_radius),
            resolve(&style.border_bottom_right_radius),
            resolve(&style.border_bottom_left_radius),
        ];
        let shapes = [
            style.corner_top_left_shape.parameter(),
            style.corner_top_right_shape.parameter(),
            style.corner_bottom_right_shape.parameter(),
            style.corner_bottom_left_shape.parameter(),
        ];
        let mut rounded = Self {
            rect,
            radii,
            shapes,
        };
        rounded.fit();
        rounded
    }

    /// Scales the radii so that adjacent ones do not overlap.
    fn fit(&mut self) {
        let [tl, tr, br, bl] = self.radii;
        let ratio = |length: f32, sum: f32| {
            if sum > 0.0 {
                length / sum
            } else {
                f32::INFINITY
            }
        };
        let f = ratio(self.rect.width, tl.0 + tr.0)
            .min(ratio(self.rect.height, tr.1 + br.1))
            .min(ratio(self.rect.width, br.0 + bl.0))
            .min(ratio(self.rect.height, bl.1 + tl.1));
        if f < 1.0 {
            for radius in &mut self.radii {
                *radius = (radius.0 * f, radius.1 * f);
            }
        }
    }

    pub fn is_square(&self) -> bool {
        self.radii.iter().all(|&(h, v)| h <= 0.0 || v <= 0.0)
    }

    /// The rounded rectangle shrunk by the given amounts, with each corner's radii reduced by
    /// the widths of its two sides (§ 5.6).
    pub fn deflate(&self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        let shrink = |(h, v): (f32, f32), dh: f32, dv: f32| {
            let (h, v) = ((h - dh).max(0.0), (v - dv).max(0.0));
            if h > 0.0 && v > 0.0 {
                (h, v)
            } else {
                (0.0, 0.0)
            }
        };
        let [tl, tr, br, bl] = self.radii;
        Self {
            rect: self.rect.deflate(top, right, bottom, left),
            radii: [
                shrink(tl, left, top),
                shrink(tr, right, top),
                shrink(br, right, bottom),
                shrink(bl, left, bottom),
            ],
            shapes: self.shapes,
        }
    }

    /// The rounded rectangle grown by `amount` on every side; rounded corners grow with it,
    /// square ones stay square.
    pub fn inflate(&self, amount: f32) -> Self {
        let rect = self.rect.inflate(amount);
        let rect = Rect::new(rect.x, rect.y, rect.width.max(0.0), rect.height.max(0.0));
        let grow = |(h, v): (f32, f32)| {
            if h > 0.0 && v > 0.0 {
                let (h, v) = ((h + amount).max(0.0), (v + amount).max(0.0));
                if h > 0.0 && v > 0.0 {
                    (h, v)
                } else {
                    (0.0, 0.0)
                }
            } else {
                (0.0, 0.0)
            }
        };
        let mut rounded = Self {
            rect,
            radii: self.radii.map(grow),
            shapes: self.shapes,
        };
        rounded.fit();
        rounded
    }

    /// The outline as a closed path, clockwise from the top-left corner.
    pub fn path(&self) -> Path {
        let r = self.rect;
        let [tl, tr, br, bl] = self.radii;
        let (left, top, right, bottom) = (r.x, r.y, r.right(), r.bottom());
        let mut segments = vec![
            PathSegment::MoveTo(Point::new(left + tl.0, top)),
            PathSegment::LineTo(Point::new(right - tr.0, top)),
        ];
        // Each corner's start, the rectangle's corner, its end, and which corner it is.
        let corners = [
            (
                tr,
                Point::new(right - tr.0, top),
                Point::new(right, top),
                Point::new(right, top + tr.1),
                1,
            ),
            (
                br,
                Point::new(right, bottom - br.1),
                Point::new(right, bottom),
                Point::new(right - br.0, bottom),
                2,
            ),
            (
                bl,
                Point::new(left + bl.0, bottom),
                Point::new(left, bottom),
                Point::new(left, bottom - bl.1),
                3,
            ),
            (
                tl,
                Point::new(left, top + tl.1),
                Point::new(left, top),
                Point::new(left + tl.0, top),
                0,
            ),
        ];
        for (index, &(radius, start, corner, end, which)) in corners.iter().enumerate() {
            if radius.0 > 0.0 {
                curve(&mut segments, start, corner, end, self.shapes[which]);
            }
            // The side after the corner, unless it closes the path.
            if index < 3 {
                segments.push(PathSegment::LineTo(corners[index + 1].1));
            }
        }
        segments.push(PathSegment::Close);
        Path { segments }
    }
}

/// Adds a corner's curve from `start` to `end` around the rectangle's `corner`: a quarter of
/// the superellipse `|x|^(2^k) + |y|^(2^k) = 1` (CSS Borders 4 § 6).
fn curve(segments: &mut Vec<PathSegment>, start: Point, corner: Point, end: Point, k: f32) {
    if (k - 1.0).abs() < 1.0e-6 {
        segments.push(PathSegment::CubicTo(
            Point::new(
                start.x + (corner.x - start.x) * KAPPA,
                start.y + (corner.y - start.y) * KAPPA,
            ),
            Point::new(
                end.x + (corner.x - end.x) * KAPPA,
                end.y + (corner.y - end.y) * KAPPA,
            ),
            end,
        ));
        return;
    }
    // The center of the corner's ellipse.
    let center = Point::new(start.x + end.x - corner.x, start.y + end.y - corner.y);
    if k >= 16.0 {
        segments.push(PathSegment::LineTo(corner));
    } else if k <= -16.0 {
        segments.push(PathSegment::LineTo(center));
    } else {
        // Sampled at even angles; the exponent bends the curve toward or away from the
        // corner.
        let exponent = 2.0 / 2f32.powf(k);
        let to_start = (start.x - center.x, start.y - center.y);
        let to_end = (end.x - center.x, end.y - center.y);
        for step in 1..CORNER_STEPS {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / CORNER_STEPS as f32;
            let (u, v) = (angle.cos().powf(exponent), angle.sin().powf(exponent));
            segments.push(PathSegment::LineTo(Point::new(
                center.x + to_start.0 * u + to_end.0 * v,
                center.y + to_start.1 * u + to_end.1 * v,
            )));
        }
    }
    segments.push(PathSegment::LineTo(end));
}

/// How many straight pieces a corner that is not elliptical is drawn with.
const CORNER_STEPS: usize = 24;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_radii_scale_down() {
        let mut rounded = RoundedRect::new(Rect::new(0.0, 0.0, 100.0, 50.0), [(100.0, 100.0); 4]);
        rounded.fit();
        // The height fits two radii of 25.
        assert_eq!(rounded.radii[0], (25.0, 25.0));
    }

    #[test]
    fn deflating_shrinks_radii_by_side_widths() {
        let rounded = RoundedRect::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            [(20.0, 20.0), (5.0, 5.0), (0.0, 0.0), (20.0, 20.0)],
        );
        let inner = rounded.deflate(10.0, 10.0, 10.0, 4.0);
        assert_eq!(inner.radii[0], (16.0, 10.0));
        assert_eq!(inner.radii[1], (0.0, 0.0));
        assert_eq!(inner.rect, Rect::new(4.0, 10.0, 86.0, 80.0));
    }

    #[test]
    fn corner_shapes_bend_the_corners() {
        let mut rounded = RoundedRect::new(Rect::new(0.0, 0.0, 100.0, 100.0), [(20.0, 20.0); 4]);
        rounded.shapes = [0.0, f32::INFINITY, f32::NEG_INFINITY, 1.0];
        let points: Vec<Point> = rounded.path().points().collect();
        // A square corner keeps its point and a notch goes in to the corner's center; the
        // bevel at the top-left cuts across, leaving out the corner.
        assert!(points.contains(&Point::new(100.0, 0.0)));
        assert!(points.contains(&Point::new(80.0, 80.0)));
        assert!(!points.contains(&Point::new(0.0, 0.0)));
    }

    #[test]
    fn square_paths_have_no_curves() {
        let path = RoundedRect::new(Rect::new(0.0, 0.0, 10.0, 10.0), [(0.0, 0.0); 4]).path();
        assert!(
            !path
                .segments
                .iter()
                .any(|s| matches!(s, PathSegment::CubicTo(..)))
        );
    }
}
