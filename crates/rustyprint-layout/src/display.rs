//! The display list: the output of layout and the input of backends.
//!
//! Lengths are CSS pixels (1/96 inch). The origin is the top-left corner of the page and y grows
//! downwards.

use std::sync::Arc;

use crate::{Font, Glyph, Image, shape};

/// A laid-out document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    /// Document title for the backend's metadata.
    pub title: Option<String>,
    pub pages: Vec<Page>,
    /// The document outline, in document order.
    pub bookmarks: Vec<Bookmark>,
    /// The ICC profiles that native colors refer to by index.
    pub color_profiles: Vec<ColorProfile>,
    /// The profile of `device-cmyk()` colors, an index into `color_profiles`; without one
    /// they are device CMYK.
    pub device_cmyk: Option<u32>,
}

/// An ICC profile that native colors can be in.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorProfile {
    pub data: std::sync::Arc<[u8]>,
    /// The number of color components: 1, 3 or 4.
    pub components: usize,
}

/// An entry of the document outline, such as a heading.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    /// The nesting level, from 1 for the outermost entries.
    pub level: u8,
    pub title: String,
    pub target: Destination,
}

/// A place in the document: a page index and a vertical position on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Destination {
    pub page: usize,
    /// Distance from the top of the page.
    pub y: f32,
}

/// An area of a page that links somewhere.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub rect: Rect,
    pub target: LinkTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// An external URI, such as `https://example.com/`.
    Uri(String),
    /// A place in this document.
    Internal(Destination),
}

/// A smooth color transition, interpolated linearly in sRGB between its stops.
#[derive(Debug, Clone, PartialEq)]
pub struct Shading {
    pub kind: ShadingKind,
    /// Colors at fractions of the way from the start to the end, in order.
    pub stops: Vec<(f32, Color)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShadingKind {
    /// Colors along the line from `start` to `end`, constant across it.
    Axial { start: Point, end: Point },
    /// Colors on circles around `center` from radius `r0` to `r1`, stretched vertically by
    /// `scale_y` into ellipses.
    Radial {
        center: Point,
        r0: f32,
        r1: f32,
        scale_y: f32,
    },
}

impl Shading {
    pub fn translated(&self, dx: f32, dy: f32) -> Shading {
        let kind = match self.kind {
            ShadingKind::Axial { start, end } => ShadingKind::Axial {
                start: Point::new(start.x + dx, start.y + dy),
                end: Point::new(end.x + dx, end.y + dy),
            },
            ShadingKind::Radial {
                center,
                r0,
                r1,
                scale_y,
            } => ShadingKind::Radial {
                center: Point::new(center.x + dx, center.y + dy),
                r0,
                r1,
                scale_y,
            },
        };
        Shading {
            kind,
            stops: self.stops.clone(),
        }
    }
}

/// One page and everything painted on it, in paint order.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// The trim size. Items may paint outside it, into the bleed.
    pub size: Size,
    pub items: Vec<DisplayItem>,
    /// How far the page extends past its trim size, for content printed to the edge.
    pub bleed: f32,
    /// Printer's marks drawn outside the bleed.
    pub marks: PrintMarks,
    /// Clockwise rotation in degrees for viewing: 0, 90, 180 or 270.
    pub rotate: i32,
    /// Link areas, in document order.
    pub links: Vec<Link>,
}

impl Page {
    /// A page of `size` without bleed, marks or rotation.
    pub fn new(size: Size, items: Vec<DisplayItem>) -> Self {
        Self {
            size,
            items,
            bleed: 0.0,
            marks: PrintMarks::default(),
            rotate: 0,
            links: Vec::new(),
        }
    }
}

/// Crop marks at the corners of the trim size and registration crosses at the middle of
/// each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrintMarks {
    pub crop: bool,
    pub cross: bool,
}

