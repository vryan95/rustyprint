//! Float shapes (CSS Shapes 1 § 4): the area lines wrap around, from `shape-outside`.
//!
//! A shape becomes horizontal spans, one per pixel row, of the part of each row it covers.
//! Lines beside the float then wrap around the spans in their band, widened by
//! `shape-margin`.

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    BasicShape, InsetFunction, LengthPercentage, PositionComponent, ShapeBox, ShapeCommand,
    ShapeOutside, ShapeRadius,
};

use crate::boxes::BlockBox;
use crate::display::{DisplayItem, Point, Rect, ShadingKind};
use crate::fragment::{BoxFragment, Sides};
use crate::rounded::RoundedRect;

/// How finely curves are flattened into lines.
const CURVE_SEGMENTS: usize = 16;

/// The most rows an image shape is sampled in, and the most samples across each.
const MAX_IMAGE_SAMPLES: f32 = 512.0;

/// The part of one row that a shape covers.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Span {
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
}

/// A float's shape, in the coordinates of its block formatting context.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shape {
    /// Rows the shape covers, top to bottom.
    spans: Vec<Span>,
    /// `shape-margin`, which widens the shape all around.
    margin: f32,
}

impl Shape {
    /// The left and right edges of the shape in the band from `top` down `height`, or
    /// `None` if it does not reach into the band.
    pub fn extent(&self, top: f32, height: f32) -> Option<(f32, f32)> {
        let bottom = top + height.max(0.01);
        let m = self.margin;
        let mut extent: Option<(f32, f32)> = None;
        for span in &self.spans {
            if span.top >= bottom + m || span.bottom <= top - m {
                continue;
            }
            // Rows near the band reach into it through the margin, less the farther away.
            let gap = if span.bottom <= top {
                top - span.bottom
            } else if span.top >= bottom {
                span.top - bottom
            } else {
                0.0
            };
            let reach = (m * m - gap * gap).max(0.0).sqrt();
            let (left, right) = (span.left - reach, span.right + reach);
            extent = Some(match extent {
                Some((l, r)) => (l.min(left), r.max(right)),
                None => (left, right),
            });
        }
        extent
    }
}

/// A float's shape from its `shape-outside`, given where its border box is in its
/// formatting context and its margins. `None` for `shape-outside: none`, which leaves the
/// margin box as the float's area.
pub(crate) fn float_shape(
    float: &BlockBox,
    fragment: &BoxFragment,
    border_box: Rect,
    margin: Sides,
) -> Option<Shape> {
    let style = &fragment.style;
    let reference = |which: ShapeBox| -> RoundedRect {
        let border = RoundedRect::border_box(style, border_box);
        let (b, p) = (fragment.border, fragment.padding);
        match which {
            ShapeBox::MarginBox => {
                let mut outer = border;
                outer.rect = Rect::new(
                    border_box.x - margin.left,
                    border_box.y - margin.top,
                    border_box.width + margin.horizontal(),
                    border_box.height + margin.vertical(),
                );
                // Corners grow with the margin (CSS Shapes 1 § 4.1).
                for (i, radius) in outer.radii.iter_mut().enumerate() {
                    let (h, v) = match i {
                        0 => (margin.left, margin.top),
                        1 => (margin.right, margin.top),
                        2 => (margin.right, margin.bottom),
                        _ => (margin.left, margin.bottom),
                    };
                    if radius.0 > 0.0 || radius.1 > 0.0 {
                        *radius = (radius.0 + h, radius.1 + v);
                    }
                }
                outer
            }
            ShapeBox::BorderBox => border,
            ShapeBox::PaddingBox => border.deflate(b.top, b.right, b.bottom, b.left),
            ShapeBox::ContentBox => border
                .deflate(b.top, b.right, b.bottom, b.left)
                .deflate(p.top, p.right, p.bottom, p.left),
        }
    };
    let shape_margin = style.shape_margin.resolve(border_box.width);
    let spans = match &style.shape_outside {
        ShapeOutside::None => return None,
        ShapeOutside::Shape {
            shape,
            reference: which,
        } => {
            let reference = reference(which.unwrap_or(ShapeBox::MarginBox));
            match shape {
                None => polygon_spans(&[rounded_polygon(&reference)]),
                Some(shape) => polygon_spans(&basic_shape(shape, reference.rect)),
            }
        }
        ShapeOutside::Image(_) => {
            let picture = float.shape_image.as_ref()?;
            let content = reference(ShapeBox::ContentBox).rect;
            image_spans(picture, style, content)
        }
    };
    Some(Shape {
        spans,
        margin: shape_margin,
    })
}

