//! Positioned layout (CSS Positioned Layout 3, CSS 2.1 §§ 9.3, 10.3.7, 10.6.4): relative
//! offsets, and absolutely positioned boxes placed in their containing blocks, around their
//! anchors (CSS Anchor Positioning 1).
//!
//! An absolutely positioned box leaves a placeholder in the flow at its static position.
//! Once its containing block is laid out, the containing block finds the placeholders among
//! its fragments, lays their boxes out against its padding box and adds them as children.
//! The anchors those boxes refer to are found among the same fragments.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    AlignPosition, AnchorScope, Direction, ItemAlignment, LengthPercentage, MaybeAuto,
    OverflowPosition, Position, PositionAnchor, PositionTryOrder, Size, TryTactic, Visibility,
};
use rustyprint_css::values::{
    AnchorFunction, AnchorQuery, AnchorResolver, AnchorSide, AnchorSize, with_anchor_resolver,
};
use rustyprint_dom::NodeId;

use crate::boxes::{BlockBox, Positioned};
use crate::display::{Point, Rect};
use crate::flow::{
    BlockKind, ContainingBlock, content_width, intrinsic_content, layout_block, padding_and_border,
};
use crate::fragment::{BoxFragment, Fragment, LineItem, OutOfFlow};

/// Whether a style positions its box, which makes it a containing block for absolutely
/// positioned descendants.
pub(crate) fn is_positioned(style: &ComputedValues) -> bool {
    matches!(
        style.position,
        Position::Relative | Position::Absolute | Position::Fixed | Position::Sticky
    )
}

fn inset(value: &MaybeAuto<LengthPercentage>, basis: Option<f32>) -> Option<f32> {
    match (value, basis) {
        (MaybeAuto::Auto, _) => None,
        (MaybeAuto::Value(LengthPercentage::Length(px)), _) => Some(*px),
        // Anchor functions without an anchor or fallback make the inset `auto`.
        (MaybeAuto::Value(value), Some(basis)) => value.try_resolve(basis),
        // A percentage of a height that depends on content is `auto`.
        (MaybeAuto::Value(_), None) => None,
    }
}

/// How far `position: relative` moves a box from where it is laid out (CSS 2.1 § 9.4.3).
/// `left` wins over `right` in left-to-right containing blocks, and `top` over `bottom`.
/// Sticky boxes never scroll, so they stay where they are.
pub(crate) fn relative_offset(style: &ComputedValues, cb: ContainingBlock) -> (f32, f32) {
    if style.position != Position::Relative {
        return (0.0, 0.0);
    }
    let left = inset(&style.left, Some(cb.width));
    let right = inset(&style.right, Some(cb.width));
    let dx = match (left, right) {
        (Some(left), Some(right)) => {
            if cb.rtl {
                -right
            } else {
                left
            }
        }
        (Some(left), None) => left,
        (None, Some(right)) => -right,
        (None, None) => 0.0,
    };
    let dy = match (
        inset(&style.top, cb.height),
        inset(&style.bottom, cb.height),
    ) {
        (Some(top), _) => top,
        (None, Some(bottom)) => -bottom,
        (None, None) => 0.0,
    };
    (dx, dy)
}

/// Which out-of-flow boxes a containing block takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Takes {
    /// A positioned box: absolutely positioned descendants.
    Absolute,
    /// The root, whose containing block is the page area: fixed ones too.
    All,
}

/// A box with an `anchor-name`, found among a containing block's fragments.
#[derive(Debug, Clone)]
struct Anchor {
    names: Arc<[Arc<str>]>,
    /// Its border box, relative to the containing block.
    rect: Rect,
    visible: bool,
    /// The elements around it whose `anchor-scope` limits who sees its names.
    scopes: Vec<(NodeId, AnchorScope)>,
}

impl Anchor {
    /// The anchor with its border box moved by `(dx, dy)`.
    fn moved(&self, dx: f32, dy: f32) -> Anchor {
        Anchor {
            rect: self.rect.translated(dx, dy),
            ..self.clone()
        }
    }
}

/// The anchors a positioned box can see.
struct Anchors {
    anchors: Vec<Anchor>,
    /// The positioned box's element and its ancestors, for `anchor-scope`.
    ancestors: Vec<NodeId>,
}

impl Anchors {
    /// The last anchor in tree order with `name` that is in scope (CSS Anchor Positioning 1
    /// § 2.1).
    fn find(&self, name: &str) -> Option<&Anchor> {
        self.anchors.iter().rev().find(|anchor| {
            anchor.names.iter().any(|n| &**n == name)
                && anchor
                    .scopes
                    .iter()
                    .all(|(node, scope)| !scope.covers(name) || self.ancestors.contains(node))
        })
    }
}

