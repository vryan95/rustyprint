//! Drawing a parsed SVG as display items: its shapes stay shapes and its text stays text,
//! so the picture prints sharply at any size and takes no pixels.
//!
//! What display items cannot express is refused, and the SVG is then drawn to pixels: a
//! gradient, a pattern, a mask, a filter, and a clip path made of several shapes.

use std::collections::HashMap;
use std::sync::Arc;

use resvg::usvg;
use rustyprint_css::ComputedValues;
use unicode_bidi::{BidiClass, bidi_class};

use crate::display::{
    BlendMode, Color, DisplayItem, FillRule, Group, IDENTITY, LineCap, LineJoin, LineStyle, Matrix,
    Path, PathSegment, Point, Rect, StrokeShape, TextRun, multiply,
};
use crate::fonts::FontChoice;
use crate::images::Image;
use crate::{Font, fonts};

/// What a drawing needs besides the SVG.
pub(crate) struct Context<'a> {
    /// The style of the SVG, whose fonts its text is drawn in.
    pub style: &'a ComputedValues,
    /// Decodes an image file the SVG holds, or gives `None` for one that cannot be shown.
    pub decode: &'a dyn Fn(&[u8]) -> Option<Arc<Image>>,
}

/// What an SVG has that display items cannot express.
pub(crate) type Refused = &'static str;

/// The display items of `tree`, in its own CSS pixels from its top left corner.
pub(crate) fn items(tree: &usvg::Tree, context: &Context) -> Result<Vec<DisplayItem>, Refused> {
    let mut painter = Painter {
        context,
        items: Vec::new(),
        fonts: HashMap::new(),
    };
    painter.group(tree.root())?;
    Ok(painter.items)
}

struct Painter<'a> {
    context: &'a Context<'a>,
    items: Vec<DisplayItem>,
    /// The engine's choice for each font the text asks for.
    fonts: HashMap<usvg::Font, FontChoice>,
}

