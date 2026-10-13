//! Masks (CSS Masking 1 §§ 6–7): mask layers and the mask border, rasterised into the
//! grayscale soft mask of an element's group.

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    BackgroundSize, CompositingOperator, CoordBox, ImageValue, MaskClipBox, MaskType, MaskingMode,
};

use crate::background::{BoxAreas, tiles};
use crate::border_image::{self, NineSlice};
use crate::display::{DisplayItem, Rect, SoftMask};
use crate::fragment::Sides;
use crate::images::{Image, ImageData, Images};
use crate::picture::Picture;
use crate::raster::{self, Raster};

/// The layer value at `index`: lists shorter than `mask-image` repeat.
fn nth<T>(list: &[T], index: usize) -> &T {
    &list[index % list.len()]
}

/// Whether a style masks its element.
pub(crate) fn has_mask(style: &ComputedValues) -> bool {
    style
        .mask_image
        .iter()
        .any(|image| *image != ImageValue::None)
        || style.mask_border_source != ImageValue::None
}

fn coord_box(areas: &BoxAreas, which: CoordBox) -> Rect {
    match which {
        CoordBox::ContentBox | CoordBox::FillBox => areas.content.rect,
        CoordBox::PaddingBox => areas.padding.rect,
        CoordBox::BorderBox | CoordBox::StrokeBox | CoordBox::ViewBox => areas.border.rect,
    }
}

/// The painting area of a layer, or `None` for `no-clip`.
fn clip_box(areas: &BoxAreas, which: MaskClipBox) -> Option<Rect> {
    Some(match which {
        MaskClipBox::ContentBox | MaskClipBox::FillBox => areas.content.rect,
        MaskClipBox::PaddingBox => areas.padding.rect,
        MaskClipBox::BorderBox | MaskClipBox::StrokeBox | MaskClipBox::ViewBox => areas.border.rect,
        MaskClipBox::NoClip => return None,
    })
}

/// How much of each pixel a raster lets through: its alpha, or its luminance times alpha.
fn coverage(raster: &Raster, luminance: bool) -> Vec<f32> {
    raster
        .pixmap
        .pixels()
        .iter()
        .map(|p| {
            let a = f32::from(p.alpha()) / 255.0;
            if luminance {
                // Premultiplied channels already carry the alpha.
                (0.2126 * f32::from(p.red())
                    + 0.7152 * f32::from(p.green())
                    + 0.0722 * f32::from(p.blue()))
                    / 255.0
            } else {
                a
            }
        })
        .collect()
}

