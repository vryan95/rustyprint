//! Color glyphs (CSS Fonts 4 § 9): `COLR` layers and gradients, and bitmap glyphs, drawn as
//! display items in the colors of the font's palette.
//!
//! A run with color glyphs is painted as invisible text, so it can still be searched and
//! copied, with its plain glyphs painted as text and its color glyphs as shapes and images
//! on top.

use std::sync::Arc;

use skrifa::color::{Brush, ColorPainter, CompositeMode, Transform};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::types::BoundingBox;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::display::{
    BlendMode, Color, DisplayItem, FillRule, Group, IDENTITY, Matrix, Path, PathSegment, Point,
    Rect, Shading, ShadingKind, TextRun, multiply,
};
use crate::images::Image;

/// Paints a run that has color glyphs, and returns `false`, painting nothing, if it has
/// none.
pub(crate) fn paint_run(run: &TextRun, items: &mut Vec<DisplayItem>) -> bool {
    let font = &run.font;
    if !font.has_color() {
        return false;
    }
    let color: Vec<bool> = run
        .glyphs
        .iter()
        .map(|glyph| font.is_color_glyph(glyph.id))
        .collect();
    if !color.contains(&true) {
        return false;
    }
    let mut hidden = run.clone();
    hidden.fill = false;
    hidden.stroke = None;
    hidden.embolden = 0.0;
    items.push(DisplayItem::Text(hidden));

    let Ok(font_ref) = FontRef::from_index(font.data(), font.index()) else {
        return true;
    };
    let location = font_ref.axes().location(
        font.instance()
            .variations
            .iter()
            .map(|(tag, value)| (skrifa::Tag::new(tag), *value)),
    );
    let scale = font.em_scale(run.size);
    let skew = run.oblique.to_radians().tan();
    let palette = font.palette();
    let mut pen = run.origin.x;
    let mut plain: Option<TextRun> = None;
    for (glyph, is_color) in run.glyphs.iter().zip(&color) {
        let (x, y) = (pen + glyph.x_offset, run.origin.y + glyph.y_offset);
        if *is_color {
            if let Some(segment) = plain.take() {
                items.push(DisplayItem::Text(segment));
            }
            let mut painter = Painter {
                outlines: font_ref.outline_glyphs(),
                location: LocationRef::from(&location),
                palette: &palette,
                foreground: run.color,
                clips: Vec::new(),
                layers: Vec::new(),
                items: Vec::new(),
            };
            let id = GlyphId::new(u32::from(glyph.id));
            let painted = font_ref
                .color_glyphs()
                .get(id)
                .is_some_and(|colored| colored.paint(&location, &mut painter).is_ok());
            if painted {
                // Font units point up; the slant leans the tops of the glyphs right.
                items.push(DisplayItem::PushTransform([
                    scale,
                    0.0,
                    skew * scale,
                    -scale,
                    x,
                    y,
                ]));
                items.extend(painter.items);
                items.push(DisplayItem::PopClip);
            } else {
                paint_bitmap(run, glyph.id, x, y, items);
            }
        } else {
            match &mut plain {
                Some(segment) => segment.glyphs.push(glyph.clone()),
                None => {
                    let mut segment = run.clone();
                    segment.origin = Point::new(pen, run.origin.y);
                    segment.glyphs = vec![glyph.clone()];
                    plain = Some(segment);
                }
            }
        }
        pen += glyph.x_advance;
    }
    if let Some(segment) = plain {
        items.push(DisplayItem::Text(segment));
    }
    true
}