impl Painter<'_> {
    fn group(&mut self, group: &usvg::Group) -> Result<(), Refused> {
        rustyprint_dom::budget::check_deadline();
        let opacity = group.opacity().get();
        // What is wholly transparent paints nothing, and its text is not text of the page.
        if opacity == 0.0 {
            return Ok(());
        }
        if !group.filters().is_empty() {
            return Err("a filter");
        }
        if group.mask().is_some() {
            return Err("a mask");
        }
        let start = self.items.len();
        let mut pushed = 0;
        let transform = matrix(group.transform());
        if transform != IDENTITY {
            self.items.push(DisplayItem::PushTransform(transform));
            pushed += 1;
        }
        if let Some(clip) = group.clip_path() {
            let mut clips = Vec::new();
            if !clip_paths(clip, &mut clips)? {
                // Clipped to nothing.
                self.items.truncate(start);
                return Ok(());
            }
            for (path, rule) in clips {
                self.items.push(DisplayItem::PushClipPath { path, rule });
                pushed += 1;
            }
        }
        let blend = blend_mode(group.blend_mode());
        match group.children() {
            // The opacity of a lone shape with one paint is that paint's.
            [usvg::Node::Path(path)]
                if blend == BlendMode::Normal
                    && path.fill().is_some() != path.stroke().is_some() =>
            {
                self.path(path, opacity)?;
            }
            children => {
                let grouped = opacity < 1.0 || blend != BlendMode::Normal || group.isolate();
                if grouped {
                    self.items.push(DisplayItem::PushGroup(Group {
                        opacity,
                        blend,
                        isolated: true,
                        mask: None,
                    }));
                }
                for child in children {
                    match child {
                        usvg::Node::Group(group) => self.group(group)?,
                        usvg::Node::Path(path) => self.path(path, 1.0)?,
                        usvg::Node::Image(image) => self.image(image)?,
                        usvg::Node::Text(text) => self.text(text)?,
                    }
                }
                if grouped {
                    self.items.push(DisplayItem::PopGroup);
                }
            }
        }
        for _ in 0..pushed {
            self.items.push(DisplayItem::PopClip);
        }
        Ok(())
    }

    /// A shape, its paints made more transparent by `opacity`.
    fn path(&mut self, path: &usvg::Path, opacity: f32) -> Result<(), Refused> {
        if !path.is_visible() {
            return Ok(());
        }
        let outline = outline(path.data());
        let fill = match path.fill() {
            // A single straight line, as of a `<line>`, has no inside to fill.
            Some(_) if !encloses(&outline) => None,
            Some(fill) => Some(DisplayItem::Path {
                path: outline.clone(),
                color: color(fill.paint(), fill.opacity().get() * opacity)?,
                rule: fill_rule(fill.rule()),
            }),
            None => None,
        };
        let stroke = match path.stroke() {
            Some(stroke) => Some(DisplayItem::StrokePath {
                path: outline,
                width: stroke.width().get(),
                color: color(stroke.paint(), stroke.opacity().get() * opacity)?,
                style: LineStyle::Solid,
                shape: Some(Arc::new(StrokeShape {
                    cap: match stroke.linecap() {
                        usvg::LineCap::Butt => LineCap::Butt,
                        usvg::LineCap::Round => LineCap::Round,
                        usvg::LineCap::Square => LineCap::Square,
                    },
                    join: match stroke.linejoin() {
                        // A miter past its limit is beveled, which is the nearest there
                        // is to clipping it.
                        usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
                        usvg::LineJoin::Round => LineJoin::Round,
                        usvg::LineJoin::Bevel => LineJoin::Bevel,
                    },
                    miter_limit: stroke.miterlimit().get(),
                    dashes: stroke.dasharray().map(<[f32]>::to_vec).unwrap_or_default(),
                    dash_offset: stroke.dashoffset(),
                })),
            }),
            None => None,
        };
        let painted = match path.paint_order() {
            usvg::PaintOrder::FillAndStroke => [fill, stroke],
            usvg::PaintOrder::StrokeAndFill => [stroke, fill],
        };
        self.items
            .extend(painted.into_iter().flatten().filter(|item| match item {
                DisplayItem::Path { color, .. } | DisplayItem::StrokePath { color, .. } => {
                    !color.is_transparent()
                }
                _ => true,
            }));
        Ok(())
    }

    /// An image the SVG holds, in a box of its own size from its top left corner.
    fn image(&mut self, image: &usvg::Image) -> Result<(), Refused> {
        if !image.is_visible() {
            return Ok(());
        }
        let rect = Rect::new(0.0, 0.0, image.size().width(), image.size().height());
        match image.kind() {
            usvg::ImageKind::JPEG(bytes)
            | usvg::ImageKind::PNG(bytes)
            | usvg::ImageKind::GIF(bytes)
            | usvg::ImageKind::WEBP(bytes) => {
                if let Some(decoded) = (self.context.decode)(bytes) {
                    // The file's own orientation, as `image-orientation: from-image`
                    // shows an image of the page. One turned a quarter is as tall as its
                    // pixels are wide, and keeps that shape in the box of its pixels.
                    let orientation = decoded.orientation;
                    let rect = if Image::turned(orientation) {
                        let scale = (rect.width / rect.height).min(rect.height / rect.width);
                        let (width, height) = (rect.height * scale, rect.width * scale);
                        Rect::new(
                            (rect.width - width) / 2.0,
                            (rect.height - height) / 2.0,
                            width,
                            height,
                        )
                    } else {
                        rect
                    };
                    self.items.push(DisplayItem::Image {
                        image: decoded,
                        rect,
                        smooth: !matches!(
                            image.rendering_mode(),
                            usvg::ImageRendering::OptimizeSpeed
                                | usvg::ImageRendering::CrispEdges
                                | usvg::ImageRendering::Pixelated
                        ),
                        orientation,
                        alpha: 255,
                    });
                }
            }
            usvg::ImageKind::SVG(tree) => {
                self.items.push(DisplayItem::PushClip(rect));
                self.group(tree.root())?;
                self.items.push(DisplayItem::PopClip);
            }
        }
        Ok(())
    }

    /// Text, as runs of page text where it is laid out simply, or else as the outlines of
    /// its glyphs.
    fn text(&mut self, text: &usvg::Text) -> Result<(), Refused> {
        match self.runs(text) {
            Some(runs) => {
                self.items.extend(runs.into_iter().map(DisplayItem::Text));
                Ok(())
            }
            None => self.group(text.flattened()),
        }
    }

    /// The runs of text that is set along straight horizontal lines from left to right,
    /// each span in one font, or `None` for any other.
    fn runs(&mut self, text: &usvg::Text) -> Option<Vec<TextRun>> {
        if text.writing_mode() != usvg::WritingMode::LeftToRight
            || text.rotate().iter().any(|angle| *angle != 0.0)
        {
            return None;
        }
        let mut runs = Vec::new();
        // Where the chunk before ended, and how many characters came before this one.
        let (mut last_x, mut last_y, mut before) = (0.0, 0.0, 0);
        for chunk in text.chunks() {
            if !matches!(chunk.text_flow(), usvg::TextFlow::Linear) {
                return None;
            }
            let count = chunk.text().chars().count();
            // `dx` and `dy` may move the chunk, but no character inside it.
            let shift = |list: &[f32]| {
                let inside = list.iter().skip(before + 1).take(count.saturating_sub(1));
                inside
                    .clone()
                    .all(|shift| *shift == 0.0)
                    .then(|| list.get(before).copied().unwrap_or(0.0))
            };
            let (dx, dy) = (shift(text.dx())?, shift(text.dy())?);
            let (x, y) = (chunk.x().unwrap_or(last_x), chunk.y().unwrap_or(last_y));
            let first = runs.len();
            let mut width = 0.0;
            for span in chunk.spans() {
                let part = chunk.text().get(span.start()..span.end())?;
                if part.is_empty() {
                    continue;
                }
                let run = self.run(span, part, width)?;
                width += run.width();
                if span.is_visible() && span.fill().is_some() && !run.color.is_transparent() {
                    runs.push(run);
                }
            }
            let start = x
                + dx
                + match chunk.anchor() {
                    usvg::TextAnchor::Start => 0.0,
                    usvg::TextAnchor::Middle => -width / 2.0,
                    usvg::TextAnchor::End => -width,
                };
            for run in &mut runs[first..] {
                run.origin.x += start;
                run.origin.y += y + dy;
            }
            (last_x, last_y) = (start + width, y + dy);
            before += count;
        }
        Some(runs)
    }

    /// The run of a span's text `part`, starting `x` along its line, with its baseline
    /// measured from the line's. `None` for a span that page text cannot express.
    fn run(&mut self, span: &usvg::TextSpan, part: &str, x: f32) -> Option<TextRun> {
        let decorated = span.decoration().underline().is_some()
            || span.decoration().overline().is_some()
            || span.decoration().line_through().is_some();
        let shifted = span
            .baseline_shift()
            .iter()
            .any(|shift| !matches!(shift, usvg::BaselineShift::Baseline));
        if span.stroke().is_some()
            || decorated
            || shifted
            || span.letter_spacing() != 0.0
            || span.word_spacing() != 0.0
            || span.text_length().is_some()
            || span.small_caps()
            || !span.apply_kerning()
            || part.chars().any(right_to_left)
        {
            return None;
        }
        let color = match span.fill().map(|fill| (fill.paint(), fill.opacity().get())) {
            Some((usvg::Paint::Color(c), opacity)) => {
                Color::rgba(c.red, c.green, c.blue, alpha(opacity))
            }
            Some(_) => return None,
            None => Color::rgba(0, 0, 0, 0),
        };
        let style = self.context.style;
        let choice = self
            .fonts
            .entry(span.font().clone())
            .or_insert_with(|| fonts::select(&crate::svg::font_style(style, span.font())));
        // One font for the whole span: the first of the fallbacks that has all of it.
        let font = std::iter::once(&choice.font)
            .chain(&choice.fallbacks)
            .find(|font| part.chars().all(|c| c.is_whitespace() || font.covers(c)))?
            .clone();
        let size = span.font_size().get();
        let origin = Point::new(x, baseline(span, &font, size));
        let mut run = TextRun::new(font, size, color, origin, part);
        run.oblique = choice.synthetic_oblique;
        if choice.synthetic_bold {
            run.embolden = size / 24.0;
        }
        Some(run)
    }
}

