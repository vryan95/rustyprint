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
    BlendMode, Color, DisplayItem, FillRule, Group, Path, PathSegment, Point, Rect, Shading,
    ShadingKind, TextRun,
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
    items: Vec<DisplayItem>,
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
            // Porter-Duff operators other than source-over are drawn as source-over.
            _ => BlendMode::Normal,
        };
        self.items.push(DisplayItem::PushGroup(Group {
            blend,
            ..Group::ISOLATED
        }));
    }

    fn pop_layer(&mut self) {
        self.items.push(DisplayItem::PopGroup);
    }
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
