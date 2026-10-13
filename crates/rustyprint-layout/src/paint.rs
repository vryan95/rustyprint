//! Painting: fragments into display items (CSS 2.1 Appendix E, simplified to tree order).

use std::cell::RefCell;
use std::collections::HashMap;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    AccentColor, BlendMode as CssBlendMode, BorderStyle, EmphasisShape, ObjectFit, Overflow,
    Position, TextDecorationStyle, TextEmphasisPosition, TextEmphasisStyle, Visibility, VisualBox,
    ZIndex,
};
use rustyprint_css::values::Color as CssColor;

use crate::background::{BoxAreas, paint_background};
use crate::boxes::{MeterLevel, Replaced, Widget};
use crate::display::{Color, DisplayItem, FillRule, LineStyle, Path, Point, Rect, TextRun};
use crate::effects::{self, Space};
use crate::fragment::{
    BoxFragment, Decoration, DecorationKind, Fragment, LineItem, Sides, TextFragment,
};
use crate::images::Images;
use crate::paginate::LaidPage;
use crate::picture::Picture;
use crate::rounded::RoundedRect;
use crate::text::shape;

/// What painting produces: display items in the root's coordinate space.
///
/// Boxes paint in tree order, except positioned boxes, which their stacking context paints
/// in layers ordered by `z-index` (CSS 2.1 Appendix E): those below zero after its
/// background and borders, the others after its content.
pub(crate) struct Painted<'a, 'f> {
    pub items: Vec<DisplayItem>,
    images: &'a Images<'a>,
    /// The page area, which fixed backgrounds are positioned in.
    viewport: Rect,
    /// The clips in effect, outermost first, which layers painted later apply again.
    clips: Vec<DisplayItem>,
    /// Positioned boxes found in the current stacking context, to paint as layers.
    layers: Vec<Layer<'f>>,
    /// The tree-order key of the layer being painted, and how many layers it has found.
    key: Vec<u32>,
    found: u32,
    /// Where the content of the stacking context's root starts, once its background and
    /// borders are painted.
    split: Option<usize>,
    /// The 3D space the children of the box being painted are transformed in.
    space: Option<Space>,
    /// The border box of the box whose children are being painted, the containing block
    /// motion paths are laid in.
    parent: Rect,
    /// Whether something painted in the current stacking context blends with what is below
    /// it, which isolates the stacking context.
    blends: bool,
}

/// A positioned box set aside to paint in its stacking context's order.
#[derive(Clone)]
struct Layer<'f> {
    fragment: &'f BoxFragment,
    /// Where its parent's border box is.
    x: f32,
    y: f32,
    /// The clips in effect where it was found.
    clips: Vec<DisplayItem>,
    /// Its place in tree order, among the layers of its stacking context.
    key: Vec<u32>,
    /// Its `z-index`, or 0 for boxes that are layers for their effects alone.
    level: i32,
    /// The space its parent transforms it in, and its parent's border box.
    space: Option<Space>,
    parent: Rect,
}

pub(crate) fn paint<'a, 'f>(
    root: &'f BoxFragment,
    images: &'a Images<'a>,
    viewport: Rect,
) -> Painted<'a, 'f> {
    let mut painted = Painted {
        items: Vec::new(),
        images,
        viewport,
        clips: Vec::new(),
        layers: Vec::new(),
        key: Vec::new(),
        found: 0,
        split: None,
        space: None,
        parent: viewport,
        blends: false,
    };
    paint_context(root, 0.0, 0.0, &mut painted, None, viewport, true);
    painted
}

/// Whether a box paints as a layer of its stacking context: positioned boxes do, flex and
/// grid items with an integer `z-index` (CSS Flexbox 1 § 4.3, CSS Grid 2 § 6.3), and boxes
/// with effects such as opacity or transforms, as if positioned with `z-index: 0`.
fn is_layer(fragment: &BoxFragment, flex_item: bool) -> bool {
    crate::position::is_positioned(&fragment.style)
        || (flex_item && fragment.style.z_index != ZIndex::Auto)
        || effects::stacking(&fragment.style)
        || effects::contained(&fragment.style)
}

/// Whether a box is a flex or grid container, whose children are its items.
fn is_flex(fragment: &BoxFragment) -> bool {
    matches!(
        fragment.style.display,
        rustyprint_css::properties::types::Display::Normal {
            inside: rustyprint_css::properties::types::DisplayInside::Flex
                | rustyprint_css::properties::types::DisplayInside::Grid,
            ..
        }
    )
}

/// Whether a layer establishes a stacking context of its own, which paints its positioned
/// descendants: positioned boxes with an integer `z-index` do, and boxes with effects.
fn is_context(fragment: &BoxFragment) -> bool {
    fragment.style.z_index != ZIndex::Auto
        || effects::stacking(&fragment.style)
        || effects::contained(&fragment.style)
}

/// Paints a box and its descendants as a stacking context, with the box's effects. `space`
/// is the 3D space its parent transforms it in and `parent` its parent's border box; `top`
/// is set for the root of the page, which needs no isolation.
fn paint_context<'f>(
    fragment: &'f BoxFragment,
    x: f32,
    y: f32,
    out: &mut Painted<'_, 'f>,
    space: Option<Space>,
    parent: Rect,
    top: bool,
) {
    let layers = std::mem::take(&mut out.layers);
    let key = std::mem::take(&mut out.key);
    let found = std::mem::replace(&mut out.found, 0);
    let split = out.split.take();
    let blends = std::mem::replace(&mut out.blends, false);
    let start = out.items.len();
    let rect = fragment.rect.translated(x, y);
    let effects = effects::stacking(&fragment.style);
    let prepared = effects.then(|| effects::prepare(fragment, rect, parent, space));
    let space = std::mem::replace(
        &mut out.space,
        prepared.as_ref().and_then(|prepared| prepared.children),
    );
    paint_contents(fragment, x, y, out, true);
    out.space = space;
    let content = out
        .items
        .split_off(out.split.take().unwrap_or(start).max(start));
    // Painting a layer without a stacking context of its own finds more layers.
    let mut painted: Vec<(i32, Vec<u32>, Vec<DisplayItem>)> = Vec::new();
    let mut index = 0;
    while index < out.layers.len() {
        let layer = out.layers[index].clone();
        index += 1;
        let outer = std::mem::take(&mut out.items);
        out.items.extend(layer.clips.iter().cloned());
        let clips = std::mem::replace(&mut out.clips, layer.clips.clone());
        if is_context(layer.fragment) {
            paint_context(
                layer.fragment,
                layer.x,
                layer.y,
                out,
                layer.space,
                layer.parent,
                false,
            );
        } else {
            out.key = layer.key.clone();
            out.found = 0;
            paint_contents(layer.fragment, layer.x, layer.y, out, false);
        }
        out.clips = clips;
        out.items
            .extend(layer.clips.iter().map(|_| DisplayItem::PopClip));
        let items = std::mem::replace(&mut out.items, outer);
        painted.push((layer.level, layer.key, items));
    }
    painted.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let (below, above): (Vec<_>, Vec<_>) = painted.into_iter().partition(|(z, ..)| *z < 0);
    out.items
        .extend(below.into_iter().flat_map(|(.., items)| items));
    out.items.extend(content);
    out.items
        .extend(above.into_iter().flat_map(|(.., items)| items));
    // Blending inside a stacking context only blends with what is in it.
    let isolate = out.blends && !top;
    if effects || isolate {
        let items = out.items.split_off(start);
        let geometry = effects::Geometry::of(fragment, rect);
        out.items.extend(effects::wrap(
            &geometry,
            items,
            prepared.as_ref(),
            out.images,
            isolate,
        ));
    }
    out.blends = blends || fragment.style.mix_blend_mode != CssBlendMode::Normal;
    out.layers = layers;
    out.key = key;
    out.found = found;
    out.split = split;
}

