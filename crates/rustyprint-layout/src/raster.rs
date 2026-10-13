//! Rasterising display items with `tiny-skia`, for what PDF cannot draw as vectors: blurs,
//! masks and the backdrops of `backdrop-filter`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use tiny_skia as sk;

use crate::display::{
    BlendMode, Color, DisplayItem, FillRule, Group, LineStyle, Matrix, Path, PathSegment, Point,
    Rect, Shading, ShadingKind, TextRun, multiply,
};
use crate::images::{Image, ImageData};

/// Pixels per CSS pixel that effects are rasterised at: 192 dpi.
pub(crate) const SCALE: f32 = 2.0;

/// The most pixels one raster may have. Larger areas are rasterised at a lower resolution.
pub(crate) const MAX_PIXELS: f32 = 16_000_000.0;

/// Premultiplied RGBA pixels covering `rect` at `scale` pixels per CSS pixel.
pub(crate) struct Raster {
    pub pixmap: sk::Pixmap,
    pub rect: Rect,
    pub scale: f32,
}

impl Raster {
    /// A transparent raster of `rect`, or `None` if it is empty.
    pub fn new(rect: Rect) -> Option<Raster> {
        if !(rect.width > 0.0 && rect.height > 0.0) || !rect.width.is_finite() {
            return None;
        }
        let area = rect.width * rect.height;
        let scale = SCALE.min((MAX_PIXELS / area).sqrt());
        let width = (rect.width * scale).ceil().max(1.0) as u32;
        let height = (rect.height * scale).ceil().max(1.0) as u32;
        let pixmap = sk::Pixmap::new(width, height)?;
        Some(Raster {
            pixmap,
            rect,
            scale,
        })
    }

    /// The transformation from CSS pixels to the raster's pixels.
    pub fn matrix(&self) -> Matrix {
        let s = self.scale;
        [s, 0.0, 0.0, s, -self.rect.x * s, -self.rect.y * s]
    }

    /// The raster as an image with straight alpha, to draw over `self.rect`.
    pub fn to_image(&self) -> Image {
        let (width, height) = (self.pixmap.width(), self.pixmap.height());
        let mut samples = Vec::with_capacity((width * height * 3) as usize);
        let mut alpha = Vec::with_capacity((width * height) as usize);
        for pixel in self.pixmap.pixels() {
            let c = pixel.demultiply();
            samples.extend_from_slice(&[c.red(), c.green(), c.blue()]);
            alpha.push(c.alpha());
        }
        let opaque = alpha.iter().all(|&a| a == 255);
        Image {
            width,
            height,
            data: ImageData::Pixels {
                samples,
                gray: false,
                alpha: (!opaque).then_some(alpha),
            },
            orientation: 1,
            resolution: None,
        }
    }

    /// The image item that draws the raster where it came from.
    pub fn to_item(&self) -> DisplayItem {
        DisplayItem::Image {
            image: Arc::new(self.to_image()),
            rect: Rect::new(
                self.rect.x,
                self.rect.y,
                self.pixmap.width() as f32 / self.scale,
                self.pixmap.height() as f32 / self.scale,
            ),
            smooth: true,
            orientation: 1,
            alpha: 255,
        }
    }
}

/// Paints `items` into `raster`.
pub(crate) fn draw(items: &[DisplayItem], raster: &mut Raster) {
    let base = raster.matrix();
    let (width, height) = (raster.pixmap.width(), raster.pixmap.height());
    let mut painter = Painter {
        layers: vec![Layer {
            pixmap: std::mem::replace(&mut raster.pixmap, sk::Pixmap::new(1, 1).expect("1×1")),
            group: None,
            states: Vec::new(),
        }],
        state: State {
            matrix: base,
            clip: None,
        },
        width,
        height,
    };
    for item in items {
        painter.item(item);
    }
    // Close groups left open.
    while painter.layers.len() > 1 {
        painter.pop_group();
    }
    raster.pixmap = painter.layers.pop().expect("the base layer").pixmap;
}

