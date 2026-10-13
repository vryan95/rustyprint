//! Visual effects on a box and its descendants: filters, clip paths, masks, opacity, blend
//! modes, backdrop filters and transforms, applied in that order from the inside out
//! (Filter Effects 1 § 2, CSS Masking 1 § 1, Compositing and Blending 1 § 3).

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    BasicShape, BlendMode as CssBlendMode, ClipPath, FillRule as CssFillRule, GeometryBox,
    Isolation,
};

use crate::background::BoxAreas;
use crate::display::{
    Backdrop, BlendMode, DisplayItem, FillRule, Group, IDENTITY, Matrix, Path, Rect, invert,
    multiply,
};
use crate::fragment::{BoxFragment, Sides};
use crate::images::Images;
use crate::rounded::RoundedRect;
use crate::transform::{self, Mat4};

/// Whether a style gives its box a stacking context for an effect (CSS Transforms 1 § 3,
/// CSS Masking 1 § 1, Compositing and Blending 1 § 3).
pub(crate) fn stacking(style: &ComputedValues) -> bool {
    style.opacity < 1.0
        || style.mix_blend_mode != CssBlendMode::Normal
        || style.isolation == Isolation::Isolate
        || !style.filter.is_empty()
        || !style.backdrop_filter.is_empty()
        || style.clip_path != ClipPath::None
        || crate::mask::has_mask(style)
        || transform::has_transform(style)
        || style.perspective.is_some()
        || transform::preserves_3d(style)
}

/// Whether a box is the containing block of its fixed and absolutely positioned
/// descendants, as transformed and filtered boxes are, and boxes with layout or paint
/// containment.
pub(crate) fn contains_fixed(style: &ComputedValues) -> bool {
    let contain = style.used_contain();
    transform::has_transform(style)
        || style.perspective.is_some()
        || transform::preserves_3d(style)
        || !style.filter.is_empty()
        || !style.backdrop_filter.is_empty()
        || contain.layout
        || contain.paint
}

/// Whether layout or paint containment makes a box a stacking context (CSS Containment 2
/// §§ 3.2, 3.4).
pub(crate) fn contained(style: &ComputedValues) -> bool {
    let contain = style.used_contain();
    contain.layout || contain.paint
}

pub(crate) fn blend_mode(mode: CssBlendMode) -> BlendMode {
    match mode {
        CssBlendMode::Normal => BlendMode::Normal,
        CssBlendMode::Multiply => BlendMode::Multiply,
        CssBlendMode::Screen => BlendMode::Screen,
        CssBlendMode::Overlay => BlendMode::Overlay,
        CssBlendMode::Darken => BlendMode::Darken,
        CssBlendMode::Lighten => BlendMode::Lighten,
        CssBlendMode::ColorDodge => BlendMode::ColorDodge,
        CssBlendMode::ColorBurn => BlendMode::ColorBurn,
        CssBlendMode::HardLight => BlendMode::HardLight,
        CssBlendMode::SoftLight => BlendMode::SoftLight,
        CssBlendMode::Difference => BlendMode::Difference,
        CssBlendMode::Exclusion => BlendMode::Exclusion,
        CssBlendMode::Hue => BlendMode::Hue,
        CssBlendMode::Saturation => BlendMode::Saturation,
        CssBlendMode::Color => BlendMode::Color,
        CssBlendMode::Luminosity => BlendMode::Luminosity,
        CssBlendMode::PlusDarker => BlendMode::PlusDarker,
        CssBlendMode::PlusLighter => BlendMode::PlusLighter,
    }
}

/// The 3D space a box's children are transformed in: the matrix that comes before theirs,
/// and the 2D matrix already drawn for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Space {
    pub matrix: Mat4,
    pub drawn: Matrix,
}

/// A box's transformation, worked out before its descendants are painted.
pub(crate) struct Prepared {
    /// The 2D matrix to draw the box with, if it is transformed.
    transform: Option<Matrix>,
    /// Whether the box is not drawn: behind the viewer, or showing a hidden back face.
    hidden: bool,
    /// The space its children are transformed in.
    pub children: Option<Space>,
}