/// A placeholder found among a containing block's fragments.
struct Found {
    positioned: Arc<Positioned>,
    /// The static position, relative to the containing block's border box.
    at: Point,
    /// The elements the placeholder is inside, innermost last, with their `anchor-scope`.
    ancestors: Vec<(NodeId, AnchorScope)>,
    /// The grid area that is the box's containing block, relative to the containing
    /// block's border box, when its grid container is that.
    area: Option<Rect>,
}

/// Where `justify-self` or `align-self` puts an absolutely positioned box in the space
/// between its insets, when it does not stretch there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SelfAlign {
    /// The share of the free space before the box.
    share: f32,
    /// The share for `safe` alignment when the box overflows: the start.
    safe: Option<f32>,
}

impl SelfAlign {
    pub(crate) fn fraction(self, free: f32) -> f32 {
        match self.safe {
            Some(start) if free < 0.0 => start,
            _ => self.share,
        }
    }
}

/// The alignment of an absolutely positioned box along an axis that starts on the right
/// when `reversed`; `None` when it stretches, as `normal` does (CSS Box Alignment 3 § 6.1).
pub(crate) fn self_alignment(value: ItemAlignment, reversed: bool) -> Option<SelfAlign> {
    let (start, end) = if reversed { (1.0, 0.0) } else { (0.0, 1.0) };
    let share = |position: AlignPosition| match position {
        AlignPosition::Center => 0.5,
        AlignPosition::Start | AlignPosition::FlexStart | AlignPosition::SelfStart => start,
        AlignPosition::End | AlignPosition::FlexEnd | AlignPosition::SelfEnd => end,
        AlignPosition::Left => 0.0,
        AlignPosition::Right => 1.0,
    };
    match value {
        ItemAlignment::Auto
        | ItemAlignment::Normal
        | ItemAlignment::Stretch
        | ItemAlignment::AnchorCenter => None,
        ItemAlignment::Baseline { last } => Some(SelfAlign {
            share: if last { end } else { start },
            safe: None,
        }),
        ItemAlignment::Position { position, overflow } => Some(SelfAlign {
            share: share(position),
            safe: (overflow == OverflowPosition::Safe).then_some(start),
        }),
        ItemAlignment::Legacy(position) => Some(SelfAlign {
            share: share(position),
            safe: None,
        }),
    }
}

/// Lays out the out-of-flow boxes whose placeholders are among `fragment`'s descendants and
/// adds them as its children. `cb` is the containing block, relative to `fragment`'s border
/// box.
pub(crate) fn place_out_of_flow(fragment: &mut BoxFragment, cb: Rect, takes: Takes) {
    let mut found = Vec::new();
    let mut path = Vec::new();
    collect(
        &mut fragment.children,
        Point::new(0.0, 0.0),
        takes,
        &mut path,
        &mut found,
    );
    if found.is_empty() {
        return;
    }
    let mut anchors = Vec::new();
    let mut scopes = Vec::new();
    let outer = cb;
    find_anchors(
        &fragment.children,
        Point::new(-cb.x, -cb.y),
        &mut scopes,
        &mut anchors,
    );
    for found in found {
        // A grid area is the containing block of a box its grid container places.
        let cb = found.area.unwrap_or(cb);
        let static_position = Point::new(found.at.x - cb.x, found.at.y - cb.y);
        // The box is placed outside the elements its placeholder is in, which still scope
        // the anchors inside it.
        let mut scopes: Vec<(NodeId, AnchorScope)> = found
            .ancestors
            .iter()
            .filter(|(_, scope)| *scope != AnchorScope::None)
            .cloned()
            .collect();
        let mut ancestors: Vec<NodeId> = found.ancestors.iter().map(|(node, _)| *node).collect();
        ancestors.extend(found.positioned.block.node);
        let context = Anchors {
            anchors: match found.area {
                // Anchors are found relative to the padding box; move them into the area.
                Some(area) => anchors
                    .iter()
                    .map(|anchor| anchor.moved(-(area.x - outer.x), -(area.y - outer.y)))
                    .collect(),
                None => anchors.clone(),
            },
            ancestors,
        };
        let Some(mut placed) = place_positioned(
            &found.positioned,
            cb.width,
            cb.height,
            static_position,
            &context,
        ) else {
            continue;
        };
        // Anchors inside it are there for the boxes placed after it.
        find_anchors(
            std::slice::from_ref(&Fragment::Box(placed.clone())),
            Point::new(0.0, 0.0),
            &mut scopes,
            &mut anchors,
        );
        placed.rect.x += cb.x;
        placed.rect.y += cb.y;
        fragment.children.push(Fragment::Box(placed));
    }
}