/// A rounded rectangle as a polygon.
fn rounded_polygon(shape: &RoundedRect) -> Vec<Point> {
    let r = shape.rect;
    let corners = [
        (Point::new(r.x, r.y), (1.0, 1.0), 0),
        (Point::new(r.right(), r.y), (-1.0, 1.0), 1),
        (Point::new(r.right(), r.bottom()), (-1.0, -1.0), 2),
        (Point::new(r.x, r.bottom()), (1.0, -1.0), 3),
    ];
    let mut points = Vec::new();
    for (corner, (sx, sy), index) in corners {
        let (rx, ry) = shape.radii[index];
        if rx <= 0.0 || ry <= 0.0 {
            points.push(corner);
            continue;
        }
        let center = Point::new(corner.x + sx * rx, corner.y + sy * ry);
        // Each corner's arc, clockwise from the side before it to the side after it.
        let start = match index {
            0 => std::f32::consts::PI,
            1 => -std::f32::consts::FRAC_PI_2,
            2 => 0.0,
            _ => std::f32::consts::FRAC_PI_2,
        };
        for step in 0..=CURVE_SEGMENTS {
            let angle = start + std::f32::consts::FRAC_PI_2 * step as f32 / CURVE_SEGMENTS as f32;
            points.push(Point::new(
                center.x + rx * angle.cos(),
                center.y + ry * angle.sin(),
            ));
        }
    }
    points
}

/// Where a position falls along `length` from `start`.
fn position(component: &PositionComponent, start: f32, length: f32) -> f32 {
    let offset = component.offset.resolve(length);
    start
        + if component.from_end {
            length - offset
        } else {
            offset
        }
}

/// The subpaths of a basic shape in reference box `r`, as polygons.
pub(crate) fn shape_polygons(shape: &BasicShape<LengthPercentage>, r: Rect) -> Vec<Vec<Point>> {
    basic_shape(shape, r)
}