/// Paints a box whose parent's border box is at `(x, y)`, or sets it aside as a layer of
/// the stacking context if it is positioned.
fn paint_box<'f>(
    fragment: &'f BoxFragment,
    x: f32,
    y: f32,
    out: &mut Painted<'_, 'f>,
    flex_item: bool,
) {
    if is_layer(fragment, flex_item) {
        out.found += 1;
        let mut key = out.key.clone();
        key.push(out.found);
        let ordered = crate::position::is_positioned(&fragment.style) || flex_item;
        out.layers.push(Layer {
            fragment,
            x,
            y,
            clips: out.clips.clone(),
            key,
            level: if ordered {
                fragment.style.z_index.level()
            } else {
                0
            },
            space: out.space,
            parent: out.parent,
        });
        return;
    }
    paint_contents(fragment, x, y, out, false);
}

/// Paints a page's background over the whole sheet, bleed included, and the page box's
/// borders (CSS Paged Media 3 § 3.2).
pub(crate) fn paint_page(page: &LaidPage, images: &Images, bleed: f32) -> Vec<DisplayItem> {
    let style = &page.style.style;
    let mut items = Vec::new();
    let border_box = RoundedRect::border_box(style, page.page_box);
    let (b, p) = (page.border, page.padding);
    let padding = border_box.deflate(b.top, b.right, b.bottom, b.left);
    let areas = BoxAreas {
        border: RoundedRect::new(
            Rect::new(
                -bleed,
                -bleed,
                page.size.width + 2.0 * bleed,
                page.size.height + 2.0 * bleed,
            ),
            [(0.0, 0.0); 4],
        ),
        padding,
        content: padding.deflate(p.top, p.right, p.bottom, p.left),
    };
    paint_background(style, &areas, &Vec::new, images, page.page_box, &mut items);
    paint_borders(style, page.border, &border_box, &mut items);
    items
}

fn color(value: &CssColor, style: &ComputedValues) -> Color {
    Color::from_css(&value.resolve(&style.color))
}

/// Paints a box and its descendants, other than positioned ones, whose parent's border box
/// is at `(x, y)`. The root of a stacking context marks where its content starts.
fn paint_contents<'f>(
    fragment: &'f BoxFragment,
    x: f32,
    y: f32,
    out: &mut Painted<'_, 'f>,
    root: bool,
) {
    let style = &fragment.style;
    let rect = fragment.rect.translated(x, y);
    let visible = style.visibility == Visibility::Visible;
    // A fieldset's border and background start at its top border, which runs through the
    // middle of its legend.
    let decorated = match fragment.legend {
        Some(gap) => Rect::new(rect.x, rect.y + gap.y, rect.width, rect.height - gap.y),
        None => rect,
    };
    let shape = RoundedRect::border_box(style, decorated);
    // Only a stacking context's root passes its 3D space on to its children.
    let inherited = if root { out.space } else { None };
    let space = std::mem::replace(&mut out.space, inherited);
    let parent = std::mem::replace(&mut out.parent, rect);
    // `clip` clips an absolutely positioned box and its descendants (CSS 2.1 § 11.1.2).
    let clip = clip_rect(style, rect);
    if let Some(clip) = clip {
        out.items.push(DisplayItem::PushClip(clip));
        out.clips.push(DisplayItem::PushClip(clip));
    }
    if visible {
        let areas = areas(&shape, fragment.border, fragment.padding);
        let text = || {
            let mut runs = Vec::new();
            box_runs(&fragment.children, rect.x, rect.y, &mut runs);
            runs
        };
        crate::shadow::paint_outer(style, &shape, &mut out.items);
        let border_shape = border_shape(style, &areas);
        if let Some((inner, _)) = &border_shape {
            out.items.push(DisplayItem::PushClipPath {
                path: inner.clone(),
                rule: FillRule::NonZero,
            });
        }
        paint_background(
            style,
            &areas,
            &text,
            out.images,
            out.viewport,
            &mut out.items,
        );
        if border_shape.is_some() {
            out.items.push(DisplayItem::PopClip);
        }
        crate::shadow::paint_inset(style, &areas.padding, &mut out.items);
        match fragment.legend {
            Some(gap) => {
                // The top border has a gap behind the legend.
                let corners = |r: Rect| {
                    [
                        Point::new(r.x, r.y),
                        Point::new(r.right(), r.y),
                        Point::new(r.right(), r.bottom()),
                        Point::new(r.x, r.bottom()),
                    ]
                };
                let hole = gap.translated(rect.x, rect.y);
                let mut path = Path::polygon(&corners(shape.rect.inflate(1.0)));
                path.extend(Path::polygon(&corners(hole)));
                out.items.push(DisplayItem::PushClipPath {
                    path,
                    rule: FillRule::EvenOdd,
                });
                paint_border_or_image(style, fragment.border, &shape, out.images, &mut out.items);
                out.items.push(DisplayItem::PopClip);
            }
            None => match border_shape {
                Some((_, border)) => out.items.extend(border),
                None => paint_border_or_image(
                    style,
                    fragment.border,
                    &shape,
                    out.images,
                    &mut out.items,
                ),
            },
        }
        if let Some(replaced) = &fragment.replaced {
            paint_replaced(
                style,
                replaced,
                areas.content.rect,
                out.images,
                &mut out.items,
            );
        }
    }
    if root {
        out.split = Some(out.items.len());
    }
    // Paint containment clips like `overflow: clip` in both axes.
    let paint_contained = style.used_contain().paint;
    let overflow = |o: Overflow| {
        if paint_contained && o == Overflow::Visible {
            Overflow::Clip
        } else {
            o
        }
    };
    let (overflow_x, overflow_y) = (overflow(style.overflow_x), overflow(style.overflow_y));
    let clip_x = overflow_x != Overflow::Visible;
    let clip_y = overflow_y != Overflow::Visible;
    if clip_x || clip_y {
        // `overflow: clip` clips at `overflow-clip-margin`; other values at the padding box.
        let b = fragment.border;
        let padding_shape = shape.deflate(b.top, b.right, b.bottom, b.left);
        let margin = style.overflow_clip_margin;
        let p = fragment.padding;
        let margin_shape = match margin.visual_box {
            VisualBox::BorderBox => shape,
            VisualBox::PaddingBox => padding_shape,
            VisualBox::ContentBox => padding_shape.deflate(p.top, p.right, p.bottom, p.left),
        }
        .inflate(margin.margin);
        let edge = |overflow: Overflow| {
            if overflow == Overflow::Clip {
                margin_shape
            } else {
                padding_shape
            }
        };
        let (shape_x, shape_y) = (edge(overflow_x), edge(overflow_y));
        let clip = if clip_x && clip_y && shape_x == shape_y && !shape_x.is_square() {
            DisplayItem::PushClipPath {
                path: shape_x.path(),
                rule: FillRule::NonZero,
            }
        } else {
            // Content overflows freely along an axis that is not clipped.
            let (x, y) = (shape_x.rect, shape_y.rect);
            let far = 1.0e7;
            let clip = Rect::new(
                if clip_x { x.x } else { -far },
                if clip_y { y.y } else { -far },
                if clip_x { x.width } else { 2.0 * far },
                if clip_y { y.height } else { 2.0 * far },
            );
            DisplayItem::PushClip(clip)
        };
        out.items.push(clip.clone());
        out.clips.push(clip);
    }
    let flex = is_flex(fragment);
    for child in &fragment.children {
        match child {
            Fragment::Box(child) => paint_box(child, rect.x, rect.y, out, flex),
            Fragment::Line(line) => {
                let line_rect = line.rect.translated(rect.x, rect.y);
                match line.turned() {
                    None => paint_items(&line.items, line_rect.x, line_rect.y, out, None),
                    Some(turn) => {
                        // Text and inline boxes paint in the axes they were laid out in.
                        let [a, b, c, d, e, f] = turn.matrix(line.content_size());
                        let matrix = [a, b, c, d, e + line_rect.x, f + line_rect.y];
                        let turned = Turned {
                            matrix,
                            x: line_rect.x,
                            y: line_rect.y,
                        };
                        out.items.push(DisplayItem::PushTransform(matrix));
                        paint_items(&line.items, 0.0, 0.0, out, Some(&turned));
                        out.items.push(DisplayItem::PopClip);
                    }
                }
            }
        }
    }
    if clip_x || clip_y {
        out.items.push(DisplayItem::PopClip);
        out.clips.pop();
    }
    if visible {
        paint_outline(style, &shape, &mut out.items);
    }
    if clip.is_some() {
        out.items.push(DisplayItem::PopClip);
        out.clips.pop();
    }
    out.space = space;
    out.parent = parent;
}