/// Takes the boxes of placeholders among `children`, in tree order, with the placeholders'
/// positions relative to the box the children are in, offset by `origin`.
fn collect(
    children: &mut [Fragment],
    origin: Point,
    takes: Takes,
    path: &mut Vec<(NodeId, AnchorScope)>,
    found: &mut Vec<Found>,
) {
    for child in children {
        match child {
            Fragment::Box(child) => collect_box(child, origin, takes, path, found),
            Fragment::Line(line) => {
                let origin = Point::new(origin.x + line.rect.x, origin.y + line.rect.y);
                collect_items(&mut line.items, origin, takes, path, found);
            }
        }
    }
}

fn collect_items(
    items: &mut [LineItem],
    origin: Point,
    takes: Takes,
    path: &mut Vec<(NodeId, AnchorScope)>,
    found: &mut Vec<Found>,
) {
    for item in items {
        match item {
            LineItem::Box(child) => collect_box(child, origin, takes, path, found),
            // Inline boxes position their children relative to the line too.
            LineItem::Inline(inline) => {
                path.extend(inline.node.map(|n| (n, inline.style.anchor_scope.clone())));
                collect_items(&mut inline.children, origin, takes, path, found);
                if inline.node.is_some() {
                    path.pop();
                }
            }
            LineItem::Text(_) => {}
        }
    }
}

fn collect_box(
    child: &mut BoxFragment,
    origin: Point,
    takes: Takes,
    path: &mut Vec<(NodeId, AnchorScope)>,
    found: &mut Vec<Found>,
) {
    let at = Point::new(origin.x + child.rect.x, origin.y + child.rect.y);
    if let Some(OutOfFlow::Positioned(positioned) | OutOfFlow::InArea(positioned)) =
        &child.out_of_flow
    {
        let absolute = positioned.block.style.position == Position::Absolute;
        if takes == Takes::All || absolute {
            let area = matches!(child.out_of_flow, Some(OutOfFlow::InArea(_)))
                .then(|| Rect::new(at.x, at.y, child.rect.width, child.rect.height));
            found.push(Found {
                positioned: positioned.clone(),
                at,
                ancestors: path.clone(),
                area,
            });
            child.out_of_flow = None;
        }
        return;
    }
    path.extend(child.node.map(|n| (n, child.style.anchor_scope.clone())));
    collect(&mut child.children, at, takes, path, found);
    if child.node.is_some() {
        path.pop();
    }
}

/// Finds the boxes with an `anchor-name` among fragments, in tree order, with their border
/// boxes offset by `origin`.
fn find_anchors(
    children: &[Fragment],
    origin: Point,
    scopes: &mut Vec<(NodeId, AnchorScope)>,
    out: &mut Vec<Anchor>,
) {
    for child in children {
        match child {
            Fragment::Box(child) => find_box_anchors(child, origin, scopes, out),
            Fragment::Line(line) => {
                let origin = Point::new(origin.x + line.rect.x, origin.y + line.rect.y);
                find_item_anchors(&line.items, origin, scopes, out);
            }
        }
    }
}

fn find_item_anchors(
    items: &[LineItem],
    origin: Point,
    scopes: &mut Vec<(NodeId, AnchorScope)>,
    out: &mut Vec<Anchor>,
) {
    for item in items {
        match item {
            LineItem::Box(child) => find_box_anchors(child, origin, scopes, out),
            LineItem::Inline(inline) => {
                let scoped = push_scope(inline.node, &inline.style, scopes);
                add_anchor(
                    &inline.style,
                    inline.rect.translated(origin.x, origin.y),
                    scopes,
                    out,
                );
                find_item_anchors(&inline.children, origin, scopes, out);
                if scoped {
                    scopes.pop();
                }
            }
            LineItem::Text(_) => {}
        }
    }
}

fn find_box_anchors(
    child: &BoxFragment,
    origin: Point,
    scopes: &mut Vec<(NodeId, AnchorScope)>,
    out: &mut Vec<Anchor>,
) {
    if child.out_of_flow.is_some() {
        return;
    }
    let scoped = push_scope(child.node, &child.style, scopes);
    let rect = child.rect.translated(origin.x, origin.y);
    add_anchor(&child.style, rect, scopes, out);
    find_anchors(&child.children, Point::new(rect.x, rect.y), scopes, out);
    if scoped {
        scopes.pop();
    }
}