fn basic_shape(shape: &BasicShape<LengthPercentage>, r: Rect) -> Vec<Vec<Point>> {
    match shape {
        BasicShape::Inset {
            insets,
            radii,
            function,
        } => {
            let [a, b, c, d] = insets;
            let rect = match function {
                InsetFunction::Inset => {
                    let (top, right) = (a.resolve(r.height), b.resolve(r.width));
                    let (bottom, left) = (c.resolve(r.height), d.resolve(r.width));
                    Rect::new(
                        r.x + left,
                        r.y + top,
                        r.width - left - right,
                        r.height - top - bottom,
                    )
                }
                // The right and bottom edges are measured from the left and top.
                InsetFunction::Rect(_) => {
                    let (top, right) = (a.resolve(r.height), b.resolve(r.width));
                    let (bottom, left) = (c.resolve(r.height), d.resolve(r.width));
                    Rect::new(
                        r.x + left,
                        r.y + top,
                        right.max(left) - left,
                        bottom.max(top) - top,
                    )
                }
                InsetFunction::Xywh => Rect::new(
                    r.x + d.resolve(r.width),
                    r.y + a.resolve(r.height),
                    b.resolve(r.width),
                    c.resolve(r.height),
                ),
            };
            let rect = Rect::new(rect.x, rect.y, rect.width.max(0.0), rect.height.max(0.0));
            let mut radii_px = [(0.0, 0.0); 4];
            if let Some(radii) = radii {
                for (out, (h, v)) in radii_px.iter_mut().zip(radii) {
                    *out = (h.resolve(rect.width), v.resolve(rect.height));
                }
                // Radii that overlap shrink together, as for borders.
                let sum = |a: f32, b: f32, length: f32| {
                    if a + b > length && a + b > 0.0 {
                        length / (a + b)
                    } else {
                        1.0
                    }
                };
                let f = sum(radii_px[0].0, radii_px[1].0, rect.width)
                    .min(sum(radii_px[3].0, radii_px[2].0, rect.width))
                    .min(sum(radii_px[0].1, radii_px[3].1, rect.height))
                    .min(sum(radii_px[1].1, radii_px[2].1, rect.height));
                for radius in &mut radii_px {
                    *radius = (radius.0 * f, radius.1 * f);
                }
            }
            vec![rounded_polygon(&RoundedRect::new(rect, radii_px))]
        }
        BasicShape::Circle {
            radius,
            position: at,
        } => {
            let (cx, cy) = center(at, r);
            let sides = [cx - r.x, r.right() - cx, cy - r.y, r.bottom() - cy].map(f32::abs);
            let radius = match radius {
                ShapeRadius::Length(length) => {
                    length.resolve((r.width.powi(2) + r.height.powi(2)).sqrt() / 2f32.sqrt())
                }
                ShapeRadius::ClosestSide => sides.into_iter().fold(f32::INFINITY, f32::min),
                ShapeRadius::FarthestSide => sides.into_iter().fold(0.0, f32::max),
            };
            vec![ellipse(cx, cy, radius, radius)]
        }
        BasicShape::Ellipse {
            radii,
            position: at,
        } => {
            let (cx, cy) = center(at, r);
            let side = |radius: &ShapeRadius<LengthPercentage>, a: f32, b: f32, length: f32| {
                match radius {
                    ShapeRadius::Length(value) => value.resolve(length),
                    ShapeRadius::ClosestSide => a.abs().min(b.abs()),
                    ShapeRadius::FarthestSide => a.abs().max(b.abs()),
                }
            };
            let rx = side(&radii.0, cx - r.x, r.right() - cx, r.width);
            let ry = side(&radii.1, cy - r.y, r.bottom() - cy, r.height);
            vec![ellipse(cx, cy, rx, ry)]
        }
        BasicShape::Polygon { points, .. } => vec![
            points
                .iter()
                .map(|(x, y)| Point::new(r.x + x.resolve(r.width), r.y + y.resolve(r.height)))
                .collect(),
        ],
        BasicShape::Path { data, .. } => svg_path(data)
            .into_iter()
            .map(|path| {
                path.into_iter()
                    .map(|p| Point::new(r.x + p.x, r.y + p.y))
                    .collect()
            })
            .collect(),
        BasicShape::Shape { from, commands, .. } => shape_path(from, commands, r),
    }
}

fn center(at: &Option<(PositionComponent, PositionComponent)>, r: Rect) -> (f32, f32) {
    match at {
        Some((x, y)) => (position(x, r.x, r.width), position(y, r.y, r.height)),
        None => (r.x + r.width / 2.0, r.y + r.height / 2.0),
    }
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Point> {
    let steps = 4 * CURVE_SEGMENTS;
    (0..steps)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / steps as f32;
            Point::new(cx + rx * angle.cos(), cy + ry * angle.sin())
        })
        .collect()
}

/// Builds polygons from path segments: moves start new subpaths, the rest add points.
#[derive(Default)]
struct PathBuilder {
    paths: Vec<Vec<Point>>,
    current: Point,
    start: Point,
    /// The last control point of the previous curve, for smooth curves.
    control: Option<Point>,
}

impl PathBuilder {
    fn move_to(&mut self, p: Point) {
        self.paths.push(vec![p]);
        self.current = p;
        self.start = p;
        self.control = None;
    }

