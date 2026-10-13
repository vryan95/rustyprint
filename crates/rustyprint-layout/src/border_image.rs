//! Border images (CSS Backgrounds 3 § 6) and mask borders (CSS Masking 1 § 7): an image cut
//! into nine slices and drawn over a box's border image area.

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    ImageRepeat, ImageSideWidth, ImageSlice, LengthPercentage,
};

use crate::display::{DisplayItem, Rect};
use crate::fragment::Sides;
use crate::picture::Picture;

/// How an image is cut and drawn: `border-image-*` or `mask-border-*`.
pub(crate) struct NineSlice<'a> {
    pub slice: &'a ImageSlice,
    pub width: &'a [ImageSideWidth<LengthPercentage>; 4],
    pub outset: &'a [ImageSideWidth<LengthPercentage>; 4],
    pub repeat: (ImageRepeat, ImageRepeat),
}

/// The border image area: the border box grown by the outsets (§ 6.5).
pub(crate) fn area(nine: &NineSlice, border: Sides, border_box: Rect) -> Rect {
    let widths = [border.top, border.right, border.bottom, border.left];
    let outset = |i: usize| match &nine.outset[i] {
        ImageSideWidth::Number(n) => n * widths[i],
        ImageSideWidth::Length(l) => l.resolve(0.0),
        ImageSideWidth::Auto => 0.0,
    };
    Rect::new(
        border_box.x - outset(3),
        border_box.y - outset(0),
        border_box.width + outset(1) + outset(3),
        border_box.height + outset(0) + outset(2),
    )
}

/// Paints `picture` in nine slices over the border image area of a box whose border box is
/// `border_box` and whose border widths are `border`.
pub(crate) fn paint(
    picture: &Picture,
    style: &ComputedValues,
    nine: &NineSlice,
    border: Sides,
    border_box: Rect,
    items: &mut Vec<DisplayItem>,
) {
    let area = area(nine, border, border_box);
    if area.width <= 0.0 || area.height <= 0.0 {
        return;
    }
    // An image without a natural size is sized to the area.
    let (iw, ih) = picture
        .natural_size(style)
        .unwrap_or((area.width, area.height));
    if iw <= 0.0 || ih <= 0.0 {
        return;
    }
    // Slices: numbers are image pixels, percentages of the image's size.
    let o = &nine.slice.offsets;
    let slice = [
        o[0].resolve(ih).clamp(0.0, ih),
        o[1].resolve(iw).clamp(0.0, iw),
        o[2].resolve(ih).clamp(0.0, ih),
        o[3].resolve(iw).clamp(0.0, iw),
    ];
    let borders = [border.top, border.right, border.bottom, border.left];
    let mut widths = [0.0f32; 4];
    for (i, width) in widths.iter_mut().enumerate() {
        let basis = if i % 2 == 0 { area.height } else { area.width };
        *width = match &nine.width[i] {
            ImageSideWidth::Number(n) => n * borders[i],
            ImageSideWidth::Length(l) => l.resolve(basis),
            ImageSideWidth::Auto => slice[i],
        }
        .max(0.0);
    }
    // Opposite widths that do not fit shrink together (§ 6.3).
    let ratio = |length: f32, sum: f32| if sum > 0.0 { length / sum } else { 1.0 };
    let f = ratio(area.width, widths[1] + widths[3])
        .min(ratio(area.height, widths[0] + widths[2]))
        .min(1.0);
    for width in &mut widths {
        *width *= f;
    }
    let [wt, wr, wb, wl] = widths;
    let [st, sr, sb, sl] = slice;
    let (mid_w, mid_h) = ((iw - sl - sr).max(0.0), (ih - st - sb).max(0.0));
    let (dx0, dx1, dx2) = (area.x, area.x + wl, area.right() - wr);
    let (dy0, dy1, dy2) = (area.y, area.y + wt, area.bottom() - wb);
    let (dmid_w, dmid_h) = ((dx2 - dx1).max(0.0), (dy2 - dy1).max(0.0));

    let mut draw = |source: Rect, dest: Rect, scale: (Option<f32>, Option<f32>)| {
        draw_slice(
            picture,
            style,
            (iw, ih),
            source,
            dest,
            scale,
            nine.repeat,
            items,
        );
    };
    // Corners stretch to their area.
    draw(
        Rect::new(0.0, 0.0, sl, st),
        Rect::new(dx0, dy0, wl, wt),
        (None, None),
    );
    draw(
        Rect::new(iw - sr, 0.0, sr, st),
        Rect::new(dx2, dy0, wr, wt),
        (None, None),
    );
    draw(
        Rect::new(iw - sr, ih - sb, sr, sb),
        Rect::new(dx2, dy2, wr, wb),
        (None, None),
    );
    draw(
        Rect::new(0.0, ih - sb, sl, sb),
        Rect::new(dx0, dy2, wl, wb),
        (None, None),
    );
    // Edges scale to the border's width and repeat along it.
    let factor = |dest: f32, source: f32| (source > 0.0).then(|| dest / source);
    let top = factor(wt, st);
    let bottom = factor(wb, sb);
    let left = factor(wl, sl);
    let right = factor(wr, sr);
    draw(
        Rect::new(sl, 0.0, mid_w, st),
        Rect::new(dx1, dy0, dmid_w, wt),
        (top, None),
    );
    draw(
        Rect::new(sl, ih - sb, mid_w, sb),
        Rect::new(dx1, dy2, dmid_w, wb),
        (bottom, None),
    );
    draw(
        Rect::new(0.0, st, sl, mid_h),
        Rect::new(dx0, dy1, wl, dmid_h),
        (None, left),
    );
    draw(
        Rect::new(iw - sr, st, sr, mid_h),
        Rect::new(dx2, dy1, wr, dmid_h),
        (None, right),
    );
    // The middle scales like the top (or bottom) edge across and the left (or right) edge
    // down.
    if nine.slice.fill {
        let usable = |f: Option<f32>| f.filter(|f| *f > 0.0 && f.is_finite());
        let across = usable(top).or(usable(bottom));
        let down = usable(left).or(usable(right));
        draw(
            Rect::new(sl, st, mid_w, mid_h),
            Rect::new(dx1, dy1, dmid_w, dmid_h),
            (across, down),
        );
    }
}