fn push_scope(
    node: Option<NodeId>,
    style: &ComputedValues,
    scopes: &mut Vec<(NodeId, AnchorScope)>,
) -> bool {
    match node {
        Some(node) if style.anchor_scope != AnchorScope::None => {
            scopes.push((node, style.anchor_scope.clone()));
            true
        }
        _ => false,
    }
}

fn add_anchor(
    style: &ComputedValues,
    rect: Rect,
    scopes: &[(NodeId, AnchorScope)],
    out: &mut Vec<Anchor>,
) {
    if style.anchor_name.is_empty() {
        return;
    }
    out.push(Anchor {
        names: style.anchor_name.clone(),
        rect,
        visible: style.visibility == Visibility::Visible,
        scopes: scopes.to_vec(),
    });
}

/// The property an anchor function is resolved for, which decides the edge `anchor()` measures
/// from and the axis `anchor-size()` measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Property {
    Top,
    Right,
    Bottom,
    Left,
    Width,
    Height,
}

/// What resolves anchor functions for a positioned box.
struct Resolution {
    property: Rc<Cell<Property>>,
    /// Whether an anchor function found no anchor.
    missed: Rc<Cell<bool>>,
}

/// The default anchor of a box: the one `position-anchor` names.
fn default_anchor<'a>(style: &ComputedValues, anchors: &'a Anchors) -> Option<&'a Anchor> {
    match &style.position_anchor {
        PositionAnchor::Name(name) => anchors.find(name),
        PositionAnchor::Auto | PositionAnchor::None => None,
    }
}

/// An anchor side as written, read in a property after the tactics have moved the value to
/// it from another.
fn flipped_side(side: AnchorSide, vertical: bool, tactics: &[TryTactic]) -> AnchorSide {
    let mut side = side;
    for tactic in tactics {
        side = match (tactic, side) {
            (TryTactic::FlipBlock, AnchorSide::Top) => AnchorSide::Bottom,
            (TryTactic::FlipBlock, AnchorSide::Bottom) => AnchorSide::Top,
            (TryTactic::FlipInline, AnchorSide::Left) => AnchorSide::Right,
            (TryTactic::FlipInline, AnchorSide::Right) => AnchorSide::Left,
            (TryTactic::FlipStart, AnchorSide::Top) => AnchorSide::Left,
            (TryTactic::FlipStart, AnchorSide::Left) => AnchorSide::Top,
            (TryTactic::FlipStart, AnchorSide::Bottom) => AnchorSide::Right,
            (TryTactic::FlipStart, AnchorSide::Right) => AnchorSide::Bottom,
            (TryTactic::FlipBlock, s) if vertical => flip_start_end(s),
            (TryTactic::FlipInline, s) if !vertical => flip_start_end(s),
            (_, s) => s,
        };
    }
    side
}

fn flip_start_end(side: AnchorSide) -> AnchorSide {
    match side {
        AnchorSide::Start => AnchorSide::End,
        AnchorSide::End => AnchorSide::Start,
        AnchorSide::SelfStart => AnchorSide::SelfEnd,
        AnchorSide::SelfEnd => AnchorSide::SelfStart,
        AnchorSide::Percentage(p) => AnchorSide::Percentage(1.0 - p),
        other => other,
    }
}