    fn line_to(&mut self, p: Point) {
        if self.paths.is_empty() {
            self.paths.push(vec![self.current]);
        }
        self.paths.last_mut().expect("a subpath is open").push(p);
        self.current = p;
        self.control = None;
    }

    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        let p0 = self.current;
        for i in 1..=CURVE_SEGMENTS {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let u = 1.0 - t;
            let x = u * u * u * p0.x
                + 3.0 * u * u * t * c1.x
                + 3.0 * u * t * t * c2.x
                + t * t * t * p.x;
            let y = u * u * u * p0.y
                + 3.0 * u * u * t * c1.y
                + 3.0 * u * t * t * c2.y
                + t * t * t * p.y;
            self.line_to(Point::new(x, y));
        }
        self.control = Some(c2);
    }

    fn quadratic_to(&mut self, c: Point, p: Point) {
        let p0 = self.current;
        for i in 1..=CURVE_SEGMENTS {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let u = 1.0 - t;
            let x = u * u * p0.x + 2.0 * u * t * c.x + t * t * p.x;
            let y = u * u * p0.y + 2.0 * u * t * c.y + t * t * p.y;
            self.line_to(Point::new(x, y));
        }
        self.control = Some(c);
    }

    /// The reflection of the previous curve's control point, for smooth curves.
    fn reflected(&self) -> Point {
        match self.control {
            Some(c) => Point::new(2.0 * self.current.x - c.x, 2.0 * self.current.y - c.y),
            None => self.current,
        }
    }

    /// An elliptical arc, from SVG's endpoint form (SVG 2 § B.2.4).
    fn arc_to(&mut self, rx: f32, ry: f32, rotate: f32, large: bool, sweep: bool, p: Point) {
        let p0 = self.current;
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx == 0.0 || ry == 0.0 || (p0.x == p.x && p0.y == p.y) {
            self.line_to(p);
            return;
        }
        let (sin, cos) = rotate.to_radians().sin_cos();
        let (dx, dy) = ((p0.x - p.x) / 2.0, (p0.y - p.y) / 2.0);
        let x1 = cos * dx + sin * dy;
        let y1 = -sin * dx + cos * dy;
        let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
        if lambda > 1.0 {
            rx *= lambda.sqrt();
            ry *= lambda.sqrt();
        }
        let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
        let mut factor = (num / den).max(0.0).sqrt();
        if large == sweep {
            factor = -factor;
        }
        let cx1 = factor * rx * y1 / ry;
        let cy1 = -factor * ry * x1 / rx;
        let cx = cos * cx1 - sin * cy1 + (p0.x + p.x) / 2.0;
        let cy = sin * cx1 + cos * cy1 + (p0.y + p.y) / 2.0;
        let angle = |ux: f32, uy: f32| uy.atan2(ux);
        let start = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
        let mut delta = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry) - start;
        if sweep && delta < 0.0 {
            delta += std::f32::consts::TAU;
        } else if !sweep && delta > 0.0 {
            delta -= std::f32::consts::TAU;
        }
        let steps = (CURVE_SEGMENTS as f32 * delta.abs() / std::f32::consts::FRAC_PI_2)
            .ceil()
            .max(1.0) as usize;
        for i in 1..=steps {
            let a = start + delta * i as f32 / steps as f32;
            let (x, y) = (rx * a.cos(), ry * a.sin());
            self.line_to(Point::new(cos * x - sin * y + cx, sin * x + cos * y + cy));
        }
    }

    fn close(&mut self) {
        self.current = self.start;
        self.control = None;
    }
}