/// Draws the `source` part of an image of `size` into `dest`. Along an axis with a scale,
/// the slice is that much larger and repeats as `repeat` says; along the others it
/// stretches.
#[allow(clippy::too_many_arguments)]
fn draw_slice(
    picture: &Picture,
    style: &ComputedValues,
    size: (f32, f32),
    source: Rect,
    dest: Rect,
    scale: (Option<f32>, Option<f32>),
    repeat: (ImageRepeat, ImageRepeat),
    items: &mut Vec<DisplayItem>,
) {
    if source.width <= 0.0 || source.height <= 0.0 || dest.width <= 0.0 || dest.height <= 0.0 {
        return;
    }
    // A slice scaled along one axis keeps its proportions in the other.
    let (tile_w, tile_h) = match scale {
        (Some(s), None) => (source.width * s, dest.height),
        (None, Some(s)) => (dest.width, source.height * s),
        (Some(sx), Some(sy)) => (source.width * sx, source.height * sy),
        (None, None) => (dest.width, dest.height),
    };
    let xs = if scale.0.is_some() {
        tiles(repeat.0, dest.x, dest.width, tile_w)
    } else {
        vec![(dest.x, dest.width)]
    };
    let ys = if scale.1.is_some() {
        tiles(repeat.1, dest.y, dest.height, tile_h)
    } else {
        vec![(dest.y, dest.height)]
    };
    items.push(DisplayItem::PushClip(dest));
    for &(y, h) in &ys {
        for &(x, w) in &xs {
            let (sx, sy) = (w / source.width, h / source.height);
            let rect = Rect::new(
                x - source.x * sx,
                y - source.y * sy,
                size.0 * sx,
                size.1 * sy,
            );
            items.push(DisplayItem::PushClip(Rect::new(x, y, w, h)));
            picture.paint(style, rect, items);
            items.push(DisplayItem::PopClip);
        }
    }
    items.push(DisplayItem::PopClip);
}

/// Most tiles along one edge, so a tiny slice cannot flood the page.
const MAX_TILES: usize = 1_000;

/// The tiles along an axis from `start` across `length`, each `(position, size)`, for tiles
/// of `size` (§ 6.4).
fn tiles(repeat: ImageRepeat, start: f32, length: f32, size: f32) -> Vec<(f32, f32)> {
    if size <= 0.0 || !size.is_finite() {
        return Vec::new();
    }
    match repeat {
        ImageRepeat::Stretch => vec![(start, length)],
        ImageRepeat::Repeat => {
            // Tiles centered on the middle of the edge.
            let first = start + (length - size) / 2.0;
            let before = ((first - start) / size).ceil().max(0.0);
            let mut x = first - before * size;
            let mut out = Vec::new();
            while x < start + length && out.len() < MAX_TILES {
                out.push((x, size));
                x += size;
            }
            out
        }
        ImageRepeat::Round => {
            let count = (length / size).round().max(1.0).min(MAX_TILES as f32);
            let size = length / count;
            (0..count as usize)
                .map(|i| (start + i as f32 * size, size))
                .collect()
        }
        ImageRepeat::Space => {
            let count = (length / size).floor().min(MAX_TILES as f32);
            if count < 1.0 {
                return Vec::new();
            }
            let gap = (length - count * size) / (count + 1.0);
            (0..count as usize)
                .map(|i| (start + gap + i as f32 * (size + gap), size))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_modes_tile_an_edge() {
        assert_eq!(
            tiles(ImageRepeat::Stretch, 0.0, 100.0, 30.0),
            [(0.0, 100.0)]
        );
        // Centered: the middle tile spans 35 to 65.
        let repeated = tiles(ImageRepeat::Repeat, 0.0, 100.0, 30.0);
        assert!(repeated.contains(&(35.0, 30.0)));
        assert_eq!(repeated.len(), 5);
        let round = tiles(ImageRepeat::Round, 0.0, 100.0, 30.0);
        assert_eq!(round.len(), 3);
        assert!((round[0].1 - 100.0 / 3.0).abs() < 1e-4);
        assert_eq!(
            tiles(ImageRepeat::Space, 0.0, 100.0, 30.0),
            [(2.5, 30.0), (35.0, 30.0), (67.5, 30.0)]
        );
    }
}