/// The clip of `clip: rect(...)` on an absolutely positioned box whose border box is
/// `rect`: offsets from its top-left corner, with `auto` at the border box's edges.
fn clip_rect(style: &ComputedValues, rect: Rect) -> Option<Rect> {
    if !matches!(style.position, Position::Absolute | Position::Fixed) {
        return None;
    }
    let [top, right, bottom, left] = style.clip?;
    let (x0, y0) = (left.unwrap_or(0.0), top.unwrap_or(0.0));
    let (x1, y1) = (right.unwrap_or(rect.width), bottom.unwrap_or(rect.height));
    Some(Rect::new(
        rect.x + x0,
        rect.y + y0,
        (x1 - x0).max(0.0),
        (y1 - y0).max(0.0),
    ))
}

/// Paints the border image if `border-image-source` gives one, and the borders otherwise
/// (CSS Backgrounds 3 § 6).
fn paint_border_or_image(
    style: &ComputedValues,
    border: Sides,
    shape: &RoundedRect,
    images: &Images,
    items: &mut Vec<DisplayItem>,
) {
    if let Some(picture) =
        crate::picture::Picture::resolve(&style.border_image_source, images, style)
    {
        let nine = crate::border_image::NineSlice {
            slice: &style.border_image_slice,
            width: &style.border_image_width,
            outset: &style.border_image_outset,
            repeat: style.border_image_repeat,
        };
        crate::border_image::paint(&picture, style, &nine, border, shape.rect, items);
        return;
    }
    paint_borders(style, border, shape, items);
}

/// For `border-shape` (CSS Borders 4 § 7): the shape the background is clipped to, and the
/// items of the border. With one shape the border is stroked along it, with the top
/// border's width; with two it fills the area between them. Either way it takes the top
/// border's color.
fn border_shape(style: &ComputedValues, areas: &BoxAreas) -> Option<(Path, Vec<DisplayItem>)> {
    use rustyprint_css::properties::types::GeometryBox;
    let shapes = &style.border_shape;
    if shapes.is_empty() {
        return None;
    }
    let path = |index: usize, default: GeometryBox| {
        let value = &shapes[index];
        let reference = match value.reference.unwrap_or(default) {
            GeometryBox::ContentBox | GeometryBox::FillBox => areas.content.rect,
            GeometryBox::PaddingBox => areas.padding.rect,
            _ => areas.border.rect,
        };
        let mut path = Path::default();
        for polygon in crate::shapes::shape_polygons(&value.shape, reference) {
            path.extend(Path::polygon(&polygon));
        }
        path
    };
    let border_color = color(&style.border_top_color, style);
    let mut items = Vec::new();
    let inner = if shapes.len() == 1 {
        let outline = path(0, GeometryBox::BorderBox);
        if style.border_top_width > 0.0 && !border_color.is_transparent() {
            items.push(DisplayItem::StrokePath {
                path: outline.clone(),
                width: style.border_top_width,
                color: border_color,
                style: LineStyle::Solid,
            });
        }
        outline
    } else {
        let outer = path(0, GeometryBox::BorderBox);
        let inner = path(1, GeometryBox::PaddingBox);
        if !border_color.is_transparent() {
            let mut ring = outer;
            ring.extend(inner.clone());
            items.push(DisplayItem::Path {
                path: ring,
                color: border_color,
                rule: FillRule::EvenOdd,
            });
        }
        inner
    };
    Some((inner, items))
}

/// Paints the content of a replaced element in its content box: an image fitted by
/// `object-fit`, placed by `object-position` and cropped by `object-view-box`, or media
/// controls.
fn paint_replaced(
    style: &ComputedValues,
    replaced: &Replaced,
    content: Rect,
    images: &Images,
    items: &mut Vec<DisplayItem>,
) {
    if let Some(widget) = &replaced.widget {
        paint_widget(style, widget, content, items);
    }
    let Some(picture) = &replaced.picture else {
        return;
    };
    // A framed document, clipped to the content box, which is its viewport.
    if let Picture::Frame(frame) = picture {
        if let Some(laid) = &frame.laid {
            let viewport = Rect::new(0.0, 0.0, content.width, content.height);
            items.push(DisplayItem::PushClip(content));
            items.extend(
                paint(laid, images, viewport)
                    .items
                    .iter()
                    .map(|item| item.translated(content.x, content.y)),
            );
            items.push(DisplayItem::PopClip);
        }
        return;
    }
    // Without a natural size, the image fills the content box.
    let Some((natural_w, natural_h)) = replaced.size else {
        picture.paint(style, content, items);
        return;
    };
    if natural_w <= 0.0 || natural_h <= 0.0 || content.width <= 0.0 || content.height <= 0.0 {
        return;
    }
    // The part of the image shown, and the size it is fitted at.
    let (vx, vy, vw, vh) = style
        .object_view_box
        .resolve(natural_w, natural_h)
        .filter(|&(_, _, w, h)| w > 0.0 && h > 0.0)
        .unwrap_or((0.0, 0.0, natural_w, natural_h));
    let contain = (content.width / vw).min(content.height / vh);
    let (sx, sy) = match style.object_fit {
        ObjectFit::Fill => (content.width / vw, content.height / vh),
        ObjectFit::Contain => (contain, contain),
        ObjectFit::Cover => {
            let cover = (content.width / vw).max(content.height / vh);
            (cover, cover)
        }
        ObjectFit::None => (1.0, 1.0),
        ObjectFit::ScaleDown => (contain.min(1.0), contain.min(1.0)),
    };
    let (shown_w, shown_h) = (vw * sx, vh * sy);
    let (position_x, position_y) = &style.object_position;
    let x = content.x + position_x.resolve(content.width - shown_w);
    let y = content.y + position_y.resolve(content.height - shown_h);
    let rect = Rect::new(x - vx * sx, y - vy * sy, natural_w * sx, natural_h * sy);
    let overflows = rect.x < content.x - 0.01
        || rect.y < content.y - 0.01
        || rect.right() > content.right() + 0.01
        || rect.bottom() > content.bottom() + 0.01;
    if overflows {
        items.push(DisplayItem::PushClip(content));
    }
    picture.paint(style, rect, items);
    if overflows {
        items.push(DisplayItem::PopClip);
    }
}

