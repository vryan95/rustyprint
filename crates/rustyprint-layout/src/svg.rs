//! SVG images: an SVG file is drawn to pixels, with more of them than its size in CSS
//! pixels so that it prints sharply.

use resvg::{tiny_skia, usvg};

use crate::images::{Fit, Image, ImageData};

/// The image pixels drawn for each CSS pixel of the SVG's size: 384 per inch.
const SCALE: f32 = 4.0;
/// A small SVG, such as an icon, is drawn with at least this many pixels along its longer
/// side, since it may be shown much larger than its own size.
const MIN_LONG_SIDE: f32 = 1024.0;
/// The most pixels along the longer side.
const MAX_LONG_SIDE: f32 = 4096.0;

/// Whether `bytes` is an XML file, which for an image means SVG: markup after an optional
/// byte order mark and white space.
pub(crate) fn is_svg(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|&byte| byte == b'<')
}

/// Draws an SVG file with at most `max_pixels` pixels.
///
/// As for an SVG image in a browser, nothing outside the file is loaded: an image the SVG
/// links to by URL is not drawn. Text is not drawn either.
pub(crate) fn rasterize(bytes: &[u8], max_pixels: u64) -> Result<Image, String> {
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let xml = usvg::roxmltree::Document::parse_with_options(
        text,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let fit = fit(xml.root_element());
    let tree = usvg::Tree::from_xmltree(&xml, &options).map_err(|error| error.to_string())?;
    let (width, height) = (tree.size().width(), tree.size().height());
    let long = width.max(height);
    let mut scale = SCALE.max(MIN_LONG_SIDE / long).min(MAX_LONG_SIDE / long);
    let pixels = f64::from(width * scale) * f64::from(height * scale);
    if pixels > max_pixels as f64 {
        scale *= (max_pixels as f64 / pixels).sqrt() as f32;
    }
    // Rounded down, so that rounding never passes the limit.
    let size = |length: f32| ((length * scale).floor() as u32).max(1);
    let (pixel_width, pixel_height) = (size(width), size(height));
    let mut pixmap = tiny_skia::Pixmap::new(pixel_width, pixel_height)
        .ok_or_else(|| format!("an SVG image of {width}×{height} cannot be drawn"))?;
    let transform =
        tiny_skia::Transform::from_scale(pixel_width as f32 / width, pixel_height as f32 / height);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let count = pixmap.pixels().len();
    let mut samples = Vec::with_capacity(count * 3);
    let mut alpha = Vec::with_capacity(count);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        samples.extend_from_slice(&[color.red(), color.green(), color.blue()]);
        alpha.push(color.alpha());
    }
    let opaque = alpha.iter().all(|&a| a == 255);
    Ok(Image {
        width: pixel_width,
        height: pixel_height,
        data: ImageData::Pixels {
            samples,
            gray: false,
            alpha: (!opaque).then_some(alpha),
        },
        orientation: 1,
        resolution: None,
        natural: Some((width, height)),
        shape_only: shape_only(xml.root_element()),
        fit,
    })
}

/// Whether the SVG has a shape and no size of its own: a `viewBox`, and neither a `width`
/// nor a `height` that is a length. A percentage, as in `width="100%"`, is not a size.
fn shape_only(root: usvg::roxmltree::Node) -> bool {
    let sized = |name: &str| {
        root.attribute(name)
            .is_some_and(|value| !value.trim().ends_with('%'))
    };
    root.has_attribute("viewBox") && !sized("width") && !sized("height")
}

/// How the SVG keeps its shape in a box of another shape. It does so when it has a
/// `viewBox`, as `preserveAspectRatio` says; `none`, or no `viewBox`, stretches it.
///
/// The picture placed is the SVG at its own size. For an SVG whose `width` and `height`
/// are not in the proportion of its `viewBox`, that is the `viewBox` already placed in
/// that size, where a browser places the `viewBox` in the box itself.
fn fit(root: usvg::roxmltree::Node) -> Option<Fit> {
    root.attribute("viewBox")?;
    let value = root.attribute("preserveAspectRatio").unwrap_or("xMidYMid");
    let mut words = value
        .split_ascii_whitespace()
        .filter(|word| *word != "defer");
    let align = words.next().unwrap_or("xMidYMid");
    let slice = words.next() == Some("slice");
    let (x, y) = align.strip_prefix('x')?.split_once('Y')?;
    let along = |word: &str| match word {
        "Min" => Some(0.0),
        "Mid" => Some(0.5),
        "Max" => Some(1.0),
        _ => None,
    };
    Some(Fit {
        align: (along(x)?, along(y)?),
        slice,
    })
}