/// A resolver for the anchor functions of a box in containing block `cb`, from its top-left
/// corner.
fn resolver(
    anchors: &Anchors,
    style: &ComputedValues,
    cb: Rect,
    tactics: &[TryTactic],
) -> (AnchorResolver, Resolution) {
    let property = Rc::new(Cell::new(Property::Top));
    let missed = Rc::new(Cell::new(false));
    // Each name, with the anchor it finds.
    let mut named: Vec<(Arc<str>, Rect)> = Vec::new();
    for name in anchors
        .anchors
        .iter()
        .flat_map(|anchor| anchor.names.iter())
    {
        if !named.iter().any(|(n, _)| n == name)
            && let Some(anchor) = anchors.find(name)
        {
            named.push((name.clone(), anchor.rect));
        }
    }
    let default = default_anchor(style, anchors).map(|anchor| anchor.rect);
    let rtl = style.direction == Direction::Rtl;
    let tactics = tactics.to_vec();
    let (state, misses) = (property.clone(), missed.clone());
    let resolve: AnchorResolver = Rc::new(move |function: &AnchorFunction| {
        let anchor = match &function.name {
            Some(name) => named.iter().find(|(n, _)| n == name).map(|(_, r)| *r),
            None => default,
        };
        let Some(a) = anchor else {
            misses.set(true);
            return None;
        };
        let property = state.get();
        match function.query {
            AnchorQuery::Side(side) => {
                let (vertical, from_end) = match property {
                    Property::Top => (true, false),
                    Property::Bottom => (true, true),
                    Property::Left => (false, false),
                    Property::Right => (false, true),
                    // `anchor()` only applies to insets.
                    Property::Width | Property::Height => return None,
                };
                let (start, size) = if vertical {
                    (a.y, a.height)
                } else {
                    (a.x, a.width)
                };
                let end = start + size;
                // Logical sides of the inline axis run right to left in right-to-left text.
                let reversed = !vertical && rtl;
                let coordinate = match flipped_side(side, vertical, &tactics) {
                    AnchorSide::Top if vertical => start,
                    AnchorSide::Bottom if vertical => end,
                    AnchorSide::Left if !vertical => start,
                    AnchorSide::Right if !vertical => end,
                    AnchorSide::Top | AnchorSide::Bottom | AnchorSide::Left | AnchorSide::Right => {
                        return None;
                    }
                    AnchorSide::Inside => {
                        if from_end {
                            end
                        } else {
                            start
                        }
                    }
                    AnchorSide::Outside => {
                        if from_end {
                            start
                        } else {
                            end
                        }
                    }
                    AnchorSide::Start | AnchorSide::SelfStart => {
                        if reversed {
                            end
                        } else {
                            start
                        }
                    }
                    AnchorSide::End | AnchorSide::SelfEnd => {
                        if reversed {
                            start
                        } else {
                            end
                        }
                    }
                    AnchorSide::Center => start + size / 2.0,
                    AnchorSide::Percentage(p) => {
                        if reversed {
                            end - p as f32 * size
                        } else {
                            start + p as f32 * size
                        }
                    }
                };
                // The distance from the containing block's edge on the property's side.
                let (cb_start, cb_end) = if vertical {
                    (cb.y, cb.bottom())
                } else {
                    (cb.x, cb.right())
                };
                Some(f64::from(if from_end {
                    cb_end - coordinate
                } else {
                    coordinate - cb_start
                }))
            }
            AnchorQuery::Size(size) => {
                let horizontal = match size {
                    Some(AnchorSize::Width | AnchorSize::Inline | AnchorSize::SelfInline) => true,
                    Some(AnchorSize::Height | AnchorSize::Block | AnchorSize::SelfBlock) => false,
                    None => matches!(property, Property::Left | Property::Right | Property::Width),
                };
                Some(f64::from(if horizontal { a.width } else { a.height }))
            }
        }
    });
    (resolve, Resolution { property, missed })
}

/// How a box aligns in its `position-area`: toward the anchor, or centered on it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Align {
    Start,
    End,
    /// `anchor-center`: centered on the anchor, at this offset in the area.
    AnchorCenter(f32),
}

/// The tracks a `position-area` covers along one axis, as a region between the containing
/// block's and the anchor's edges, and how the box aligns in it.
fn area_track((from, to): (u8, u8), cb: (f32, f32), anchor: (f32, f32)) -> ((f32, f32), Align) {
    let edges = [cb.0, anchor.0.max(cb.0), anchor.1.min(cb.1), cb.1];
    let region = (edges[from as usize], edges[to as usize + 1]);
    // An area along the anchor, or across the whole axis, centers the box on the anchor
    // (CSS Anchor Positioning 1 § 3.1.1).
    let align = match (from, to) {
        (1, 1) | (0, 2) => Align::AnchorCenter((anchor.0 + anchor.1) / 2.0 - region.0),
        (0, _) => Align::End,
        _ => Align::Start,
    };
    (region, align)
}

/// A box's style with try tactics applied: flips swap its insets, margins and
/// `position-area`.
fn with_tactics(style: &Arc<ComputedValues>, tactics: &[TryTactic]) -> Arc<ComputedValues> {
    if tactics.is_empty() {
        return style.clone();
    }
    let mut out = (**style).clone();
    for tactic in tactics {
        match tactic {
            TryTactic::FlipBlock => {
                std::mem::swap(&mut out.top, &mut out.bottom);
                std::mem::swap(&mut out.margin_top, &mut out.margin_bottom);
                out.position_area = out.position_area.as_ref().map(|a| a.flip_block());
            }
            TryTactic::FlipInline => {
                std::mem::swap(&mut out.left, &mut out.right);
                std::mem::swap(&mut out.margin_left, &mut out.margin_right);
                out.position_area = out.position_area.as_ref().map(|a| a.flip_inline());
            }
            TryTactic::FlipStart => {
                let (top, left) = (out.top.clone(), out.left.clone());
                (out.top, out.left) = (left, top);
                let (bottom, right) = (out.bottom.clone(), out.right.clone());
                (out.bottom, out.right) = (right, bottom);
                let (mt, ml) = (out.margin_top.clone(), out.margin_left.clone());
                (out.margin_top, out.margin_left) = (ml, mt);
                let (mb, mr) = (out.margin_bottom.clone(), out.margin_right.clone());
                (out.margin_bottom, out.margin_right) = (mr, mb);
                std::mem::swap(&mut out.width, &mut out.height);
                std::mem::swap(&mut out.min_width, &mut out.min_height);
                std::mem::swap(&mut out.max_width, &mut out.max_height);
                out.position_area = out.position_area.as_ref().map(|a| a.flip_start());
            }
        }
    }
    Arc::new(out)
}