/// Paints a bitmap glyph (`sbix`, `CBDT`) whose origin is at `(x, y)`.
fn paint_bitmap(run: &TextRun, id: u16, x: f32, y: f32, items: &mut Vec<DisplayItem>) {
    let Some(bitmap) = run.font.bitmap(id) else {
        return;
    };
    if bitmap.pixels_per_em <= 0.0 {
        return;
    }
    let Ok(image) = Image::decode(bitmap.png) else {
        return;
    };
    // Strike pixels to CSS pixels.
    let scale = run.size * run.font.size_adjust() / bitmap.pixels_per_em;
    let width = bitmap.width as f32 * scale;
    let height = bitmap.height as f32 * scale;
    let left = x + bitmap.x * scale;
    let bottom = y - bitmap.y * scale;
    items.push(DisplayItem::Image {
        image: Arc::new(image),
        rect: Rect::new(left, bottom - height, width, height),
        smooth: true,
        orientation: 1,
        alpha: run.color.a,
    });
}

/// Turns `COLR` paint operations into display items in font units.
struct Painter<'a> {
    outlines: skrifa::outline::OutlineGlyphCollection<'a>,
    location: LocationRef<'a>,
    palette: &'a [Color],
    /// The text color, for palette index `0xFFFF`.
    foreground: Color,
    /// The bounds of the clips in effect, innermost last.
    clips: Vec<Rect>,
    /// The layers that are open, innermost last.
    layers: Vec<Layer>,
    items: Vec<DisplayItem>,
}

/// A layer that is open: a group of items composited as one.
struct Layer {
    /// Where the layer's group starts among the items.
    start: usize,
    /// Whether the layer is kept only where the items before it in the layer around it
    /// are: the source of source-in compositing, over its backdrop.
    source_in: bool,
}

impl Painter<'_> {
    fn color(&self, index: u16, alpha: f32) -> Color {
        let base = if index == 0xFFFF {
            self.foreground
        } else {
            self.palette
                .get(usize::from(index))
                .copied()
                .unwrap_or(Color::BLACK)
        };
        let a = (f32::from(base.a) * alpha.clamp(0.0, 1.0)).round() as u8;
        Color::rgba(base.r, base.g, base.b, a)
    }

    /// What a fill covers: the innermost clip.
    fn area(&self) -> Rect {
        self.clips
            .last()
            .copied()
            .unwrap_or(Rect::new(-1000.0, -1000.0, 4000.0, 4000.0))
    }

    fn stops(&self, stops: &[skrifa::color::ColorStop]) -> Vec<(f32, Color)> {
        stops
            .iter()
            .map(|s| {
                (
                    s.offset.clamp(0.0, 1.0),
                    self.color(s.palette_index, s.alpha),
                )
            })
            .collect()
    }
}

