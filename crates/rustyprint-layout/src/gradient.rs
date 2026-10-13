//! Gradients: resolving a gradient for the box it fills, and painting it (CSS Images 3 § 3).
//!
//! Linear and radial gradients become PDF shadings that interpolate linearly in sRGB
//! between stops, so stops are added where CSS interpolates otherwise: across transition
//! hints, in other color spaces, and between colors of different alpha, which CSS blends
//! premultiplied. Conic gradients have no PDF shading and are drawn as images.

use std::sync::Arc;

use rustyprint_css::properties::types::{
    Gradient, GradientItem, GradientKind, LinearDirection, RadialSize,
};
use rustyprint_css::values::{AbsoluteColor, ColorSpace, interpolate};

use crate::display::{Color, DisplayItem, Point, Rect, Shading, ShadingKind};
use crate::images::{Image, ImageData};

/// Samples added between two stops that CSS blends differently from PDF.
const SUBDIVISIONS: usize = 16;

/// The most repetitions of a repeating gradient drawn.
const MAX_REPEATS: i64 = 512;

/// The largest side of a conic gradient's image, in pixels.
const MAX_CONIC_PIXELS: f32 = 1024.0;

/// A color at a position along the gradient line or ray, in pixels.
type Sample = (f32, AbsoluteColor);

/// Paints `gradient` filling `rect`.
pub(crate) fn paint(gradient: &Gradient, rect: Rect, items: &mut Vec<DisplayItem>) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    match &gradient.kind {
        GradientKind::Linear(direction) => linear(gradient, *direction, rect, items),
        GradientKind::Radial {
            ellipse,
            size,
            position,
        } => {
            let cx = position.0.resolve(rect.width);
            let cy = position.1.resolve(rect.height);
            let (rx, ry) = radii(*ellipse, size, cx, cy, rect.width, rect.height);
            let (rx, ry) = (rx.max(0.01), ry.max(0.01));
            let scale_y = ry / rx;
            // The ray reaches every corner within this distance, in the unscaled space.
            let reach = corners(cx, cy, rect.width, rect.height)
                .iter()
                .map(|(dx, dy)| dx.hypot(dy / scale_y))
                .fold(0.0, f32::max);
            let Some(samples) = samples(gradient, rx, (0.0, reach)) else {
                return;
            };
            // Radii cannot be negative: the part before the center is cut off.
            let samples = clip_start(samples, 0.0);
            let (start, end, stops) = normalize(&samples, 0.0);
            items.push(DisplayItem::Gradient {
                rect,
                shading: Arc::new(Shading {
                    kind: ShadingKind::Radial {
                        center: Point::new(rect.x + cx, rect.y + cy),
                        r0: start,
                        r1: end,
                        scale_y,
                    },
                    stops,
                }),
            });
        }
        GradientKind::Conic { from, position } => {
            let cx = position.0.resolve(rect.width);
            let cy = position.1.resolve(rect.height);
            if let Some(image) = conic(gradient, *from, cx, cy, rect.width, rect.height) {
                items.push(DisplayItem::Image {
                    image: Arc::new(image),
                    rect,
                    smooth: true,
                    orientation: 1,
                    alpha: 255,
                });
            }
        }
    }
}

fn linear(
    gradient: &Gradient,
    direction: LinearDirection,
    rect: Rect,
    items: &mut Vec<DisplayItem>,
) {
    let (w, h) = (rect.width, rect.height);
    // The unit direction, with y down.
    let (dx, dy) = match direction {
        LinearDirection::Angle(degrees) => {
            let radians = degrees.to_radians();
            (radians.sin(), -radians.cos())
        }
        LinearDirection::Corner { right, bottom } => {
            let sx = if right { 1.0 } else { -1.0 };
            let sy = if bottom { 1.0 } else { -1.0 };
            let (x, y) = (sx * h, sy * w);
            let length = x.hypot(y);
            (x / length, y / length)
        }
    };
    // The gradient line passes through the center, long enough that the corners are at
    // its ends (§ 3.1.1).
    let length = (w * dx).abs() + (h * dy).abs();
    if length <= 0.0 {
        return;
    }
    let Some(samples) = samples(gradient, length, (0.0, length)) else {
        return;
    };
    let (start, end, stops) = normalize(&samples, f32::NEG_INFINITY);
    let origin = (
        rect.x + w / 2.0 - dx * length / 2.0,
        rect.y + h / 2.0 - dy * length / 2.0,
    );
    let at = |p: f32| Point::new(origin.0 + dx * p, origin.1 + dy * p);
    items.push(DisplayItem::Gradient {
        rect,
        shading: Arc::new(Shading {
            kind: ShadingKind::Axial {
                start: at(start),
                end: at(end),
            },
            stops,
        }),
    });
}