/// The bounds of what `items` paint, with transforms applied and clips ignored.
pub(crate) fn bounds(items: &[DisplayItem]) -> Option<Rect> {
    let mut matrices: Vec<Matrix> = vec![crate::display::IDENTITY];
    let mut out: Option<Rect> = None;
    for item in items {
        let current = *matrices.last().expect("never empty");
        match item {
            DisplayItem::PushTransform(m) => matrices.push(multiply(current, *m)),
            DisplayItem::PushClip(_)
            | DisplayItem::PushClipPath { .. }
            | DisplayItem::PushClipText(_) => matrices.push(current),
            DisplayItem::PopClip => {
                if matrices.len() > 1 {
                    matrices.pop();
                }
            }
            _ => {
                if let Some(rect) = item.bounds() {
                    let rect = crate::display::transform_rect(current, rect);
                    out = Some(match out {
                        Some(o) => union(o, rect),
                        None => rect,
                    });
                }
            }
        }
    }
    out
}

pub(crate) fn union(a: Rect, b: Rect) -> Rect {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = (a.right().max(b.right()), a.bottom().max(b.bottom()));
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

pub(crate) fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = (a.right().min(b.right()), a.bottom().min(b.bottom()));
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// Blurs premultiplied pixels with a Gaussian of standard deviation `sigma` pixels,
/// approximated by three box blurs in each direction.
pub(crate) fn blur(pixmap: &mut sk::Pixmap, sigma: f32) {
    if sigma <= 0.0 {
        return;
    }
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let boxes = box_sizes(sigma);
    let data = pixmap.data_mut();
    let mut buffer: Vec<f32> = data.iter().map(|&v| f32::from(v)).collect();
    let mut scratch = vec![0.0f32; buffer.len()];
    for radius in boxes {
        box_blur(&buffer, &mut scratch, width, height, radius, true);
        box_blur(&scratch, &mut buffer, width, height, radius, false);
    }
    for (out, value) in data.iter_mut().zip(&buffer) {
        *out = value.round().clamp(0.0, 255.0) as u8;
    }
    // Rounding may leave a channel above its alpha, which premultiplied pixels cannot have.
    for pixel in data.as_chunks_mut::<4>().0 {
        let a = pixel[3];
        for c in &mut pixel[..3] {
            *c = (*c).min(a);
        }
    }
}

/// The radii of three box blurs that together approximate a Gaussian (W3C Filter Effects
/// § 15.5.1 uses three boxes of size `d`).
fn box_sizes(sigma: f32) -> [usize; 3] {
    let d = (sigma * 3.0 * (2.0 * std::f32::consts::PI).sqrt() / 4.0 + 0.5).floor() as usize;
    let r = d / 2;
    [r.max(1), r.max(1), r.max(1)]
}

/// One box blur pass of `radius` pixels along rows (`horizontal`) or columns.
fn box_blur(
    from: &[f32],
    to: &mut [f32],
    width: usize,
    height: usize,
    radius: usize,
    horizontal: bool,
) {
    let (lines, length) = if horizontal {
        (height, width)
    } else {
        (width, height)
    };
    let index = |line: usize, i: usize| {
        if horizontal {
            (line * width + i) * 4
        } else {
            (i * width + line) * 4
        }
    };
    let size = (2 * radius + 1) as f32;
    for line in 0..lines {
        for channel in 0..4 {
            let at = |i: isize| -> f32 {
                if i < 0 || i >= length as isize {
                    0.0
                } else {
                    from[index(line, i as usize) + channel]
                }
            };
            let mut sum: f32 = (-(radius as isize)..=radius as isize).map(at).sum();
            for i in 0..length {
                to[index(line, i) + channel] = sum / size;
                sum += at(i as isize + radius as isize + 1) - at(i as isize - radius as isize);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Painting

#[derive(Clone)]
struct State {
    /// CSS pixels to raster pixels.
    matrix: Matrix,
    clip: Option<Arc<sk::Mask>>,
}

struct Layer {
    pixmap: sk::Pixmap,
    /// The group this layer paints, for all but the base layer.
    group: Option<(Group, State)>,
    /// States saved by the clips and transforms pushed in this layer.
    states: Vec<State>,
}

struct Painter {
    layers: Vec<Layer>,
    state: State,
    width: u32,
    height: u32,
}

fn transform(m: Matrix) -> sk::Transform {
    sk::Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5])
}

fn color(c: Color) -> sk::Color {
    sk::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn fill_rule(rule: FillRule) -> sk::FillRule {
    match rule {
        FillRule::NonZero => sk::FillRule::Winding,
        FillRule::EvenOdd => sk::FillRule::EvenOdd,
    }
}

pub(crate) fn blend_mode(mode: BlendMode) -> sk::BlendMode {
    match mode {
        BlendMode::Normal => sk::BlendMode::SourceOver,
        BlendMode::Multiply | BlendMode::PlusDarker => sk::BlendMode::Multiply,
        BlendMode::Screen => sk::BlendMode::Screen,
        BlendMode::Overlay => sk::BlendMode::Overlay,
        BlendMode::Darken => sk::BlendMode::Darken,
        BlendMode::Lighten => sk::BlendMode::Lighten,
        BlendMode::ColorDodge => sk::BlendMode::ColorDodge,
        BlendMode::ColorBurn => sk::BlendMode::ColorBurn,
        BlendMode::HardLight => sk::BlendMode::HardLight,
        BlendMode::SoftLight => sk::BlendMode::SoftLight,
        BlendMode::Difference => sk::BlendMode::Difference,
        BlendMode::Exclusion => sk::BlendMode::Exclusion,
        BlendMode::Hue => sk::BlendMode::Hue,
        BlendMode::Saturation => sk::BlendMode::Saturation,
        BlendMode::Color => sk::BlendMode::Color,
        BlendMode::Luminosity => sk::BlendMode::Luminosity,
        BlendMode::PlusLighter => sk::BlendMode::Plus,
    }
}

/// A display path as a `tiny-skia` path.
pub(crate) fn sk_path(path: &Path) -> Option<sk::Path> {
    let mut builder = sk::PathBuilder::new();
    for segment in &path.segments {
        match *segment {
            PathSegment::MoveTo(p) => builder.move_to(p.x, p.y),
            PathSegment::LineTo(p) => builder.line_to(p.x, p.y),
            PathSegment::CubicTo(a, b, c) => builder.cubic_to(a.x, a.y, b.x, b.y, c.x, c.y),
            PathSegment::Close => builder.close(),
        }
    }
    builder.finish()
}

fn polygon(points: &[Point], close: bool) -> Option<sk::Path> {
    let mut builder = sk::PathBuilder::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            builder.move_to(p.x, p.y);
        } else {
            builder.line_to(p.x, p.y);
        }
    }
    if close {
        builder.close();
    }
    builder.finish()
}

fn rect_path(rect: Rect) -> Option<sk::Path> {
    sk::Rect::from_xywh(rect.x, rect.y, rect.width, rect.height).map(sk::PathBuilder::from_rect)
}

impl Painter {
    fn pixmap(&mut self) -> &mut sk::Pixmap {
        &mut self.layers.last_mut().expect("never empty").pixmap
    }

    fn save(&mut self) {
        let state = self.state.clone();
        self.layers
            .last_mut()
            .expect("never empty")
            .states
            .push(state);
    }

    fn restore(&mut self) {
        if let Some(state) = self.layers.last_mut().expect("never empty").states.pop() {
            self.state = state;
        }
    }

    /// Narrows the clip to `path` in CSS pixels.
    fn clip(&mut self, path: Option<sk::Path>, rule: sk::FillRule) {
        let mut mask = match &self.state.clip {
            Some(mask) => (**mask).clone(),
            None => {
                let mut mask = sk::Mask::new(self.width, self.height).expect("non-empty");
                mask.data_mut().fill(255);
                mask
            }
        };
        match path {
            Some(path) => {
                mask.intersect_path(&path, rule, true, transform(self.state.matrix));
            }
            None => mask.clear(),
        }
        self.state.clip = Some(Arc::new(mask));
    }

    fn paint(&self, c: Color) -> sk::Paint<'static> {
        let mut paint = sk::Paint::default();
        paint.set_color(color(c));
        paint.anti_alias = true;
        paint
    }

    fn fill(&mut self, path: Option<sk::Path>, c: Color, rule: sk::FillRule) {
        let Some(path) = path else { return };
        let paint = self.paint(c);
        let m = transform(self.state.matrix);
        let clip = self.state.clip.clone();
        self.pixmap()
            .fill_path(&path, &paint, rule, m, clip.as_deref());
    }

    fn stroke(&mut self, path: Option<sk::Path>, c: Color, width: f32, style: LineStyle) {
        let Some(path) = path else { return };
        let paint = self.paint(c);
        let mut stroke = sk::Stroke {
            width,
            line_join: sk::LineJoin::Round,
            ..Default::default()
        };
        match style {
            LineStyle::Solid => {}
            LineStyle::Dashed => stroke.dash = sk::StrokeDash::new(vec![3.0 * width; 2], 0.0),
            LineStyle::Dotted => {
                stroke.line_cap = sk::LineCap::Round;
                stroke.dash = sk::StrokeDash::new(vec![0.0, 2.0 * width], 0.0);
            }
        }
        let m = transform(self.state.matrix);
        let clip = self.state.clip.clone();
        self.pixmap()
            .stroke_path(&path, &paint, &stroke, m, clip.as_deref());
    }

    fn item(&mut self, item: &DisplayItem) {
        match item {
            DisplayItem::Text(run) => self.text(run),
            DisplayItem::Rect { rect, color } => {
                self.fill(rect_path(*rect), *color, sk::FillRule::Winding)
            }
            DisplayItem::Polygon { points, color } => {
                self.fill(polygon(points, true), *color, sk::FillRule::Winding)
            }
            DisplayItem::Polyline {
                points,
                width,
                color,
                closed,
            } => self.stroke(polygon(points, *closed), *color, *width, LineStyle::Solid),
            DisplayItem::Line {
                from,
                to,
                width,
                color,
                style,
            } => self.stroke(polygon(&[*from, *to], false), *color, *width, *style),
            DisplayItem::Path { path, color, rule } => {
                self.fill(sk_path(path), *color, fill_rule(*rule))
            }
            DisplayItem::StrokePath {
                path,
                width,
                color,
                style,
            } => self.stroke(sk_path(path), *color, *width, *style),
            DisplayItem::Image {
                image,
                rect,
                smooth,
                orientation,
                alpha,
            } => self.image(image, *rect, *smooth, *orientation, *alpha),
            DisplayItem::Gradient { rect, shading } => self.gradient(*rect, shading),
            DisplayItem::PushClip(rect) => {
                self.save();
                self.clip(rect_path(*rect), sk::FillRule::Winding);
            }
            DisplayItem::PushClipPath { path, rule } => {
                self.save();
                self.clip(sk_path(path), fill_rule(*rule));
            }
            DisplayItem::PushClipText(runs) => {
                self.save();
                let mut builder = sk::PathBuilder::new();
                for run in runs {
                    glyph_paths(run, &mut builder);
                }
                self.clip(builder.finish(), sk::FillRule::Winding);
            }
            DisplayItem::PushTransform(m) => {
                self.save();
                self.state.matrix = multiply(self.state.matrix, *m);
            }
            DisplayItem::PopClip => self.restore(),
            DisplayItem::PushGroup(group) => {
                let pixmap = sk::Pixmap::new(self.width, self.height).expect("non-empty");
                self.layers.push(Layer {
                    pixmap,
                    group: Some((group.clone(), self.state.clone())),
                    states: Vec::new(),
                });
            }
            DisplayItem::PopGroup => {
                if self.layers.len() > 1 {
                    self.pop_group();
                }
            }
            DisplayItem::Backdrop(_) => {}
        }
    }

    /// Composites the top layer onto the one below it.
    fn pop_group(&mut self) {
        let layer = self.layers.pop().expect("a group layer");
        let (group, state) = layer.group.expect("only the base layer has no group");
        self.state = state;
        let mut mask = self.state.clip.as_deref().cloned();
        if let Some(soft) = &group.mask {
            let mut pixmap = sk::Pixmap::new(self.width, self.height).expect("non-empty");
            let source = pixmap_of(&soft.image);
            let r = soft.rect;
            let m = multiply(
                self.state.matrix,
                [
                    r.width / source.width() as f32,
                    0.0,
                    0.0,
                    r.height / source.height() as f32,
                    r.x,
                    r.y,
                ],
            );
            pixmap.draw_pixmap(
                0,
                0,
                sk::Pixmap::as_ref(&source),
                &sk::PixmapPaint {
                    quality: sk::FilterQuality::Bilinear,
                    ..Default::default()
                },
                transform(m),
                None,
            );
            let soft_mask = sk::Mask::from_pixmap(pixmap.as_ref(), sk::MaskType::Luminance);
            mask = Some(match mask {
                Some(mut clip) => {
                    for (a, b) in clip.data_mut().iter_mut().zip(soft_mask.data()) {
                        *a = ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8;
                    }
                    clip
                }
                None => soft_mask,
            });
        }
        let paint = sk::PixmapPaint {
            opacity: group.opacity.clamp(0.0, 1.0),
            blend_mode: blend_mode(group.blend),
            quality: sk::FilterQuality::Nearest,
        };
        self.pixmap().draw_pixmap(
            0,
            0,
            layer.pixmap.as_ref(),
            &paint,
            sk::Transform::identity(),
            mask.as_ref(),
        );
    }

    fn image(&mut self, image: &Arc<Image>, rect: Rect, smooth: bool, orientation: u8, alpha: u8) {
        let source = pixmap_of(image);
        let (w, h) = (source.width() as f32, source.height() as f32);
        // How the stored pixels' unit square maps onto the displayed one.
        let turn: Matrix = match orientation {
            2 => [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0],
            3 => [-1.0, 0.0, 0.0, -1.0, 1.0, 1.0],
            4 => [1.0, 0.0, 0.0, -1.0, 0.0, 1.0],
            5 => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
            6 => [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],
            7 => [0.0, -1.0, -1.0, 0.0, 1.0, 1.0],
            8 => [0.0, -1.0, 1.0, 0.0, 0.0, 1.0],
            _ => crate::display::IDENTITY,
        };
        let m = multiply(
            self.state.matrix,
            multiply(
                [rect.width, 0.0, 0.0, rect.height, rect.x, rect.y],
                multiply(turn, [1.0 / w, 0.0, 0.0, 1.0 / h, 0.0, 0.0]),
            ),
        );
        let paint = sk::PixmapPaint {
            opacity: f32::from(alpha) / 255.0,
            blend_mode: sk::BlendMode::SourceOver,
            quality: if smooth {
                sk::FilterQuality::Bilinear
            } else {
                sk::FilterQuality::Nearest
            },
        };
        let clip = self.state.clip.clone();
        self.pixmap().draw_pixmap(
            0,
            0,
            sk::Pixmap::as_ref(&source),
            &paint,
            transform(m),
            clip.as_deref(),
        );
    }

    fn gradient(&mut self, rect: Rect, shading: &Shading) {
        let stops: Vec<sk::GradientStop> = shading
            .stops
            .iter()
            .map(|(t, c)| sk::GradientStop::new(*t, color(*c)))
            .collect();
        let shader = match shading.kind {
            ShadingKind::Axial { start, end } => sk::LinearGradient::new(
                sk::Point::from_xy(start.x, start.y),
                sk::Point::from_xy(end.x, end.y),
                stops,
                sk::SpreadMode::Pad,
                sk::Transform::identity(),
            ),
            ShadingKind::Radial {
                center,
                r0,
                r1,
                scale_y,
            } => {
                let c = sk::Point::from_xy(center.x, center.y);
                sk::RadialGradient::new(
                    c,
                    r0,
                    c,
                    r1,
                    stops,
                    sk::SpreadMode::Pad,
                    sk::Transform::from_row(
                        1.0,
                        0.0,
                        0.0,
                        scale_y,
                        0.0,
                        center.y * (1.0 - scale_y),
                    ),
                )
            }
        };
        let Some(shader) = shader else { return };
        let Some(path) = rect_path(rect) else { return };
        let paint = sk::Paint {
            shader,
            anti_alias: true,
            ..Default::default()
        };
        let m = transform(self.state.matrix);
        let clip = self.state.clip.clone();
        self.pixmap()
            .fill_path(&path, &paint, sk::FillRule::Winding, m, clip.as_deref());
    }

    fn text(&mut self, run: &TextRun) {
        let mut builder = sk::PathBuilder::new();
        glyph_paths(run, &mut builder);
        let path = builder.finish();
        if let Some(stroke) = run.stroke.filter(|_| run.stroke_first) {
            self.stroke(path.clone(), stroke.color, stroke.width, LineStyle::Solid);
        }
        if run.fill {
            self.fill(path.clone(), run.color, sk::FillRule::Winding);
            if run.embolden > 0.0 {
                self.stroke(path.clone(), run.color, run.embolden, LineStyle::Solid);
            }
        }
        if let Some(stroke) = run.stroke.filter(|_| !run.stroke_first) {
            self.stroke(path, stroke.color, stroke.width, LineStyle::Solid);
        }
    }
}

/// Glyph outlines in font units, cached by font and glyph.
type OutlineCache = HashMap<(usize, u32, Vec<u32>, u16), Option<sk::Path>>;

/// Adds the outlines of a run's glyphs, in CSS pixels.
fn glyph_paths(run: &TextRun, builder: &mut sk::PathBuilder) {
    thread_local! {
        static CACHE: RefCell<OutlineCache> = RefCell::new(HashMap::new());
    }
    let scale = run.font.em_scale(run.size);
    let skew = run.oblique.to_radians().tan();
    let mut pen = run.origin.x;
    for glyph in &run.glyphs {
        let key = (
            run.font.data().as_ptr() as usize,
            run.font.index(),
            crate::font::variation_key(&run.font),
            glyph.id,
        );
        let outline = CACHE.with(|cache| {
            cache
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| {
                    let mut outline = Outline(sk::PathBuilder::new());
                    run.font.outline(glyph.id, &mut outline)?;
                    outline.0.finish()
                })
                .clone()
        });
        if let Some(outline) = outline {
            let (x, y) = (pen + glyph.x_offset, run.origin.y + glyph.y_offset);
            // Font units point up; the slant leans the tops of the glyphs right.
            let m = sk::Transform::from_row(scale, 0.0, skew * scale, -scale, x, y);
            if let Some(placed) = outline.transform(m) {
                builder.push_path(&placed);
            }
        }
        pen += glyph.x_advance;
    }
}

