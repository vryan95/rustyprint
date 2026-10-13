//! Transforms (CSS Transforms 1 and 2) and motion paths (CSS Motion Path 1): the matrix a
//! box is drawn with.
//!
//! Transforms compose as 4×4 matrices and are flattened to the 2D matrices PDF draws with.
//! A perspective projection is not affine, so it is approximated by the affine map that best
//! fits the projected corners of the box.

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    BackfaceVisibility, LengthPercentage, OffsetPath, OffsetPathKind, OffsetPosition,
    PositionComponent, RaySize, RotateAxis, TransformBox, TransformFunction, TransformStyle,
};

use crate::display::{Matrix, Point, Rect};
use crate::fragment::BoxFragment;

/// A 4×4 matrix, by rows, applied to column vectors `(x, y, z, 1)`.
pub(crate) type Mat4 = [[f32; 4]; 4];

pub(crate) const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// `a` after `b`: the matrix that applies `b` first.
pub(crate) fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, value) in row.iter_mut().enumerate() {
            *value = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}

fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    [
        [1.0, 0.0, 0.0, x],
        [0.0, 1.0, 0.0, y],
        [0.0, 0.0, 1.0, z],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn scale(x: f32, y: f32, z: f32) -> Mat4 {
    [
        [x, 0.0, 0.0, 0.0],
        [0.0, y, 0.0, 0.0],
        [0.0, 0.0, z, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// A rotation by `degrees` about the axis `(x, y, z)`, clockwise on the page for the z axis.
fn rotate(x: f32, y: f32, z: f32, degrees: f32) -> Mat4 {
    let length = (x * x + y * y + z * z).sqrt();
    if length == 0.0 || !length.is_finite() {
        return IDENTITY;
    }
    let (x, y, z) = (x / length, y / length, z / length);
    let half = degrees.to_radians() / 2.0;
    let sc = half.sin() * half.cos();
    let sq = half.sin() * half.sin();
    [
        [
            1.0 - 2.0 * (y * y + z * z) * sq,
            2.0 * (x * y * sq - z * sc),
            2.0 * (x * z * sq + y * sc),
            0.0,
        ],
        [
            2.0 * (x * y * sq + z * sc),
            1.0 - 2.0 * (x * x + z * z) * sq,
            2.0 * (y * z * sq - x * sc),
            0.0,
        ],
        [
            2.0 * (x * z * sq - y * sc),
            2.0 * (y * z * sq + x * sc),
            1.0 - 2.0 * (x * x + y * y) * sq,
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn skew(x_degrees: f32, y_degrees: f32) -> Mat4 {
    [
        [1.0, x_degrees.to_radians().tan(), 0.0, 0.0],
        [y_degrees.to_radians().tan(), 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// A perspective projection with the viewer `distance` pixels in front of the z = 0 plane.
/// Distances below one pixel act as one pixel.
fn perspective(distance: f32) -> Mat4 {
    let mut m = IDENTITY;
    m[3][2] = -1.0 / distance.max(1.0);
    m
}

fn from_2d(m: [f32; 6]) -> Mat4 {
    let [a, b, c, d, e, f] = m;
    [
        [a, c, 0.0, e],
        [b, d, 0.0, f],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// The matrix of one transform function, with percentages of the reference box.
fn function(f: &TransformFunction<LengthPercentage>, reference: Rect) -> Mat4 {
    use TransformFunction as T;
    let x = |l: &LengthPercentage| l.resolve(reference.width);
    let y = |l: &LengthPercentage| l.resolve(reference.height);
    let z = |l: &LengthPercentage| l.resolve(0.0);
    match f {
        T::Matrix(m) => from_2d(*m),
        // Written column by column.
        T::Matrix3d(m) => {
            let mut out = [[0.0; 4]; 4];
            for (c, column) in m.as_chunks::<4>().0.iter().enumerate() {
                for (r, value) in column.iter().enumerate() {
                    out[r][c] = *value;
                }
            }
            out
        }
        T::Translate(tx, ty) => translate(x(tx), ty.as_ref().map_or(0.0, y), 0.0),
        T::TranslateX(tx) => translate(x(tx), 0.0, 0.0),
        T::TranslateY(ty) => translate(0.0, y(ty), 0.0),
        T::TranslateZ(tz) => translate(0.0, 0.0, z(tz)),
        T::Translate3d(tx, ty, tz) => translate(x(tx), y(ty), z(tz)),
        T::Scale(sx, sy) => scale(*sx, sy.unwrap_or(*sx), 1.0),
        T::ScaleX(s) => scale(*s, 1.0, 1.0),
        T::ScaleY(s) => scale(1.0, *s, 1.0),
        T::ScaleZ(s) => scale(1.0, 1.0, *s),
        T::Scale3d(sx, sy, sz) => scale(*sx, *sy, *sz),
        T::Rotate(a) | T::RotateZ(a) => rotate(0.0, 0.0, 1.0, *a),
        T::RotateX(a) => rotate(1.0, 0.0, 0.0, *a),
        T::RotateY(a) => rotate(0.0, 1.0, 0.0, *a),
        T::Rotate3d(ax, ay, az, a) => rotate(*ax, *ay, *az, *a),
        T::Skew(ax, ay) => skew(*ax, ay.unwrap_or(0.0)),
        T::SkewX(a) => skew(*a, 0.0),
        T::SkewY(a) => skew(0.0, *a),
        T::Perspective(None) => IDENTITY,
        T::Perspective(Some(d)) => perspective(z(d)),
    }
}

/// Whether a style transforms its box: with `transform`, an individual transform property
/// or a motion path.
pub(crate) fn has_transform(style: &ComputedValues) -> bool {
    !style.transform.is_empty()
        || style.translate.is_some()
        || style.rotate.is_some()
        || style.scale.is_some()
        || style.offset_path != OffsetPath::None
}

/// The reference box of `transform` and `transform-origin` for a box whose border box is
/// `rect`: CSS boxes have no fill or stroke, so `fill-box` is the content box and
/// `stroke-box` and `view-box` the border box.
pub(crate) fn reference_box(fragment: &BoxFragment, rect: Rect) -> Rect {
    match fragment.style.transform_box {
        TransformBox::ContentBox | TransformBox::FillBox => {
            let (b, p) = (fragment.border, fragment.padding);
            rect.deflate(
                b.top + p.top,
                b.right + p.right,
                b.bottom + p.bottom,
                b.left + p.left,
            )
        }
        TransformBox::BorderBox | TransformBox::StrokeBox | TransformBox::ViewBox => rect,
    }
}

/// A box's own transformation in page coordinates (§ 7.1 of CSS Transforms 2): about its
/// origin, `translate`, `rotate`, `scale`, the motion path offset and `transform`. `rect` is
/// its border box and `cb` its containing block, which motion paths are laid in.
pub(crate) fn box_matrix(fragment: &BoxFragment, rect: Rect, cb: Rect) -> Mat4 {
    let style = &fragment.style;
    let reference = reference_box(fragment, rect);
    let (ox, oy, oz) = &style.transform_origin;
    let origin = (
        reference.x + ox.resolve(reference.width),
        reference.y + oy.resolve(reference.height),
        *oz,
    );
    let mut m = translate(origin.0, origin.1, origin.2);
    if let Some(values) = &style.translate {
        let tx = values[0].resolve(reference.width);
        let ty = values.get(1).map_or(0.0, |v| v.resolve(reference.height));
        let tz = values.get(2).map_or(0.0, |v| v.resolve(0.0));
        m = mul(&m, &translate(tx, ty, tz));
    }
    if let Some((axis, angle)) = &style.rotate {
        let (x, y, z) = match axis {
            None | Some(RotateAxis::Z) => (0.0, 0.0, 1.0),
            Some(RotateAxis::X) => (1.0, 0.0, 0.0),
            Some(RotateAxis::Y) => (0.0, 1.0, 0.0),
            Some(RotateAxis::Vector(x, y, z)) => (*x, *y, *z),
        };
        m = mul(&m, &rotate(x, y, z, *angle));
    }
    if let Some(values) = &style.scale {
        let sx = values[0];
        let sy = values.get(1).copied().unwrap_or(sx);
        let sz = values.get(2).copied().unwrap_or(1.0);
        m = mul(&m, &scale(sx, sy, sz));
    }
    if let Some(offset) = offset_matrix(style, rect, cb, origin) {
        // The offset is in page coordinates; undo the origin around it.
        m = mul(
            &m,
            &mul(
                &translate(-origin.0, -origin.1, -origin.2),
                &mul(&offset, &translate(origin.0, origin.1, origin.2)),
            ),
        );
    }
    for f in style.transform.iter() {
        m = mul(&m, &function(f, reference));
    }
    mul(&m, &translate(-origin.0, -origin.1, -origin.2))
}

/// The perspective a box gives its children: `perspective` about `perspective-origin`, with
/// the box's border box at `rect`.
pub(crate) fn child_perspective(style: &ComputedValues, rect: Rect) -> Option<Mat4> {
    let distance = style.perspective?;
    let (x, y) = &style.perspective_origin;
    let px = rect.x + x.resolve(rect.width);
    let py = rect.y + y.resolve(rect.height);
    Some(mul(
        &translate(px, py, 0.0),
        &mul(&perspective(distance), &translate(-px, -py, 0.0)),
    ))
}

/// Whether a box's children share its 3D space (`transform-style: preserve-3d`).
pub(crate) fn preserves_3d(style: &ComputedValues) -> bool {
    style.transform_style == TransformStyle::Preserve3d
}

/// Where `m` takes a point of the z = 0 plane, or `None` behind the viewer.
fn project(m: &Mat4, p: Point) -> Option<Point> {
    let x = m[0][0] * p.x + m[0][1] * p.y + m[0][3];
    let y = m[1][0] * p.x + m[1][1] * p.y + m[1][3];
    let w = m[3][0] * p.x + m[3][1] * p.y + m[3][3];
    (w > 1.0e-6).then(|| Point::new(x / w, y / w))
}

/// The 2D matrix that draws a box with border box `rect` transformed by `m`, or `None` if
/// the box is behind the viewer.
pub(crate) fn flatten(m: &Mat4, rect: Rect) -> Option<Matrix> {
    let affine = m[3][0].abs() < 1.0e-9 && m[3][1].abs() < 1.0e-9;
    if affine {
        let w = m[3][3];
        if w <= 1.0e-6 {
            return None;
        }
        return Some([
            m[0][0] / w,
            m[1][0] / w,
            m[0][1] / w,
            m[1][1] / w,
            m[0][3] / w,
            m[1][3] / w,
        ]);
    }
    // The affine map closest, by least squares, to the projection of the box's corners.
    let corners = [
        Point::new(rect.x, rect.y),
        Point::new(rect.right(), rect.y),
        Point::new(rect.right(), rect.bottom()),
        Point::new(rect.x, rect.bottom()),
    ];
    let projected: Vec<Point> = corners
        .iter()
        .map(|&p| project(m, p))
        .collect::<Option<_>>()?;
    let (row_x, row_y) = fit(&corners, &projected)?;
    Some([row_x[0], row_y[0], row_x[1], row_y[1], row_x[2], row_y[2]])
}

/// Least-squares coefficients `(a, c, e)` and `(b, d, f)` with `u ≈ a x + c y + e` and
/// `v ≈ b x + d y + f`.
fn fit(from: &[Point], to: &[Point]) -> Option<([f32; 3], [f32; 3])> {
    let mut n = [[0.0f64; 3]; 3];
    let mut bu = [0.0f64; 3];
    let mut bv = [0.0f64; 3];
    for (p, q) in from.iter().zip(to) {
        let row = [f64::from(p.x), f64::from(p.y), 1.0];
        for i in 0..3 {
            for j in 0..3 {
                n[i][j] += row[i] * row[j];
            }
            bu[i] += row[i] * f64::from(q.x);
            bv[i] += row[i] * f64::from(q.y);
        }
    }
    let solve = |b: [f64; 3]| -> Option<[f32; 3]> {
        let det = |m: [[f64; 3]; 3]| {
            m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
        };
        let d = det(n);
        if d.abs() < 1.0e-12 {
            return None;
        }
        let mut out = [0.0f32; 3];
        for (k, value) in out.iter_mut().enumerate() {
            let mut m = n;
            for (row, b) in m.iter_mut().zip(b) {
                row[k] = b;
            }
            *value = (det(m) / d) as f32;
        }
        Some(out)
    };
    Some((solve(bu)?, solve(bv)?))
}

/// Whether a box drawn with `m` shows its back, which `backface-visibility: hidden` hides:
/// when its normal, transformed, points away from the viewer.
pub(crate) fn hides_backface(style: &ComputedValues, m: &Mat4) -> bool {
    if style.backface_visibility != BackfaceVisibility::Hidden {
        return false;
    }
    // The z component of the transformed normal is the cofactor of m33 over the
    // determinant of the upper-left 3×3 matrix.
    let a = |r: usize, c: usize| m[r][c];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1))
        - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    let cofactor = a(0, 0) * a(1, 1) - a(0, 1) * a(1, 0);
    det != 0.0 && cofactor / det < 0.0
}

// ---------------------------------------------------------------------------------------------
// Motion paths

fn position_in(position: &(PositionComponent, PositionComponent), r: Rect) -> Point {
    let at = |c: &PositionComponent, start: f32, length: f32| {
        let offset = c.offset.resolve(length);
        start + if c.from_end { length - offset } else { offset }
    };
    Point::new(
        at(&position.0, r.x, r.width),
        at(&position.1, r.y, r.height),
    )
}

/// The offset transformation of a box on a motion path (CSS Motion Path 1 § 4), in page
/// coordinates: its anchor moved to the point `offset-distance` along the path and turned by
/// `offset-rotate`. `origin` is the transform origin, which `offset-anchor: auto` uses.
fn offset_matrix(
    style: &ComputedValues,
    rect: Rect,
    cb: Rect,
    origin: (f32, f32, f32),
) -> Option<Mat4> {
    let OffsetPath::Path { path, .. } = &style.offset_path else {
        return None;
    };
    let anchor = match &style.offset_anchor {
        Some(position) => position_in(position, rect),
        None => Point::new(origin.0, origin.1),
    };
    let (point, direction) = match path {
        Some(OffsetPathKind::Ray(ray)) => {
            let start = match (&ray.at, &style.offset_position) {
                (Some(at), _) => position_in(at, cb),
                (None, OffsetPosition::Position(x, y)) => position_in(&(x.clone(), y.clone()), cb),
                (None, OffsetPosition::Auto) => anchor,
                (None, OffsetPosition::Normal) => {
                    Point::new(cb.x + cb.width / 2.0, cb.y + cb.height / 2.0)
                }
            };
            let sides = [
                start.x - cb.x,
                cb.right() - start.x,
                start.y - cb.y,
                cb.bottom() - start.y,
            ]
            .map(f32::abs);
            let (dx, dy) = (sides[0].min(sides[1]), sides[2].min(sides[3]));
            let (fx, fy) = (sides[0].max(sides[1]), sides[2].max(sides[3]));
            let (sin, cos) = ray.angle.to_radians().sin_cos();
            let length = match ray.size {
                RaySize::ClosestSide => dx.min(dy),
                RaySize::FarthestSide => fx.max(fy),
                RaySize::ClosestCorner => dx.hypot(dy),
                RaySize::FarthestCorner => fx.hypot(fy),
                // Where the ray leaves the containing block.
                RaySize::Sides => {
                    let tx = if sin > 0.0 {
                        sides[1] / sin
                    } else if sin < 0.0 {
                        sides[0] / -sin
                    } else {
                        f32::INFINITY
                    };
                    let ty = if cos > 0.0 {
                        sides[2] / cos
                    } else if cos < 0.0 {
                        sides[3] / -cos
                    } else {
                        f32::INFINITY
                    };
                    tx.min(ty)
                }
            };
            let distance = style.offset_distance.resolve(length);
            (
                Point::new(start.x + distance * sin, start.y - distance * cos),
                ray.angle - 90.0,
            )
        }
        Some(OffsetPathKind::Url(_)) => return None,
        Some(OffsetPathKind::Shape(shape)) => {
            let polygons = crate::shapes::shape_polygons(shape, cb);
            along(polygons.first()?, closed(shape), &style.offset_distance)?
        }
        // Only a box: its border box's outline.
        None => {
            let outline = [
                Point::new(cb.x, cb.y),
                Point::new(cb.right(), cb.y),
                Point::new(cb.right(), cb.bottom()),
                Point::new(cb.x, cb.bottom()),
            ];
            along(&outline, true, &style.offset_distance)?
        }
    };
    let rotation = match style.offset_rotate {
        r if r.reverse => direction + 180.0 + r.angle,
        r if r.auto => direction + r.angle,
        r => r.angle,
    };
    Some(mul(
        &translate(point.x, point.y, 0.0),
        &mul(
            &rotate(0.0, 0.0, 1.0, rotation),
            &translate(-anchor.x, -anchor.y, 0.0),
        ),
    ))
}

/// Whether a shape's path is closed: all are but `path()` and `shape()`, which are closed
/// only when they end where they start.
fn closed(shape: &rustyprint_css::properties::types::BasicShape<LengthPercentage>) -> bool {
    use rustyprint_css::properties::types::BasicShape;
    !matches!(shape, BasicShape::Path { .. } | BasicShape::Shape { .. })
}

/// The point `distance` along a polyline and its direction in degrees clockwise from the
/// x axis. Distances wrap around closed paths and stop at the ends of open ones.
fn along(points: &[Point], closed: bool, distance: &LengthPercentage) -> Option<(Point, f32)> {
    let mut points = points.to_vec();
    if points.len() < 2 {
        return None;
    }
    let ends_at_start = points.first() == points.last();
    if closed && !ends_at_start {
        points.push(points[0]);
    }
    let closed = closed || ends_at_start;
    let lengths: Vec<f32> = points
        .windows(2)
        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
        .collect();
    let total: f32 = lengths.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut d = distance.resolve(total);
    d = if closed {
        d.rem_euclid(total)
    } else {
        d.clamp(0.0, total)
    };
    for (i, length) in lengths.iter().enumerate() {
        if d <= *length || i == lengths.len() - 1 {
            let (a, b) = (points[i], points[i + 1]);
            let t = if *length > 0.0 {
                (d / length).min(1.0)
            } else {
                0.0
            };
            let point = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            return Some((point, (b.y - a.y).atan2(b.x - a.x).to_degrees()));
        }
        d -= length;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 6], b: [f32; 6]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1.0e-4)
    }

    #[test]
    fn rotations_turn_clockwise_on_the_page() {
        let m = rotate(0.0, 0.0, 1.0, 90.0);
        let flat = flatten(&m, Rect::new(0.0, 0.0, 10.0, 10.0)).unwrap();
        assert!(close(flat, [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]), "{flat:?}");
    }

    #[test]
    fn rotating_about_y_by_half_a_turn_shows_the_back() {
        let mut style = ComputedValues::initial();
        style.backface_visibility = BackfaceVisibility::Hidden;
        assert!(hides_backface(&style, &rotate(0.0, 1.0, 0.0, 180.0)));
        assert!(!hides_backface(&style, &rotate(0.0, 1.0, 0.0, 30.0)));
        // A 2D mirror image is not a back face.
        assert!(!hides_backface(&style, &scale(-1.0, 1.0, 1.0)));
    }

    #[test]
    fn perspective_shrinks_what_is_farther_away() {
        let m = mul(&perspective(100.0), &translate(0.0, 0.0, -100.0));
        let flat = flatten(&m, Rect::new(0.0, 0.0, 10.0, 10.0)).unwrap();
        assert!((flat[0] - 0.5).abs() < 1.0e-4, "{flat:?}");
    }

    #[test]
    fn points_along_a_closed_path_wrap_around() {
        let square = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let (p, angle) = along(&square, true, &LengthPercentage::Length(15.0)).unwrap();
        assert_eq!(p, Point::new(10.0, 5.0));
        assert_eq!(angle, 90.0);
        let (p, _) = along(&square, true, &LengthPercentage::Length(45.0)).unwrap();
        assert_eq!(p, Point::new(5.0, 0.0));
    }
}