/// The subpaths of SVG path data, in its units. Invalid data ends the path where it stops
/// making sense, as SVG does.
fn svg_path(data: &str) -> Vec<Vec<Point>> {
    let mut path = PathBuilder::default();
    let mut tokens = PathTokens { rest: data };
    let mut command = None;
    loop {
        if let Some(c) = tokens.command() {
            command = Some(c);
        }
        let Some(c) = command else {
            break;
        };
        let relative = c.is_ascii_lowercase();
        let origin = if relative {
            path.current
        } else {
            Point::new(0.0, 0.0)
        };
        let point = |tokens: &mut PathTokens| -> Option<Point> {
            Some(Point::new(
                origin.x + tokens.number()?,
                origin.y + tokens.number()?,
            ))
        };
        let done = match c.to_ascii_uppercase() {
            'M' => point(&mut tokens).map(|p| {
                path.move_to(p);
                // Coordinates after a move are lines.
                command = Some(if relative { 'l' } else { 'L' });
            }),
            'L' => point(&mut tokens).map(|p| path.line_to(p)),
            'H' => tokens
                .number()
                .map(|x| path.line_to(Point::new(origin.x + x, path.current.y))),
            'V' => {
                let y0 = if relative { path.current.y } else { 0.0 };
                tokens
                    .number()
                    .map(|y| path.line_to(Point::new(path.current.x, y0 + y)))
            }
            'C' => (|| {
                let (c1, c2, p) = (
                    point(&mut tokens)?,
                    point(&mut tokens)?,
                    point(&mut tokens)?,
                );
                path.cubic_to(c1, c2, p);
                Some(())
            })(),
            'S' => (|| {
                let (c2, p) = (point(&mut tokens)?, point(&mut tokens)?);
                let c1 = path.reflected();
                path.cubic_to(c1, c2, p);
                Some(())
            })(),
            'Q' => (|| {
                let (c, p) = (point(&mut tokens)?, point(&mut tokens)?);
                path.quadratic_to(c, p);
                Some(())
            })(),
            'T' => point(&mut tokens).map(|p| {
                let c = path.reflected();
                path.quadratic_to(c, p);
            }),
            'A' => (|| {
                let (rx, ry, rotate) = (tokens.number()?, tokens.number()?, tokens.number()?);
                let (large, sweep) = (tokens.flag()?, tokens.flag()?);
                let p = point(&mut tokens)?;
                path.arc_to(rx, ry, rotate, large, sweep, p);
                Some(())
            })(),
            'Z' => {
                path.close();
                command = None;
                Some(())
            }
            _ => None,
        };
        if done.is_none() {
            break;
        }
    }
    path.paths
}

/// Reads SVG path data.
struct PathTokens<'a> {
    rest: &'a str,
}

impl PathTokens<'_> {
    fn skip(&mut self) {
        self.rest = self
            .rest
            .trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
    }

    fn command(&mut self) -> Option<char> {
        self.skip();
        let c = self.rest.chars().next()?;
        if c.is_ascii_alphabetic() && !matches!(c, 'e' | 'E') {
            self.rest = &self.rest[1..];
            Some(c)
        } else {
            None
        }
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        let bytes = self.rest.as_bytes();
        let mut end = 0;
        if end < bytes.len() && matches!(bytes[end], b'+' | b'-') {
            end += 1;
        }
        let mut dot = false;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || (bytes[end] == b'.' && !dot)) {
            dot |= bytes[end] == b'.';
            end += 1;
        }
        if end < bytes.len() && matches!(bytes[end], b'e' | b'E') {
            let mut exp = end + 1;
            if exp < bytes.len() && matches!(bytes[exp], b'+' | b'-') {
                exp += 1;
            }
            if exp < bytes.len() && bytes[exp].is_ascii_digit() {
                end = exp;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
        }
        let value = self.rest[..end].parse().ok()?;
        self.rest = &self.rest[end..];
        Some(value)
    }

    /// An arc flag: a single `0` or `1`, which may run into the next number.
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let value = match self.rest.as_bytes().first()? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.rest = &self.rest[1..];
        Some(value)
    }
}