/// The soft mask of an element whose areas are `areas` and border widths `border`.
/// `painted` is everything the element paints, which a layer without a clip may reach.
pub(crate) fn soft_mask(
    style: &ComputedValues,
    areas: &BoxAreas,
    border: Sides,
    images: &Images,
    painted: &[DisplayItem],
) -> Option<SoftMask> {
    if !has_mask(style) {
        return None;
    }
    let layers = style
        .mask_image
        .iter()
        .any(|image| *image != ImageValue::None);
    let nine = NineSlice {
        slice: &style.mask_border_slice,
        width: &style.mask_border_width,
        outset: &style.mask_border_outset,
        repeat: style.mask_border_repeat,
    };
    let border_area = border_image::area(&nine, border, areas.border.rect);
    // Everything a layer may cover.
    let mut extent = areas.border.rect;
    if layers {
        for index in 0..style.mask_image.len() {
            match clip_box(areas, *nth(&style.mask_clip, index)) {
                Some(rect) => extent = raster::union(extent, rect),
                None => {
                    if let Some(bounds) = raster::bounds(painted) {
                        extent = raster::union(extent, bounds);
                    }
                }
            }
        }
    }
    if style.mask_border_source != ImageValue::None {
        extent = raster::union(extent, border_area);
    }
    let blank = Raster::new(extent)?;
    let count = (blank.pixmap.width() * blank.pixmap.height()) as usize;

    let mut mask: Option<Vec<f32>> = None;
    if layers {
        let mut acc = vec![0.0f32; count];
        for index in (0..style.mask_image.len()).rev() {
            let mut raster = Raster::new(extent)?;
            let image = &style.mask_image[index];
            // A layer that is `none` or does not load is transparent black (§ 6.3).
            let picture = Picture::resolve(image, images, style);
            if let Some(picture) = &picture {
                let items = layer_items(style, areas, index, picture);
                raster::draw(&items, &mut raster);
            }
            let luminance = *nth(&style.mask_mode, index) == MaskingMode::Luminance;
            let source = coverage(&raster, luminance);
            // The bottom layer has nothing to composite with.
            let operator = if index == style.mask_image.len() - 1 {
                CompositingOperator::Add
            } else {
                *nth(&style.mask_composite, index)
            };
            for (d, s) in acc.iter_mut().zip(source) {
                *d = match operator {
                    CompositingOperator::Add => s + *d * (1.0 - s),
                    CompositingOperator::Subtract => s * (1.0 - *d),
                    CompositingOperator::Intersect => s * *d,
                    CompositingOperator::Exclude => s * (1.0 - *d) + *d * (1.0 - s),
                };
            }
        }
        mask = Some(acc);
    }
    if style.mask_border_source != ImageValue::None {
        let mut raster = Raster::new(extent)?;
        if let Some(picture) = Picture::resolve(&style.mask_border_source, images, style) {
            let mut items = Vec::new();
            border_image::paint(
                &picture,
                style,
                &nine,
                border,
                areas.border.rect,
                &mut items,
            );
            raster::draw(&items, &mut raster);
        }
        let luminance = style.mask_border_mode == MaskType::Luminance;
        let source = coverage(&raster, luminance);
        mask = Some(match mask {
            // Mask layers and the mask border both mask the element.
            Some(acc) => acc.iter().zip(source).map(|(a, b)| a * b).collect(),
            None => source,
        });
    }
    let mask = mask?;
    let (width, height) = (blank.pixmap.width(), blank.pixmap.height());
    let image = Image {
        width,
        height,
        data: ImageData::Pixels {
            samples: mask
                .iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
            gray: true,
            alpha: None,
        },
        orientation: 1,
        resolution: None,
    };
    Some(SoftMask {
        rect: Rect::new(
            extent.x,
            extent.y,
            width as f32 / blank.scale,
            height as f32 / blank.scale,
        ),
        image: Arc::new(image),
    })
}

/// The items one mask layer paints: its image positioned, sized and repeated in its origin
/// box like a background layer, clipped to its painting area.
fn layer_items(
    style: &ComputedValues,
    areas: &BoxAreas,
    index: usize,
    picture: &Picture,
) -> Vec<DisplayItem> {
    let positioning = coord_box(areas, *nth(&style.mask_origin, index));
    let clip = clip_box(areas, *nth(&style.mask_clip, index));
    let painting = clip.unwrap_or_else(|| positioning.inflate(1.0e4));
    let size = nth(&style.mask_size, index);
    let auto = BackgroundSize::Explicit(None, None);
    let (natural, size) = match picture.natural_size(style) {
        Some(natural) => (natural, size),
        None => {
            let natural = match size {
                BackgroundSize::Explicit(w, h) => (
                    w.as_ref()
                        .map_or(positioning.width, |w| w.resolve(positioning.width)),
                    h.as_ref()
                        .map_or(positioning.height, |h| h.resolve(positioning.height)),
                ),
                _ => (positioning.width, positioning.height),
            };
            (natural, &auto)
        }
    };
    let (x, y) = nth(&style.mask_position, index);
    let rects = tiles(
        natural,
        size,
        *nth(&style.mask_repeat, index),
        (x, y),
        positioning,
        painting,
    );
    let mut items = Vec::new();
    if let Some(clip) = clip {
        items.push(DisplayItem::PushClip(clip));
    }
    for rect in rects {
        picture.paint(style, rect, &mut items);
    }
    if clip.is_some() {
        items.push(DisplayItem::PopClip);
    }
    items
}
