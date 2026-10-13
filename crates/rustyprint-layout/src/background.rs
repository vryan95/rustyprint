//! Backgrounds (CSS Backgrounds 3 § 3): a box's color and image layers.

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    Attachment, BackgroundClip, BackgroundSize, LengthPercentage, PositionComponent, Repeat,
    VisualBox,
};

use crate::display::{BlendMode, Color, DisplayItem, FillRule, Group, Rect, TextRun};
use crate::images::Images;
use crate::picture::Picture;
use crate::rounded::RoundedRect;

/// Tiles one layer may paint at most, so a tiny image cannot flood the page.
const MAX_TILES: usize = 10_000;

/// The border, padding and content boxes of a box, with their corners.
pub(crate) struct BoxAreas {
    pub border: RoundedRect,
    pub padding: RoundedRect,
    pub content: RoundedRect,
}

impl BoxAreas {
    fn visual_box(&self, visual_box: VisualBox) -> &RoundedRect {
        match visual_box {
            VisualBox::BorderBox => &self.border,
            VisualBox::PaddingBox => &self.padding,
            VisualBox::ContentBox => &self.content,
        }
    }
}

/// The layer value at `index`: lists shorter than `background-image` repeat.
fn nth<T>(list: &[T], index: usize) -> &T {
    &list[index % list.len()]
}

/// Paints the background color and then the image layers from the bottom one up. `text`
/// gives the box's text runs, for layers clipped to text; `viewport` is the page area that
/// fixed layers are positioned in.
pub(crate) fn paint_background(
    style: &ComputedValues,
    areas: &BoxAreas,
    text: &dyn Fn() -> Vec<TextRun>,
    images: &Images,
    viewport: Rect,
    items: &mut Vec<DisplayItem>,
) {
    let count = style.background_image.len();
    let clips = &style.background_clip;
    // Layers that blend blend with the layers and color below them, but not with what is
    // behind the element (Compositing and Blending 1 § 7).
    let blends = crate::effects::blends_background(style);
    if blends {
        items.push(DisplayItem::PushGroup(Group::ISOLATED));
    }
    let mut runs = None;
    let mut text_runs = || runs.get_or_insert_with(text).clone();

    let color = Color::from_css(&style.background_color.resolve(&style.color));
    if !color.is_transparent() {
        // The color is clipped like the bottom layer.
        let clip = *nth(clips, count - 1);
        match clip_box(clip) {
            Some(visual_box) => fill(areas.visual_box(visual_box), color, items),
            None => {
                push_clip(clip, areas, &mut text_runs, items);
                fill(&areas.border, color, items);
                items.push(DisplayItem::PopClip);
            }
        }
    }

    for index in (0..count).rev() {
        let Some(picture) = Picture::resolve(&style.background_image[index], images, style) else {
            continue;
        };
        let clip = *nth(clips, index);
        let painting = match clip_box(clip) {
            Some(visual_box) => areas.visual_box(visual_box).rect,
            None => areas.border.rect,
        };
        // A fixed layer is positioned in the page area, so it repeats on every page.
        let positioning = if *nth(&style.background_attachment, index) == Attachment::Fixed {
            viewport
        } else {
            areas.visual_box(*nth(&style.background_origin, index)).rect
        };
        let size = nth(&style.background_size, index);
        let (natural, size) = match picture.natural_size(style) {
            Some(natural) => (natural, size),
            None => (size_without_natural(size, positioning), &AUTO_SIZE),
        };
        let tiles = tiles(
            natural,
            size,
            *nth(&style.background_repeat, index),
            (
                nth(&style.background_position_x, index),
                nth(&style.background_position_y, index),
            ),
            positioning,
            painting,
        );
        if tiles.is_empty() {
            continue;
        }
        let blend = crate::effects::blend_mode(*nth(&style.background_blend_mode, index));
        if blend != BlendMode::Normal {
            items.push(DisplayItem::PushGroup(Group {
                blend,
                ..Group::ISOLATED
            }));
        }
        push_clip(clip, areas, &mut text_runs, items);
        for rect in tiles {
            picture.paint(style, rect, items);
        }
        items.push(DisplayItem::PopClip);
        if blend != BlendMode::Normal {
            items.push(DisplayItem::PopGroup);
        }
    }
    if blends {
        items.push(DisplayItem::PopGroup);
    }
}

/// `background-size: auto`, for images whose size is already decided.
const AUTO_SIZE: BackgroundSize<LengthPercentage> = BackgroundSize::Explicit(None, None);

/// The size of an image without a natural size, such as a gradient, in the positioning
/// area `area`: what `background-size` gives, with the area's size for the rest (§ 3.9).
fn size_without_natural(size: &BackgroundSize<LengthPercentage>, area: Rect) -> (f32, f32) {
    match size {
        BackgroundSize::Cover | BackgroundSize::Contain => (area.width, area.height),
        BackgroundSize::Explicit(w, h) => (
            w.as_ref().map_or(area.width, |w| w.resolve(area.width)),
            h.as_ref().map_or(area.height, |h| h.resolve(area.height)),
        ),
    }
}