/// The subpaths of `shape()`, with coordinates in reference box `r`.
fn shape_path(
    from: &(LengthPercentage, LengthPercentage),
    commands: &[ShapeCommand<LengthPercentage>],
    r: Rect,
) -> Vec<Vec<Point>> {
    let mut path = PathBuilder::default();
    let at =
        |(x, y): &(LengthPercentage, LengthPercentage)| (x.resolve(r.width), y.resolve(r.height));
    let (x, y) = at(from);
    path.move_to(Point::new(r.x + x, r.y + y));
    for command in commands {
        let target =
            |path: &PathBuilder, by: bool, point: &(LengthPercentage, LengthPercentage)| {
                let (x, y) = at(point);
                if by {
                    Point::new(path.current.x + x, path.current.y + y)
                } else {
                    Point::new(r.x + x, r.y + y)
                }
            };
        match command {
            ShapeCommand::Move { by, to } => {
                let p = target(&path, *by, to);
                path.move_to(p);
            }
            ShapeCommand::Line { by, to } => {
                let p = target(&path, *by, to);
                path.line_to(p);
            }
            ShapeCommand::HLine { by, x } => {
                let x = x.resolve(r.width);
                let x = if *by { path.current.x + x } else { r.x + x };
                path.line_to(Point::new(x, path.current.y));
            }
            ShapeCommand::VLine { by, y } => {
                let y = y.resolve(r.height);
                let y = if *by { path.current.y + y } else { r.y + y };
                path.line_to(Point::new(path.current.x, y));
            }
            ShapeCommand::Curve { by, to, controls } => {
                let p = target(&path, *by, to);
                let controls: Vec<Point> = controls.iter().map(|c| target(&path, *by, c)).collect();
                match controls.as_slice() {
                    [c] => path.quadratic_to(*c, p),
                    [c1, c2, ..] => path.cubic_to(*c1, *c2, p),
                    [] => path.line_to(p),
                }
            }
            ShapeCommand::Smooth { by, to, control } => {
                let p = target(&path, *by, to);
                let c1 = path.reflected();
                match control {
                    Some(c2) => {
                        let c2 = target(&path, *by, c2);
                        path.cubic_to(c1, c2, p);
                    }
                    None => path.quadratic_to(c1, p),
                }
            }
            ShapeCommand::Arc {
                by,
                to,
                radii,
                clockwise,
                large,
                rotate,
            } => {
                let p = target(&path, *by, to);
                let (rx, ry) = (radii.0.resolve(r.width), radii.1.resolve(r.height));
                path.arc_to(rx, ry, *rotate, *large, *clockwise, p);
            }
            ShapeCommand::Close => path.close(),
        }
    }
    path.paths
}

/// The rows that polygons cover: in each one-pixel row, the leftmost and rightmost points
/// of the polygons in it.
fn polygon_spans(polygons: &[Vec<Point>]) -> Vec<Span> {
    let points = polygons.iter().flatten();
    let (top, bottom) = points.fold((f32::INFINITY, f32::NEG_INFINITY), |(t, b), p| {
        (t.min(p.y), b.max(p.y))
    });
    if !top.is_finite() || bottom <= top {
        return Vec::new();
    }
    let mut spans = Vec::new();
    let mut y = top;
    while y < bottom {
        let next = (y.floor() + 1.0).min(bottom);
        let mut extent: Option<(f32, f32)> = None;
        let mut add = |x: f32| {
            extent = Some(match extent {
                Some((l, r)) => (l.min(x), r.max(x)),
                None => (x, x),
            });
        };
        for polygon in polygons {
            let n = polygon.len();
            for i in 0..n {
                let (a, b) = (polygon[i], polygon[(i + 1) % n]);
                if a.y >= y && a.y <= next {
                    add(a.x);
                }
                // Where the edge crosses the row's top and bottom.
                for row in [y, next] {
                    if (a.y - row) * (b.y - row) < 0.0 {
                        add(a.x + (b.x - a.x) * (row - a.y) / (b.y - a.y));
                    }
                }
            }
        }
        if let Some((left, right)) = extent {
            spans.push(Span {
                top: y,
                bottom: next,
                left,
                right,
            });
        }
        y = next;
    }
    spans
}

/// The rows of an image shape: where the image, filling `content`, is more opaque than
/// `shape-image-threshold`.
fn image_spans(
    picture: &crate::picture::Picture,
    style: &ComputedValues,
    content: Rect,
) -> Vec<Span> {
    if content.width <= 0.0 || content.height <= 0.0 {
        return Vec::new();
    }
    let mut items = Vec::new();
    picture.paint(style, content, &mut items);
    let threshold = style.shape_image_threshold;
    let step_y = (content.height / MAX_IMAGE_SAMPLES).max(1.0);
    let step_x = (content.width / MAX_IMAGE_SAMPLES).max(0.5);
    let mut spans = Vec::new();
    let mut y = content.y;
    while y < content.bottom() {
        let next = (y + step_y).min(content.bottom());
        let mid = (y + next) / 2.0;
        let mut extent: Option<(f32, f32)> = None;
        let mut x = content.x;
        while x < content.right() {
            let sample = Point::new(x + step_x / 2.0, mid);
            if alpha_at(&items, sample) > threshold {
                extent = Some(match extent {
                    Some((l, _)) => (l, x + step_x),
                    None => (x, x + step_x),
                });
            }
            x += step_x;
        }
        if let Some((left, right)) = extent {
            spans.push(Span {
                top: y,
                bottom: next,
                left,
                right: right.min(content.right()),
            });
        }
        y = next;
    }
    spans
}