/// A painting operation.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    Text(TextRun),
    /// A filled rectangle.
    Rect {
        rect: Rect,
        color: Color,
    },
    /// A filled polygon.
    Polygon {
        points: Vec<Point>,
        color: Color,
    },
    /// A stroked open or closed path through points, such as a wavy underline.
    Polyline {
        points: Vec<Point>,
        width: f32,
        color: Color,
        closed: bool,
    },
    /// A stroked line, for dashed and dotted borders.
    Line {
        from: Point,
        to: Point,
        width: f32,
        color: Color,
        style: LineStyle,
    },
    /// A filled path, such as a rounded background or border.
    Path {
        path: Path,
        color: Color,
        rule: FillRule,
    },
    /// A stroked path, for dashed and dotted rounded borders.
    StrokePath {
        path: Path,
        width: f32,
        color: Color,
        style: LineStyle,
    },
    /// An image scaled to fill `rect`.
    Image {
        image: Arc<Image>,
        rect: Rect,
        /// Whether scaling smooths the image, rather than showing its pixels.
        smooth: bool,
        /// The EXIF orientation to display the stored pixels in, 1 to 8.
        orientation: u8,
        /// The opacity the image is drawn with, from 0 to 255.
        alpha: u8,
    },
    /// A gradient shading clipped to `rect`.
    Gradient {
        rect: Rect,
        shading: Arc<Shading>,
    },
    /// Clips the following items to `rect` until the matching [`DisplayItem::PopClip`].
    PushClip(Rect),
    /// Clips the following items to the glyphs of `runs` until the matching
    /// [`DisplayItem::PopClip`].
    PushClipText(Vec<TextRun>),
    /// Clips the following items to the inside of `path` until the matching
    /// [`DisplayItem::PopClip`].
    PushClipPath {
        path: Path,
        rule: FillRule,
    },
    /// Transforms the following items by `[a, b, c, d, e, f]`, which maps `(x, y)` to
    /// `(a x + c y + e, b x + d y + f)`, until the matching [`DisplayItem::PopClip`].
    PushTransform([f32; 6]),
    /// Ends the most recent clip or transform.
    PopClip,
    /// Paints the following items, until the matching [`DisplayItem::PopGroup`], as one
    /// group that is then composited onto what is below it.
    PushGroup(Group),
    PopGroup,
    /// Stands for a filtered copy of what is painted below it, clipped to a shape
    /// (`backdrop-filter`). Layout replaces it before the document reaches a backend.
    Backdrop(Arc<Backdrop>),
}

/// How a group composites (Compositing and Blending 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The opacity the whole group is drawn with, from 0 to 1.
    pub opacity: f32,
    pub blend: BlendMode,
    /// Whether the group starts from a transparent backdrop, rather than blending its items
    /// with what is below it.
    pub isolated: bool,
    /// A mask whose luminance gives the group's alpha.
    pub mask: Option<SoftMask>,
}

impl Group {
    /// A group that only isolates its items.
    pub const ISOLATED: Group = Group {
        opacity: 1.0,
        blend: BlendMode::Normal,
        isolated: true,
        mask: None,
    };
}

/// A grayscale image covering `rect` whose luminance is the alpha of what it masks;
/// outside `rect` everything is masked away.
#[derive(Debug, Clone, PartialEq)]
pub struct SoftMask {
    pub rect: Rect,
    pub image: Arc<Image>,
}

/// A blend mode (Compositing and Blending 1 § 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    PlusDarker,
    PlusLighter,
}

/// The backdrop of an element with `backdrop-filter`: everything painted below it since the
/// start of the innermost group, filtered and clipped to `clip`.
#[derive(Debug, Clone, PartialEq)]
pub struct Backdrop {
    pub clip: Path,
    pub filters: Vec<Filter>,
    /// The opacity the filtered backdrop is drawn with.
    pub opacity: f32,
}

/// A filter primitive, from a `<filter-function>` (Filter Effects 1 § 13).
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    /// A 4×5 matrix applied to straight RGBA colors with components from 0 to 1, row by
    /// row: the fifth column is added.
    ColorMatrix([f32; 20]),
    /// Multiplies alpha.
    Opacity(f32),
    /// A Gaussian blur with this standard deviation, in pixels.
    Blur(f32),
    /// A copy of the alpha in `color`, moved and blurred, painted below.
    DropShadow {
        offset: Point,
        blur: f32,
        color: Color,
    },
}

/// An affine transformation `[a, b, c, d, e, f]` that maps `(x, y)` to
/// `(a x + c y + e, b x + d y + f)`.
pub type Matrix = [f32; 6];

/// The transformation that leaves everything where it is.
pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `m` after `n`: the transformation that applies `n` first.
pub fn multiply(m: Matrix, n: Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

/// The transformation that undoes `m`, unless it flattens everything onto a line.
pub fn invert(m: Matrix) -> Option<Matrix> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1.0e-9 || !det.is_finite() {
        return None;
    }
    Some([
        m[3] / det,
        -m[1] / det,
        -m[2] / det,
        m[0] / det,
        (m[2] * m[5] - m[3] * m[4]) / det,
        (m[1] * m[4] - m[0] * m[5]) / det,
    ])
}