/// The distances from `(cx, cy)` to the corners of a `w` × `h` box.
fn corners(cx: f32, cy: f32, w: f32, h: f32) -> [(f32, f32); 4] {
    [(cx, cy), (w - cx, cy), (cx, h - cy), (w - cx, h - cy)].map(|(x, y)| (x.abs(), y.abs()))
}

/// The radii of a radial gradient's ending shape (§ 3.2.2).
fn radii(ellipse: bool, size: &RadialSize, cx: f32, cy: f32, w: f32, h: f32) -> (f32, f32) {
    let (left, right, top, bottom) = (cx.abs(), (w - cx).abs(), cy.abs(), (h - cy).abs());
    let corners = corners(cx, cy, w, h);
    let closest = *corners
        .iter()
        .min_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)))
        .expect("four corners");
    let farthest = *corners
        .iter()
        .max_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)))
        .expect("four corners");
    // An ellipse through a corner, with the ratio of the ellipse that touches the sides.
    let through = |(dx, dy): (f32, f32), (sx, sy): (f32, f32)| {
        if sx <= 0.0 || sy <= 0.0 {
            return (0.0, 0.0);
        }
        let ratio = sy / sx;
        let rx = (dx * dx + (dy / ratio).powi(2)).sqrt();
        (rx, rx * ratio)
    };
    let closest_side = (left.min(right), top.min(bottom));
    let farthest_side = (left.max(right), top.max(bottom));
    match size {
        RadialSize::Circle(r) => (*r, *r),
        RadialSize::Ellipse(x, y) => (x.resolve(w), y.resolve(h)),
        RadialSize::ClosestSide if ellipse => closest_side,
        RadialSize::FarthestSide if ellipse => farthest_side,
        RadialSize::ClosestCorner if ellipse => through(closest, closest_side),
        RadialSize::FarthestCorner if ellipse => through(farthest, farthest_side),
        RadialSize::ClosestSide => {
            let r = closest_side.0.min(closest_side.1);
            (r, r)
        }
        RadialSize::FarthestSide => {
            let r = farthest_side.0.max(farthest_side.1);
            (r, r)
        }
        RadialSize::ClosestCorner => {
            let r = closest.0.hypot(closest.1);
            (r, r)
        }
        RadialSize::FarthestCorner => {
            let r = farthest.0.hypot(farthest.1);
            (r, r)
        }
    }
}

/// The colors along a gradient line of `length` pixels, covering `range` for repeating
/// gradients. `None` if the gradient has no stops.
fn samples(gradient: &Gradient, length: f32, range: (f32, f32)) -> Option<Vec<Sample>> {
    let stops = fixed_up(gradient, length)?;
    let samples = blend(gradient, &stops);
    if !gradient.repeating {
        return Some(samples);
    }
    let first = samples.first()?.0;
    let last = samples.last()?.0;
    let period = last - first;
    if period <= 0.0 {
        // A repeating gradient without length is its last color.
        let color = samples.last()?.1;
        return Some(vec![(range.0, color), (range.1, color)]);
    }
    let low = ((range.0 - first) / period).floor() as i64;
    let high = ((range.1 - first) / period).ceil() as i64;
    let (low, high) = (low.max(-MAX_REPEATS), high.min(low + MAX_REPEATS));
    let mut out = Vec::new();
    for k in low..high {
        let offset = k as f32 * period;
        out.extend(samples.iter().map(|&(p, c)| (p + offset, c)));
    }
    Some(out)
}