/// How opaque painted items are at a point, from 0 to 1, compositing them over each other.
fn alpha_at(items: &[DisplayItem], p: Point) -> f32 {
    let inside = |r: &Rect| p.x >= r.x && p.x < r.right() && p.y >= r.y && p.y < r.bottom();
    let mut clear = 1.0;
    for item in items {
        let alpha = match item {
            DisplayItem::Rect { rect, color } if inside(rect) => color.a as f32 / 255.0,
            DisplayItem::Image {
                image, rect, alpha, ..
            } if inside(rect) => {
                let u = ((p.x - rect.x) / rect.width * image.width as f32) as u32;
                let v = ((p.y - rect.y) / rect.height * image.height as f32) as u32;
                image_alpha(image, u.min(image.width - 1), v.min(image.height - 1))
                    * (*alpha as f32 / 255.0)
            }
            DisplayItem::Gradient { rect, shading } if inside(rect) => {
                let t = match shading.kind {
                    ShadingKind::Axial { start, end } => {
                        let (dx, dy) = (end.x - start.x, end.y - start.y);
                        let length = dx * dx + dy * dy;
                        if length > 0.0 {
                            ((p.x - start.x) * dx + (p.y - start.y) * dy) / length
                        } else {
                            0.0
                        }
                    }
                    ShadingKind::Radial {
                        center,
                        r0,
                        r1,
                        scale_y,
                    } => {
                        let dy = (p.y - center.y) / scale_y.max(1e-6);
                        let d = ((p.x - center.x).powi(2) + dy * dy).sqrt();
                        if r1 > r0 { (d - r0) / (r1 - r0) } else { 0.0 }
                    }
                };
                stop_alpha(&shading.stops, t.clamp(0.0, 1.0))
            }
            _ => 0.0,
        };
        clear *= 1.0 - alpha.clamp(0.0, 1.0);
    }
    1.0 - clear
}

fn image_alpha(image: &crate::images::Image, x: u32, y: u32) -> f32 {
    match &image.data {
        crate::images::ImageData::Pixels {
            alpha: Some(alpha), ..
        } => alpha
            .get((y * image.width + x) as usize)
            .map_or(1.0, |&a| a as f32 / 255.0),
        _ => 1.0,
    }
}

fn stop_alpha(stops: &[(f32, crate::display::Color)], t: f32) -> f32 {
    let alpha = |c: &crate::display::Color| c.a as f32 / 255.0;
    let next = stops.partition_point(|(at, _)| *at <= t);
    match (next.checked_sub(1).map(|i| &stops[i]), stops.get(next)) {
        (Some((a, c0)), Some((b, c1))) if b > a => {
            alpha(c0) + (alpha(c1) - alpha(c0)) * (t - a) / (b - a)
        }
        (Some((_, c)), _) | (None, Some((_, c))) => alpha(c),
        (None, None) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygons_cover_rows() {
        let triangle = vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(0.0, 100.0),
        ];
        let shape = Shape {
            spans: polygon_spans(&[triangle]),
            margin: 0.0,
        };
        let (left, right) = shape.extent(50.0, 1.0).unwrap();
        assert_eq!(left, 0.0);
        assert!((right - 50.0).abs() < 1.01);
        assert_eq!(shape.extent(150.0, 10.0), None);
        let wide = Shape {
            margin: 10.0,
            ..shape
        };
        assert!(wide.extent(105.0, 1.0).is_some());
    }

    #[test]
    fn svg_paths_flatten() {
        let paths = svg_path("M0 0 H10 V10 h-10 z M 20,20 l5-5 a5 5 0 0 1 10 0");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].len(), 4);
        let last = *paths[1].last().unwrap();
        assert!((last.x - 35.0).abs() < 0.01 && (last.y - 15.0).abs() < 0.01);
    }
}