/// Where `m` takes `p`.
pub fn apply(m: Matrix, p: Point) -> Point {
    Point::new(
        m[0] * p.x + m[2] * p.y + m[4],
        m[1] * p.x + m[3] * p.y + m[5],
    )
}

/// The smallest rectangle containing `rect` transformed by `m`.
pub fn transform_rect(m: Matrix, rect: Rect) -> Rect {
    let corners = [
        Point::new(rect.x, rect.y),
        Point::new(rect.right(), rect.y),
        Point::new(rect.right(), rect.bottom()),
        Point::new(rect.x, rect.bottom()),
    ];
    Rect::enclosing(corners.map(|p| apply(m, p))).unwrap_or(rect)
}

/// Which parts of a self-overlapping path count as inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    /// Inside where a ray crosses the path an odd number of times, so a path with an inner
    /// contour has a hole.
    EvenOdd,
}

/// One step of a [`Path`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    /// A cubic Bézier curve through two control points to an end point.
    CubicTo(Point, Point, Point),
    Close,
}

/// Contours of lines and cubic Bézier curves.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Path {
    pub segments: Vec<PathSegment>,
}

impl Path {
    /// A closed polygon through `points`.
    pub fn polygon(points: &[Point]) -> Path {
        let mut path = Path::default();
        for (i, &p) in points.iter().enumerate() {
            path.segments.push(if i == 0 {
                PathSegment::MoveTo(p)
            } else {
                PathSegment::LineTo(p)
            });
        }
        if !points.is_empty() {
            path.segments.push(PathSegment::Close);
        }
        path
    }

    /// Adds the contours of `other`.
    pub fn extend(&mut self, other: Path) {
        self.segments.extend(other.segments);
    }

    /// Every point of the path, control points included.
    pub fn points(&self) -> impl Iterator<Item = Point> + '_ {
        self.segments.iter().flat_map(|segment| match *segment {
            PathSegment::MoveTo(p) | PathSegment::LineTo(p) => vec![p],
            PathSegment::CubicTo(a, b, c) => vec![a, b, c],
            PathSegment::Close => vec![],
        })
    }

    /// The path moved by `(dx, dy)`.
    pub fn translated(&self, dx: f32, dy: f32) -> Path {
        self.transformed([1.0, 0.0, 0.0, 1.0, dx, dy])
    }

    /// The path transformed by `m`.
    pub fn transformed(&self, m: Matrix) -> Path {
        let point = |p: Point| apply(m, p);
        Path {
            segments: self
                .segments
                .iter()
                .map(|segment| match *segment {
                    PathSegment::MoveTo(p) => PathSegment::MoveTo(point(p)),
                    PathSegment::LineTo(p) => PathSegment::LineTo(point(p)),
                    PathSegment::CubicTo(a, b, c) => {
                        PathSegment::CubicTo(point(a), point(b), point(c))
                    }
                    PathSegment::Close => PathSegment::Close,
                })
                .collect(),
        }
    }

    /// The approximate length of the path, with curves flattened.
    pub fn length(&self) -> f32 {
        let distance = |a: Point, b: Point| (b.x - a.x).hypot(b.y - a.y);
        let mut length = 0.0;
        let mut start = Point::default();
        let mut pen = Point::default();
        for segment in &self.segments {
            match *segment {
                PathSegment::MoveTo(p) => {
                    start = p;
                    pen = p;
                }
                PathSegment::LineTo(p) => {
                    length += distance(pen, p);
                    pen = p;
                }
                PathSegment::CubicTo(a, b, c) => {
                    const STEPS: usize = 16;
                    let mut previous = pen;
                    for step in 1..=STEPS {
                        let t = step as f32 / STEPS as f32;
                        let u = 1.0 - t;
                        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                        let point = Point::new(
                            w[0] * pen.x + w[1] * a.x + w[2] * b.x + w[3] * c.x,
                            w[0] * pen.y + w[1] * a.y + w[2] * b.y + w[3] * c.y,
                        );
                        length += distance(previous, point);
                        previous = point;
                    }
                    pen = c;
                }
                PathSegment::Close => {
                    length += distance(pen, start);
                    pen = start;
                }
            }
        }
        length
    }

    /// A rectangle that contains the path.
    pub fn bounds(&self) -> Option<Rect> {
        Rect::enclosing(self.points())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    /// Dashes three line widths long.
    Dashed,
    /// Round dots one line width apart.
    Dotted,
}