/// How far below the line's position a span's alphabetic baseline is, as browsers place the
/// other baselines: from the font's ascent, descent and x-height.
fn baseline(span: &usvg::TextSpan, font: &Font, size: f32) -> f32 {
    use usvg::AlignmentBaseline as Alignment;
    use usvg::DominantBaseline as Dominant;
    let metrics = font.metrics();
    let ascent = font.scale(metrics.ascender, size);
    let descent = font.scale(metrics.descender, size);
    let alignment = match span.alignment_baseline() {
        Alignment::Auto | Alignment::Baseline => match span.dominant_baseline() {
            Dominant::Ideographic => Alignment::Ideographic,
            Dominant::Hanging => Alignment::Hanging,
            Dominant::Mathematical => Alignment::Mathematical,
            Dominant::Central => Alignment::Central,
            Dominant::Middle => Alignment::Middle,
            Dominant::TextAfterEdge => Alignment::TextAfterEdge,
            Dominant::TextBeforeEdge => Alignment::TextBeforeEdge,
            _ => Alignment::Alphabetic,
        },
        other => other,
    };
    match alignment {
        Alignment::Auto | Alignment::Baseline | Alignment::Alphabetic => 0.0,
        Alignment::BeforeEdge | Alignment::TextBeforeEdge => ascent,
        Alignment::Middle => font.scale(metrics.x_height, size) * 0.5,
        Alignment::Central => ascent - (ascent - descent) * 0.5,
        Alignment::AfterEdge | Alignment::TextAfterEdge | Alignment::Ideographic => descent,
        Alignment::Hanging => ascent * 0.8,
        Alignment::Mathematical => ascent * 0.5,
    }
}