/// The light gray of control tracks and the border of unchecked controls.
const TRACK: Color = Color::rgb(239, 239, 239);
const CONTROL_BORDER: Color = Color::rgb(118, 118, 118);
/// Disabled controls are drawn in lighter grays.
const DISABLED_BORDER: Color = Color::rgb(198, 198, 198);
const DISABLED_FILL: Color = Color::rgb(232, 232, 232);
const WHITE: Color = Color::rgb(255, 255, 255);

/// The `accent-color` of a control: its value, or the default blue for `auto`.
fn accent(style: &ComputedValues) -> Color {
    match &style.accent_color {
        AccentColor::Auto => Color::rgb(0, 117, 255),
        AccentColor::Color(value) => color(value, style),
    }
}

/// White or black, whichever shows better on `background`.
fn contrasting(background: Color) -> Color {
    let channel = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(background.r)
        + 0.7152 * channel(background.g)
        + 0.0722 * channel(background.b);
    if luminance > 0.36 {
        Color::BLACK
    } else {
        WHITE
    }
}

/// A filled rectangle with all corners rounded by `radius`.
fn rounded(rect: Rect, radius: f32, color: Color, items: &mut Vec<DisplayItem>) {
    let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
    items.push(DisplayItem::Path {
        path: RoundedRect::new(rect, [(radius, radius); 4]).path(),
        color,
        rule: FillRule::NonZero,
    });
}

/// A filled circle.
fn disc(center: Point, radius: f32, color: Color, items: &mut Vec<DisplayItem>) {
    let rect = Rect::new(
        center.x - radius,
        center.y - radius,
        2.0 * radius,
        2.0 * radius,
    );
    rounded(rect, radius, color, items);
}

/// A rounded rectangle with a border of `width`, as two filled shapes.
fn framed(
    rect: Rect,
    radius: f32,
    width: f32,
    (fill, border): (Color, Color),
    items: &mut Vec<DisplayItem>,
) {
    rounded(rect, radius, border, items);
    rounded(
        rect.deflate(width, width, width, width),
        radius - width,
        fill,
        items,
    );
}

/// The largest square that fits in `content`, centered.
fn centered_square(content: Rect) -> Rect {
    let side = content.width.min(content.height);
    Rect::new(
        content.x + (content.width - side) / 2.0,
        content.y + (content.height - side) / 2.0,
        side,
        side,
    )
}

/// A check mark stroked inside `square`.
fn check_mark(square: Rect, color: Color, items: &mut Vec<DisplayItem>) {
    let point =
        |x: f32, y: f32| Point::new(square.x + x * square.width, square.y + y * square.height);
    items.push(DisplayItem::Polyline {
        points: vec![point(0.2, 0.52), point(0.42, 0.74), point(0.8, 0.3)],
        width: (square.width * 0.15).max(1.0),
        color,
        closed: false,
    });
}

/// A bar from the start of `track` filled to `fraction` of it.
fn bar(track: Rect, fraction: f32, color: Color, items: &mut Vec<DisplayItem>) {
    if fraction > 0.0 {
        let rect = Rect::new(
            track.x,
            track.y,
            track.width * fraction.min(1.0),
            track.height,
        );
        rounded(rect, track.height / 2.0, color, items);
    }
}

/// Paints a form or media control in the content box of its element, in its static state.
fn paint_widget(
    style: &ComputedValues,
    widget: &Widget,
    content: Rect,
    items: &mut Vec<DisplayItem>,
) {
    if content.width <= 0.0 || content.height <= 0.0 {
        return;
    }
    let current = || color(&CssColor::CurrentColor, style);
    // Unchecked controls are white with a gray border; disabled ones lighter.
    let unchecked = |disabled: bool| {
        if disabled {
            (DISABLED_FILL, DISABLED_BORDER)
        } else {
            (WHITE, CONTROL_BORDER)
        }
    };
    let fill = |disabled: bool| {
        if disabled {
            DISABLED_BORDER
        } else {
            accent(style)
        }
    };
    match *widget {
        Widget::MediaControls => paint_controls(content, items),
        Widget::Checkbox { checked, disabled } => {
            let square = centered_square(content);
            let radius = square.width * 0.15;
            if checked {
                rounded(square, radius, fill(disabled), items);
                check_mark(square, contrasting(fill(disabled)), items);
            } else {
                framed(square, radius, 1.0, unchecked(disabled), items);
            }
        }
        Widget::Radio { checked, disabled } => {
            let square = centered_square(content);
            let center = Point::new(
                square.x + square.width / 2.0,
                square.y + square.height / 2.0,
            );
            let r = square.width / 2.0;
            if checked {
                disc(center, r, fill(disabled), items);
                disc(center, r * 0.75, contrasting(fill(disabled)), items);
                disc(center, r * 0.5, fill(disabled), items);
            } else {
                let (inside, border) = unchecked(disabled);
                disc(center, r, border, items);
                disc(center, (r - 1.0).max(0.0), inside, items);
            }
        }
        Widget::Progress { position } => {
            framed(
                content,
                content.height / 2.0,
                1.0,
                (TRACK, CONTROL_BORDER),
                items,
            );
            // An indeterminate bar shows no value.
            if let Some(position) = position {
                bar(content, position, accent(style), items);
            }
        }
        Widget::Meter { fraction, level } => {
            framed(
                content,
                content.height / 2.0,
                1.0,
                (TRACK, CONTROL_BORDER),
                items,
            );
            let color = match level {
                MeterLevel::Optimum => Color::rgb(16, 124, 16),
                MeterLevel::Suboptimum => Color::rgb(255, 185, 0),
                MeterLevel::EvenLessGood => Color::rgb(209, 52, 56),
            };
            bar(content, fraction, color, items);
        }
        Widget::Range { fraction, disabled } => {
            let thumb = content.height.min(16.0) / 2.0;
            let mid = content.y + content.height / 2.0;
            let track = Rect::new(content.x, mid - 2.0, content.width, 4.0);
            framed(track, 2.0, 1.0, (TRACK, Color::rgb(178, 178, 178)), items);
            let span = (content.width - 2.0 * thumb).max(0.0);
            let x = content.x + thumb + span * fraction.clamp(0.0, 1.0);
            let filled = Rect::new(content.x, mid - 2.0, x - content.x, 4.0);
            rounded(filled, 2.0, fill(disabled), items);
            disc(Point::new(x, mid), thumb, fill(disabled), items);
        }
        Widget::Color([r, g, b]) => {
            framed(
                content,
                0.0,
                1.0,
                (Color::rgb(r, g, b), Color::rgb(119, 119, 119)),
                items,
            );
        }
        Widget::PickerIcon => {
            // A chevron pointing down, in the text color.
            let square = centered_square(content);
            let point = |x: f32, y: f32| {
                Point::new(square.x + x * square.width, square.y + y * square.height)
            };
            items.push(DisplayItem::Polyline {
                points: vec![point(0.15, 0.35), point(0.5, 0.7), point(0.85, 0.35)],
                width: (square.width * 0.14).max(1.0),
                color: current(),
                closed: false,
            });
        }
        Widget::Checkmark => check_mark(centered_square(content), current(), items),
        Widget::Dot => {
            let square = centered_square(content);
            let center = Point::new(
                square.x + square.width / 2.0,
                square.y + square.height / 2.0,
            );
            disc(center, square.width * 0.3, current(), items);
        }
    }
}