/// The box a clip value clips to, or `None` for `text` and `border-area`.
fn clip_box(clip: BackgroundClip) -> Option<VisualBox> {
    match clip {
        BackgroundClip::BorderBox => Some(VisualBox::BorderBox),
        BackgroundClip::PaddingBox => Some(VisualBox::PaddingBox),
        BackgroundClip::ContentBox => Some(VisualBox::ContentBox),
        BackgroundClip::Text | BackgroundClip::BorderArea => None,
    }
}

fn fill(shape: &RoundedRect, color: Color, items: &mut Vec<DisplayItem>) {
    if shape.is_square() {
        items.push(DisplayItem::Rect {
            rect: shape.rect,
            color,
        });
    } else {
        items.push(DisplayItem::Path {
            path: shape.path(),
            color,
            rule: FillRule::NonZero,
        });
    }
}

/// Starts clipping to the background painting area of `clip`.
fn push_clip(
    clip: BackgroundClip,
    areas: &BoxAreas,
    text_runs: &mut dyn FnMut() -> Vec<TextRun>,
    items: &mut Vec<DisplayItem>,
) {
    items.push(match clip_box(clip) {
        Some(visual_box) => {
            let shape = areas.visual_box(visual_box);
            if shape.is_square() {
                DisplayItem::PushClip(shape.rect)
            } else {
                DisplayItem::PushClipPath {
                    path: shape.path(),
                    rule: FillRule::NonZero,
                }
            }
        }
        None if clip == BackgroundClip::Text => DisplayItem::PushClipText(text_runs()),
        // `border-area`: the ring between the border box and the padding box.
        None => {
            let mut path = areas.border.path();
            path.extend(areas.padding.path());
            DisplayItem::PushClipPath {
                path,
                rule: FillRule::EvenOdd,
            }
        }
    });
}

/// Where one layer's image is drawn: the size of its image (§ 3.9), its position in the
/// positioning area `area` (§ 3.6) and its repetition across the painting area `painting`
/// (§ 3.4).
pub(crate) fn tiles(
    image: (f32, f32),
    size: &BackgroundSize<LengthPercentage>,
    repeat: (Repeat, Repeat),
    position: (&PositionComponent, &PositionComponent),
    area: Rect,
    painting: Rect,
) -> Vec<Rect> {
    let (iw, ih) = image;
    if iw <= 0.0 || ih <= 0.0 {
        return Vec::new();
    }
    let (aw, ah) = (area.width, area.height);
    let (mut w, mut h, auto_w, auto_h) = match size {
        BackgroundSize::Cover | BackgroundSize::Contain => {
            let (sx, sy) = (aw / iw, ah / ih);
            let scale = if matches!(size, BackgroundSize::Cover) {
                sx.max(sy)
            } else {
                sx.min(sy)
            };
            (iw * scale, ih * scale, false, false)
        }
        BackgroundSize::Explicit(None, None) => (iw, ih, true, true),
        BackgroundSize::Explicit(Some(w), None) => {
            let w = w.resolve(aw);
            (w, w * ih / iw, false, true)
        }
        BackgroundSize::Explicit(None, Some(h)) => {
            let h = h.resolve(ah);
            (h * iw / ih, h, true, false)
        }
        BackgroundSize::Explicit(Some(w), Some(h)) => (w.resolve(aw), h.resolve(ah), false, false),
    };
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    // `round` scales the image to fit a whole number of times; with `auto` on the other axis,
    // that axis scales too, keeping the image's proportions.
    let (w0, h0) = (w, h);
    if repeat.0 == Repeat::Round && aw > 0.0 {
        w = aw / (aw / w).round().max(1.0);
    }
    if repeat.1 == Repeat::Round && ah > 0.0 {
        h = ah / (ah / h).round().max(1.0);
    }
    match (repeat.0 == Repeat::Round, repeat.1 == Repeat::Round) {
        (true, false) if auto_h => h = w * h0 / w0,
        (false, true) if auto_w => w = h * w0 / h0,
        _ => {}
    }

    let x = area.x + position.0.resolve(aw - w);
    let y = area.y + position.1.resolve(ah - h);
    let xs = axis(repeat.0, x, w, area.x, aw, painting.x, painting.right());
    let ys = axis(repeat.1, y, h, area.y, ah, painting.y, painting.bottom());
    let mut out = Vec::new();
    'rows: for &ty in &ys {
        for &tx in &xs {
            if out.len() == MAX_TILES {
                break 'rows;
            }
            out.push(Rect::new(tx, ty, w, h));
        }
    }
    out
}