/// A laid-out position of a box: its fragment, whether it overflows the containing block,
/// and whether all its anchor functions found anchors.
struct Placement {
    fragment: BoxFragment,
    overflows: bool,
    anchored: bool,
    /// The room it had: the size of its inset-modified containing block.
    room: (f32, f32),
}

/// Lays out an absolutely positioned box in a containing block of `cb_width` by
/// `cb_height`, trying its fallback positions if it overflows, or `None` if
/// `position-visibility` hides it. `static_position` is where the box's margin box would
/// start in flow; both are relative to the containing block's padding box.
fn place_positioned(
    positioned: &Positioned,
    cb_width: f32,
    cb_height: f32,
    static_position: Point,
    anchors: &Anchors,
) -> Option<BoxFragment> {
    let block = &positioned.block;
    let base = &block.style;
    let place = |style: &Arc<ComputedValues>, tactics: &[TryTactic]| {
        let style = with_tactics(style, tactics);
        let boxed;
        let block = if Arc::ptr_eq(&style, base) {
            block
        } else {
            boxed = BlockBox {
                style: style.clone(),
                ..block.clone()
            };
            &boxed
        };
        layout_positioned(
            block,
            cb_width,
            cb_height,
            static_position,
            anchors,
            tactics,
        )
    };
    let first = place(base, &[]);
    let mut chosen = None;
    if first.overflows && !positioned.fallbacks.is_empty() {
        let mut options: Vec<Placement> = positioned
            .fallbacks
            .iter()
            .map(|fallback| place(&fallback.style, &fallback.tactics))
            .collect();
        // `position-try-order` prefers the options with the most room.
        let room = |p: &Placement| match base.position_try_order {
            PositionTryOrder::Normal => 0.0,
            PositionTryOrder::MostWidth | PositionTryOrder::MostInlineSize => -p.room.0,
            PositionTryOrder::MostHeight | PositionTryOrder::MostBlockSize => -p.room.1,
        };
        options.sort_by(|a, b| room(a).total_cmp(&room(b)));
        chosen = options.into_iter().find(|option| !option.overflows);
    }
    let placement = chosen.unwrap_or(first);
    let visibility = base.position_visibility;
    if visibility.no_overflow && placement.overflows {
        return None;
    }
    if visibility.anchors_valid && !placement.anchored {
        return None;
    }
    if visibility.anchors_visible
        && let Some(anchor) = default_anchor(&placement.fragment.style, anchors)
        && !anchor.visible
    {
        return None;
    }
    Some(placement.fragment)
}

/// Lays out an absolutely positioned box with one set of positioning properties.
fn layout_positioned(
    block: &BlockBox,
    cb_width: f32,
    cb_height: f32,
    static_position: Point,
    anchors: &Anchors,
    tactics: &[TryTactic],
) -> Placement {
    let style = &block.style;
    // `position-area` makes a region of the grid around the default anchor the containing
    // block, and aligns the box in it toward the anchor.
    let default = default_anchor(style, anchors);
    let full = Rect::new(0.0, 0.0, cb_width, cb_height);
    let (cb_rect, align) = match (&style.position_area, default) {
        (Some(area), Some(anchor)) => {
            let a = anchor.rect;
            let ((x0, x1), align_x) = area_track(area.columns, (0.0, cb_width), (a.x, a.right()));
            let ((y0, y1), align_y) = area_track(area.rows, (0.0, cb_height), (a.y, a.bottom()));
            (
                Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)),
                Some((align_x, align_y)),
            )
        }
        _ => (full, None),
    };
    let (resolve, resolution) = resolver(anchors, style, cb_rect, tactics);
    let missing_area = style.position_area.is_some() && default.is_none();
    let mut placement = with_anchor_resolver(resolve, || {
        layout_in(block, cb_rect, static_position, align, &resolution.property)
    });
    placement.anchored = !resolution.missed.get() && !missing_area;
    let r = placement.fragment.rect;
    placement.overflows = r.x < full.x - 0.01
        || r.y < full.y - 0.01
        || r.right() > full.right() + 0.01
        || r.bottom() > full.bottom() + 0.01;
    placement
}