struct Outline(sk::PathBuilder);

impl crate::font::GlyphPen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.0.close();
    }
}

/// Decoded images as premultiplied pixels, by image.
type PixmapCache = Vec<(Weak<Image>, Arc<sk::Pixmap>)>;

/// An image's stored pixels as a premultiplied pixmap.
pub(crate) fn pixmap_of(image: &Arc<Image>) -> Arc<sk::Pixmap> {
    thread_local! {
        static CACHE: RefCell<PixmapCache> = const { RefCell::new(Vec::new()) };
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|(weak, _)| weak.strong_count() > 0);
        if let Some((_, pixmap)) = cache
            .iter()
            .find(|(weak, _)| std::ptr::eq(weak.as_ptr(), Arc::as_ptr(image)))
        {
            return pixmap.clone();
        }
        let pixmap = Arc::new(decode(image));
        cache.push((Arc::downgrade(image), pixmap.clone()));
        pixmap
    })
}

/// Straight RGBA pixels of an image, row by row.
pub(crate) fn rgba(image: &Image) -> Vec<u8> {
    let count = (image.width * image.height) as usize;
    match &image.data {
        ImageData::Jpeg { data, .. } => match ::image::load_from_memory(data) {
            Ok(decoded) => decoded.to_rgba8().into_raw(),
            Err(_) => vec![0; count * 4],
        },
        ImageData::Pixels {
            samples,
            gray,
            alpha,
        } => {
            let mut out = Vec::with_capacity(count * 4);
            for i in 0..count {
                let a = alpha.as_ref().map_or(255, |alpha| alpha[i]);
                if *gray {
                    let g = samples[i];
                    out.extend_from_slice(&[g, g, g, a]);
                } else {
                    out.extend_from_slice(&[
                        samples[i * 3],
                        samples[i * 3 + 1],
                        samples[i * 3 + 2],
                        a,
                    ]);
                }
            }
            out
        }
    }
}