/// Whether `c` is set from right to left, which a run of one direction does not do.
fn right_to_left(c: char) -> bool {
    matches!(
        bidi_class(c),
        BidiClass::R | BidiClass::AL | BidiClass::AN | BidiClass::RLE | BidiClass::RLO
    )
}

/// The shapes a clip path clips to, each to be applied in turn, in the coordinates of the
/// clipped group. `false` if it has no shape, and so clips everything away.
fn clip_paths(clip: &usvg::ClipPath, out: &mut Vec<(Path, FillRule)>) -> Result<bool, Refused> {
    if let Some(outer) = clip.clip_path()
        && !clip_paths(outer, out)?
    {
        return Ok(false);
    }
    let mut shapes = Vec::new();
    clip_shapes(clip.root(), matrix(clip.transform()), &mut shapes)?;
    // Several shapes clip to all of them together, which one path cannot say where they
    // overlap.
    if shapes.len() > 1 {
        return Err("a clip path of several shapes");
    }
    let found = !shapes.is_empty();
    out.extend(shapes);
    Ok(found)
}

/// The shapes of a clip path's `group`, moved by `transform`.
fn clip_shapes(
    group: &usvg::Group,
    transform: Matrix,
    out: &mut Vec<(Path, FillRule)>,
) -> Result<(), Refused> {
    if group.clip_path().is_some() || group.mask().is_some() || !group.filters().is_empty() {
        return Err("a clip path whose shapes are clipped");
    }
    let transform = multiply(transform, matrix(group.transform()));
    for child in group.children() {
        match child {
            usvg::Node::Path(path) if path.is_visible() => {
                let rule = path
                    .fill()
                    .map_or(usvg::FillRule::NonZero, usvg::Fill::rule);
                out.push((outline(path.data()).transformed(transform), fill_rule(rule)));
            }
            usvg::Node::Group(group) => clip_shapes(group, transform, out)?,
            usvg::Node::Text(text) => clip_shapes(text.flattened(), transform, out)?,
            usvg::Node::Path(_) | usvg::Node::Image(_) => {}
        }
    }
    Ok(())
}

