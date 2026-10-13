//! Filter effects (Filter Effects 1 § 13): filter functions applied to the display items an
//! element paints.
//!
//! Color filters change the colors of the items, so text and shapes stay vectors: a color
//! matrix commutes with the source-over compositing of the element's items, except where
//! it clamps. Blurs are rasterised.

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::Filter as CssFilter;
use rustyprint_css::properties::types::FilterFunction;

use crate::display::{Color, DisplayItem, Filter, Group, Point, Rect, Shading};
use crate::images::{Image, ImageData};
use crate::raster::{self, Raster};

/// The filter primitives of a `filter` or `backdrop-filter` value. A reference to an SVG
/// filter cannot be resolved, which leaves the element unfiltered (§ 6).
pub(crate) fn resolve(filters: &[CssFilter], style: &ComputedValues) -> Vec<Filter> {
    if filters.iter().any(|f| matches!(f, FilterFunction::Url(_))) {
        return Vec::new();
    }
    filters
        .iter()
        .map(|filter| match filter {
            FilterFunction::Blur(radius) => Filter::Blur(*radius),
            FilterFunction::Brightness(b) => Filter::ColorMatrix(linear(*b, 0.0)),
            FilterFunction::Contrast(c) => Filter::ColorMatrix(linear(*c, 0.5 - 0.5 * c)),
            FilterFunction::Invert(a) => Filter::ColorMatrix(linear(1.0 - 2.0 * a, *a)),
            FilterFunction::Grayscale(a) => Filter::ColorMatrix(grayscale(*a)),
            FilterFunction::Sepia(a) => Filter::ColorMatrix(sepia(*a)),
            FilterFunction::Saturate(s) => Filter::ColorMatrix(saturate(*s)),
            FilterFunction::HueRotate(degrees) => Filter::ColorMatrix(hue_rotate(*degrees)),
            FilterFunction::Opacity(a) => Filter::Opacity(*a),
            FilterFunction::DropShadow { color, x, y, blur } => {
                let [r, g, b, a] = color.resolve(&style.color).to_rgba8();
                Filter::DropShadow {
                    offset: Point::new(*x, *y),
                    // The blur radius is twice the standard deviation.
                    blur: blur / 2.0,
                    color: Color::rgba(r, g, b, a),
                }
            }
            FilterFunction::Url(_) => unreachable!("handled above"),
        })
        .collect()
}