/// A static media control bar: a play button and a timeline.
fn paint_controls(content: Rect, items: &mut Vec<DisplayItem>) {
    let bar = RoundedRect::new(content, [(content.height / 2.0, content.height / 2.0); 4]);
    items.push(DisplayItem::Path {
        path: bar.path(),
        color: Color::rgba(241, 243, 244, 255),
        rule: FillRule::NonZero,
    });
    let mid = content.y + content.height / 2.0;
    let size = (content.height * 0.3).min(12.0);
    let left = content.x + content.height / 2.0;
    items.push(DisplayItem::Polygon {
        points: vec![
            Point::new(left - size * 0.4, mid - size / 2.0),
            Point::new(left + size * 0.6, mid),
            Point::new(left - size * 0.4, mid + size / 2.0),
        ],
        color: Color::rgba(32, 33, 36, 255),
    });
    let start = left + content.height / 2.0;
    let end = content.right() - content.height / 2.0;
    if end > start {
        items.push(DisplayItem::Rect {
            rect: Rect::new(start, mid - 1.5, end - start, 3.0),
            color: Color::rgba(160, 160, 160, 255),
        });
    }
}

/// A line whose content paints turned by `matrix`, which is in effect. Its atomic inlines
/// are turned already, relative to the line at `(x, y)`.
struct Turned {
    matrix: [f32; 6],
    x: f32,
    y: f32,
}

/// Paints line items positioned relative to a line at `(x, y)`.
fn paint_items<'f>(
    items: &'f [LineItem],
    x: f32,
    y: f32,
    out: &mut Painted<'_, 'f>,
    turned: Option<&Turned>,
) {
    for item in items {
        match item {
            LineItem::Text(text) => {
                if text.style.visibility == Visibility::Visible {
                    paint_text(text, x, y, &mut out.items);
                }
            }
            LineItem::Box(atomic) => match turned {
                None => paint_box(atomic, x, y, out, false),
                Some(turned) => {
                    out.items.push(DisplayItem::PopClip);
                    paint_box(atomic, turned.x, turned.y, out, false);
                    out.items.push(DisplayItem::PushTransform(turned.matrix));
                }
            },
            LineItem::Inline(inline) => {
                let visible = inline.style.visibility == Visibility::Visible;
                let shape = RoundedRect::border_box(&inline.style, inline.rect.translated(x, y));
                let start = out.items.len();
                if visible {
                    crate::shadow::paint_outer(&inline.style, &shape, &mut out.items);
                    let areas = areas(&shape, inline.border, inline.padding);
                    let text = || {
                        let mut runs = Vec::new();
                        item_runs(&inline.children, x, y, &mut runs);
                        runs
                    };
                    paint_background(
                        &inline.style,
                        &areas,
                        &text,
                        out.images,
                        out.viewport,
                        &mut out.items,
                    );
                    crate::shadow::paint_inset(&inline.style, &areas.padding, &mut out.items);
                    paint_border_or_image(
                        &inline.style,
                        inline.border,
                        &shape,
                        out.images,
                        &mut out.items,
                    );
                }
                paint_items(&inline.children, x, y, out, turned);
                if visible {
                    paint_outline(&inline.style, &shape, &mut out.items);
                }
                // Inline boxes are not transformed, but take the other effects.
                if effects::stacking(&inline.style) {
                    let items = out.items.split_off(start);
                    let geometry = effects::Geometry {
                        style: &inline.style,
                        rect: shape.rect,
                        border: inline.border,
                        padding: inline.padding,
                    };
                    out.items
                        .extend(effects::wrap(&geometry, items, None, out.images, false));
                    if inline.style.mix_blend_mode != CssBlendMode::Normal {
                        out.blends = true;
                    }
                }
            }
        }
    }
}

/// The border, padding and content boxes of a box whose border box is `border_box`.
fn areas(border_box: &RoundedRect, border: Sides, padding: Sides) -> BoxAreas {
    let padding_box = border_box.deflate(border.top, border.right, border.bottom, border.left);
    let content = padding_box.deflate(padding.top, padding.right, padding.bottom, padding.left);
    BoxAreas {
        border: *border_box,
        padding: padding_box,
        content,
    }
}

/// The text runs inside a box whose border box is at `(x, y)`, in the root's coordinate
/// space.
fn box_runs(children: &[Fragment], x: f32, y: f32, out: &mut Vec<TextRun>) {
    for child in children {
        match child {
            Fragment::Box(child) => {
                let rect = child.rect.translated(x, y);
                box_runs(&child.children, rect.x, rect.y, out);
            }
            Fragment::Line(line) => {
                item_runs(&line.items, x + line.rect.x, y + line.rect.y, out);
            }
        }
    }
}