impl DisplayItem {
    /// A rectangle that contains everything the item paints, or `None` for clip markers.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            DisplayItem::Text(run) => Some(Rect::new(
                run.origin.x,
                run.origin.y - run.size * 1.5,
                run.width(),
                run.size * 2.5,
            )),
            DisplayItem::Rect { rect, .. } => Some(*rect),
            DisplayItem::Polygon { points, .. } => Rect::enclosing(points.iter().copied()),
            DisplayItem::Polyline { points, width, .. } => {
                Rect::enclosing(points.iter().copied()).map(|r| r.inflate(width / 2.0))
            }
            DisplayItem::Line {
                from, to, width, ..
            } => Rect::enclosing([*from, *to]).map(|r| r.inflate(width / 2.0)),
            DisplayItem::Path { path, .. } => path.bounds(),
            DisplayItem::StrokePath { path, width, .. } => {
                path.bounds().map(|r| r.inflate(width / 2.0))
            }
            DisplayItem::Image { rect, .. } | DisplayItem::Gradient { rect, .. } => Some(*rect),
            DisplayItem::PushClip(_)
            | DisplayItem::PushClipPath { .. }
            | DisplayItem::PushClipText(_)
            | DisplayItem::PushTransform(_)
            | DisplayItem::PopClip
            | DisplayItem::PushGroup(_)
            | DisplayItem::PopGroup
            | DisplayItem::Backdrop(_) => None,
        }
    }

    /// The item moved by `(dx, dy)`.
    pub fn translated(&self, dx: f32, dy: f32) -> DisplayItem {
        let point = |p: Point| Point::new(p.x + dx, p.y + dy);
        match self {
            DisplayItem::Text(run) => {
                let mut run = run.clone();
                run.origin = point(run.origin);
                DisplayItem::Text(run)
            }
            DisplayItem::Rect { rect, color } => DisplayItem::Rect {
                rect: rect.translated(dx, dy),
                color: *color,
            },
            DisplayItem::Polygon { points, color } => DisplayItem::Polygon {
                points: points.iter().copied().map(point).collect(),
                color: *color,
            },
            DisplayItem::Polyline {
                points,
                width,
                color,
                closed,
            } => DisplayItem::Polyline {
                points: points.iter().copied().map(point).collect(),
                width: *width,
                color: *color,
                closed: *closed,
            },
            DisplayItem::Line {
                from,
                to,
                width,
                color,
                style,
            } => DisplayItem::Line {
                from: point(*from),
                to: point(*to),
                width: *width,
                color: *color,
                style: *style,
            },
            DisplayItem::Path { path, color, rule } => DisplayItem::Path {
                path: path.translated(dx, dy),
                color: *color,
                rule: *rule,
            },
            DisplayItem::StrokePath {
                path,
                width,
                color,
                style,
            } => DisplayItem::StrokePath {
                path: path.translated(dx, dy),
                width: *width,
                color: *color,
                style: *style,
            },
            DisplayItem::Image {
                image,
                rect,
                smooth,
                orientation,
                alpha,
            } => DisplayItem::Image {
                image: image.clone(),
                rect: rect.translated(dx, dy),
                smooth: *smooth,
                orientation: *orientation,
                alpha: *alpha,
            },
            DisplayItem::Gradient { rect, shading } => DisplayItem::Gradient {
                rect: rect.translated(dx, dy),
                shading: Arc::new(shading.translated(dx, dy)),
            },
            DisplayItem::PushClip(rect) => DisplayItem::PushClip(rect.translated(dx, dy)),
            DisplayItem::PushClipText(runs) => DisplayItem::PushClipText(
                runs.iter()
                    .map(|run| {
                        let mut run = run.clone();
                        run.origin = point(run.origin);
                        run
                    })
                    .collect(),
            ),
            DisplayItem::PushClipPath { path, rule } => DisplayItem::PushClipPath {
                path: path.translated(dx, dy),
                rule: *rule,
            },
            // The items inside move too, so the transform is conjugated by the move.
            DisplayItem::PushTransform([a, b, c, d, e, f]) => DisplayItem::PushTransform([
                *a,
                *b,
                *c,
                *d,
                e + dx - (a * dx + c * dy),
                f + dy - (b * dx + d * dy),
            ]),
            DisplayItem::PopClip => DisplayItem::PopClip,
            DisplayItem::PushGroup(group) => DisplayItem::PushGroup(Group {
                mask: group.mask.as_ref().map(|mask| SoftMask {
                    rect: mask.rect.translated(dx, dy),
                    image: mask.image.clone(),
                }),
                ..group.clone()
            }),
            DisplayItem::PopGroup => DisplayItem::PopGroup,
            DisplayItem::Backdrop(backdrop) => DisplayItem::Backdrop(Arc::new(Backdrop {
                clip: backdrop.clip.translated(dx, dy),
                ..(**backdrop).clone()
            })),
        }
    }
}