/// `slope * C + intercept` for each color component.
fn linear(slope: f32, intercept: f32) -> [f32; 20] {
    [
        slope, 0.0, 0.0, 0.0, intercept, //
        0.0, slope, 0.0, 0.0, intercept, //
        0.0, 0.0, slope, 0.0, intercept, //
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

/// A 3×3 matrix on RGB, with alpha unchanged.
fn rgb(m: [f32; 9]) -> [f32; 20] {
    [
        m[0], m[1], m[2], 0.0, 0.0, //
        m[3], m[4], m[5], 0.0, 0.0, //
        m[6], m[7], m[8], 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

fn grayscale(amount: f32) -> [f32; 20] {
    let s = 1.0 - amount.clamp(0.0, 1.0);
    rgb([
        0.2126 + 0.7874 * s,
        0.7152 - 0.7152 * s,
        0.0722 - 0.0722 * s,
        0.2126 - 0.2126 * s,
        0.7152 + 0.2848 * s,
        0.0722 - 0.0722 * s,
        0.2126 - 0.2126 * s,
        0.7152 - 0.7152 * s,
        0.0722 + 0.9278 * s,
    ])
}

fn sepia(amount: f32) -> [f32; 20] {
    let s = 1.0 - amount.clamp(0.0, 1.0);
    rgb([
        0.393 + 0.607 * s,
        0.769 - 0.769 * s,
        0.189 - 0.189 * s,
        0.349 - 0.349 * s,
        0.686 + 0.314 * s,
        0.168 - 0.168 * s,
        0.272 - 0.272 * s,
        0.534 - 0.534 * s,
        0.131 + 0.869 * s,
    ])
}

fn saturate(s: f32) -> [f32; 20] {
    rgb([
        0.213 + 0.787 * s,
        0.715 - 0.715 * s,
        0.072 - 0.072 * s,
        0.213 - 0.213 * s,
        0.715 + 0.285 * s,
        0.072 - 0.072 * s,
        0.213 - 0.213 * s,
        0.715 - 0.715 * s,
        0.072 + 0.928 * s,
    ])
}

fn hue_rotate(degrees: f32) -> [f32; 20] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    rgb([
        0.213 + cos * 0.787 - sin * 0.213,
        0.715 - cos * 0.715 - sin * 0.715,
        0.072 - cos * 0.072 + sin * 0.928,
        0.213 - cos * 0.213 + sin * 0.143,
        0.715 + cos * 0.285 + sin * 0.140,
        0.072 - cos * 0.072 - sin * 0.283,
        0.213 - cos * 0.213 - sin * 0.787,
        0.715 - cos * 0.715 + sin * 0.715,
        0.072 + cos * 0.928 + sin * 0.072,
    ])
}

/// A straight color through a color matrix, clamped.
pub(crate) fn apply_matrix(m: &[f32; 20], c: Color) -> Color {
    let v = [c.r, c.g, c.b, c.a].map(|v| f32::from(v) / 255.0);
    let row = |i: usize| {
        let r = &m[i * 5..i * 5 + 5];
        let value = r[0] * v[0] + r[1] * v[1] + r[2] * v[2] + r[3] * v[3] + r[4];
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    Color::rgba(row(0), row(1), row(2), row(3))
}

/// Applies filter primitives, in order, to the items an element paints.
pub(crate) fn apply(items: Vec<DisplayItem>, filters: &[Filter]) -> Vec<DisplayItem> {
    apply_within(items, filters, None)
}

/// Applies filter primitives to items of which only the part inside `limit` shows.
pub(crate) fn apply_within(
    mut items: Vec<DisplayItem>,
    filters: &[Filter],
    limit: Option<Rect>,
) -> Vec<DisplayItem> {
    for filter in filters {
        items = match filter {
            Filter::ColorMatrix(m) => map_colors(items, &|c| apply_matrix(m, c)),
            Filter::Opacity(a) => grouped(items, *a),
            Filter::Blur(sigma) => blurred_within(items, *sigma, limit),
            Filter::DropShadow {
                offset,
                blur,
                color,
            } => {
                let shadow = shadow(&items, *offset, *blur, *color);
                shadow.into_iter().chain(items).collect()
            }
        };
    }
    items
}

/// The items in a group drawn with `opacity`.
fn grouped(items: Vec<DisplayItem>, opacity: f32) -> Vec<DisplayItem> {
    if opacity >= 1.0 {
        return items;
    }
    let mut out = Vec::with_capacity(items.len() + 2);
    out.push(DisplayItem::PushGroup(Group {
        opacity,
        ..Group::ISOLATED
    }));
    out.extend(items);
    out.push(DisplayItem::PopGroup);
    out
}

/// The items rasterised and blurred with standard deviation `sigma`.
pub(crate) fn blurred(items: Vec<DisplayItem>, sigma: f32) -> Vec<DisplayItem> {
    blurred_within(items, sigma, None)
}

fn blurred_within(items: Vec<DisplayItem>, sigma: f32, limit: Option<Rect>) -> Vec<DisplayItem> {
    if sigma <= 0.0 {
        return items;
    }
    let Some(mut bounds) = raster::bounds(&items) else {
        return items;
    };
    // What blurs into the visible part comes from at most three deviations away.
    if let Some(limit) = limit {
        match raster::intersection(bounds, limit.inflate(3.0 * sigma)) {
            Some(visible) => bounds = visible,
            None => return Vec::new(),
        }
    }
    let Some(mut raster) = Raster::new(bounds.inflate(3.0 * sigma)) else {
        return Vec::new();
    };
    raster::draw(&items, &mut raster);
    let scale = raster.scale;
    raster::blur(&mut raster.pixmap, sigma * scale);
    vec![raster.to_item()]
}

/// A drop shadow of the items: their alpha in `color`, moved by `offset` and blurred.
pub(crate) fn shadow(
    items: &[DisplayItem],
    offset: Point,
    sigma: f32,
    color: Color,
) -> Vec<DisplayItem> {
    let opaque = Color::rgb(color.r, color.g, color.b);
    let silhouette = map_colors(items.to_vec(), &|c| Color { a: c.a, ..opaque });
    let moved: Vec<DisplayItem> = silhouette
        .iter()
        .map(|item| item.translated(offset.x, offset.y))
        .collect();
    grouped(blurred(moved, sigma), f32::from(color.a) / 255.0)
}

/// The items with every color mapped by `f`, images included.
pub(crate) fn map_colors(items: Vec<DisplayItem>, f: &dyn Fn(Color) -> Color) -> Vec<DisplayItem> {
    items
        .into_iter()
        .map(|mut item| {
            match &mut item {
                DisplayItem::Text(run) => {
                    run.color = f(run.color);
                    if let Some(stroke) = &mut run.stroke {
                        stroke.color = f(stroke.color);
                    }
                }
                DisplayItem::Rect { color, .. }
                | DisplayItem::Polygon { color, .. }
                | DisplayItem::Polyline { color, .. }
                | DisplayItem::Line { color, .. }
                | DisplayItem::Path { color, .. }
                | DisplayItem::StrokePath { color, .. } => *color = f(*color),
                DisplayItem::Image { image, .. } => *image = Arc::new(map_image(image, f)),
                DisplayItem::Gradient { shading, .. } => {
                    let mut mapped = Shading::clone(shading);
                    for (_, color) in &mut mapped.stops {
                        *color = f(*color);
                    }
                    *shading = Arc::new(mapped);
                }
                DisplayItem::PushClip(_)
                | DisplayItem::PushClipText(_)
                | DisplayItem::PushClipPath { .. }
                | DisplayItem::PushTransform(_)
                | DisplayItem::PopClip
                | DisplayItem::PushGroup(_)
                | DisplayItem::PopGroup
                | DisplayItem::Backdrop(_) => {}
            }
            item
        })
        .collect()
}

/// An image with every pixel's color mapped by `f`.
fn map_image(image: &Image, f: &dyn Fn(Color) -> Color) -> Image {
    let pixels = raster::rgba(image);
    let count = (image.width * image.height) as usize;
    let mut samples = Vec::with_capacity(count * 3);
    let mut alpha = Vec::with_capacity(count);
    for pixel in pixels.as_chunks::<4>().0 {
        let c = f(Color::rgba(pixel[0], pixel[1], pixel[2], pixel[3]));
        samples.extend_from_slice(&[c.r, c.g, c.b]);
        alpha.push(c.a);
    }
    let opaque = alpha.iter().all(|&a| a == 255);
    Image {
        width: image.width,
        height: image.height,
        data: ImageData::Pixels {
            samples,
            gray: false,
            alpha: (!opaque).then_some(alpha),
        },
        orientation: image.orientation,
        resolution: image.resolution,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_matrices_match_the_spec_extremes() {
        let red = Color::rgb(255, 0, 0);
        assert_eq!(apply_matrix(&grayscale(1.0), red), Color::rgb(54, 54, 54));
        assert_eq!(apply_matrix(&grayscale(0.0), red), red);
        assert_eq!(
            apply_matrix(&linear(-1.0, 1.0), red),
            Color::rgb(0, 255, 255)
        );
        assert_eq!(apply_matrix(&linear(0.5, 0.0), red), Color::rgb(128, 0, 0));
        assert_eq!(apply_matrix(&hue_rotate(0.0), red), red);
        assert_eq!(apply_matrix(&saturate(1.0), red), red);
    }
}