impl ColorPainter for Painter<'_> {
    fn push_transform(&mut self, t: Transform) {
        self.items.push(DisplayItem::PushTransform([
            t.xx, t.yx, t.xy, t.yy, t.dx, t.dy,
        ]));
        self.clips.push(self.area());
    }

    fn pop_transform(&mut self) {
        self.items.push(DisplayItem::PopClip);
        self.clips.pop();
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        let mut pen = Pen::default();
        if let Some(outline) = self.outlines.get(glyph_id) {
            let _ = outline.draw(
                DrawSettings::unhinted(Size::unscaled(), self.location),
                &mut pen,
            );
        }
        let bounds = pen.path.bounds().unwrap_or_default();
        self.items.push(DisplayItem::PushClipPath {
            path: pen.path,
            rule: FillRule::NonZero,
        });
        self.clips.push(bounds);
    }

    fn push_clip_box(&mut self, b: BoundingBox<f32>) {
        let rect = Rect::new(b.x_min, b.y_min, b.x_max - b.x_min, b.y_max - b.y_min);
        self.items.push(DisplayItem::PushClip(rect));
        self.clips.push(rect);
    }

    fn pop_clip(&mut self) {
        self.items.push(DisplayItem::PopClip);
        self.clips.pop();
    }

    fn fill(&mut self, brush: Brush<'_>) {
        let rect = self.area();
        let item = match brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => DisplayItem::Rect {
                rect,
                color: self.color(palette_index, alpha),
            },
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                ..
            } => DisplayItem::Gradient {
                rect,
                shading: Arc::new(Shading {
                    kind: ShadingKind::Axial {
                        start: Point::new(p0.x, p0.y),
                        end: Point::new(p1.x, p1.y),
                    },
                    stops: self.stops(color_stops),
                }),
            },
            // Two circles with different centers are drawn around the second center.
            Brush::RadialGradient {
                c1,
                r0,
                r1,
                color_stops,
                ..
            } => DisplayItem::Gradient {
                rect,
                shading: Arc::new(Shading {
                    kind: ShadingKind::Radial {
                        center: Point::new(c1.x, c1.y),
                        r0: r0.max(0.0),
                        r1: r1.max(0.0),
                        scale_y: 1.0,
                    },
                    stops: self.stops(color_stops),
                }),
            },
            // Sweep gradients are drawn in their middle color.
            Brush::SweepGradient { color_stops, .. } => {
                let stops = self.stops(color_stops);
                let color = stops
                    .get(stops.len() / 2)
                    .map_or(self.foreground, |(_, c)| *c);
                DisplayItem::Rect { rect, color }
            }
        };
        self.items.push(item);
    }

    fn push_layer(&mut self, mode: CompositeMode) {
        let blend = match mode {
            CompositeMode::Multiply => BlendMode::Multiply,
            CompositeMode::Screen => BlendMode::Screen,
            CompositeMode::Overlay => BlendMode::Overlay,
            CompositeMode::Darken => BlendMode::Darken,
            CompositeMode::Lighten => BlendMode::Lighten,
            CompositeMode::ColorDodge => BlendMode::ColorDodge,
            CompositeMode::ColorBurn => BlendMode::ColorBurn,
            CompositeMode::HardLight => BlendMode::HardLight,
            CompositeMode::SoftLight => BlendMode::SoftLight,
            CompositeMode::Difference => BlendMode::Difference,
            CompositeMode::Exclusion => BlendMode::Exclusion,
            CompositeMode::HslHue => BlendMode::Hue,
            CompositeMode::HslSaturation => BlendMode::Saturation,
            CompositeMode::HslColor => BlendMode::Color,
            CompositeMode::HslLuminosity => BlendMode::Luminosity,
            CompositeMode::Plus => BlendMode::PlusLighter,
            // Source-in is composited when its layer ends. The other Porter-Duff operators
            // are drawn as source-over.
            _ => BlendMode::Normal,
        };
        self.layers.push(Layer {
            start: self.items.len(),
            source_in: mode == CompositeMode::SrcIn,
        });
        self.items.push(DisplayItem::PushGroup(Group {
            blend,
            ..Group::ISOLATED
        }));
    }

    fn pop_layer(&mut self) {
        if let Some(layer) = self.layers.pop()
            && layer.source_in
            && let Some(around) = self.layers.last()
        {
            // Source-in keeps the source where the backdrop is, and nothing of the backdrop
            // itself. The backdrop is what the layer around this one holds so far: it gives
            // way to the source, clipped to the shapes the backdrop fills.
            let source: Vec<DisplayItem> = self.items.drain(layer.start + 1..).collect();
            let backdrop: Vec<DisplayItem> =
                self.items.drain(around.start + 1..layer.start).collect();
            let shapes = filled_shapes(&backdrop);
            if !shapes.segments.is_empty() {
                self.items.push(DisplayItem::PushClipPath {
                    path: shapes,
                    rule: FillRule::NonZero,
                });
                self.items.extend(source);
                self.items.push(DisplayItem::PopClip);
            }
        }
        self.items.push(DisplayItem::PopGroup);
    }
}

