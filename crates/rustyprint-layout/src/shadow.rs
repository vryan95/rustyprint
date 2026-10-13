//! Box shadows (CSS Backgrounds 3 § 7.1): outer shadows below the background, outside the
//! border box, and inset shadows above it, inside the padding box.

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::BoxShadowValue;

use crate::display::{Color, DisplayItem, FillRule, Path, Rect};
use crate::rounded::RoundedRect;

fn color(shadow: &BoxShadowValue, style: &ComputedValues) -> Color {
    Color::from_css(&shadow.shadow.color.resolve(&style.color))
}

/// A rectangle around `rect` so large that blurring a shape's edge never reaches its sides.
fn surrounding(rect: Rect, reach: f32) -> Path {
    let outer = rect.inflate(reach);
    Path::polygon(&[
        crate::display::Point::new(outer.x, outer.y),
        crate::display::Point::new(outer.right(), outer.y),
        crate::display::Point::new(outer.right(), outer.bottom()),
        crate::display::Point::new(outer.x, outer.bottom()),
    ])
}

/// Fills `path`, blurred by a Gaussian of standard deviation half the blur radius.
fn fill(path: Path, rule: FillRule, color: Color, blur: f32, items: &mut Vec<DisplayItem>) {
    let shape = vec![DisplayItem::Path { path, color, rule }];
    items.extend(crate::filter::blurred(shape, blur / 2.0));
}

/// Paints the outer shadows of a box whose border box is `border_box`, the last shadow
/// lowest.
pub(crate) fn paint_outer(
    style: &ComputedValues,
    border_box: &RoundedRect,
    items: &mut Vec<DisplayItem>,
) {
    for shadow in style.box_shadow.iter().rev().filter(|s| !s.inset) {
        let color = color(shadow, style);
        if color.is_transparent() {
            continue;
        }
        let s = &shadow.shadow;
        let mut shape = border_box.inflate(shadow.spread);
        shape.rect = shape.rect.translated(s.x, s.y);
        if shape.rect.width <= 0.0 || shape.rect.height <= 0.0 {
            continue;
        }
        // Only the part outside the border box shows.
        let reach = s.blur * 2.0 + shadow.spread.abs() + s.x.abs() + s.y.abs() + 1.0;
        let mut outside = surrounding(border_box.rect, reach);
        outside.extend(border_box.path());
        items.push(DisplayItem::PushClipPath {
            path: outside,
            rule: FillRule::EvenOdd,
        });
        fill(shape.path(), FillRule::NonZero, color, s.blur, items);
        items.push(DisplayItem::PopClip);
    }
}

/// Paints the inset shadows of a box whose padding box is `padding_box`, the last shadow
/// lowest.
pub(crate) fn paint_inset(
    style: &ComputedValues,
    padding_box: &RoundedRect,
    items: &mut Vec<DisplayItem>,
) {
    for shadow in style.box_shadow.iter().rev().filter(|s| s.inset) {
        let color = color(shadow, style);
        if color.is_transparent() {
            continue;
        }
        let s = &shadow.shadow;
        let spread = shadow.spread;
        let mut hole = padding_box.deflate(spread, spread, spread, spread);
        hole.rect = hole.rect.translated(s.x, s.y);
        // The shadow covers the padding box except the hole, and only shows inside it.
        let reach = s.blur * 2.0 + spread.abs() + s.x.abs() + s.y.abs() + 1.0;
        let mut shape = surrounding(padding_box.rect, reach);
        if hole.rect.width > 0.0 && hole.rect.height > 0.0 {
            shape.extend(hole.path());
        }
        items.push(if padding_box.is_square() {
            DisplayItem::PushClip(padding_box.rect)
        } else {
            DisplayItem::PushClipPath {
                path: padding_box.path(),
                rule: FillRule::NonZero,
            }
        });
        fill(shape, FillRule::EvenOdd, color, s.blur, items);
        items.push(DisplayItem::PopClip);
    }
}