fn decode(image: &Image) -> sk::Pixmap {
    let (width, height) = (image.width.max(1), image.height.max(1));
    let mut data = rgba(image);
    data.resize((width * height * 4) as usize, 0);
    for pixel in data.as_chunks_mut::<4>().0 {
        let a = u16::from(pixel[3]);
        for c in &mut pixel[..3] {
            *c = ((u16::from(*c) * a + 127) / 255) as u8;
        }
    }
    sk::IntSize::from_wh(width, height)
        .and_then(|size| sk::Pixmap::from_vec(data, size))
        .unwrap_or_else(|| sk::Pixmap::new(1, 1).expect("1×1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterises_and_blurs_a_rectangle() {
        let mut raster = Raster::new(Rect::new(0.0, 0.0, 20.0, 20.0)).unwrap();
        draw(
            &[DisplayItem::Rect {
                rect: Rect::new(5.0, 5.0, 10.0, 10.0),
                color: Color::rgb(255, 0, 0),
            }],
            &mut raster,
        );
        let at = |raster: &Raster, x: u32, y: u32| {
            raster.pixmap.pixel(x, y).map(|p| p.alpha()).unwrap_or(0)
        };
        assert_eq!(at(&raster, 20, 20), 255);
        assert_eq!(at(&raster, 2, 2), 0);
        blur(&mut raster.pixmap, 4.0);
        // The blur spreads the edge out and softens the middle a little.
        assert!(at(&raster, 7, 20) > 0);
        assert!(at(&raster, 20, 20) < 255);
    }

    #[test]
    fn groups_composite_with_their_opacity() {
        let mut raster = Raster::new(Rect::new(0.0, 0.0, 10.0, 10.0)).unwrap();
        draw(
            &[
                DisplayItem::PushGroup(Group {
                    opacity: 0.5,
                    ..Group::ISOLATED
                }),
                DisplayItem::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    color: Color::rgb(0, 0, 0),
                },
                DisplayItem::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    color: Color::rgb(0, 0, 0),
                },
                DisplayItem::PopGroup,
            ],
            &mut raster,
        );
        let alpha = raster.pixmap.pixel(5, 5).unwrap().alpha();
        assert!((126..=129).contains(&alpha), "{alpha}");
    }
}