/// Works out the transformation of a box whose border box is `rect`, in containing block
/// `cb`, transformed in `space` by its parent.
pub(crate) fn prepare(
    fragment: &BoxFragment,
    rect: Rect,
    cb: Rect,
    space: Option<Space>,
) -> Prepared {
    let style = &fragment.style;
    let mut hidden = false;
    let mut transform = None;
    let (matrix, drawn) = if transform::has_transform(style) {
        let parent = space.map_or(transform::IDENTITY, |s| s.matrix);
        let matrix = transform::mul(&parent, &transform::box_matrix(fragment, rect, cb));
        hidden = transform::hides_backface(style, &matrix);
        match transform::flatten(&matrix, rect) {
            Some(flat) => {
                // Drawn inside the matrix already drawn for the parent's space.
                let before = space.map_or(IDENTITY, |s| s.drawn);
                transform = Some(multiply(invert(before).unwrap_or(IDENTITY), flat));
                (matrix, flat)
            }
            None => {
                hidden = true;
                (matrix, IDENTITY)
            }
        }
    } else {
        match space {
            Some(space) if transform::preserves_3d(style) => (space.matrix, space.drawn),
            _ => (transform::IDENTITY, IDENTITY),
        }
    };
    let perspective = transform::child_perspective(style, rect);
    let children = if transform::preserves_3d(style) {
        Some(Space {
            matrix: match &perspective {
                Some(p) => transform::mul(&matrix, p),
                None => matrix,
            },
            drawn,
        })
    } else {
        perspective.map(|matrix| Space {
            matrix,
            drawn: IDENTITY,
        })
    };
    Prepared {
        transform,
        hidden,
        children,
    }
}

/// The geometry of a box an effect applies to.
pub(crate) struct Geometry<'a> {
    pub style: &'a ComputedValues,
    /// The border box.
    pub rect: Rect,
    pub border: Sides,
    pub padding: Sides,
}

impl Geometry<'_> {
    pub fn of(fragment: &BoxFragment, rect: Rect) -> Geometry<'_> {
        Geometry {
            style: &fragment.style,
            rect,
            border: fragment.border,
            padding: fragment.padding,
        }
    }

    fn areas(&self) -> BoxAreas {
        let border = RoundedRect::border_box(self.style, self.rect);
        let (b, p) = (self.border, self.padding);
        let padding = border.deflate(b.top, b.right, b.bottom, b.left);
        let content = padding.deflate(p.top, p.right, p.bottom, p.left);
        BoxAreas {
            border,
            padding,
            content,
        }
    }
}

/// The items a box and its descendants painted, with the box's effects applied. `isolate`
/// asks for an isolated group, for descendants that blend.
pub(crate) fn wrap(
    geometry: &Geometry,
    mut items: Vec<DisplayItem>,
    prepared: Option<&Prepared>,
    images: &Images,
    isolate: bool,
) -> Vec<DisplayItem> {
    let style = geometry.style;
    if prepared.is_some_and(|p| p.hidden) {
        return Vec::new();
    }
    let filters = crate::filter::resolve(&style.filter, style);
    if !filters.is_empty() {
        items = crate::filter::apply(items, &filters);
    }
    let areas = geometry.areas();
    let mask = crate::mask::soft_mask(style, &areas, geometry.border, images, &items);
    let blend = blend_mode(style.mix_blend_mode);
    if style.opacity < 1.0
        || blend != BlendMode::Normal
        || style.isolation == Isolation::Isolate
        || mask.is_some()
        || isolate
    {
        items = surround(
            DisplayItem::PushGroup(Group {
                opacity: style.opacity,
                blend,
                isolated: true,
                mask,
            }),
            items,
            DisplayItem::PopGroup,
        );
    }
    if !style.backdrop_filter.is_empty() {
        let filters = crate::filter::resolve(&style.backdrop_filter, style);
        if !filters.is_empty() {
            items.insert(
                0,
                DisplayItem::Backdrop(Arc::new(Backdrop {
                    clip: areas.border.path(),
                    filters,
                    opacity: style.opacity,
                })),
            );
        }
    }
    if let Some((path, rule)) = clip_path(style, &areas) {
        items = surround(
            DisplayItem::PushClipPath { path, rule },
            items,
            DisplayItem::PopClip,
        );
    }
    if let Some(m) = prepared.and_then(|p| p.transform) {
        items = surround(DisplayItem::PushTransform(m), items, DisplayItem::PopClip);
    }
    items
}

fn surround(start: DisplayItem, items: Vec<DisplayItem>, end: DisplayItem) -> Vec<DisplayItem> {
    let mut out = Vec::with_capacity(items.len() + 2);
    out.push(start);
    out.extend(items);
    out.push(end);
    out
}