/// A solid paint with `opacity`. Any other paint is refused.
fn color(paint: &usvg::Paint, opacity: f32) -> Result<Color, Refused> {
    match paint {
        usvg::Paint::Color(c) => Ok(Color::rgba(c.red, c.green, c.blue, alpha(opacity))),
        usvg::Paint::LinearGradient(_) | usvg::Paint::RadialGradient(_) => Err("a gradient"),
        usvg::Paint::Pattern(_) => Err("a pattern"),
    }
}

fn alpha(opacity: f32) -> u8 {
    (opacity.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn fill_rule(rule: usvg::FillRule) -> FillRule {
    match rule {
        usvg::FillRule::NonZero => FillRule::NonZero,
        usvg::FillRule::EvenOdd => FillRule::EvenOdd,
    }
}

fn matrix(transform: usvg::Transform) -> Matrix {
    [
        transform.sx,
        transform.ky,
        transform.kx,
        transform.sy,
        transform.tx,
        transform.ty,
    ]
}

/// Whether a shape has an inside to fill: a curve, or a contour of two lines or more. A
/// curve and the line that closes it enclose an area, as in `M0 0 Q50 50 100 0`.
fn encloses(outline: &Path) -> bool {
    let mut lines = 0;
    outline.segments.iter().any(|segment| match segment {
        PathSegment::MoveTo(_) => {
            lines = 0;
            false
        }
        PathSegment::LineTo(_) => {
            lines += 1;
            lines >= 2
        }
        PathSegment::CubicTo(..) => true,
        PathSegment::Close => false,
    })
}

/// The contours of a shape. A quadratic curve is the cubic curve of the same shape.
fn outline(path: &usvg::tiny_skia_path::Path) -> Path {
    use usvg::tiny_skia_path::PathSegment as Segment;
    let point = |p: usvg::tiny_skia_path::Point| Point::new(p.x, p.y);
    let mut pen = Point::default();
    let mut start = pen;
    let mut segments = Vec::with_capacity(path.len());
    for segment in path.segments() {
        segments.push(match segment {
            Segment::MoveTo(p) => {
                pen = point(p);
                start = pen;
                PathSegment::MoveTo(pen)
            }
            Segment::LineTo(p) => {
                pen = point(p);
                PathSegment::LineTo(pen)
            }
            Segment::QuadTo(control, p) => {
                let (control, end) = (point(control), point(p));
                let toward = |from: Point| {
                    Point::new(
                        from.x + (control.x - from.x) * 2.0 / 3.0,
                        from.y + (control.y - from.y) * 2.0 / 3.0,
                    )
                };
                let curve = PathSegment::CubicTo(toward(pen), toward(end), end);
                pen = end;
                curve
            }
            Segment::CubicTo(a, b, p) => {
                pen = point(p);
                PathSegment::CubicTo(point(a), point(b), pen)
            }
            Segment::Close => {
                pen = start;
                PathSegment::Close
            }
        });
    }
    Path { segments }
}

fn blend_mode(mode: usvg::BlendMode) -> BlendMode {
    match mode {
        usvg::BlendMode::Normal => BlendMode::Normal,
        usvg::BlendMode::Multiply => BlendMode::Multiply,
        usvg::BlendMode::Screen => BlendMode::Screen,
        usvg::BlendMode::Overlay => BlendMode::Overlay,
        usvg::BlendMode::Darken => BlendMode::Darken,
        usvg::BlendMode::Lighten => BlendMode::Lighten,
        usvg::BlendMode::ColorDodge => BlendMode::ColorDodge,
        usvg::BlendMode::ColorBurn => BlendMode::ColorBurn,
        usvg::BlendMode::HardLight => BlendMode::HardLight,
        usvg::BlendMode::SoftLight => BlendMode::SoftLight,
        usvg::BlendMode::Difference => BlendMode::Difference,
        usvg::BlendMode::Exclusion => BlendMode::Exclusion,
        usvg::BlendMode::Hue => BlendMode::Hue,
        usvg::BlendMode::Saturation => BlendMode::Saturation,
        usvg::BlendMode::Color => BlendMode::Color,
        usvg::BlendMode::Luminosity => BlendMode::Luminosity,
        usvg::BlendMode::PlusLighter => BlendMode::PlusLighter,
    }
}