/// The shapes that `items` fill, as one path whose inside by the nonzero rule is what any
/// of them covers. A fill covers the clip it is in; one inside several clips counts as
/// covering the innermost, and one inside none covers everything.
fn filled_shapes(items: &[DisplayItem]) -> Path {
    /// What a clip or transform that is open does.
    enum Open {
        Clip(Path),
        Transform(Matrix),
    }
    let mut open: Vec<Open> = Vec::new();
    let mut shapes = Path::default();
    for item in items {
        // The transform in effect, from the coordinates of `items`.
        let matrix = open.iter().fold(IDENTITY, |matrix, open| match open {
            Open::Transform(t) => multiply(matrix, *t),
            Open::Clip(_) => matrix,
        });
        match item {
            DisplayItem::PushTransform(t) => open.push(Open::Transform(*t)),
            DisplayItem::PushClipPath { path, .. } => {
                open.push(Open::Clip(outward(path.transformed(matrix))));
            }
            DisplayItem::PushClip(rect) => {
                let corners = [
                    Point::new(rect.x, rect.y),
                    Point::new(rect.right(), rect.y),
                    Point::new(rect.right(), rect.bottom()),
                    Point::new(rect.x, rect.bottom()),
                ];
                let path = Path::polygon(&corners).transformed(matrix);
                open.push(Open::Clip(outward(path)));
            }
            DisplayItem::PushClipText(_) => open.push(Open::Transform(IDENTITY)),
            DisplayItem::PopClip => {
                open.pop();
            }
            DisplayItem::Rect { rect, .. } | DisplayItem::Gradient { rect, .. } => {
                let clip = open.iter().rev().find_map(|open| match open {
                    Open::Clip(path) => Some(path.clone()),
                    Open::Transform(_) => None,
                });
                shapes.extend(clip.unwrap_or_else(|| {
                    let corners = [
                        Point::new(rect.x, rect.y),
                        Point::new(rect.right(), rect.y),
                        Point::new(rect.right(), rect.bottom()),
                        Point::new(rect.x, rect.bottom()),
                    ];
                    outward(Path::polygon(&corners).transformed(matrix))
                }));
            }
            _ => {}
        }
    }
    shapes
}

/// Twice the area a path encloses, counted with the direction of its contours, taking its
/// curves as the polygons of their control points.
fn signed_area(path: &Path) -> f32 {
    let mut area = 0.0;
    let mut start = Point::new(0.0, 0.0);
    let mut last = start;
    let mut edge = |from: Point, to: Point| area += from.x * to.y - to.x * from.y;
    for segment in &path.segments {
        match *segment {
            PathSegment::MoveTo(p) => {
                edge(last, start);
                (start, last) = (p, p);
            }
            PathSegment::LineTo(p) => {
                edge(last, p);
                last = p;
            }
            PathSegment::CubicTo(a, b, c) => {
                edge(last, a);
                edge(a, b);
                edge(b, c);
                last = c;
            }
            PathSegment::Close => {
                edge(last, start);
                last = start;
            }
        }
    }
    edge(last, start);
    area
}

/// The path with its contours turned so that it encloses a positive area: paths turned the
/// same way add up under the nonzero rule, where one turned the other way would cut a hole
/// in the rest.
fn outward(path: Path) -> Path {
    if signed_area(&path) >= 0.0 {
        return path;
    }
    let mut reversed = Path::default();
    let mut contour: Vec<PathSegment> = Vec::new();
    let mut flush = |contour: &mut Vec<PathSegment>, closed: bool| {
        let Some(PathSegment::MoveTo(first)) = contour.first().copied() else {
            contour.clear();
            return;
        };
        // The end points of the segments, and each segment drawn back to the one before.
        let ends: Vec<Point> = contour
            .iter()
            .map(|segment| match *segment {
                PathSegment::MoveTo(p) | PathSegment::LineTo(p) | PathSegment::CubicTo(_, _, p) => {
                    p
                }
                PathSegment::Close => first,
            })
            .collect();
        reversed
            .segments
            .push(PathSegment::MoveTo(ends[ends.len() - 1]));
        for index in (1..contour.len()).rev() {
            let to = ends[index - 1];
            reversed.segments.push(match contour[index] {
                PathSegment::CubicTo(a, b, _) => PathSegment::CubicTo(b, a, to),
                _ => PathSegment::LineTo(to),
            });
        }
        if closed {
            reversed.segments.push(PathSegment::Close);
        }
        contour.clear();
    };
    for segment in path.segments {
        match segment {
            PathSegment::MoveTo(_) => {
                flush(&mut contour, false);
                contour.push(segment);
            }
            PathSegment::Close => flush(&mut contour, true),
            _ => contour.push(segment),
        }
    }
    flush(&mut contour, false);
    reversed
}