/// Tile positions along one axis.
fn axis(
    repeat: Repeat,
    position: f32,
    size: f32,
    area_start: f32,
    area_length: f32,
    from: f32,
    to: f32,
) -> Vec<f32> {
    match repeat {
        Repeat::NoRepeat => vec![position],
        Repeat::Repeat | Repeat::Round => repeated(position, size, size, from, to),
        Repeat::Space => {
            // As many whole images as fit, spread out so the first and last touch the edges;
            // with room for only one, it is placed like `no-repeat`.
            let count = (area_length / size).floor();
            if count >= 2.0 {
                let gap = (area_length - count * size) / (count - 1.0);
                repeated(area_start, size, size + gap, from, to)
            } else {
                vec![position]
            }
        }
    }
}

/// Positions `step` apart through `anchor` whose tiles of `size` reach into `[from, to)`.
fn repeated(anchor: f32, size: f32, step: f32, from: f32, to: f32) -> Vec<f32> {
    let mut x = anchor - ((anchor - from) / step).ceil() * step;
    let mut out = Vec::new();
    while x < to && out.len() < MAX_TILES {
        if x + size > from {
            out.push(x);
        }
        x += step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto() -> BackgroundSize<LengthPercentage> {
        BackgroundSize::Explicit(None, None)
    }

    fn at(x: f32, y: f32) -> (PositionComponent, PositionComponent) {
        let p = |v| PositionComponent {
            from_end: false,
            offset: LengthPercentage::Length(v),
        };
        (p(x), p(y))
    }

    const AREA: Rect = Rect::new(0.0, 0.0, 100.0, 50.0);

    #[test]
    fn no_repeat_places_one_image() {
        let (x, y) = at(10.0, 5.0);
        let tiles = tiles(
            (20.0, 10.0),
            &auto(),
            (Repeat::NoRepeat, Repeat::NoRepeat),
            (&x, &y),
            AREA,
            AREA,
        );
        assert_eq!(tiles, [Rect::new(10.0, 5.0, 20.0, 10.0)]);
    }

    #[test]
    fn repeat_covers_the_painting_area_through_the_position() {
        let (x, y) = at(10.0, 0.0);
        let tiles = tiles(
            (40.0, 50.0),
            &auto(),
            (Repeat::Repeat, Repeat::NoRepeat),
            (&x, &y),
            AREA,
            AREA,
        );
        let xs: Vec<f32> = tiles.iter().map(|t| t.x).collect();
        assert_eq!(xs, [-30.0, 10.0, 50.0, 90.0]);
    }

    #[test]
    fn percentages_align_the_image_with_the_area() {
        let center = PositionComponent {
            from_end: false,
            offset: LengthPercentage::Percentage(0.5),
        };
        let end = PositionComponent {
            from_end: true,
            offset: LengthPercentage::Length(5.0),
        };
        let tiles = tiles(
            (20.0, 10.0),
            &auto(),
            (Repeat::NoRepeat, Repeat::NoRepeat),
            (&center, &end),
            AREA,
            AREA,
        );
        assert_eq!(tiles, [Rect::new(40.0, 35.0, 20.0, 10.0)]);
    }

    #[test]
    fn cover_and_contain_keep_proportions() {
        let (x, y) = at(0.0, 0.0);
        let one = |size| {
            tiles(
                (10.0, 10.0),
                &size,
                (Repeat::NoRepeat, Repeat::NoRepeat),
                (&x, &y),
                AREA,
                AREA,
            )[0]
        };
        assert_eq!(one(BackgroundSize::Cover).width, 100.0);
        assert_eq!(one(BackgroundSize::Contain).width, 50.0);
        let half = BackgroundSize::Explicit(Some(LengthPercentage::Percentage(0.5)), None);
        assert_eq!(one(half), Rect::new(0.0, 0.0, 50.0, 50.0));
    }

    #[test]
    fn space_spreads_whole_images_to_the_edges() {
        let (x, y) = at(0.0, 0.0);
        let tiles = tiles(
            (30.0, 50.0),
            &auto(),
            (Repeat::Space, Repeat::NoRepeat),
            (&x, &y),
            AREA,
            AREA,
        );
        let xs: Vec<f32> = tiles.iter().map(|t| t.x).collect();
        assert_eq!(xs, [0.0, 35.0, 70.0]);
    }

    #[test]
    fn round_fits_a_whole_number_and_keeps_proportions() {
        let (x, y) = at(0.0, 0.0);
        let tiles = tiles(
            (30.0, 15.0),
            &auto(),
            (Repeat::Round, Repeat::NoRepeat),
            (&x, &y),
            AREA,
            AREA,
        );
        // 100 / 30 rounds to 3 images of 33.33px, each 16.67px tall.
        assert_eq!(tiles.len(), 3);
        assert!((tiles[0].width - 100.0 / 3.0).abs() < 1e-3);
        assert!((tiles[0].height - 50.0 / 3.0).abs() < 1e-3);
    }
}
