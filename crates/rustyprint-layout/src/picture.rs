//! Resolving an `<image>` value for painting (CSS Images 4 § 2): loading its file, choosing
//! from `image-set()`, and falling back from `image()`.

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    Direction, FadeSource, Gradient, ImageSetOption, ImageValue, fade_fractions,
};
use rustyprint_css::values::Color as CssColor;

use rustyprint_dom::Document;

use crate::boxes::BlockBox;
use crate::display::{Color, DisplayItem, Rect, Shading};
use crate::fragment::BoxFragment;
use crate::fragment::ElementBox;
use crate::images::{ElementImage, Image, Images, natural_size, orientation};
use crate::paginate::LaidPage;

/// The image types `image-set()` options may name and that can be decoded.
const SUPPORTED_TYPES: [&str; 6] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/x-bmp",
];

/// An image ready to paint.
#[derive(Debug, Clone)]
pub enum Picture {
    /// A raster image, with the resolution `image-set()` gives it, whether `image(rtl …)`
    /// flips it, and the region of a media fragment in image pixels.
    Raster {
        image: Arc<Image>,
        dppx: Option<f32>,
        flip: bool,
        crop: Option<[f32; 4]>,
    },
    Gradient(Arc<Gradient>),
    /// A solid color image without a natural size, from `image(<color>)`.
    Color(Color),
    /// `cross-fade()`: images and the fraction of each.
    Fade(Vec<(Picture, f32)>),
    /// `element()`: an element as painted.
    Element(ElementImage),
    /// The document of an `iframe`.
    Frame(Frame),
}

/// A document shown in an `iframe` (HTML § 4.8.5).
#[derive(Debug, Clone)]
pub struct Frame {
    /// The document's box tree.
    pub root: Arc<BlockBox>,
    /// Its layout in the frame's content box, once the frame is laid out.
    pub laid: Option<Arc<BoxFragment>>,
}

impl Picture {
    /// A raster image as it is.
    pub fn raster(image: Arc<Image>) -> Self {
        Picture::Raster {
            image,
            dppx: None,
            flip: false,
            crop: None,
        }
    }

    /// The image `value` gives in an element with `style`, or `None` for `none` and for
    /// invalid images, which paint nothing.
    pub fn resolve(value: &ImageValue, images: &Images, style: &ComputedValues) -> Option<Self> {
        match value {
            ImageValue::None | ImageValue::SpecifiedGradient(_) => None,
            ImageValue::Url(url) => url_picture(url, images),
            ImageValue::Gradient(gradient) => Some(Picture::Gradient(gradient.clone())),
            ImageValue::Image(function) => {
                let loaded = function
                    .src
                    .as_ref()
                    .and_then(|src| url_picture(src, images));
                match loaded {
                    Some(Picture::Raster {
                        image, dppx, crop, ..
                    }) => {
                        // A directional image flips in an element of the other direction.
                        let flip = function
                            .rtl
                            .is_some_and(|rtl| rtl != (style.direction == Direction::Rtl));
                        Some(Picture::Raster {
                            image,
                            dppx,
                            flip,
                            crop,
                        })
                    }
                    Some(other) => Some(other),
                    None => function
                        .color
                        .as_ref()
                        .map(|c| Picture::Color(color(c, style))),
                }
            }
            ImageValue::ImageSet(options) => {
                // Print has no device resolution, so the sharpest supported option wins; an
                // option that fails to load falls back to the next sharpest.
                let mut options: Vec<&ImageSetOption> = options
                    .iter()
                    .filter(|option| option.resolution > 0.0)
                    .filter(|option| {
                        option.mime.as_ref().is_none_or(|mime| {
                            SUPPORTED_TYPES.contains(&&*mime.trim().to_ascii_lowercase())
                        })
                    })
                    .collect();
                options.sort_by(|a, b| b.resolution.total_cmp(&a.resolution));
                options.into_iter().find_map(|option| {
                    match Picture::resolve(&option.image, images, style)? {
                        Picture::Raster {
                            image, flip, crop, ..
                        } => Some(Picture::Raster {
                            image,
                            dppx: Some(option.resolution),
                            flip,
                            crop,
                        }),
                        other => Some(other),
                    }
                })
            }
            ImageValue::CrossFade(inputs) => {
                let fractions = fade_fractions(inputs);
                let layers: Vec<(Picture, f32)> = inputs
                    .iter()
                    .zip(fractions)
                    .filter_map(|((source, _), fraction)| {
                        let picture = match source {
                            FadeSource::Image(image) => Picture::resolve(image, images, style)?,
                            FadeSource::Color(c) => Picture::Color(color(c, style)),
                        };
                        Some((picture, fraction))
                    })
                    .collect();
                (!layers.is_empty()).then_some(Picture::Fade(layers))
            }
            ImageValue::Element(id) => images.element(id).map(Picture::Element),
            ImageValue::Paint(_) => None,
        }
    }