/// Collects a glyph outline as a path, with quadratic curves made cubic.
#[derive(Default)]
struct Pen {
    path: Path,
    last: Point,
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.last = Point::new(x, y);
        self.path.segments.push(PathSegment::MoveTo(self.last));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.last = Point::new(x, y);
        self.path.segments.push(PathSegment::LineTo(self.last));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let p0 = self.last;
        let c1 = Point::new(
            p0.x + 2.0 / 3.0 * (cx - p0.x),
            p0.y + 2.0 / 3.0 * (cy - p0.y),
        );
        let c2 = Point::new(x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
        self.last = Point::new(x, y);
        self.path
            .segments
            .push(PathSegment::CubicTo(c1, c2, self.last));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.last = Point::new(x, y);
        self.path.segments.push(PathSegment::CubicTo(
            Point::new(cx0, cy0),
            Point::new(cx1, cy1),
            self.last,
        ));
    }

    fn close(&mut self) {
        self.path.segments.push(PathSegment::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `paint` makes of a glyph's paint operations.
    fn painted(paint: impl FnOnce(&mut Painter)) -> Vec<DisplayItem> {
        let data = include_bytes!("../../../fonts/NotoSans-Regular.ttf");
        let font = FontRef::from_index(data, 0).unwrap();
        let mut painter = Painter {
            outlines: font.outline_glyphs(),
            location: LocationRef::default(),
            palette: &[Color::rgba(255, 0, 0, 255)],
            foreground: Color::BLACK,
            clips: Vec::new(),
            layers: Vec::new(),
            items: Vec::new(),
        };
        paint(&mut painter);
        assert!(painter.layers.is_empty() && painter.clips.is_empty());
        painter.items
    }

    const SOLID: Brush<'static> = Brush::Solid {
        palette_index: 0,
        alpha: 1.0,
    };

    fn clip_box(x: f32, y: f32, width: f32, height: f32) -> BoundingBox<f32> {
        BoundingBox {
            x_min: x,
            y_min: y,
            x_max: x + width,
            y_max: y + height,
        }
    }

    /// A box filled in the palette's color.
    fn fill_box(painter: &mut Painter, x: f32, y: f32, width: f32, height: f32) {
        painter.push_clip_box(clip_box(x, y, width, height));
        painter.fill(SOLID);
        painter.pop_clip();
    }

    fn square(x: f32, y: f32, side: f32) -> Path {
        Path::polygon(&[
            Point::new(x, y),
            Point::new(x + side, y),
            Point::new(x + side, y + side),
            Point::new(x, y + side),
        ])
    }

    #[test]
    fn source_in_keeps_the_source_only_where_the_backdrop_is() {
        // Two boxes are the backdrop, and a fill of everything the source: what is left is
        // the source, clipped to the two boxes, and nothing of the boxes themselves.
        let items = painted(|painter| {
            painter.push_layer(CompositeMode::SrcOver);
            fill_box(painter, 0.0, 0.0, 10.0, 10.0);
            fill_box(painter, 20.0, 0.0, 10.0, 10.0);
            painter.push_layer(CompositeMode::SrcIn);
            painter.fill(SOLID);
            painter.pop_layer();
            painter.pop_layer();
        });
        let mut shapes = square(0.0, 0.0, 10.0);
        shapes.extend(square(20.0, 0.0, 10.0));
        let [
            DisplayItem::PushGroup(_),
            DisplayItem::PushGroup(_),
            DisplayItem::PushClipPath { path, rule },
            DisplayItem::Rect { .. },
            DisplayItem::PopClip,
            DisplayItem::PopGroup,
            DisplayItem::PopGroup,
        ] = &items[..]
        else {
            panic!("{items:?}");
        };
        assert_eq!((path, *rule), (&shapes, FillRule::NonZero));
    }

    #[test]
    fn source_in_over_an_empty_backdrop_paints_nothing() {
        let items = painted(|painter| {
            painter.push_layer(CompositeMode::SrcOver);
            painter.push_layer(CompositeMode::SrcIn);
            painter.fill(SOLID);
            painter.pop_layer();
            painter.pop_layer();
        });
        assert!(
            !items
                .iter()
                .any(|item| matches!(item, DisplayItem::Rect { .. })),
            "{items:?}"
        );
    }

    #[test]
    fn blended_layers_keep_their_backdrop() {
        // A blend mode draws the source over the backdrop, which stays.
        let items = painted(|painter| {
            painter.push_layer(CompositeMode::SrcOver);
            fill_box(painter, 0.0, 0.0, 10.0, 10.0);
            painter.push_layer(CompositeMode::SoftLight);
            painter.fill(SOLID);
            painter.pop_layer();
            painter.pop_layer();
        });
        let fills = items
            .iter()
            .filter(|item| matches!(item, DisplayItem::Rect { .. }))
            .count();
        assert_eq!(fills, 2, "{items:?}");
        assert!(
            !items
                .iter()
                .any(|item| matches!(item, DisplayItem::PushClipPath { .. })),
            "{items:?}"
        );
    }

    #[test]
    fn filled_shapes_are_in_the_coordinates_of_the_items() {
        // A fill inside a transform covers its clip as the transform places it, and one
        // inside two clips counts as covering the inner one.
        let items = painted(|painter| {
            painter.push_transform(Transform {
                xx: 2.0,
                yx: 0.0,
                xy: 0.0,
                yy: 2.0,
                dx: 100.0,
                dy: 0.0,
            });
            painter.push_clip_box(clip_box(0.0, 0.0, 50.0, 50.0));
            fill_box(painter, 0.0, 0.0, 10.0, 10.0);
            painter.pop_clip();
            painter.pop_transform();
        });
        assert_eq!(filled_shapes(&items), square(100.0, 0.0, 20.0));
    }

    #[test]
    fn shapes_turned_the_other_way_are_turned_back() {
        // Mirrored, a shape's contours run the other way: added to another shape as it is,
        // the nonzero rule would leave a hole where the two overlap.
        let items = painted(|painter| {
            fill_box(painter, 0.0, 0.0, 10.0, 10.0);
            painter.push_transform(Transform {
                xx: -1.0,
                yx: 0.0,
                xy: 0.0,
                yy: 1.0,
                dx: 15.0,
                dy: 0.0,
            });
            fill_box(painter, 0.0, 0.0, 10.0, 10.0);
            painter.pop_transform();
        });
        let shapes = filled_shapes(&items);
        assert_eq!(signed_area(&shapes), 2.0 * (100.0 + 100.0));
        // A curve keeps its shape when its contour is turned.
        let curve = Path {
            segments: vec![
                PathSegment::MoveTo(Point::new(0.0, 0.0)),
                PathSegment::LineTo(Point::new(0.0, 10.0)),
                PathSegment::CubicTo(
                    Point::new(4.0, 10.0),
                    Point::new(10.0, 6.0),
                    Point::new(10.0, 0.0),
                ),
                PathSegment::Close,
            ],
        };
        assert!(signed_area(&curve) < 0.0);
        let turned = outward(curve.clone());
        assert_eq!(signed_area(&turned), -signed_area(&curve));
        assert_eq!(
            turned.segments,
            [
                PathSegment::MoveTo(Point::new(10.0, 0.0)),
                PathSegment::CubicTo(
                    Point::new(10.0, 6.0),
                    Point::new(4.0, 10.0),
                    Point::new(0.0, 10.0),
                ),
                PathSegment::LineTo(Point::new(0.0, 0.0)),
                PathSegment::Close,
            ]
        );
    }
}