/// Color stop positions in pixels after the fix-up rules (§ 3.5.3), with transition hints
/// as `None` colors.
fn fixed_up(gradient: &Gradient, length: f32) -> Option<Vec<(Option<f32>, Option<AbsoluteColor>)>> {
    let mut items: Vec<(Option<f32>, Option<AbsoluteColor>)> = gradient
        .items
        .iter()
        .map(|item| match item {
            GradientItem::Stop { color, position } => {
                (position.as_ref().map(|p| p.resolve(length)), Some(*color))
            }
            GradientItem::Hint(p) => (Some(p.resolve(length)), None),
        })
        .collect();
    let stop_indices: Vec<usize> = (0..items.len()).filter(|&i| items[i].1.is_some()).collect();
    let (&first, &last) = (stop_indices.first()?, stop_indices.last()?);
    if items[first].0.is_none() {
        items[first].0 = Some(0.0);
    }
    if items[last].0.is_none() {
        items[last].0 = Some(length);
    }
    // Positions never go backwards.
    let mut max = f32::NEG_INFINITY;
    for item in &mut items {
        if let Some(p) = &mut item.0 {
            *p = p.max(max);
            max = *p;
        }
    }
    // Stops without a position spread evenly between the ones around them.
    let mut i = 0;
    while i < stop_indices.len() {
        if items[stop_indices[i]].0.is_some() {
            i += 1;
            continue;
        }
        let start = i - 1;
        let mut end = i;
        while items[stop_indices[end]].0.is_none() {
            end += 1;
        }
        let from = items[stop_indices[start]].0.expect("positioned");
        let to = items[stop_indices[end]].0.expect("positioned");
        let count = (end - start) as f32;
        for (k, j) in (start + 1..end).enumerate() {
            items[stop_indices[j]].0 = Some(from + (to - from) * (k + 1) as f32 / count);
        }
        i = end;
    }
    Some(items)
}

/// Samples of the gradient's colors between its stops, closely enough that linear sRGB
/// interpolation between them matches CSS.
fn blend(gradient: &Gradient, items: &[(Option<f32>, Option<AbsoluteColor>)]) -> Vec<Sample> {
    let mut out: Vec<Sample> = Vec::new();
    let mut previous: Option<Sample> = None;
    let mut hint: Option<f32> = None;
    for &(position, color) in items {
        let position = position.unwrap_or(0.0);
        let Some(color) = color else {
            hint = Some(position);
            continue;
        };
        let Some((p0, c0)) = previous else {
            out.push((position, color));
            previous = Some((position, color));
            continue;
        };
        let span = position - p0;
        let direct = gradient.space == ColorSpace::Srgb
            && (c0.alpha() - color.alpha()).abs() < 1e-6
            && hint.is_none();
        if span > 0.0 && !direct {
            // Where the hint sits as a fraction of the span: the color at it is halfway.
            let midpoint = hint.map(|h| ((h - p0) / span).clamp(0.0, 1.0));
            for k in 1..SUBDIVISIONS {
                let t = k as f32 / SUBDIVISIONS as f32;
                let weight = match midpoint {
                    None => t,
                    Some(m) if m <= 0.0 => 1.0,
                    Some(m) if m >= 1.0 => 0.0,
                    Some(m) => t.powf(0.5f32.ln() / m.ln()),
                };
                let mixed = interpolate(gradient.space, gradient.hue, &c0, &color, weight);
                out.push((p0 + span * t, mixed));
            }
        }
        out.push((position, color));
        previous = Some((position, color));
        hint = None;
    }
    out
}

/// Drops the part of the samples before `start`, starting with the color there.
fn clip_start(samples: Vec<Sample>, start: f32) -> Vec<Sample> {
    if samples.first().is_none_or(|s| s.0 >= start) {
        return samples;
    }
    let mut out = Vec::new();
    for pair in samples.windows(2) {
        let ((p0, c0), (p1, c1)) = (pair[0], pair[1]);
        if p1 < start {
            continue;
        }
        if p0 < start {
            let t = if p1 > p0 {
                (start - p0) / (p1 - p0)
            } else {
                1.0
            };
            out.push((start, lerp(&c0, &c1, t)));
        }
        out.push((p1, c1));
    }
    if out.is_empty() {
        let last = samples.last().expect("not empty").1;
        out.push((start, last));
    }
    out
}