/// The clipping path of `clip-path` (CSS Masking 1 § 5.1). A reference to an SVG
/// `<clipPath>` cannot be resolved, which leaves the element unclipped.
fn clip_path(style: &ComputedValues, areas: &BoxAreas) -> Option<(Path, FillRule)> {
    let ClipPath::Shape { shape, reference } = &style.clip_path else {
        return None;
    };
    // Boxes have no margins here, so `margin-box` is the border box.
    let reference = match reference.unwrap_or(GeometryBox::BorderBox) {
        GeometryBox::ContentBox | GeometryBox::FillBox => &areas.content,
        GeometryBox::PaddingBox => &areas.padding,
        GeometryBox::MarginBox
        | GeometryBox::BorderBox
        | GeometryBox::StrokeBox
        | GeometryBox::ViewBox => &areas.border,
    };
    let Some(shape) = shape else {
        return Some((reference.path(), FillRule::NonZero));
    };
    let mut path = Path::default();
    for polygon in crate::shapes::shape_polygons(shape, reference.rect) {
        path.extend(Path::polygon(&polygon));
    }
    let fill = match shape {
        BasicShape::Polygon { fill, .. }
        | BasicShape::Path { fill, .. }
        | BasicShape::Shape { fill, .. } => *fill,
        _ => None,
    };
    let rule = match fill {
        Some(CssFillRule::Evenodd) => FillRule::EvenOdd,
        _ => FillRule::NonZero,
    };
    Some((path, rule))
}

// ---------------------------------------------------------------------------------------------
// Backdrops

/// Replaces each backdrop marker with what is painted below it since the start of the
/// innermost group, filtered and clipped to the element (Filter Effects 2 § 3).
pub(crate) fn resolve_backdrops(items: Vec<DisplayItem>) -> Vec<DisplayItem> {
    if !items.iter().any(|i| matches!(i, DisplayItem::Backdrop(_))) {
        return items;
    }
    // Open clips, transforms and groups: where each started in `out`, and its matrix.
    let mut open: Vec<(usize, Option<Matrix>, bool)> = Vec::new();
    let mut out: Vec<DisplayItem> = Vec::with_capacity(items.len());
    for item in items {
        match &item {
            DisplayItem::PushClip(_)
            | DisplayItem::PushClipPath { .. }
            | DisplayItem::PushClipText(_) => open.push((out.len(), None, false)),
            DisplayItem::PushTransform(m) => open.push((out.len(), Some(*m), false)),
            DisplayItem::PushGroup(_) => open.push((out.len(), None, true)),
            DisplayItem::PopClip | DisplayItem::PopGroup => {
                open.pop();
            }
            DisplayItem::Backdrop(backdrop) => {
                // The backdrop starts after the innermost group's start.
                let root = open.iter().rposition(|(_, _, group)| *group);
                let from = root.map_or(0, |i| open[i].0 + 1);
                let inner = &open[root.map_or(0, |i| i + 1)..];
                // Undo the transforms entered since then, and close what is still open.
                let mut entered = IDENTITY;
                for (_, matrix, _) in inner {
                    if let Some(m) = matrix {
                        entered = multiply(entered, *m);
                    }
                }
                let mut replay: Vec<DisplayItem> = out[from..].to_vec();
                replay.extend(inner.iter().map(|_| DisplayItem::PopClip));
                let visible = backdrop
                    .clip
                    .bounds()
                    .map(|b| crate::display::transform_rect(entered, b));
                let replay = crate::filter::apply_within(replay, &backdrop.filters, visible);
                out.push(DisplayItem::PushClipPath {
                    path: backdrop.clip.clone(),
                    rule: FillRule::NonZero,
                });
                out.push(DisplayItem::PushGroup(Group {
                    opacity: backdrop.opacity,
                    ..Group::ISOLATED
                }));
                out.push(DisplayItem::PushTransform(
                    invert(entered).unwrap_or(IDENTITY),
                ));
                out.extend(replay);
                out.push(DisplayItem::PopClip);
                out.push(DisplayItem::PopGroup);
                out.push(DisplayItem::PopClip);
                continue;
            }
            _ => {}
        }
        out.push(item);
    }
    out
}

/// Whether an element's background layers blend, which isolates its background.
pub(crate) fn blends_background(style: &ComputedValues) -> bool {
    style
        .background_blend_mode
        .iter()
        .any(|mode| *mode != CssBlendMode::Normal)
}