    /// The natural size in CSS pixels, or `None` for images without one, such as gradients.
    pub fn natural_size(&self, style: &ComputedValues) -> Option<(f32, f32)> {
        match self {
            Picture::Raster {
                image, dppx, crop, ..
            } => {
                let (w, h) = natural_size(image, style);
                let scale = match dppx {
                    Some(dppx) => style.image_resolution.dppx(image.resolution) / dppx,
                    None => 1.0,
                };
                // A media fragment's region is in displayed image pixels.
                let (w, h) = match crop {
                    Some([_, _, cw, ch]) => {
                        let turned = Image::turned(orientation(image, style.image_orientation));
                        let pixels = if turned { image.height } else { image.width };
                        let px = w / pixels as f32;
                        (cw * px, ch * px)
                    }
                    None => (w, h),
                };
                Some((w * scale, h * scale))
            }
            Picture::Gradient(_) | Picture::Color(_) | Picture::Frame(_) => None,
            Picture::Element(element) => Some(element.size),
            // The weighted average of the sizes of the images that have one (§ 2.6).
            Picture::Fade(layers) => {
                let sized: Vec<((f32, f32), f32)> = layers
                    .iter()
                    .filter_map(|(picture, f)| Some((picture.natural_size(style)?, *f)))
                    .collect();
                let total: f32 = sized.iter().map(|(_, f)| f).sum();
                if sized.is_empty() {
                    return None;
                }
                if total <= 0.0 {
                    return Some(sized[0].0);
                }
                let (w, h) = sized
                    .iter()
                    .fold((0.0, 0.0), |(w, h), ((sw, sh), f)| (w + sw * f, h + sh * f));
                Some((w / total, h / total))
            }
        }
    }

    /// Paints the image filling `rect`.
    pub fn paint(&self, style: &ComputedValues, rect: Rect, items: &mut Vec<DisplayItem>) {
        self.paint_faded(style, rect, 1.0, items);
    }