/// Lays out an absolutely positioned box in containing block `cb`, from its static position
/// or, with `align`, aligned in it (CSS 2.1 §§ 10.3.7, 10.6.4).
fn layout_in(
    block: &BlockBox,
    cb: Rect,
    static_position: Point,
    align: Option<(Align, Align)>,
    property: &Cell<Property>,
) -> Placement {
    let style = &block.style;
    let rtl = style.direction == Direction::Rtl;
    let (cb_width, cb_height) = (cb.width, cb.height);
    let containing = ContainingBlock {
        width: cb_width,
        height: Some(cb_height),
        rtl,
    };
    let resolve = |which: Property, value: &MaybeAuto<LengthPercentage>, basis: f32| {
        property.set(which);
        inset(value, Some(basis))
    };
    let left = resolve(Property::Left, &style.left, cb_width);
    let right = resolve(Property::Right, &style.right, cb_width);
    let top = resolve(Property::Top, &style.top, cb_height);
    let bottom = resolve(Property::Bottom, &style.bottom, cb_height);
    // Without an inset, the static position stands in, or the alignment in the area.
    let static_x = static_position.x - cb.x;
    let static_y = static_position.y - cb.y;
    let margin = |value: &MaybeAuto<LengthPercentage>| match value {
        MaybeAuto::Auto => None,
        MaybeAuto::Value(value) => value.try_resolve(cb_width),
    };
    property.set(Property::Width);
    let (mut margin_left, mut margin_right) =
        (margin(&style.margin_left), margin(&style.margin_right));
    property.set(Property::Height);
    let (mut margin_top, mut margin_bottom) =
        (margin(&style.margin_top), margin(&style.margin_bottom));
    let (padding, border) = padding_and_border(style, cb_width);
    let extra_w = padding.horizontal() + border.horizontal();
    let extra_h = padding.vertical() + border.vertical();
    // `justify-self` and `align-self` place a box that does not stretch between its insets
    // (CSS Box Alignment 3 § 6.1); a `position-area` aligns it instead.
    let (justify, align_self) = match align {
        Some(_) => (None, None),
        None => (
            self_alignment(style.justify_self, rtl),
            self_alignment(style.align_self, false),
        ),
    };

    // The used width (CSS 2.1 § 10.3.7): stretched between `left` and `right`, or else
    // shrink-to-fit. Replaced elements have their own width (§ 10.3.8).
    property.set(Property::Width);
    let width = if block.replaced.is_some() {
        let laid = layout_block(block, containing, BlockKind::Atomic, &[], None, None);
        (laid.fragment.rect.width - extra_w).max(0.0)
    } else {
        let fixed_margins = margin_left.unwrap_or(0.0) + margin_right.unwrap_or(0.0);
        let tentative =
            content_width(block, &style.width, containing, extra_w).unwrap_or_else(|| {
                match (left, right) {
                    (Some(left), Some(right)) if justify.is_none() => {
                        (cb_width - left - right - fixed_margins - extra_w).max(0.0)
                    }
                    (Some(left), Some(right)) => {
                        let available =
                            (cb_width - left - right - fixed_margins - extra_w).max(0.0);
                        let (min, max) = intrinsic_content(block);
                        available.min(max).max(min)
                    }
                    _ => {
                        let start = match (left, right) {
                            (None, Some(_)) => 0.0,
                            (left, _) if align.is_some() => left.unwrap_or(0.0),
                            (left, _) => left.unwrap_or(static_x),
                        };
                        let available =
                            (cb_width - start - right.unwrap_or(0.0) - fixed_margins - extra_w)
                                .max(0.0);
                        let (min, max) = intrinsic_content(block);
                        available.min(max).max(min)
                    }
                }
            });
        let max = content_width(block, &style.max_width, containing, extra_w);
        let min = content_width(block, &style.min_width, containing, extra_w).unwrap_or(0.0);
        tentative.min(max.unwrap_or(f32::INFINITY)).max(min)
    };
    let outer_w = width + extra_w;

    // Auto margins center a box between `left` and `right` (CSS 2.1 § 10.3.7).
    if let (Some(left), Some(right)) = (left, right) {
        let free = cb_width - left - right - outer_w;
        match (margin_left, margin_right) {
            (None, None) => {
                let half = free / 2.0;
                if half >= 0.0 {
                    (margin_left, margin_right) = (Some(half), Some(half));
                } else if rtl {
                    (margin_left, margin_right) = (Some(free), Some(0.0));
                } else {
                    (margin_left, margin_right) = (Some(0.0), Some(free));
                }
            }
            (None, Some(right_margin)) => margin_left = Some(free - right_margin),
            (Some(left_margin), None) => margin_right = Some(free - left_margin),
            (Some(_), Some(_)) => {}
        }
    }
    let (margin_left, margin_right) = (margin_left.unwrap_or(0.0), margin_right.unwrap_or(0.0));
    // Where the margin box starts, for a margin box of `outer` in an area with `free` room.
    let aligned = |a: Align, free: f32, outer: f32| match a {
        Align::Start => 0.0,
        Align::End => free,
        Align::AnchorCenter(center) => center - outer / 2.0,
    };
    let x = match (left, right) {
        (Some(left), Some(right)) if justify.is_some() => {
            let free = cb_width - left - right - margin_left - margin_right - outer_w;
            left + margin_left + free * justify.map_or(0.0, |a| a.fraction(free))
        }
        (Some(_), Some(right)) if rtl => cb_width - right - margin_right - outer_w,
        (Some(left), _) => left + margin_left,
        (None, Some(right)) => cb_width - right - margin_right - outer_w,
        (None, None) => match align {
            Some((align_x, _)) => {
                let outer = outer_w + margin_left + margin_right;
                margin_left + aligned(align_x, cb_width - outer, outer)
            }
            None => static_x + margin_left,
        },
    };

    // A height between `top` and `bottom` stretches when it is auto (CSS 2.1 § 10.6.4).
    property.set(Property::Height);
    let height = match &style.height {
        Size::Value(value) if block.replaced.is_none() => value.try_resolve(cb_height).map(|h| {
            if style.box_sizing == rustyprint_css::properties::types::BoxSizing::BorderBox {
                (h - extra_h).max(0.0)
            } else {
                h
            }
        }),
        _ => None,
    };
    let stretched = match (top, bottom, &style.height) {
        (Some(top), Some(bottom), Size::Auto)
            if block.replaced.is_none() && align_self.is_none() =>
        {
            Some(
                (cb_height
                    - top
                    - bottom
                    - margin_top.unwrap_or(0.0)
                    - margin_bottom.unwrap_or(0.0)
                    - extra_h)
                    .max(0.0),
            )
        }
        _ => height,
    };
    let kind = if block.replaced.is_some() {
        BlockKind::Atomic
    } else {
        BlockKind::Absolute {
            width,
            height: stretched,
        }
    };
    let mut fragment = layout_block(block, containing, kind, &[], None, None).fragment;
    let outer_h = fragment.rect.height;
    if let (Some(top), Some(bottom)) = (top, bottom) {
        let free = cb_height - top - bottom - outer_h;
        match (margin_top, margin_bottom) {
            (None, None) => (margin_top, margin_bottom) = (Some(free / 2.0), Some(free / 2.0)),
            (None, Some(bottom_margin)) => margin_top = Some(free - bottom_margin),
            (Some(top_margin), None) => margin_bottom = Some(free - top_margin),
            (Some(_), Some(_)) => {}
        }
    }
    let (margin_top, margin_bottom) = (margin_top.unwrap_or(0.0), margin_bottom.unwrap_or(0.0));
    let y = match (top, bottom) {
        (Some(top), Some(bottom)) if align_self.is_some() => {
            let free = cb_height - top - bottom - margin_top - margin_bottom - outer_h;
            top + margin_top + free * align_self.map_or(0.0, |a| a.fraction(free))
        }
        (Some(top), _) => top + margin_top,
        (None, Some(bottom)) => cb_height - bottom - margin_bottom - outer_h,
        (None, None) => match align {
            Some((_, align_y)) => {
                let outer = outer_h + margin_top + margin_bottom;
                margin_top + aligned(align_y, cb_height - outer, outer)
            }
            None => static_y + margin_top,
        },
    };
    fragment.rect.x = cb.x + x;
    fragment.rect.y = cb.y + y;
    let room = (
        (cb_width - left.unwrap_or(0.0) - right.unwrap_or(0.0)).max(0.0),
        (cb_height - top.unwrap_or(0.0) - bottom.unwrap_or(0.0)).max(0.0),
    );
    Placement {
        fragment,
        overflows: false,
        anchored: true,
        room,
    }
}