/// Linear interpolation in sRGB, which is what the shading does between samples.
fn lerp(a: &AbsoluteColor, b: &AbsoluteColor, t: f32) -> AbsoluteColor {
    let (a, b) = (a.to_srgb(), b.to_srgb());
    let mix = |i: usize| a[i] + (b[i] - a[i]) * t;
    AbsoluteColor::srgb(mix(0), mix(1), mix(2), mix(3))
}

/// The start and end of the samples along the line, and the stops as fractions of it.
///
/// The colors at the ends continue for a pixel past them, but not before `min_start`, so
/// that a sharp step at an end stays inside the range the shading extends from.
fn normalize(samples: &[Sample], min_start: f32) -> (f32, f32, Vec<(f32, Color)>) {
    let mut padded = samples.to_vec();
    if let (Some(&(first, c0)), Some(&(last, c1))) = (samples.first(), samples.last()) {
        let before = (first - 1.0).max(min_start);
        if before < first {
            padded.insert(0, (before, c0));
        }
        padded.push((last + 1.0, c1));
    }
    let samples = &padded[..];
    let start = samples.first().map_or(0.0, |s| s.0);
    let mut end = samples.last().map_or(0.0, |s| s.0);
    if end - start < 1e-3 {
        end = start + 1e-3;
    }
    let stops = samples
        .iter()
        .map(|(p, c)| {
            let [r, g, b, a] = c.to_rgba8();
            (
                ((p - start) / (end - start)).clamp(0.0, 1.0),
                Color::rgba(r, g, b, a),
            )
        })
        .collect();
    (start, end, stops)
}

/// A conic gradient drawn into an image of a `w` × `h` box, centered at `(cx, cy)`.
fn conic(gradient: &Gradient, from: f32, cx: f32, cy: f32, w: f32, h: f32) -> Option<Image> {
    // Positions are fractions of a turn: resolve them against 1.
    let samples = samples(gradient, 1.0, (0.0, 1.0))?;
    let (first, last) = (samples.first()?.0, samples.last()?.0);
    let color_at = |turn: f32| -> AbsoluteColor {
        let turn = if gradient.repeating && last > first {
            first + (turn - first).rem_euclid(last - first)
        } else {
            turn
        };
        let next = samples.partition_point(|s| s.0 <= turn);
        match (next.checked_sub(1).map(|i| samples[i]), samples.get(next)) {
            (Some((p0, c0)), Some(&(p1, c1))) if p1 > p0 => lerp(&c0, &c1, (turn - p0) / (p1 - p0)),
            (Some((_, c0)), _) => c0,
            (None, Some(&(_, c1))) => c1,
            (None, None) => AbsoluteColor::TRANSPARENT,
        }
    };
    // A table of colors around the turn, then one lookup per pixel.
    const STEPS: usize = 2048;
    let table: Vec<[u8; 4]> = (0..STEPS)
        .map(|i| color_at(i as f32 / STEPS as f32).to_rgba8())
        .collect();
    let scale = (2.0f32).min(MAX_CONIC_PIXELS / w.max(h));
    let (pw, ph) = (
        ((w * scale).ceil() as u32).max(1),
        ((h * scale).ceil() as u32).max(1),
    );
    let mut samples_rgb = Vec::with_capacity((pw * ph * 3) as usize);
    let mut alpha = Vec::with_capacity((pw * ph) as usize);
    for y in 0..ph {
        for x in 0..pw {
            let dx = (x as f32 + 0.5) / scale - cx;
            let dy = (y as f32 + 0.5) / scale - cy;
            // Clockwise from the top.
            let angle = (dx.atan2(-dy).to_degrees() - from).rem_euclid(360.0);
            let index = ((angle / 360.0 * STEPS as f32) as usize).min(STEPS - 1);
            let [r, g, b, a] = table[index];
            samples_rgb.extend_from_slice(&[r, g, b]);
            alpha.push(a);
        }
    }
    let opaque = alpha.iter().all(|&a| a == 255);
    Some(Image {
        width: pw,
        height: ph,
        data: ImageData::Pixels {
            samples: samples_rgb,
            gray: false,
            alpha: (!opaque).then_some(alpha),
        },
        orientation: 1,
        resolution: None,
    })
}