    fn paint_faded(
        &self,
        style: &ComputedValues,
        rect: Rect,
        opacity: f32,
        items: &mut Vec<DisplayItem>,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 || opacity <= 0.0 {
            return;
        }
        match self {
            Picture::Raster {
                image, flip, crop, ..
            } => {
                let mut turn = orientation(image, style.image_orientation);
                if *flip {
                    turn = flipped(turn);
                }
                // A media fragment shows part of the image: draw it whole, scaled so the
                // part fills `rect`, and clip.
                let (full, clip) = match crop {
                    Some([x, y, w, h]) => {
                        let (iw, ih) = if Image::turned(turn) {
                            (image.height as f32, image.width as f32)
                        } else {
                            (image.width as f32, image.height as f32)
                        };
                        let (sx, sy) = (rect.width / w, rect.height / h);
                        let full = Rect::new(rect.x - x * sx, rect.y - y * sy, iw * sx, ih * sy);
                        (full, true)
                    }
                    None => (rect, false),
                };
                if clip {
                    items.push(DisplayItem::PushClip(rect));
                }
                items.push(DisplayItem::Image {
                    image: image.clone(),
                    rect: full,
                    smooth: style.image_rendering.smooth(),
                    orientation: turn,
                    alpha: (opacity * 255.0).round() as u8,
                });
                if clip {
                    items.push(DisplayItem::PopClip);
                }
            }
            Picture::Gradient(gradient) => {
                let start = items.len();
                crate::gradient::paint(gradient, rect, items);
                if opacity < 1.0 {
                    for item in &mut items[start..] {
                        fade(item, opacity);
                    }
                }
            }
            Picture::Color(color) => {
                let mut color = *color;
                color.a = (f32::from(color.a) * opacity).round() as u8;
                if !color.is_transparent() {
                    items.push(DisplayItem::Rect { rect, color });
                }
            }
            // Frames paint with the images of their own document; see `paint_frame`.
            Picture::Frame(_) => {}
            Picture::Element(element) => {
                let (w, h) = element.size;
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                items.push(DisplayItem::PushClip(rect));
                items.push(DisplayItem::PushTransform([
                    rect.width / w,
                    0.0,
                    0.0,
                    rect.height / h,
                    rect.x,
                    rect.y,
                ]));
                let start = items.len();
                items.extend(element.items.iter().cloned());
                if opacity < 1.0 {
                    for item in &mut items[start..] {
                        fade(item, opacity);
                    }
                }
                items.push(DisplayItem::PopClip);
                items.push(DisplayItem::PopClip);
            }
            Picture::Fade(layers) => {
                // Each layer is drawn over the ones before, so its opacity is its fraction
                // of what the layers above leave: exact for opaque images.
                let mut above: f32 = layers.iter().map(|(_, f)| f).sum();
                for (picture, fraction) in layers {
                    above -= fraction;
                    let left = 1.0 - above;
                    let layer_opacity = if left > 0.0 {
                        (fraction / left).min(1.0)
                    } else {
                        1.0
                    };
                    picture.paint_faded(style, rect, opacity * layer_opacity, items);
                }
            }
        }
    }
}

/// The image at a URL, which may end in a `#xywh=` media fragment.
fn url_picture(url: &str, images: &Images) -> Option<Picture> {
    let (file, crop) = match url.split_once('#') {
        Some((file, fragment)) => (file, Some(fragment)),
        None => (url, None),
    };
    let image = images.get(file)?;
    let crop = crop.and_then(|fragment| media_fragment(fragment, &image));
    Some(Picture::Raster {
        image,
        dppx: None,
        flip: false,
        crop,
    })
}

/// The region of a spatial media fragment, `xywh=[pixel:|percent:]x,y,w,h`, in image pixels
/// and clamped to the image (Media Fragments URI 1.0 § 4.2.2).
fn media_fragment(fragment: &str, image: &Image) -> Option<[f32; 4]> {
    let value = fragment
        .split('&')
        .find_map(|part| part.strip_prefix("xywh="))?;
    let (percent, numbers) = if let Some(rest) = value.strip_prefix("percent:") {
        (true, rest)
    } else {
        (false, value.strip_prefix("pixel:").unwrap_or(value))
    };
    let numbers: Vec<f32> = numbers
        .split(',')
        .map(|n| n.trim().parse::<f32>().ok().filter(|n| *n >= 0.0))
        .collect::<Option<_>>()?;
    let [x, y, w, h] = <[f32; 4]>::try_from(numbers).ok()?;
    let (iw, ih) = (image.width as f32, image.height as f32);
    let [x, y, w, h] = if percent {
        [
            x * iw / 100.0,
            y * ih / 100.0,
            w * iw / 100.0,
            h * ih / 100.0,
        ]
    } else {
        [x, y, w, h]
    };
    let (x, y) = (x.min(iw), y.min(ih));
    let (w, h) = (w.min(iw - x), h.min(ih - y));
    (w > 0.0 && h > 0.0).then_some([x, y, w, h])
}

/// An EXIF orientation followed by a horizontal flip.
fn flipped(orientation: u8) -> u8 {
    match orientation {
        1 => 2,
        2 => 1,
        3 => 4,
        4 => 3,
        5 => 6,
        6 => 5,
        7 => 8,
        8 => 7,
        other => other,
    }
}