/// The text runs of line items on a line at `(x, y)`.
fn item_runs(items: &[LineItem], x: f32, y: f32, out: &mut Vec<TextRun>) {
    for item in items {
        match item {
            LineItem::Text(text) => {
                let mut run = text.run.clone();
                run.origin = Point::new(run.origin.x + x, run.origin.y + y);
                out.push(run);
            }
            LineItem::Box(atomic) => {
                let rect = atomic.rect.translated(x, y);
                box_runs(&atomic.children, rect.x, rect.y, out);
            }
            LineItem::Inline(inline) => item_runs(&inline.children, x, y, out),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Text (CSS Text Decoration 4 § 6: shadows, underlines, overlines, text, emphasis marks and
// line-throughs, in that order)

fn paint_text(text: &TextFragment, x: f32, y: f32, items: &mut Vec<DisplayItem>) {
    let mut run = text.run.clone();
    run.origin = Point::new(run.origin.x + x, run.origin.y + y);
    let style = &text.style;
    // Shadows paint back to front: the first shadow is on top.
    for shadow in style.text_shadow.iter().rev() {
        let shadow_color = color(&shadow.color, style);
        let mut copy = run.clone();
        copy.origin = Point::new(copy.origin.x + shadow.x, copy.origin.y + shadow.y);
        copy.color = shadow_color;
        copy.stroke = copy.stroke.map(|mut s| {
            s.color = shadow_color;
            s
        });
        let mut shadow_items = Vec::new();
        for decoration in &text.decorations {
            let mut d = decoration.clone();
            d.color = shadow_color;
            paint_decoration(&d, &copy, y + shadow.y, &mut shadow_items);
        }
        push_run(copy, &mut shadow_items);
        // The blur radius is twice the standard deviation.
        items.extend(crate::filter::blurred(shadow_items, shadow.blur / 2.0));
    }
    for decoration in &text.decorations {
        if decoration.kind != DecorationKind::LineThrough {
            paint_decoration(decoration, &run, y, items);
        }
    }
    paint_emphasis(style, &run, items);
    push_run(run.clone(), items);
    for decoration in &text.decorations {
        if decoration.kind == DecorationKind::LineThrough {
            paint_decoration(decoration, &run, y, items);
        }
    }
}

/// Paints a run of glyphs. Upright glyphs of vertical text are turned back upright, each
/// about the middle of its advance on the central baseline, halfway between the font's
/// ascent and descent (CSS Writing Modes 4 § 4.2).
fn push_run(run: TextRun, items: &mut Vec<DisplayItem>) {
    if run.upright.is_none() && crate::color_glyphs::paint_run(&run, items) {
        return;
    }
    let Some(upright) = run.upright else {
        items.push(DisplayItem::Text(run));
        return;
    };
    let scale = run.font.em_scale(run.size);
    let m = run.font.metrics();
    let (ascent, descent) = (
        f32::from(m.ascender) * scale,
        -f32::from(m.descender) * scale,
    );
    let central = run.origin.y - (ascent - descent) / 2.0;
    let horizontal = |id: u16| {
        run.font
            .advance(id)
            .map_or(0.0, |advance| f32::from(advance) * scale)
    };
    // Draws glyphs, set horizontally from the origin and squeezed by `squeeze`, upright and
    // centered on `(x, central)`.
    let mut draw = |glyphs: Vec<crate::Glyph>, x: f32, squeeze: f32| {
        let width: f32 = glyphs.iter().map(|g| g.x_advance).sum::<f32>() * squeeze;
        // The middle of the glyphs' em box, from their origin, maps to the middle of the cell.
        let (cx, cy) = (width / 2.0, -(ascent - descent) / 2.0);
        items.push(DisplayItem::PushTransform([
            0.0,
            -squeeze,
            1.0,
            0.0,
            x - cy,
            central + cx,
        ]));
        let mut upright = run.clone();
        upright.origin = Point::new(0.0, 0.0);
        upright.glyphs = glyphs;
        upright.upright = None;
        items.push(DisplayItem::Text(upright));
        items.push(DisplayItem::PopClip);
    };
    match upright {
        crate::display::Upright::Each => {
            let mut pen = run.origin.x;
            let mut index = 0;
            while index < run.glyphs.len() {
                // A cluster's glyphs, such as a letter and its marks, stand together.
                let cluster = run.glyphs[index].text.clone();
                let count = run.glyphs[index..]
                    .iter()
                    .take_while(|g| g.text == cluster)
                    .count();
                let cell: f32 = run.glyphs[index..index + count]
                    .iter()
                    .map(|g| g.x_advance)
                    .sum();
                let glyphs = run.glyphs[index..index + count]
                    .iter()
                    .map(|g| crate::Glyph {
                        x_advance: horizontal(g.id),
                        ..g.clone()
                    })
                    .collect();
                draw(glyphs, pen + cell / 2.0, 1.0);
                pen += cell;
                index += count;
            }
        }
        crate::display::Upright::Combined => {
            let cell = run.width();
            let glyphs: Vec<crate::Glyph> = run
                .glyphs
                .iter()
                .map(|g| crate::Glyph {
                    x_advance: horizontal(g.id),
                    ..g.clone()
                })
                .collect();
            let width: f32 = glyphs.iter().map(|g| g.x_advance).sum();
            let squeeze = if width > cell { cell / width } else { 1.0 };
            draw(glyphs, run.origin.x + cell / 2.0, squeeze);
        }
    }
}

/// Paints one decoration line across a run whose line is at `y`.
fn paint_decoration(d: &Decoration, run: &TextRun, y: f32, items: &mut Vec<DisplayItem>) {
    if d.color.is_transparent() || d.thickness <= 0.0 {
        return;
    }
    let center = y + d.y;
    let segments = decoration_segments(d, run, center);
    for (from, to) in segments {
        if to - from <= 0.01 {
            continue;
        }
        draw_line(d, from, to, center, items);
    }
}

/// The horizontal pieces of a decoration across a run, leaving out spaces and ink as the
/// decoration asks.
fn decoration_segments(d: &Decoration, run: &TextRun, center: f32) -> Vec<(f32, f32)> {
    let mut gaps: Vec<(f32, f32)> = Vec::new();
    let scale = run.font.em_scale(run.size);
    let mut pen = run.origin.x;
    let mut start = run.origin.x;
    let mut end = run.origin.x;
    for glyph in &run.glyphs {
        let gx = pen + glyph.x_offset;
        let text = &run.text[glyph.text.clone()];
        if d.skip_spaces && text.chars().all(char::is_whitespace) {
            gaps.push((pen, pen + glyph.x_advance));
        } else if d.skip_ink {
            // Font units are positive upwards from the glyph's baseline.
            let baseline = run.origin.y + glyph.y_offset;
            let gap = d.thickness.max(1.0);
            let top = (baseline - (center - d.thickness / 2.0 - gap)) / scale;
            let bottom = (baseline - (center + d.thickness / 2.0 + gap)) / scale;
            if let Some((a, b)) = ink(&run.font, glyph.id, bottom, top) {
                gaps.push((gx + a * scale - gap, gx + b * scale + gap));
            }
        }
        pen += glyph.x_advance;
        end = end.max(pen);
        start = start.min(pen - glyph.x_advance);
    }
    subtract(start, end, gaps)
}

/// Ink extents by font file, variations, glyph and band.
type InkCache = HashMap<(usize, Vec<u32>, u16, i32, i32), Option<(f32, f32)>>;

/// Glyph ink inside a band, cached because underlines check the same glyphs again and again.
fn ink(font: &crate::Font, id: u16, bottom: f32, top: f32) -> Option<(f32, f32)> {
    thread_local! {
        static CACHE: RefCell<InkCache> = RefCell::new(HashMap::new());
    }
    let key = (
        font.data().as_ptr() as usize,
        crate::font::variation_key(font),
        id,
        bottom.round() as i32,
        top.round() as i32,
    );
    CACHE.with(|cache| {
        *cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| font.ink_in_band(id, bottom, top))
    })
}

/// `[start, end)` without the gaps.
fn subtract(start: f32, end: f32, mut gaps: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut from = start;
    for (a, b) in gaps {
        if a > from {
            out.push((from, a.min(end)));
        }
        from = from.max(b);
        if from >= end {
            break;
        }
    }
    if from < end {
        out.push((from, end));
    }
    out
}

fn draw_line(d: &Decoration, from: f32, to: f32, center: f32, items: &mut Vec<DisplayItem>) {
    let t = d.thickness;
    let rect = |y: f32| DisplayItem::Rect {
        rect: Rect::new(from, y - t / 2.0, to - from, t),
        color: d.color,
    };
    match d.style {
        TextDecorationStyle::Solid => items.push(rect(center)),
        TextDecorationStyle::Double => {
            let (a, b) = match d.kind {
                DecorationKind::Underline => (center, center + 2.0 * t),
                DecorationKind::Overline => (center - 2.0 * t, center),
                DecorationKind::LineThrough => (center - t, center + t),
            };
            items.push(rect(a));
            items.push(rect(b));
        }
        TextDecorationStyle::Dotted | TextDecorationStyle::Dashed => {
            items.push(DisplayItem::Line {
                from: Point::new(from, center),
                to: Point::new(to, center),
                width: t,
                color: d.color,
                style: if d.style == TextDecorationStyle::Dotted {
                    LineStyle::Dotted
                } else {
                    LineStyle::Dashed
                },
            });
        }
        TextDecorationStyle::Wavy => {
            // A sine wave whose phase follows the page, so neighbouring pieces join up.
            let amplitude = t * 1.5;
            let wavelength = (t * 6.0).max(4.0);
            let step = wavelength / 12.0;
            let mut points = Vec::new();
            let mut px = (from / step).floor() * step;
            while px <= to + step {
                let clamped = px.clamp(from, to);
                let phase = clamped / wavelength * std::f32::consts::TAU;
                points.push(Point::new(clamped, center + amplitude * phase.sin()));
                if px >= to {
                    break;
                }
                px += step;
            }
            items.push(DisplayItem::Polyline {
                points,
                width: t,
                color: d.color,
                closed: false,
            });
        }
    }
}

/// Emphasis marks over or under each character (CSS Text Decoration 4 § 3).
fn paint_emphasis(style: &ComputedValues, run: &TextRun, items: &mut Vec<DisplayItem>) {
    if style.text_emphasis_style == TextEmphasisStyle::None {
        return;
    }
    let mark_color = color(&style.text_emphasis_color, style);
    // Marks go `over` or `under` horizontal text, and `left` or `right` of vertical text.
    let under = match style.text_emphasis_position {
        TextEmphasisPosition::Position { under, left } => left
            .and_then(|left| crate::writing::left_is_under(style.writing_mode, left))
            .unwrap_or(under),
        TextEmphasisPosition::Auto => false,
    };
    let m = run.font.metrics();
    let cap = run.font.scale(m.cap_height, run.size);
    let descent = -run.font.scale(m.descender, run.size);
    let mark = run.size / 2.0;
    let center_y = if under {
        run.origin.y + descent + mark / 2.0
    } else {
        run.origin.y - cap - mark * 0.6
    };
    let mut pen = run.origin.x;
    let mut index = 0;
    while index < run.glyphs.len() {
        // A cluster's glyphs share one mark.
        let range = run.glyphs[index].text.clone();
        let mut advance = 0.0;
        while index < run.glyphs.len() && run.glyphs[index].text == range {
            advance += run.glyphs[index].x_advance;
            index += 1;
        }
        let c = run.text[range].chars().next().unwrap_or(' ');
        let skipped =
            c.is_whitespace() || c.is_control() || c.is_ascii_punctuation() || is_punctuation(c);
        if !skipped {
            draw_mark(
                &style.text_emphasis_style,
                pen + advance / 2.0,
                center_y,
                mark,
                mark_color,
                run,
                items,
            );
        }
        pen += advance;
    }
}

fn is_punctuation(c: char) -> bool {
    matches!(c as u32, 0x2010..=0x2027 | 0x2030..=0x205E | 0x3000..=0x303F | 0xFF01..=0xFF0F)
}

fn draw_mark(
    style: &TextEmphasisStyle,
    cx: f32,
    cy: f32,
    size: f32,
    color: Color,
    run: &TextRun,
    items: &mut Vec<DisplayItem>,
) {
    match style {
        TextEmphasisStyle::None => {}
        TextEmphasisStyle::String(text) => {
            let glyphs = shape(&run.font, size, text);
            let width: f32 = glyphs.iter().map(|g| g.x_advance).sum();
            let mut mark = TextRun::new(
                run.font.clone(),
                size,
                color,
                Point::new(cx - width / 2.0, cy + size * 0.35),
                text.to_string(),
            );
            mark.glyphs = glyphs;
            items.push(DisplayItem::Text(mark));
        }
        TextEmphasisStyle::Shape { filled, shape } => {
            let stroke = size * 0.08;
            let shape_item = |points: Vec<Point>| {
                if *filled {
                    DisplayItem::Polygon { points, color }
                } else {
                    DisplayItem::Polyline {
                        points,
                        width: stroke,
                        color,
                        closed: true,
                    }
                }
            };
            match shape {
                EmphasisShape::Dot => items.push(shape_item(circle(cx, cy, size * 0.15))),
                EmphasisShape::Circle => items.push(shape_item(circle(cx, cy, size * 0.3))),
                EmphasisShape::DoubleCircle => {
                    items.push(DisplayItem::Polyline {
                        points: circle(cx, cy, size * 0.32),
                        width: stroke,
                        color,
                        closed: true,
                    });
                    items.push(shape_item(circle(cx, cy, size * 0.16)));
                }
                EmphasisShape::Triangle => {
                    let r = size * 0.32;
                    items.push(shape_item(vec![
                        Point::new(cx, cy - r),
                        Point::new(cx + r * 0.866, cy + r * 0.5),
                        Point::new(cx - r * 0.866, cy + r * 0.5),
                    ]));
                }
                EmphasisShape::Sesame => {
                    // A slanted teardrop.
                    let r = size * 0.3;
                    let points = (0..16)
                        .map(|i| {
                            let a = i as f32 / 16.0 * std::f32::consts::TAU;
                            let (dx, dy) = (a.cos() * r * 0.35, a.sin() * r);
                            Point::new(cx + dx + dy * 0.3, cy + dy)
                        })
                        .collect();
                    items.push(shape_item(points));
                }
            }
        }
    }
}

fn circle(cx: f32, cy: f32, r: f32) -> Vec<Point> {
    (0..20)
        .map(|i| {
            let a = i as f32 / 20.0 * std::f32::consts::TAU;
            Point::new(cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Borders

/// The corners of a rectangle: top-left, top-right, bottom-right, bottom-left.
fn corners(r: Rect) -> [Point; 4] {
    [
        Point::new(r.x, r.y),
        Point::new(r.right(), r.y),
        Point::new(r.right(), r.bottom()),
        Point::new(r.x, r.bottom()),
    ]
}

/// The trapezoid of one side (0 top, 1 right, 2 bottom, 3 left) between two rectangles.
fn side_band(outer: Rect, inner: Rect, side: usize) -> Vec<Point> {
    let o = corners(outer);
    let i = corners(inner);
    let (a, b) = (side, (side + 1) % 4);
    vec![o[a], o[b], i[b], i[a]]
}

fn shade(color: Color, factor: f32) -> Color {
    let scale = |c: u8| (f32::from(c) * factor).round().clamp(0.0, 255.0) as u8;
    Color::rgba(scale(color.r), scale(color.g), scale(color.b), color.a)
}

fn lighten(color: Color) -> Color {
    let up = |c: u8| (f32::from(c) + (255.0 - f32::from(c)) / 3.0).round() as u8;
    Color::rgba(up(color.r), up(color.g), up(color.b), color.a)
}

/// The four sides of a border or an outline: top, right, bottom and left.
struct Frame {
    widths: [f32; 4],
    styles: [BorderStyle; 4],
    colors: [Color; 4],
}

impl Frame {
    fn visible(&self, side: usize) -> bool {
        self.widths[side] > 0.0
            && !self.colors[side].is_transparent()
            && !matches!(self.styles[side], BorderStyle::None | BorderStyle::Hidden)
    }
}

fn paint_borders(
    style: &ComputedValues,
    b: Sides,
    shape: &RoundedRect,
    items: &mut Vec<DisplayItem>,
) {
    let frame = Frame {
        widths: [b.top, b.right, b.bottom, b.left],
        styles: [
            style.border_top_style,
            style.border_right_style,
            style.border_bottom_style,
            style.border_left_style,
        ],
        colors: [
            color(&style.border_top_color, style),
            color(&style.border_right_color, style),
            color(&style.border_bottom_color, style),
            color(&style.border_left_color, style),
        ],
    };
    paint_frame(&frame, shape, items);
}

/// Paints the outline outside the border box, following its corners (CSS UI 4 § 3).
fn paint_outline(style: &ComputedValues, border_box: &RoundedRect, items: &mut Vec<DisplayItem>) {
    let width = style.outline_width;
    let outline_style = style.outline_style.border_style();
    if width <= 0.0 || outline_style == BorderStyle::None {
        return;
    }
    let outline_color = match &style.outline_color {
        Some(value) => color(value, style),
        None => color(&CssColor::CurrentColor, style),
    };
    let frame = Frame {
        widths: [width; 4],
        styles: [outline_style; 4],
        colors: [outline_color; 4],
    };
    let outer = border_box.inflate(style.outline_offset + width);
    paint_frame(&frame, &outer, items);
}

/// Paints the sides of `frame` inward from the edge of `outer`.
fn paint_frame(frame: &Frame, outer: &RoundedRect, items: &mut Vec<DisplayItem>) {
    if outer.is_square() {
        paint_square_frame(frame, outer.rect, items);
    } else {
        paint_rounded_frame(frame, outer, items);
    }
}

fn paint_square_frame(frame: &Frame, rect: Rect, items: &mut Vec<DisplayItem>) {
    let [top, right, bottom, left] = frame.widths;
    let outer = rect;
    let inner = rect.deflate(top, right, bottom, left);
    // Fractions of each border width, for styles drawn in bands.
    let inset = |fraction: f32| {
        rect.deflate(
            top * fraction,
            right * fraction,
            bottom * fraction,
            left * fraction,
        )
    };

    for side in 0..4 {
        if !frame.visible(side) {
            continue;
        }
        let (width, border_style, color) =
            (frame.widths[side], frame.styles[side], frame.colors[side]);
        let top_left = side == 0 || side == 3;
        let mut band = |outer: Rect, inner: Rect, color: Color| {
            items.push(DisplayItem::Polygon {
                points: side_band(outer, inner, side),
                color,
            });
        };
        match border_style {
            BorderStyle::Solid => band(outer, inner, color),
            BorderStyle::Double if width >= 3.0 => {
                band(outer, inset(1.0 / 3.0), color);
                band(inset(2.0 / 3.0), inner, color);
            }
            BorderStyle::Double => band(outer, inner, color),
            BorderStyle::Inset | BorderStyle::Outset => {
                band(outer, inner, bevel(border_style, color, top_left));
            }
            BorderStyle::Groove | BorderStyle::Ridge => {
                let (outer_color, inner_color) = grooves(border_style, color, top_left);
                band(outer, inset(0.5), outer_color);
                band(inset(0.5), inner, inner_color);
            }
            BorderStyle::Dashed | BorderStyle::Dotted => {
                // Stroke along the middle of the side.
                let middle = inset(0.5);
                let c = corners(middle);
                items.push(DisplayItem::Line {
                    from: c[side],
                    to: c[(side + 1) % 4],
                    width,
                    color,
                    style: line_style(border_style),
                });
            }
            BorderStyle::None | BorderStyle::Hidden => unreachable!("skipped above"),
        }
    }
}

/// Paints a frame with rounded corners. Each side fills the ring between two rounded
/// rectangles, clipped to its wedge of the box unless every side looks the same.
fn paint_rounded_frame(frame: &Frame, outer: &RoundedRect, items: &mut Vec<DisplayItem>) {
    let [top, right, bottom, left] = frame.widths;
    let inner = outer.deflate(top, right, bottom, left);
    let inset = |fraction: f32| {
        outer.deflate(
            top * fraction,
            right * fraction,
            bottom * fraction,
            left * fraction,
        )
    };
    let uniform = (0..4).all(|side| {
        frame.visible(side)
            && frame.widths[side] == top
            && frame.styles[side] == frame.styles[0]
            && frame.colors[side] == frame.colors[0]
    }) && matches!(
        frame.styles[0],
        BorderStyle::Solid | BorderStyle::Double | BorderStyle::Dashed | BorderStyle::Dotted
    );
    for side in 0..4 {
        if !frame.visible(side) {
            continue;
        }
        if !uniform {
            items.push(DisplayItem::PushClipPath {
                path: Path::polygon(&wedge(outer.rect, inner.rect, side)),
                rule: FillRule::NonZero,
            });
        }
        let (width, border_style, color) =
            (frame.widths[side], frame.styles[side], frame.colors[side]);
        let top_left = side == 0 || side == 3;
        let mut ring = |outer: &RoundedRect, inner: &RoundedRect, color: Color| {
            let mut path = outer.path();
            path.extend(inner.path());
            items.push(DisplayItem::Path {
                path,
                color,
                rule: FillRule::EvenOdd,
            });
        };
        match border_style {
            BorderStyle::Solid => ring(outer, &inner, color),
            BorderStyle::Double if width >= 3.0 => {
                ring(outer, &inset(1.0 / 3.0), color);
                ring(&inset(2.0 / 3.0), &inner, color);
            }
            BorderStyle::Double => ring(outer, &inner, color),
            BorderStyle::Inset | BorderStyle::Outset => {
                ring(outer, &inner, bevel(border_style, color, top_left));
            }
            BorderStyle::Groove | BorderStyle::Ridge => {
                let (outer_color, inner_color) = grooves(border_style, color, top_left);
                ring(outer, &inset(0.5), outer_color);
                ring(&inset(0.5), &inner, inner_color);
            }
            BorderStyle::Dashed | BorderStyle::Dotted => items.push(DisplayItem::StrokePath {
                path: inset(0.5).path(),
                width,
                color,
                style: line_style(border_style),
            }),
            BorderStyle::None | BorderStyle::Hidden => unreachable!("skipped above"),
        }
        if uniform {
            break;
        }
        items.push(DisplayItem::PopClip);
    }
}

/// The part of a box that belongs to one side: from the side's outer edge to the inner
/// corners and the center of the inner rectangle. The four wedges tile the box.
fn wedge(outer: Rect, inner: Rect, side: usize) -> [Point; 5] {
    let o = corners(outer);
    let i = corners(inner);
    let center = Point::new(inner.x + inner.width / 2.0, inner.y + inner.height / 2.0);
    let (a, b) = (side, (side + 1) % 4);
    [o[a], o[b], i[b], center, i[a]]
}

fn line_style(style: BorderStyle) -> LineStyle {
    if style == BorderStyle::Dashed {
        LineStyle::Dashed
    } else {
        LineStyle::Dotted
    }
}

/// The color of an `inset` or `outset` side: the top and left sides of an inset border are
/// darker, the others lighter.
fn bevel(style: BorderStyle, color: Color, top_left: bool) -> Color {
    if (style == BorderStyle::Inset) == top_left {
        shade(color, 2.0 / 3.0)
    } else {
        lighten(color)
    }
}

/// The outer and inner colors of a `groove` or `ridge` side.
fn grooves(style: BorderStyle, color: Color, top_left: bool) -> (Color, Color) {
    let (dark, light) = (shade(color, 2.0 / 3.0), lighten(color));
    if (style == BorderStyle::Groove) == top_left {
        (dark, light)
    } else {
        (light, dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtracts_gaps() {
        assert_eq!(subtract(0.0, 10.0, vec![]), [(0.0, 10.0)]);
        assert_eq!(
            subtract(0.0, 10.0, vec![(6.0, 7.0), (2.0, 3.0)]),
            [(0.0, 2.0), (3.0, 6.0), (7.0, 10.0)]
        );
        assert_eq!(subtract(0.0, 10.0, vec![(-1.0, 11.0)]), []);
    }
}