/// A run of shaped glyphs in one font, size and color.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// Start of the baseline.
    pub origin: Point,
    pub font: Font,
    /// Font size in CSS pixels per em.
    pub size: f32,
    /// The fill color, or `None` to only stroke the glyphs.
    pub color: Color,
    pub glyphs: Vec<Glyph>,
    /// The source text that the glyphs' text ranges point into.
    pub text: Arc<str>,
    /// Slant for synthesized oblique text, in degrees.
    pub oblique: f32,
    /// Extra stroke width for synthesized bold text, in pixels.
    pub embolden: f32,
    /// An outline drawn around the glyphs.
    pub stroke: Option<TextStroke>,
    /// Whether the fill is skipped, leaving only the stroke.
    pub fill: bool,
    /// Whether the stroke is painted below the fill instead of over it.
    pub stroke_first: bool,
    /// For text on a turned line of vertical text, how its glyphs stand upright. Painting
    /// turns them; backends never see it.
    pub upright: Option<Upright>,
}

/// How the glyphs of a run of vertical text stand upright on a line that is turned a
/// quarter. The run's advances are the space the glyphs take along the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upright {
    /// Each glyph stands upright in the middle of its advance.
    Each,
    /// The glyphs are set side by side, squeezed into the run's advance if they are wider,
    /// and stand upright together (`text-combine-upright`).
    Combined,
}

/// The outline of text glyphs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStroke {
    pub color: Color,
    pub width: f32,
}

impl TextRun {
    /// Shapes `text` into a run whose baseline starts at `origin`.
    pub fn new(
        font: Font,
        size: f32,
        color: Color,
        origin: Point,
        text: impl Into<String>,
    ) -> Self {
        let text: String = text.into();
        let glyphs = shape(&font, size, &text);
        Self {
            origin,
            font,
            size,
            color,
            glyphs,
            text: Arc::from(text),
            oblique: 0.0,
            embolden: 0.0,
            stroke: None,
            fill: true,
            stroke_first: false,
            upright: None,
        }
    }

    /// Total advance width of the run.
    pub fn width(&self) -> f32 {
        self.glyphs.iter().map(|g| g.x_advance).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    /// ISO A4, 210 × 297 mm.
    pub const A4: Self = Self::new(210.0 * 96.0 / 25.4, 297.0 * 96.0 / 25.4);
    /// US Letter, 8.5 × 11 in.
    pub const LETTER: Self = Self::new(8.5 * 96.0, 11.0 * 96.0);

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn translated(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    pub fn inflate(&self, amount: f32) -> Rect {
        Rect::new(
            self.x - amount,
            self.y - amount,
            self.width + 2.0 * amount,
            self.height + 2.0 * amount,
        )
    }

    /// The rectangle shrunk by `top`, `right`, `bottom` and `left`.
    pub fn deflate(&self, top: f32, right: f32, bottom: f32, left: f32) -> Rect {
        Rect::new(
            self.x + left,
            self.y + top,
            (self.width - left - right).max(0.0),
            (self.height - top - bottom).max(0.0),
        )
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// The smallest rectangle containing `points`.
    pub fn enclosing(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for p in points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
}

/// An sRGB color with straight (not premultiplied) alpha, and the device or ICC color it
/// stands for, when it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub native: Option<NativeColor>,
}

/// The [`NativeColor`] space of `device-cmyk()` colors.
pub const DEVICE_CMYK: u32 = rustyprint_css::values::DEVICE_CMYK;

/// A color in device CMYK or a document color profile, for output that can use it as is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeColor {
    /// [`DEVICE_CMYK`], or the index of the document's color profile.
    pub space: u32,
    /// The components, from 0 to 65535; unused ones are 0.
    pub components: [u16; 4],
}

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r,
            g,
            b,
            a,
            native: None,
        }
    }

    /// A computed CSS color, keeping its native color.
    pub fn from_css(color: &rustyprint_css::values::AbsoluteColor) -> Self {
        let [r, g, b, a] = color.to_rgba8();
        Self {
            native: color.native.map(|native| NativeColor {
                space: native.space,
                components: native
                    .components
                    .map(|c| (c.clamp(0.0, 1.0) * 65535.0).round() as u16),
            }),
            ..Self::rgba(r, g, b, a)
        }
    }

    pub fn is_transparent(&self) -> bool {
        self.a == 0
    }
}