fn color(value: &CssColor, style: &ComputedValues) -> Color {
    Color::from_css(&value.resolve(&style.color))
}

/// Scales the opacity of an item a gradient painted.
fn fade(item: &mut DisplayItem, opacity: f32) {
    let scale = |a: u8| (f32::from(a) * opacity).round() as u8;
    match item {
        DisplayItem::Gradient { shading, .. } => {
            let mut faded = Shading::clone(shading);
            for (_, color) in &mut faded.stops {
                color.a = scale(color.a);
            }
            *shading = Arc::new(faded);
        }
        DisplayItem::Image { alpha, .. } => *alpha = scale(*alpha),
        DisplayItem::Text(run) => {
            run.color.a = scale(run.color.a);
            if let Some(stroke) = &mut run.stroke {
                stroke.color.a = scale(stroke.color.a);
            }
        }
        DisplayItem::Rect { color, .. }
        | DisplayItem::Polygon { color, .. }
        | DisplayItem::Polyline { color, .. }
        | DisplayItem::Line { color, .. }
        | DisplayItem::Path { color, .. }
        | DisplayItem::StrokePath { color, .. } => color.a = scale(color.a),
        DisplayItem::PushClip(_)
        | DisplayItem::PushClipText(_)
        | DisplayItem::PushClipPath { .. }
        | DisplayItem::PushTransform(_)
        | DisplayItem::PopClip
        | DisplayItem::PushGroup(_)
        | DisplayItem::PopGroup
        | DisplayItem::Backdrop(_) => {}
    }
}

/// Paints the elements that `element()` images on the pages refer to and registers them in
/// `images`, in the order they are first referred to. An element is painted from its first
/// box, so `element()` inside it, which refers to elements painted later, paints nothing.
pub fn register_elements(document: &Document, pages: &[LaidPage], images: &Images) {
    let mut ids = Vec::new();
    for page in pages {
        let roots = page
            .root
            .iter()
            .chain(page.margin_boxes.iter().map(|margin| &margin.fragment));
        for root in roots {
            element_ids(&root.style.background_image, &mut ids);
            for (_, _, element) in root.element_boxes() {
                let style = match element {
                    ElementBox::Block(block) => &block.style,
                    ElementBox::Inline(inline) => &inline.style,
                };
                element_ids(&style.background_image, &mut ids);
            }
        }
    }
    for id in ids {
        let Some(node) = document.descendants(document.root()).find(|&node| {
            document
                .element(node)
                .is_some_and(|element| element.attr("id") == Some(&*id))
        }) else {
            continue;
        };
        let block = pages
            .iter()
            .filter_map(|page| page.root.as_ref())
            .find_map(|root| {
                root.element_boxes()
                    .into_iter()
                    .find_map(|(n, _, element)| match element {
                        ElementBox::Block(block) if n == node => Some(block.clone()),
                        _ => None,
                    })
            });
        let Some(block) = block else {
            continue;
        };
        let (x, y) = (block.rect.x, block.rect.y);
        let painted: Vec<DisplayItem> = crate::paint_fragment(&block, images)
            .iter()
            .map(|item| item.translated(-x, -y))
            .collect();
        images.set_element(
            &id,
            ElementImage {
                items: painted.into(),
                size: (block.rect.width, block.rect.height),
            },
        );
    }
}

/// Adds the IDs that `element()` refers to in an image list, once each.
fn element_ids(values: &[ImageValue], out: &mut Vec<Arc<str>>) {
    for value in values {
        match value {
            ImageValue::Element(id) if !out.contains(id) => out.push(id.clone()),
            ImageValue::ImageSet(options) => {
                let images: Vec<ImageValue> = options.iter().map(|o| o.image.clone()).collect();
                element_ids(&images, out);
            }
            ImageValue::CrossFade(inputs) => {
                let images: Vec<ImageValue> = inputs
                    .iter()
                    .filter_map(|(source, _)| match source {
                        FadeSource::Image(image) => Some(image.clone()),
                        FadeSource::Color(_) => None,
                    })
                    .collect();
                element_ids(&images, out);
            }
            _ => {}
        }
    }
}
