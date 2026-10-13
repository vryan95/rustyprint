//! Block layout: widths, heights and margin collapsing (CSS 2.1 §§ 8.3, 10.3–10.7), and
//! breaking block and inline content across pages (CSS Fragmentation 3).

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    AlignPosition, BoxDecorationBreak, BoxSizing, BreakBetween, CalcSizeBasis, ContentAlignment,
    ContentDistribution, Direction, Display, DisplayInside, LengthPercentage, MaybeAuto, Overflow,
    PageSide, Size,
};
use rustyprint_css::values::{CalcContext, CalcNode};

use crate::boxes::{BlockBox, BlockContents, InlineItem, Replaced};
use crate::display::{Point, Rect};
use crate::floats::Floats;
use crate::fragment::{BoxFragment, Fragment, LineFragment, LineItem, OutOfFlow, Sides};
use crate::inline;
use crate::picture::Picture;
use rustyprint_css::properties::types::FrameSizing;

/// Slack for comparing positions with the end of the page.
pub(crate) const EPSILON: f32 = 0.01;

/// A margin that may collapse with adjoining margins: the largest positive and the most
/// negative margin seen.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct CollapsibleMargin {
    positive: f32,
    negative: f32,
}

impl CollapsibleMargin {
    pub(crate) fn of(margin: f32) -> Self {
        Self {
            positive: margin.max(0.0),
            negative: margin.min(0.0),
        }
    }

    fn adjoin(&mut self, other: CollapsibleMargin) {
        self.positive = self.positive.max(other.positive);
        self.negative = self.negative.min(other.negative);
    }

    pub(crate) fn value(&self) -> f32 {
        self.positive + self.negative
    }
}

/// The space a box has when its content breaks across pages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fragmentainer {
    /// Space from the top of the box's border box to the end of the page.
    pub available: f32,
    /// Whether nothing precedes the box on the page, so it has to place some content
    /// rather than move to the next page.
    pub at_top: bool,
    /// Whether the fragmentainer is a column of a multi-column container, where column
    /// breaks apply.
    pub columns: bool,
}

/// Where a box broke across pages, to continue its layout on the next page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BreakToken {
    /// The content height placed on earlier pages, which a definite height no longer needs.
    pub consumed: f32,
    pub resume: Resume,
    /// Whether the break was forced. Margins after a forced break are kept; after an
    /// unforced one they are truncated.
    pub forced: bool,
    /// The side the next page must be on, for `break-before: left` and similar.
    pub side: Option<PageSide>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Resume {
    /// Continue with block child `index`, inside it from `token` if it broke.
    Child {
        index: usize,
        token: Option<Box<BreakToken>>,
    },
    /// Continue with line `index` of the inline content.
    Line { index: usize },
    /// Continue a table with body row `index`.
    Row { index: usize },
    /// Continue a table inside body row `index`, which broke: each cell from its token, or
    /// with nothing left if it has none.
    RowPart {
        index: usize,
        cells: Vec<Option<BreakToken>>,
    },
    /// Continue a flex container inside its line (or column item) `index`, which broke:
    /// each of its items from where it is.
    FlexPart {
        index: usize,
        items: Vec<ItemResume>,
        /// How much of the unit earlier pages took, which items that have not started
        /// yet move up by.
        consumed: f32,
    },
}

/// Where an item of a flex line that broke across pages continues.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ItemResume {
    /// It ended on an earlier page.
    Done,
    /// None of it fit on the page before.
    NotStarted,
    Continue(BreakToken),
}

/// A laid-out block box and what its parent needs to place it.
pub(crate) struct Laid {
    /// The box, with its border box at the origin.
    pub fragment: BoxFragment,
    /// Used margins.
    pub margin: Sides,
    /// The top margin after collapsing with descendants' margins that adjoin it.
    pub collapsed_top: CollapsibleMargin,
    pub collapsed_bottom: CollapsibleMargin,
    /// Whether the box is empty and its top and bottom margins collapse through it.
    pub through: bool,
    /// Where the box broke, if it continues on the next page.
    pub break_token: Option<BreakToken>,
    /// Whether the box cannot start on this page and should move to the next one whole.
    pub deferred: bool,
}

/// What a box's sizes resolve against.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContainingBlock {
    pub width: f32,
    /// `None` when the height depends on content, which makes percentage heights `auto`.
    pub height: Option<f32>,
    /// Whether the containing block is right-to-left, which decides which margin gives way
    /// when they are over-constrained.
    pub rtl: bool,
}

fn resolve(value: &LengthPercentage, basis: f32) -> f32 {
    value.resolve(basis)
}

/// A length that needs a definite basis when it has a percentage.
fn resolve_definite(value: &LengthPercentage, basis: Option<f32>) -> Option<f32> {
    match (value, basis) {
        (LengthPercentage::Length(px), _) => Some(*px),
        (other, Some(basis)) => Some(other.resolve(basis)),
        (_, None) => None,
    }
}

fn margin(value: &MaybeAuto<LengthPercentage>, basis: f32) -> Option<f32> {
    match value {
        MaybeAuto::Auto => None,
        MaybeAuto::Value(value) => Some(resolve(value, basis)),
    }
}

/// Padding and border widths. Percentages refer to the containing block's width.
pub(crate) fn padding_and_border(style: &ComputedValues, cb_width: f32) -> (Sides, Sides) {
    let padding = Sides {
        top: resolve(&style.padding_top, cb_width),
        right: resolve(&style.padding_right, cb_width),
        bottom: resolve(&style.padding_bottom, cb_width),
        left: resolve(&style.padding_left, cb_width),
    };
    let border = Sides {
        top: style.border_top_width,
        right: style.border_right_width,
        bottom: style.border_bottom_width,
        left: style.border_left_width,
    };
    (padding, border)
}

/// Whether a box establishes a block formatting context, which keeps its margins from
/// collapsing with its children's. Boxes in another writing mode than their parent do.
fn establishes_bfc(block: &BlockBox, is_root: bool, atomic: bool) -> bool {
    let style = &block.style;
    let scrolls = |o: Overflow| !matches!(o, Overflow::Visible | Overflow::Clip);
    let flow_root =
        matches!(style.display, Display::Normal { inside, .. } if inside != DisplayInside::Flow);
    let turned = matches!(block.contents, BlockContents::Turned { .. });
    let contain = style.used_contain();
    is_root
        || atomic
        || flow_root
        || turned
        || scrolls(style.overflow_x)
        || scrolls(style.overflow_y)
        || contain.layout
        || contain.paint
}

/// The break value before a box, with its first child's propagated to it (CSS Fragmentation
/// 3 § 3.1).
pub(crate) fn break_before(block: &BlockBox) -> BreakBetween {
    let inner = match &block.contents {
        BlockContents::Blocks(blocks) => blocks.first().map(break_before),
        BlockContents::Inline(_)
        | BlockContents::Flex(_)
        | BlockContents::Grid(_)
        | BlockContents::Table(_)
        | BlockContents::OutOfFlow(_)
        | BlockContents::Float(_)
        | BlockContents::Turned { .. }
        | BlockContents::Ruby(_) => None,
    };
    stronger(block.style.break_before, inner)
}

/// The break value after a box, with its last child's propagated to it.
pub(crate) fn break_after(block: &BlockBox) -> BreakBetween {
    let inner = match &block.contents {
        BlockContents::Blocks(blocks) => blocks.last().map(break_after),
        BlockContents::Inline(_)
        | BlockContents::Flex(_)
        | BlockContents::Grid(_)
        | BlockContents::Table(_)
        | BlockContents::OutOfFlow(_)
        | BlockContents::Float(_)
        | BlockContents::Turned { .. }
        | BlockContents::Ruby(_) => None,
    };
    stronger(block.style.break_after, inner)
}

/// Whether a break value forces a break in a page, or in a column when `columns`.
fn forces(value: BreakBetween, columns: bool) -> bool {
    if columns {
        value.forces_column()
    } else {
        value.forces_page()
    }
}

/// Whether a break value avoids a break in a page, or in a column when `columns`.
fn avoids(value: BreakBetween, columns: bool) -> bool {
    if columns {
        value.avoids_column()
    } else {
        value.avoids_page()
    }
}

/// The stronger of two break values: forced breaks win over avoided ones, which win over
/// `auto`; a forced break with a side wins over one without, and page breaks over column
/// breaks.
fn stronger(outer: BreakBetween, inner: Option<BreakBetween>) -> BreakBetween {
    let Some(inner) = inner else {
        return outer;
    };
    let rank = |b: BreakBetween| {
        if b.side().is_some() {
            4
        } else if b.forces_page() {
            3
        } else if b.forces_column() {
            2
        } else if b.avoids_page() || b.avoids_column() {
            1
        } else {
            0
        }
    };
    if rank(inner) > rank(outer) {
        inner
    } else {
        outer
    }
}

/// The forced break between two adjacent siblings, as the side the next page must be on,
/// in a page or in a column when `columns`. Boxes on differently named pages are always
/// separated by a break.
pub(crate) fn forced_between(
    before: &BlockBox,
    after: &BlockBox,
    columns: bool,
) -> Option<Option<PageSide>> {
    let value = stronger(break_after(before), Some(break_before(after)));
    if forces(value, columns) {
        Some(value.side())
    } else if end_page(before) != start_page(after) {
        Some(None)
    } else {
        None
    }
}

/// The used page name where a box's content starts.
fn start_page(block: &BlockBox) -> Option<&Arc<str>> {
    match &block.contents {
        BlockContents::Blocks(children) if !children.is_empty() => start_page(&children[0]),
        _ => block.page.as_ref(),
    }
}

/// The used page name where a box's content ends.
fn end_page(block: &BlockBox) -> Option<&Arc<str>> {
    match &block.contents {
        BlockContents::Blocks(children) if !children.is_empty() => {
            end_page(&children[children.len() - 1])
        }
        _ => block.page.as_ref(),
    }
}

/// The used page name of the content a page starts with, continuing from `token`.
pub(crate) fn resume_page(block: &BlockBox, token: Option<&BreakToken>) -> Option<Arc<str>> {
    match (&block.contents, token.map(|t| &t.resume)) {
        (BlockContents::Blocks(children), Some(Resume::Child { index, token })) => {
            match children.get(*index) {
                Some(child) => resume_page(child, token.as_deref()),
                None => block.page.clone(),
            }
        }
        (BlockContents::Blocks(children), None) if !children.is_empty() => {
            resume_page(&children[0], None)
        }
        _ => block.page.clone(),
    }
}

/// Whether a break between two adjacent siblings should be avoided.
fn avoided_between(before: &BlockBox, after: &BlockBox, columns: bool) -> bool {
    avoids(break_after(before), columns) || avoids(break_before(after), columns)
}

/// Whether the forced break where `token` continues a box breaks the page, not just the
/// column.
pub(crate) fn breaks_page(block: &BlockBox, token: &BreakToken) -> bool {
    match (&block.contents, &token.resume) {
        (BlockContents::Blocks(children), Resume::Child { index, token }) => match token {
            Some(inner) => children
                .get(*index)
                .is_some_and(|child| breaks_page(child, inner)),
            None => {
                *index > 0
                    && children.get(*index).is_some_and(|after| {
                        forced_between(&children[*index - 1], after, false).is_some()
                    })
            }
        },
        _ => false,
    }
}

/// Lays out the root element in a page area, from `resume` if earlier pages broke it.
/// Without a fragmentainer, everything goes on one page area as tall as it needs.
pub(crate) fn layout_root(
    root: &BlockBox,
    page_width: f32,
    page_height: f32,
    fragmentainer: Option<Fragmentainer>,
    resume: Option<&BreakToken>,
) -> (BoxFragment, Option<BreakToken>) {
    let cb = ContainingBlock {
        width: page_width,
        height: Some(page_height),
        rtl: root.style.direction == Direction::Rtl,
    };
    crate::writing::set_viewport(
        root.style.writing_mode,
        crate::display::Size::new(page_width, page_height),
    );
    // The root's margins sit inside the page area, so its box gets the space below them.
    let margin_top = if resume.is_some() {
        0.0
    } else {
        margin(&root.style.margin_top, page_width).unwrap_or(0.0)
    };
    let fragmentainer = fragmentainer.map(|f| Fragmentainer {
        available: f.available - margin_top,
        ..f
    });
    let mut laid = layout_block(root, cb, BlockKind::Root, &[], fragmentainer, resume);
    laid.fragment.rect.x = laid.margin.left;
    laid.fragment.rect.y = laid.margin.top;
    // The initial containing block, and the viewport of fixed boxes, is the page area.
    let area = Rect::new(
        -laid.fragment.rect.x,
        -laid.fragment.rect.y,
        page_width,
        page_height,
    );
    crate::position::place_out_of_flow(&mut laid.fragment, area, crate::position::Takes::All);
    (laid.fragment, laid.break_token)
}

/// Lays out an atomic inline, such as an `inline-block`, with shrink-to-fit width.
pub(crate) fn layout_atomic(block: &BlockBox, available: f32) -> Laid {
    let cb = ContainingBlock {
        width: available,
        height: None,
        rtl: block.style.direction == Direction::Rtl,
    };
    // Decorations do not propagate into atomic inlines, and atomic inlines do not break.
    layout_block(block, cb, BlockKind::Atomic, &[], None, None)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BlockKind {
    Root,
    InFlow,
    Atomic,
    /// A table cell, as wide as its columns, whose height its row decides.
    Cell,
    /// An absolutely positioned box, with the content width its containing block gave it,
    /// and a content height when it stretches between `top` and `bottom`.
    Absolute {
        width: f32,
        height: Option<f32>,
    },
    /// A flex or grid item, with the content width and height its container gave it.
    Item {
        width: f32,
        height: Option<f32>,
        /// Whether the height is only a minimum that the content can exceed, as when a
        /// stretched item breaks across pages and needs more room than unbroken.
        at_least: bool,
    },
}

impl BlockKind {
    /// Whether the box is sized on its own rather than by the flow around it: it resolves
    /// no auto margins and establishes a block formatting context.
    fn is_independent(self) -> bool {
        matches!(
            self,
            BlockKind::Atomic
                | BlockKind::Cell
                | BlockKind::Absolute { .. }
                | BlockKind::Item { .. }
        )
    }
}

/// The content-box width from `width`, `min-width` or `max-width`, or `None` for `auto`.
pub(crate) fn content_width(
    block: &BlockBox,
    size: &Size<LengthPercentage>,
    cb: ContainingBlock,
    extra: f32,
) -> Option<f32> {
    let style = &block.style;
    let border_box = style.box_sizing == BoxSizing::BorderBox;
    let from_border_box = |w: f32| if border_box { (w - extra).max(0.0) } else { w };
    match size {
        Size::Value(value) => Some(from_border_box(resolve(value, cb.width))),
        Size::MinContent => Some(intrinsic_content(block).0),
        Size::MaxContent => Some(intrinsic_content(block).1),
        Size::FitContent(limit) => {
            let (min, max) = intrinsic_content(block);
            let available = match limit {
                Some(limit) => from_border_box(resolve(limit, cb.width)),
                None => cb.width - extra,
            };
            Some(available.min(max).max(min))
        }
        Size::CalcSize(calc) => {
            let available = (cb.width - extra).max(0.0);
            let basis = match &calc.basis {
                CalcSizeBasis::Value(value) => from_border_box(resolve(value, cb.width)),
                CalcSizeBasis::MinContent => intrinsic_content(block).0,
                CalcSizeBasis::MaxContent => intrinsic_content(block).1,
                CalcSizeBasis::FitContent => {
                    let (min, max) = intrinsic_content(block);
                    available.min(max).max(min)
                }
                CalcSizeBasis::Auto => available,
                CalcSizeBasis::Any => 0.0,
            };
            calc_size(&calc.expression, basis, Some(cb.width))
        }
        Size::Auto | Size::None | Size::Stretch => None,
    }
}

/// The size `calc-size()` gives with a basis of `size`, its percentages resolving against
/// `percent_basis`.
pub(crate) fn calc_size(
    expression: &CalcNode,
    size: f32,
    percent_basis: Option<f32>,
) -> Option<f32> {
    expression
        .evaluate(&CalcContext {
            size: Some(f64::from(size)),
            percent_basis: percent_basis.map(f64::from),
            ..Default::default()
        })
        .map(|value| (value as f32).max(0.0))
}

/// A child placed on the current page, remembered so a later break can return to it.
struct Placed {
    index: usize,
    /// The number of fragments before the child's.
    fragments: usize,
    /// The content height before the child and its margins.
    cursor: f32,
}

/// Lays out a block box. `decorators` are the ancestors whose text decorations propagate to
/// its content, outermost first.
///
/// With a fragmentainer the box stops where the page ends and returns a break token to
/// continue from; `resume` is the token from the page before.
pub(crate) fn layout_block(
    block: &BlockBox,
    cb: ContainingBlock,
    kind: BlockKind,
    decorators: &[Arc<ComputedValues>],
    fragmentainer: Option<Fragmentainer>,
    resume: Option<&BreakToken>,
) -> Laid {
    layout_flow(block, cb, kind, decorators, fragmentainer, resume, None)
}

/// Lays out a block box in a block formatting context whose floats are `floats`, with its
/// border box at `origin` in the context's coordinates, roughly: margins that collapse
/// with its descendants' can still move it. Without them, or if the box establishes a
/// formatting context of its own, its floats are its own.
fn layout_flow(
    block: &BlockBox,
    cb: ContainingBlock,
    kind: BlockKind,
    decorators: &[Arc<ComputedValues>],
    fragmentainer: Option<Fragmentainer>,
    resume: Option<&BreakToken>,
    floats_in: Option<(&mut Floats, Point)>,
) -> Laid {
    rustyprint_dom::budget::check_deadline();
    match &block.contents {
        BlockContents::OutOfFlow(positioned) => {
            return placeholder(block, OutOfFlow::Positioned(positioned.clone()));
        }
        BlockContents::Float(float) => {
            return placeholder(block, OutOfFlow::Float(float.clone()));
        }
        _ => {}
    }
    if let Some(replaced) = &block.replaced {
        return layout_replaced(block, replaced, cb, kind, fragmentainer);
    }
    if let BlockContents::Table(table) = &block.contents {
        return crate::table::layout(block, table, cb, kind, decorators, fragmentainer, resume);
    }
    if let BlockContents::Ruby(ruby) = &block.contents {
        return crate::ruby::layout(block, ruby);
    }
    let style = &block.style;
    let mut own_decorators = decorators.to_vec();
    if !style.text_decoration_line.is_none() {
        own_decorators.push(style.clone());
    }
    let (mut padding, mut border) = padding_and_border(style, cb.width);
    let extra = padding.horizontal() + border.horizontal();
    // Cells have no margins, are as wide as their columns, and their rows decide their
    // heights (CSS 2.1 § 17.5).
    let cell = kind == BlockKind::Cell;
    let own_margin = |value| {
        if cell {
            Some(0.0)
        } else {
            margin(value, cb.width)
        }
    };
    let mut margin_left = own_margin(&style.margin_left);
    let mut margin_right = own_margin(&style.margin_right);
    let mut margin_top = own_margin(&style.margin_top).unwrap_or(0.0);
    let margin_bottom = own_margin(&style.margin_bottom).unwrap_or(0.0);

    // Heights that do not depend on content: CSS 2.1 § 10.6.3 and § 10.7.
    let vertical = padding.vertical() + border.vertical();
    let border_box_height = style.box_sizing == BoxSizing::BorderBox;
    let from_border_box = |h: f32| {
        if border_box_height {
            (h - vertical).max(0.0)
        } else {
            h
        }
    };
    let content_height = |size: &Size<LengthPercentage>| match size {
        _ if cell => None,
        Size::Value(value) => resolve_definite(value, cb.height).map(from_border_box),
        // A `calc-size()` of a definite basis is definite; one of the content waits for it.
        Size::CalcSize(calc) => match &calc.basis {
            CalcSizeBasis::Value(value) => resolve_definite(value, cb.height)
                .and_then(|h| calc_size(&calc.expression, from_border_box(h), cb.height)),
            CalcSizeBasis::Any => calc_size(&calc.expression, 0.0, cb.height),
            _ => None,
        },
        _ => None,
    };
    let specified_height = match kind {
        BlockKind::Absolute {
            height: Some(height),
            ..
        }
        | BlockKind::Item {
            height: Some(height),
            at_least: false,
            ..
        } => Some(height),
        // Size containment sizes an `auto` height as if the box were empty.
        _ => content_height(&style.height).or_else(|| style.contained_height().filter(|_| !cell)),
    };
    let min_height = match kind {
        BlockKind::Item {
            height: Some(height),
            at_least: true,
            ..
        } => height,
        _ => content_height(&style.min_height).unwrap_or(0.0),
    };
    let max_height = content_height(&style.max_height);

    // `aspect-ratio` relates the width and height of the box-sizing box, or of the content
    // box with `auto` (CSS Sizing 4 § 5).
    let ratio = style.aspect_ratio.value().filter(|_| !cell);
    let ratio_border_box = style.box_sizing == BoxSizing::BorderBox && !style.aspect_ratio.auto;
    let width_from_height = |h: f32| {
        ratio.map(|r| {
            if ratio_border_box {
                ((h + vertical) * r - extra).max(0.0)
            } else {
                h * r
            }
        })
    };
    let height_from_width = |w: f32| {
        ratio.map(|r| {
            if ratio_border_box {
                ((w + extra) / r - vertical).max(0.0)
            } else {
                w / r
            }
        })
    };

    // Width: CSS 2.1 § 10.3.3, or shrink-to-fit (§ 10.3.9) for atomic inlines. An auto
    // width follows a definite height through the aspect ratio.
    let specified = if cell {
        Some((cb.width - extra).max(0.0))
    } else if let BlockKind::Absolute { width, .. } | BlockKind::Item { width, .. } = kind {
        Some(width)
    } else {
        content_width(block, &style.width, cb, extra)
            .or_else(|| specified_height.and_then(width_from_height))
    };
    // `justify-self` places a block-level box that does not stretch across its containing
    // block, at its shrink-to-fit width (CSS Box Alignment 3 § 6.1).
    let justify = (kind == BlockKind::InFlow)
        .then(|| crate::position::self_alignment(style.justify_self, cb.rtl))
        .flatten();
    let tentative = specified.unwrap_or_else(|| {
        let fixed_margins = margin_left.unwrap_or(0.0) + margin_right.unwrap_or(0.0);
        let available = (cb.width - fixed_margins - extra).max(0.0);
        if kind == BlockKind::Atomic || justify.is_some() {
            let (min, max) = intrinsic_content(block);
            available.min(max).max(min)
        } else {
            available
        }
    });
    let width = if cell || matches!(kind, BlockKind::Absolute { .. } | BlockKind::Item { .. }) {
        tentative
    } else {
        let max = content_width(block, &style.max_width, cb, extra);
        let min = content_width(block, &style.min_width, cb, extra).unwrap_or(0.0);
        tentative.min(max.unwrap_or(f32::INFINITY)).max(min)
    };

    if !kind.is_independent() {
        // Auto margins share the remaining space; over-constrained boxes ignore the end margin.
        let remaining = cb.width - width - extra;
        let rtl = cb.rtl;
        match (margin_left, margin_right) {
            (None, None) => {
                let half = (remaining / 2.0).max(0.0);
                margin_left = Some(if remaining < 0.0 && rtl {
                    remaining
                } else {
                    half
                });
                margin_right = Some(if remaining < 0.0 && !rtl {
                    remaining
                } else {
                    half
                });
            }
            (None, Some(right)) => margin_left = Some(remaining - right),
            (Some(left), None) => margin_right = Some(remaining - left),
            (Some(left), Some(right)) if justify.is_some() => {
                let free = remaining - left - right;
                let before = free * justify.map_or(0.0, |a| a.fraction(free));
                margin_left = Some(left + before);
                margin_right = Some(right + free - before);
            }
            (Some(left), Some(right)) => {
                if rtl {
                    margin_left = Some(remaining - right);
                } else {
                    margin_right = Some(remaining - left);
                }
            }
        }
    }

    // An auto height follows the width through the aspect ratio.
    let ratio_height = match specified_height {
        Some(_) => None,
        None => height_from_width(width),
    };
    let definite_height = specified_height.or(ratio_height);

    // Breaking across pages. `break-inside: avoid` keeps the box whole unless it starts a
    // page, where it has to break.
    let columns = fragmentainer.is_some_and(|f| f.columns);
    let avoid_inside = if columns {
        style.break_inside.avoids_column()
    } else {
        style.break_inside.avoids_page()
    };
    let fragmentainer = fragmentainer.filter(|f| f.at_top || !avoid_inside);
    let resumed = resume.is_some();
    let consumed = resume.map_or(0.0, |token| token.consumed);
    let clone = style.box_decoration_break == BoxDecorationBreak::Clone;
    if resumed {
        // A continuation has no top margin, and no top border or padding unless cloned.
        margin_top = 0.0;
        if !clone {
            border.top = 0.0;
            padding.top = 0.0;
        }
    }
    // A fieldset's rendered legend sits in its top border, which takes the legend's height
    // when that is more. It only goes on the first fragment.
    let legend = block.legend.as_deref().filter(|_| !resumed).map(|legend| {
        place_legend(
            legend,
            border,
            padding,
            width,
            style.direction == Direction::Rtl,
        )
    });
    let legend_extra = legend.as_ref().map_or(0.0, |legend| legend.extra);
    let content_x = border.left + padding.left;
    let content_y = border.top + legend_extra + padding.top;
    // Where content must end on this page, from the top of the border box. Cloned
    // decorations need room for the bottom border and padding too.
    let limit = fragmentainer.map(|f| {
        let bottom = if clone {
            border.bottom + padding.bottom
        } else {
            0.0
        };
        f.available - bottom
    });
    // A box whose top border and padding do not fit moves to the next page. One without
    // them may still be empty, with margins that collapse through it, and its content
    // moves on by itself.
    if let (Some(f), Some(limit)) = (fragmentainer, limit)
        && !f.at_top
        && content_y > 0.0
        && content_y > limit + EPSILON
    {
        return deferred(block, border, padding, width, extra);
    }

    let bfc = establishes_bfc(block, kind == BlockKind::Root, kind.is_independent());
    let mut own_floats = Floats::default();
    let (floats, origin, owns_floats) = match floats_in {
        Some((floats, origin)) if !bfc => (floats, origin, false),
        _ => (&mut own_floats, Point::new(0.0, 0.0), true),
    };
    // The content box's edges in the formatting context.
    let (content_left, content_top) = (origin.x + content_x, origin.y + content_y);
    let content_right = content_left + width;
    let collapse_top = !bfc && border.top == 0.0 && padding.top == 0.0;
    let collapse_bottom = !bfc
        && border.bottom == 0.0
        && padding.bottom == 0.0
        && definite_height.is_none()
        && min_height == 0.0;

    let child_cb = ContainingBlock {
        width,
        height: definite_height,
        rtl: style.direction == Direction::Rtl,
    };

    let mut children = Vec::new();
    let mut collapsed_top = CollapsibleMargin::of(margin_top);
    // Margin waiting to be placed between the previous child and the next.
    let mut pending = CollapsibleMargin::default();
    // Whether no in-flow content has been placed yet, so child margins still adjoin the top.
    let mut top_open = collapse_top;
    let mut has_content = false;
    let mut cursor = 0.0;
    // `margin-trim` drops the margins of children that adjoin the block-start and block-end
    // edges, and so does an unforced break before the first child on a page.
    let trim = style.margin_trim;
    let trim_start = trim.block_start || resume.is_some_and(|token| !token.forced);
    let mut break_token = None;
    let all_children;
    // Floats from the lines of inline content.
    let mut line_floats: Vec<Fragment> = Vec::new();

    match &block.contents {
        BlockContents::Blocks(blocks) if !crate::multicol::is_multicol(style) => {
            all_children = blocks.len();
            let (start, mut start_token) = match resume.map(|token| &token.resume) {
                Some(Resume::Child { index, token }) => (*index, token.as_deref()),
                _ => (0, None),
            };
            let mut placed: Vec<Placed> = Vec::new();
            for (index, child) in blocks.iter().enumerate().skip(start) {
                let child_token = if index == start {
                    start_token.take()
                } else {
                    None
                };
                if index > start
                    && let Some(side) = forced_between(&blocks[index - 1], child, columns)
                {
                    break_token = Some(BreakToken {
                        consumed: 0.0,
                        resume: Resume::Child { index, token: None },
                        forced: true,
                        side,
                    });
                    break;
                }

                // A float goes beside earlier floats, as high as it can (CSS 2.1 § 9.5.1).
                if let Some(float_left) = child.style.float.is_left(child_cb.rtl) {
                    let at = if top_open {
                        cursor
                    } else {
                        cursor + pending.value()
                    };
                    let fragment = place_float(
                        child,
                        child_cb,
                        floats,
                        origin,
                        float_left,
                        content_top + at,
                        (content_left, content_right),
                    );
                    // A float that runs past the end of the page moves to the next one,
                    // with what follows it, unless nothing comes before it.
                    if let (Some(f), Some(limit)) = (fragmentainer, limit)
                        && (has_content || !f.at_top)
                        && fragment.rect.bottom() > limit + EPSILON
                    {
                        break_token = Some(BreakToken {
                            consumed: 0.0,
                            resume: Resume::Child { index, token: None },
                            forced: false,
                            side: None,
                        });
                        break;
                    }
                    children.push(Fragment::Box(fragment));
                    continue;
                }

                // Where the child's border box will roughly start.
                let own_top =
                    CollapsibleMargin::of(margin(&child.style.margin_top, width).unwrap_or(0.0));
                let mut estimate = if top_open || (trim_start && !has_content) {
                    cursor
                } else {
                    let mut margin = pending;
                    margin.adjoin(own_top);
                    cursor + margin.value()
                };
                // Clearance moves the box below the floats it clears (CSS 2.1 § 9.5.2).
                let (clear_left, clear_right) = child.style.clear.sides(child_cb.rtl);
                let mut forced = floats
                    .clearance(clear_left, clear_right)
                    .map(|bottom| bottom - content_top)
                    .filter(|&below| below > estimate);
                if let Some(below) = forced {
                    estimate = below;
                }
                let child_fragmentainer =
                    fragmentainer.zip(limit).map(|(f, limit)| Fragmentainer {
                        available: limit - content_y - estimate,
                        at_top: f.at_top && !has_content,
                        columns: f.columns,
                    });
                // A box with its own formatting context does not overlap floats: it is
                // narrower beside them, or goes below them (CSS 2.1 § 9.5).
                let independent = establishes_bfc(child, false, false);
                let mut shift = 0.0;
                let mut laid = if independent && !floats.is_empty() {
                    let need = intrinsic_outer(child).0;
                    let mut height = 0.0;
                    let mut laid = None;
                    for _ in 0..2 {
                        let (top, band) = floats.fit(
                            content_top + estimate,
                            height,
                            need,
                            content_left,
                            content_right,
                        );
                        let narrowed = ContainingBlock {
                            width: (width - band.left - band.right).max(0.0),
                            ..child_cb
                        };
                        let attempt = layout_flow(
                            child,
                            narrowed,
                            BlockKind::InFlow,
                            &own_decorators,
                            child_fragmentainer,
                            child_token,
                            None,
                        );
                        let moved = top - content_top;
                        if moved > estimate + EPSILON {
                            forced = Some(moved);
                        }
                        shift = if child_cb.rtl { 0.0 } else { band.left };
                        let fits = (attempt.fragment.rect.height - height).abs() < EPSILON;
                        height = attempt.fragment.rect.height;
                        laid = Some(attempt);
                        if fits {
                            break;
                        }
                    }
                    laid.expect("the box was laid out")
                } else if independent {
                    layout_flow(
                        child,
                        child_cb,
                        BlockKind::InFlow,
                        &own_decorators,
                        child_fragmentainer,
                        child_token,
                        None,
                    )
                } else {
                    let left = margin(&child.style.margin_left, width).unwrap_or(0.0);
                    let child_origin = Point::new(content_left + left, content_top + estimate);
                    layout_flow(
                        child,
                        child_cb,
                        BlockKind::InFlow,
                        &own_decorators,
                        child_fragmentainer,
                        child_token,
                        Some((&mut *floats, child_origin)),
                    )
                };
                if trim_start && !has_content {
                    laid.collapsed_top = CollapsibleMargin::default();
                    if laid.through {
                        laid.collapsed_bottom = CollapsibleMargin::default();
                    }
                }
                let record = Placed {
                    index,
                    fragments: children.len(),
                    cursor,
                };
                // Clearance separates the box's margins from those above it.
                if forced.is_some() && !laid.through {
                    top_open = false;
                }
                let y = if laid.through {
                    cursor
                } else if top_open {
                    collapsed_top.adjoin(laid.collapsed_top);
                    cursor
                } else {
                    let mut margin = pending;
                    margin.adjoin(laid.collapsed_top);
                    cursor + margin.value()
                };
                let y = match forced {
                    Some(below) if !laid.through => y.max(below),
                    _ => y,
                };

                // A child that does not fit moves to the next page, with the children after
                // the last break that is not avoided.
                if let (Some(f), Some(limit)) = (fragmentainer, limit) {
                    let overflows = laid.break_token.is_none()
                        && content_y + y + laid.fragment.rect.height > limit + EPSILON;
                    if (laid.deferred || overflows) && (has_content || !f.at_top) {
                        // Break point `k` is before `placed[k]`, or before this child when
                        // `k` is `placed.len()`.
                        let next = |k: usize| placed.get(k).map_or(index, |p| p.index);
                        let mut k = placed.len();
                        while k > 0
                            && avoided_between(
                                &blocks[placed[k - 1].index],
                                &blocks[next(k)],
                                columns,
                            )
                        {
                            k -= 1;
                        }
                        if k == 0 {
                            if !f.at_top {
                                // No break inside the box works: it moves to the next page.
                                return deferred(block, border, padding, width, extra);
                            }
                            // The box starts the page, so it breaks before this child anyway.
                            k = placed.len();
                        }
                        let (resume_at, fragments, content) = match placed.get(k) {
                            Some(back) => (back.index, back.fragments, back.cursor),
                            None => (index, children.len(), record.cursor),
                        };
                        children.truncate(fragments);
                        cursor = content;
                        pending = CollapsibleMargin::default();
                        break_token = Some(BreakToken {
                            consumed: 0.0,
                            resume: Resume::Child {
                                index: resume_at,
                                token: None,
                            },
                            forced: false,
                            side: None,
                        });
                        break;
                    }
                }

                let (dx, dy) = crate::position::relative_offset(&child.style, child_cb);
                laid.fragment.rect.x = content_x + shift + laid.margin.left + dx;
                laid.fragment.rect.y = content_y + y + dy;
                if laid.through {
                    if top_open {
                        collapsed_top.adjoin(laid.collapsed_top);
                        collapsed_top.adjoin(laid.collapsed_bottom);
                    } else {
                        pending.adjoin(laid.collapsed_top);
                        pending.adjoin(laid.collapsed_bottom);
                    }
                    children.push(Fragment::Box(laid.fragment));
                    placed.push(record);
                    continue;
                }
                top_open = false;
                cursor = y + laid.fragment.rect.height;
                pending = laid.collapsed_bottom;
                has_content = true;
                children.push(Fragment::Box(laid.fragment));
                placed.push(record);
                if let Some(token) = laid.break_token {
                    // Margins at a break are truncated.
                    pending = CollapsibleMargin::default();
                    break_token = Some(BreakToken {
                        consumed: 0.0,
                        forced: token.forced,
                        side: token.side,
                        resume: Resume::Child {
                            index,
                            token: Some(Box::new(token)),
                        },
                    });
                    break;
                }
            }
        }
        BlockContents::Inline(items) => {
            let in_lines = items
                .iter()
                .filter(|item| matches!(item, InlineItem::Atomic(b) if matches!(b.contents, BlockContents::Float(_))))
                .count();
            let lines = loop {
                let band = |top: f32, height: f32| {
                    let band =
                        floats.line_band(content_top + top, height, content_left, content_right);
                    (band.left, band.right)
                };
                let exclusion: Option<inline::Exclusion> =
                    (!floats.is_empty()).then_some(&band as inline::Exclusion);
                let lines = inline::layout(
                    items,
                    style,
                    width,
                    &own_decorators,
                    &block.first_line,
                    exclusion,
                );
                // Each float in a line goes at the top of its line, and the lines are laid
                // out again around it. Floats only go on the first page.
                if resumed || line_floats.len() == in_lines {
                    break lines;
                }
                let Some((float, top)) = line_float(&lines.lines, line_floats.len()) else {
                    break lines;
                };
                let float_left = float.style.float.is_left(child_cb.rtl).unwrap_or(true);
                line_floats.push(Fragment::Box(place_float(
                    &float,
                    child_cb,
                    floats,
                    origin,
                    float_left,
                    content_top + top,
                    (content_left, content_right),
                )));
            };
            let count = lines.lines.len();
            all_children = count;
            let first = match resume.map(|token| &token.resume) {
                Some(Resume::Line { index }) => (*index).min(count),
                _ => 0,
            };
            // Continuations start at their first line; the first fragment keeps any trim
            // that moved the first line above the content edge.
            let offset = match first {
                0 => 0.0,
                _ => lines.lines.get(first).map_or(lines.height, |l| l.rect.y),
            };
            let mut end = count;
            if let (Some(f), Some(limit)) = (fragmentainer, limit) {
                let fits =
                    |k: usize| content_y + lines.lines[k].rect.bottom() - offset <= limit + EPSILON;
                let mut fit = first;
                while fit < count && fits(fit) {
                    fit += 1;
                }
                if fit < count {
                    // At least `widows` lines go to the next page, and `orphans` stay here.
                    let widows = style.widows as usize;
                    let orphans = style.orphans as usize;
                    let mut at = fit.min(count.saturating_sub(widows));
                    if at < first + orphans.min(count - first) || at <= first {
                        if !f.at_top {
                            return deferred(block, border, padding, width, extra);
                        }
                        at = fit.max(first + 1);
                    }
                    end = at;
                }
            }
            has_content = end > first;
            for mut line in lines.lines.into_iter().take(end).skip(first) {
                line.rect.x += content_x;
                line.rect.y += content_y - offset;
                children.push(Fragment::Line(line));
            }
            cursor = if end == count {
                lines.height - offset
            } else {
                match children.last() {
                    Some(Fragment::Line(line)) => line.rect.bottom() - content_y,
                    _ => 0.0,
                }
            };
            if end < count {
                break_token = Some(BreakToken {
                    consumed: 0.0,
                    resume: Resume::Line { index: end },
                    forced: false,
                    side: None,
                });
            }
        }
        // Flex and grid containers, and multi-column containers, whose blocks are their
        // column flows and spanners.
        BlockContents::Flex(items) | BlockContents::Grid(items) | BlockContents::Blocks(items) => {
            let space = crate::units::ContentSpace {
                width,
                height: definite_height,
                min_height,
                max_height,
                padding,
            };
            let fragmentainer = fragmentainer.zip(limit).map(|(f, limit)| Fragmentainer {
                available: limit - content_y,
                at_top: f.at_top,
                columns: f.columns,
            });
            let lay = match &block.contents {
                BlockContents::Grid(_) => crate::grid::layout,
                BlockContents::Blocks(_) => crate::multicol::layout,
                _ => crate::flex::layout,
            };
            let laid = lay(block, items, space, &own_decorators, fragmentainer, resume);
            if laid.deferred {
                return deferred(block, border, padding, width, extra);
            }
            // Multi-column content continues from its segments, so a definite height that
            // breaks resumes after them.
            all_children = match &block.contents {
                BlockContents::Blocks(_) => items.len(),
                _ => 0,
            };
            has_content = !laid.children.is_empty();
            for mut child in laid.children {
                if let Fragment::Box(child) = &mut child {
                    child.rect.x += content_x;
                    child.rect.y += content_y;
                }
                children.push(child);
            }
            cursor = laid.height;
            break_token = laid.break_token;
        }
        // Content in another writing mode, which does not break across pages.
        BlockContents::Turned { inner, outer } => {
            all_children = 0;
            let space = crate::writing::Space {
                width,
                height: definite_height,
                min_height,
                max_height,
                available_height: cb.height,
            };
            let mut fragment = crate::writing::layout(inner, *outer, space, &own_decorators);
            // The content is monolithic: a box that does not fit moves to the next page.
            let needed = content_y
                + specified_height
                    .unwrap_or(fragment.rect.height)
                    .max(min_height)
                + padding.bottom
                + border.bottom;
            if let Some(f) = fragmentainer
                && !f.at_top
                && needed > f.available + EPSILON
            {
                return deferred(block, border, padding, width, extra);
            }
            fragment.rect.x += content_x;
            fragment.rect.y += content_y;
            cursor = fragment.rect.height;
            has_content = true;
            children.push(Fragment::Box(fragment));
        }
        // Laid out by `table::layout`, `placeholder` and `ruby::layout` instead.
        BlockContents::Table(_)
        | BlockContents::OutOfFlow(_)
        | BlockContents::Float(_)
        | BlockContents::Ruby(_) => all_children = 0,
    }

    if let Some(marker) = &block.marker
        && !resumed
    {
        place_marker(
            marker,
            &mut children,
            content_x,
            content_y,
            width,
            child_cb.rtl,
        );
    }

    // Floats paint below the lines beside them, and after the marker finds the first line.
    children.splice(0..0, line_floats);

    // After the marker, which goes beside the first line of the content.
    let legend_gap = legend.map(|legend| {
        children.push(Fragment::Box(legend.fragment));
        legend.gap
    });

    if trim.block_end {
        pending = CollapsibleMargin::default();
    }

    // An empty box whose margins can meet collapses through (CSS 2.1 § 8.3.1).
    let through = kind == BlockKind::InFlow
        && !has_content
        && collapse_top
        && collapse_bottom
        && break_token.is_none()
        && !resumed;
    let mut collapsed_bottom = CollapsibleMargin::of(margin_bottom);
    // The last child's bottom margin, when it stays inside the box.
    let inner_margin = if break_token.is_some() {
        0.0
    } else if collapse_bottom {
        collapsed_bottom.adjoin(pending);
        0.0
    } else {
        pending.value()
    };
    // A formatting context root grows to contain its floats (CSS 2.1 § 10.6.7).
    let content_end = match floats.bottom() {
        Some(bottom) if owns_floats && break_token.is_none() => cursor.max(bottom - content_y),
        _ => cursor,
    };
    let auto_height = content_end.max(cursor + inner_margin);
    if !collapse_top {
        collapsed_top = CollapsibleMargin::of(margin_top);
    }
    // A height from the aspect ratio still grows to fit content, unless `min-height` is set
    // or the box scrolls (CSS Sizing 4 § 5.2.1). Definite heights shrink by what earlier
    // pages took.
    let content_minimum = matches!(style.min_height, Size::Auto)
        && [style.overflow_x, style.overflow_y]
            .iter()
            .all(|o| matches!(o, Overflow::Visible | Overflow::Clip));
    let rest = |h: f32| (h - consumed).max(0.0);
    let mut height = match (specified_height, ratio_height) {
        (Some(height), _) => rest(height),
        (None, Some(height)) if content_minimum => rest(height).max(auto_height),
        (None, Some(height)) => rest(height),
        (None, None) => match &style.height {
            Size::CalcSize(calc) if !cell => {
                calc_size(&calc.expression, auto_height, cb.height).unwrap_or(auto_height)
            }
            _ => auto_height,
        },
    }
    .min(max_height.map_or(f32::INFINITY, rest))
    .max(rest(min_height));
    // `align-content` moves the content of a block container that is taller than it
    // (CSS Box Alignment 3 § 5.4); controls center theirs.
    let fraction = match &block.contents {
        BlockContents::Blocks(_) | BlockContents::Inline(_) => {
            block_content_fraction(style.align_content)
        }
        _ => 0.0,
    };
    let fraction = if block.center_contents { 0.5 } else { fraction };
    if fraction > 0.0 && break_token.is_none() && height > auto_height + EPSILON {
        let offset = (height - auto_height) * fraction;
        // The rendered legend stays in the top border.
        let content = children.len() - usize::from(legend_gap.is_some());
        for child in &mut children[..content] {
            match child {
                Fragment::Box(child) => child.rect.y += offset,
                Fragment::Line(line) => line.rect.y += offset,
            }
        }
    }

    let monolithic = matches!(block.contents, BlockContents::Turned { .. });
    if let Some(limit) = limit.filter(|_| !monolithic) {
        let space = (limit - content_y).max(0.0);
        if break_token.is_some() {
            // A box that broke fills the page if its height continues past it.
            if height > cursor {
                height = height.min(space).max(cursor);
            } else {
                height = cursor;
            }
        } else if height > space + EPSILON
            && height <= auto_height + EPSILON
            && content_end <= space + EPSILON
        {
            // Only the last child's bottom margin runs past the end of the page, and a
            // margin at the end of a page is truncated (CSS Fragmentation 3 § 5.2).
            height = space;
        } else if height > space + EPSILON {
            // A definite height runs past the end of the page: the rest goes on the next.
            height = space.max(cursor);
            break_token = Some(BreakToken {
                consumed: 0.0,
                resume: match &block.contents {
                    BlockContents::Blocks(_) => Resume::Child {
                        index: all_children,
                        token: None,
                    },
                    BlockContents::Inline(_)
                    | BlockContents::Flex(_)
                    | BlockContents::Grid(_)
                    | BlockContents::Table(_)
                    | BlockContents::OutOfFlow(_)
                    | BlockContents::Float(_)
                    | BlockContents::Turned { .. }
                    | BlockContents::Ruby(_) => Resume::Line {
                        index: all_children,
                    },
                },
                forced: false,
                side: None,
            });
        }
    }
    if let Some(token) = &mut break_token {
        token.consumed = consumed + height;
        // The fragment ends at the break, without its bottom margin, and without its bottom
        // border and padding unless cloned.
        collapsed_bottom = CollapsibleMargin::default();
        if !clone {
            border.bottom = 0.0;
            padding.bottom = 0.0;
        }
    }
    let margin = Sides {
        top: margin_top,
        right: margin_right.unwrap_or(0.0),
        bottom: if break_token.is_some() {
            0.0
        } else {
            margin_bottom
        },
        left: margin_left.unwrap_or(0.0),
    };

    let mut fragment = BoxFragment {
        node: block.node,
        style: block.style.clone(),
        rect: Rect::new(
            0.0,
            0.0,
            width + extra,
            height + border.vertical() + padding.vertical() + legend_extra,
        ),
        border,
        padding,
        children,
        replaced: None,
        legend: legend_gap,
        out_of_flow: None,
    };
    // Transformed and filtered boxes contain fixed descendants too.
    let fixed = crate::effects::contains_fixed(style);
    if crate::position::is_positioned(style) || fixed {
        let padding_box = fragment.padding_box();
        let cb = Rect::new(
            padding_box.x - fragment.rect.x,
            padding_box.y - fragment.rect.y,
            padding_box.width,
            padding_box.height,
        );
        let takes = if fixed {
            crate::position::Takes::All
        } else {
            crate::position::Takes::Absolute
        };
        crate::position::place_out_of_flow(&mut fragment, cb, takes);
    }
    if through {
        // Nothing separates the top and bottom margins: they all collapse together.
        let mut all = collapsed_top;
        all.adjoin(collapsed_bottom);
        return Laid {
            fragment,
            margin,
            collapsed_top: all,
            collapsed_bottom: CollapsibleMargin::default(),
            through,
            break_token: None,
            deferred: false,
        };
    }
    Laid {
        fragment,
        margin,
        collapsed_top,
        collapsed_bottom,
        through,
        break_token,
        deferred: false,
    }
}

/// Where `align-content` puts the content of a block container in the space left below it:
/// 0 at the top, 1 at the bottom. Distributions fall back to `start`, or to `center` for
/// those that center a single box.
fn block_content_fraction(value: ContentAlignment) -> f32 {
    match value {
        ContentAlignment::Position { position, .. } => match position {
            AlignPosition::Center => 0.5,
            AlignPosition::End | AlignPosition::FlexEnd | AlignPosition::SelfEnd => 1.0,
            _ => 0.0,
        },
        ContentAlignment::Distribution(
            ContentDistribution::SpaceAround | ContentDistribution::SpaceEvenly,
        ) => 0.5,
        ContentAlignment::Normal
        | ContentAlignment::Baseline { .. }
        | ContentAlignment::Distribution(_) => 0.0,
    }
}

/// A fieldset's rendered legend, laid out and placed in the fieldset's top border.
struct PlacedLegend {
    fragment: BoxFragment,
    /// How much taller the top border area is than the top border, to fit the legend.
    extra: f32,
    /// Where the top border is drawn, relative to the fieldset's border box: across the
    /// legend's middle, with a gap as wide as the legend.
    gap: Rect,
}

/// Lays out a fieldset's rendered legend with a shrink-to-fit width, at the inline start of
/// the content box (or where auto margins put it), centered on the top border (HTML
/// § 15.3.13).
fn place_legend(
    legend: &BlockBox,
    border: Sides,
    padding: Sides,
    width: f32,
    rtl: bool,
) -> PlacedLegend {
    let cb = ContainingBlock {
        width,
        height: None,
        rtl,
    };
    let laid = layout_block(legend, cb, BlockKind::Atomic, &[], None, None);
    let mut fragment = laid.fragment;
    let style = &legend.style;
    let (w, h) = (fragment.rect.width, fragment.rect.height);
    let left = margin(&style.margin_left, width);
    let right = margin(&style.margin_right, width);
    let free = width - w;
    let x = match (left, right) {
        (None, None) => free / 2.0,
        (None, Some(right)) => free - right,
        (Some(left), None) => left,
        (Some(left), Some(right)) => {
            if rtl {
                free - right
            } else {
                left
            }
        }
    };
    let outer = laid.margin.top + h + laid.margin.bottom;
    let extra = (outer - border.top).max(0.0);
    fragment.rect.x = border.left + padding.left + x;
    fragment.rect.y = (border.top - outer).max(0.0) / 2.0 + laid.margin.top;
    PlacedLegend {
        gap: Rect::new(fragment.rect.x, extra / 2.0, w, border.top),
        fragment,
        extra,
    }
}

/// Places an outside marker beside the first line of a list item, outside its content box at
/// the inline start, on the line's baseline (CSS Lists 3 § 3.1). The marker joins the line,
/// so it stays with it across pages. Without a line it goes at the top of the content box.
fn place_marker(
    marker: &BlockBox,
    children: &mut Vec<Fragment>,
    content_x: f32,
    content_y: f32,
    width: f32,
    rtl: bool,
) {
    let mut fragment = layout_atomic(marker, width).fragment;
    let x = if rtl {
        content_x + width
    } else {
        content_x - fragment.rect.width
    };
    match first_line(children, 0.0) {
        Some((line, dx)) => {
            let baseline = fragment.first_baseline().unwrap_or(fragment.rect.height);
            fragment.rect.x = x - dx - line.rect.x;
            fragment.rect.y = line.baseline - baseline;
            // Line items are in visual order.
            if rtl {
                line.items.push(LineItem::Box(fragment));
            } else {
                line.items.insert(0, LineItem::Box(fragment));
            }
        }
        None => {
            fragment.rect.x = x;
            fragment.rect.y = content_y;
            children.push(Fragment::Box(fragment));
        }
    }
}

/// The first line box among fragments and their descendants, with the horizontal offset of
/// the box that contains it.
fn first_line(children: &mut [Fragment], dx: f32) -> Option<(&mut LineFragment, f32)> {
    for child in children {
        match child {
            Fragment::Line(line) => return Some((line, dx)),
            Fragment::Box(child) => {
                let x = dx + child.rect.x;
                if let Some(found) = first_line(&mut child.children, x) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// The content-box size of a replaced element: its natural size and aspect ratio, fitted to
/// `width`, `height` and their limits (CSS 2.1 § 10.3.2, § 10.4, § 10.6.2, § 10.7).
fn replaced_size(block: &BlockBox, replaced: &Replaced, cb: ContainingBlock) -> (f32, f32) {
    let style = &block.style;
    let (padding, border) = padding_and_border(style, cb.width);
    let extra_w = padding.horizontal() + border.horizontal();
    let extra_h = padding.vertical() + border.vertical();
    let border_box = style.box_sizing == BoxSizing::BorderBox;
    let width_of = |size: &Size<LengthPercentage>| match size {
        Size::Value(v) => {
            let w = resolve(v, cb.width);
            Some(if border_box {
                (w - extra_w).max(0.0)
            } else {
                w
            })
        }
        _ => None,
    };
    let height_of = |size: &Size<LengthPercentage>| match size {
        Size::Value(v) => resolve_definite(v, cb.height).map(|h| {
            if border_box {
                (h - extra_h).max(0.0)
            } else {
                h
            }
        }),
        _ => None,
    };
    // `object-view-box` makes the part it shows the natural size (CSS Images 4 § 7.3).
    let natural = replaced
        .size
        .map(|(w, h)| match style.object_view_box.resolve(w, h) {
            Some((_, _, vw, vh)) if vw > 0.0 && vh > 0.0 => (vw, vh),
            _ => (w, h),
        });
    let natural_ratio = natural
        .filter(|&(w, h)| w > 0.0 && h > 0.0)
        .map(|(w, h)| w / h);
    let given = style.aspect_ratio.value();
    let ratio = if style.aspect_ratio.auto {
        natural_ratio.or(given)
    } else {
        given.or(natural_ratio)
    };
    let (width, height) = (width_of(&style.width), height_of(&style.height));
    let (mut w, mut h) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, ratio.map_or(natural.map_or(150.0, |n| n.1), |r| w / r)),
        (None, Some(h)) => (ratio.map_or(natural.map_or(300.0, |n| n.0), |r| h * r), h),
        (None, None) => match (natural, ratio) {
            (Some(n), _) => n,
            (None, Some(r)) => (300.0, 300.0 / r),
            (None, None) => (300.0, 150.0),
        },
    };
    let min_w = width_of(&style.min_width).unwrap_or(0.0);
    let max_w = width_of(&style.max_width)
        .unwrap_or(f32::INFINITY)
        .max(min_w);
    let min_h = height_of(&style.min_height).unwrap_or(0.0);
    let max_h = height_of(&style.max_height)
        .unwrap_or(f32::INFINITY)
        .max(min_h);
    let clamp = |v: f32, min: f32, max: f32| v.min(max).max(min);
    match (ratio, width, height) {
        (Some(r), None, Some(_)) => {
            h = clamp(h, min_h, max_h);
            w = clamp(h * r, min_w, max_w);
        }
        (Some(r), Some(_), None) => {
            w = clamp(w, min_w, max_w);
            h = clamp(w / r, min_h, max_h);
        }
        // Both auto: the limits keep the ratio where they can (the table in § 10.4).
        (Some(r), None, None) if w > 0.0 && h > 0.0 => {
            let (w0, h0) = (w, h);
            (w, h) = if w0 > max_w && h0 > max_h {
                if max_w / w0 <= max_h / h0 {
                    (max_w, (max_w / r).max(min_h))
                } else {
                    ((max_h * r).max(min_w), max_h)
                }
            } else if w0 < min_w && h0 < min_h {
                if min_w / w0 <= min_h / h0 {
                    ((min_h * r).min(max_w), min_h)
                } else {
                    (min_w, (min_w / r).min(max_h))
                }
            } else if w0 < min_w && h0 > max_h {
                (min_w, max_h)
            } else if w0 > max_w && h0 < min_h {
                (max_w, min_h)
            } else if w0 > max_w {
                (max_w, (max_w / r).max(min_h))
            } else if w0 < min_w {
                (min_w, (min_w / r).min(max_h))
            } else if h0 > max_h {
                ((max_h * r).max(min_w), max_h)
            } else if h0 < min_h {
                ((min_h * r).min(max_w), min_h)
            } else {
                (w0, h0)
            };
        }
        _ => {
            w = clamp(w, min_w, max_w);
            h = clamp(h, min_h, max_h);
        }
    }
    (w, h)
}

/// Lays out a replaced element, such as an image. It does not break across pages.
fn layout_replaced(
    block: &BlockBox,
    replaced: &Replaced,
    cb: ContainingBlock,
    kind: BlockKind,
    fragmentainer: Option<Fragmentainer>,
) -> Laid {
    let style = &block.style;
    let (padding, border) = padding_and_border(style, cb.width);
    let extra = padding.horizontal() + border.horizontal();
    let (mut width, mut height) = replaced_size(block, replaced, cb);
    // A flex item takes the width its container gives it, and a height to match unless it
    // has one too.
    if let BlockKind::Item {
        width: given,
        height: given_height,
        ..
    } = kind
    {
        height = given_height.unwrap_or(if width > 0.0 {
            height * given / width
        } else {
            height
        });
        width = given;
    }
    let mut replaced = replaced.clone();
    if let Some(Picture::Frame(frame)) = &mut replaced.picture {
        // `frame-sizing` sizes an auto dimension to the framed document's content.
        let (content_width, content_height) = match style.frame_sizing {
            FrameSizing::ContentWidth | FrameSizing::ContentInlineSize => (true, false),
            FrameSizing::ContentHeight | FrameSizing::ContentBlockSize => (false, true),
            FrameSizing::Auto => (false, false),
        };
        if content_width && matches!(style.width, Size::Auto) {
            width = intrinsic_outer(&frame.root).1;
        }
        let laid = layout_root(&frame.root, width, height, None, None).0;
        if content_height && matches!(style.height, Size::Auto) {
            height = laid.rect.bottom().max(0.0);
        }
        frame.laid = Some(Arc::new(laid));
    }
    let mut margin_left = margin(&style.margin_left, cb.width);
    let mut margin_right = margin(&style.margin_right, cb.width);
    let margin_top = margin(&style.margin_top, cb.width).unwrap_or(0.0);
    let margin_bottom = margin(&style.margin_bottom, cb.width).unwrap_or(0.0);
    if !matches!(kind, BlockKind::Atomic | BlockKind::Item { .. }) {
        // Block-level replaced elements center with auto margins, like other blocks.
        let remaining = cb.width - width - extra;
        match (margin_left, margin_right) {
            (None, None) => {
                margin_left = Some((remaining / 2.0).max(0.0));
                margin_right = Some((remaining / 2.0).max(0.0));
            }
            (None, Some(right)) => margin_left = Some(remaining - right),
            (Some(left), None) => margin_right = Some(remaining - left),
            _ => {}
        }
    }
    let border_box_height = height + padding.vertical() + border.vertical();
    if let Some(f) = fragmentainer
        && !f.at_top
        && border_box_height > f.available + EPSILON
    {
        return deferred(block, border, padding, width, extra);
    }
    Laid {
        fragment: BoxFragment {
            node: block.node,
            style: style.clone(),
            rect: Rect::new(0.0, 0.0, width + extra, border_box_height),
            border,
            padding,
            children: Vec::new(),
            replaced: Some(replaced),
            legend: None,
            out_of_flow: None,
        },
        margin: Sides {
            top: margin_top,
            right: margin_right.unwrap_or(0.0),
            bottom: margin_bottom,
            left: margin_left.unwrap_or(0.0),
        },
        collapsed_top: CollapsibleMargin::of(margin_top),
        collapsed_bottom: CollapsibleMargin::of(margin_bottom),
        through: false,
        break_token: None,
        deferred: false,
    }
}

/// Lays out a float and places it beside the floats before it, no higher than `top`, in a
/// containing block between `edges`, in formatting-context coordinates. Returns its
/// fragment, positioned from the border box at `origin`.
fn place_float(
    float: &BlockBox,
    cb: ContainingBlock,
    floats: &mut Floats,
    origin: Point,
    float_left: bool,
    top: f32,
    (left, right): (f32, f32),
) -> BoxFragment {
    // Text decorations do not propagate into floats.
    let laid = layout_block(float, cb, BlockKind::Atomic, &[], None, None);
    let m = laid.margin;
    let mut fragment = laid.fragment;
    let (clear_left, clear_right) = float.style.clear.sides(cb.rtl);
    let top = floats
        .clearance(clear_left, clear_right)
        .map_or(top, |below| top.max(below));
    let rect = floats.place(
        fragment.rect.width + m.horizontal(),
        fragment.rect.height + m.vertical(),
        float_left,
        top,
        left,
        right,
    );
    let border_box = Rect::new(
        rect.x + m.left,
        rect.y + m.top,
        fragment.rect.width,
        fragment.rect.height,
    );
    if let Some(shape) = crate::shapes::float_shape(float, &fragment, border_box, m) {
        floats.shape_last(shape);
    }
    let (dx, dy) = crate::position::relative_offset(&float.style, cb);
    fragment.rect.x = rect.x - origin.x + m.left + dx;
    fragment.rect.y = rect.y - origin.y + m.top + dy;
    fragment
}

/// The `index`th float in lines, and the top of its line.
fn line_float(lines: &[LineFragment], index: usize) -> Option<(Arc<BlockBox>, f32)> {
    fn find(items: &[LineItem], found: &mut Vec<Arc<BlockBox>>) {
        for item in items {
            match item {
                LineItem::Box(fragment) => {
                    if let Some(OutOfFlow::Float(float)) = &fragment.out_of_flow {
                        found.push(float.clone());
                    }
                }
                LineItem::Inline(inline) => find(&inline.children, found),
                LineItem::Text(_) => {}
            }
        }
    }
    let mut seen = 0;
    for line in lines {
        let mut found = Vec::new();
        find(&line.items, &mut found);
        if index < seen + found.len() {
            return Some((found[index - seen].clone(), line.rect.y));
        }
        seen += found.len();
    }
    None
}

/// The placeholder of an out-of-flow box: empty, with the box for its containing block to
/// lay out. Margins collapse through it.
fn placeholder(block: &BlockBox, out_of_flow: OutOfFlow) -> Laid {
    Laid {
        fragment: BoxFragment {
            node: None,
            style: block.style.clone(),
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            border: Sides::default(),
            padding: Sides::default(),
            children: Vec::new(),
            replaced: None,
            legend: None,
            out_of_flow: Some(out_of_flow),
        },
        margin: Sides::default(),
        collapsed_top: CollapsibleMargin::default(),
        collapsed_bottom: CollapsibleMargin::default(),
        through: true,
        break_token: None,
        deferred: false,
    }
}

/// A box that moves to the next page whole: an empty placeholder its parent breaks before.
pub(crate) fn deferred(
    block: &BlockBox,
    border: Sides,
    padding: Sides,
    width: f32,
    extra: f32,
) -> Laid {
    Laid {
        fragment: BoxFragment {
            node: block.node,
            style: block.style.clone(),
            rect: Rect::new(
                0.0,
                0.0,
                width + extra,
                border.vertical() + padding.vertical(),
            ),
            border,
            padding,
            children: Vec::new(),
            replaced: None,
            legend: None,
            out_of_flow: None,
        },
        margin: Sides::default(),
        collapsed_top: CollapsibleMargin::default(),
        collapsed_bottom: CollapsibleMargin::default(),
        through: false,
        break_token: None,
        deferred: true,
    }
}

/// Minimum and maximum content widths of a box's content box.
pub(crate) fn intrinsic_content(block: &BlockBox) -> (f32, f32) {
    let (min, max) = intrinsic_contents(block);
    match &block.legend {
        Some(legend) => {
            let (legend_min, legend_max) = intrinsic_outer(legend);
            (min.max(legend_min), max.max(legend_max))
        }
        None => (min, max),
    }
}

fn intrinsic_contents(block: &BlockBox) -> (f32, f32) {
    // Size containment sizes the box as if it were empty.
    if let Some(width) = block.style.contained_width() {
        return (width, width);
    }
    if let Some(replaced) = &block.replaced {
        let cb = ContainingBlock {
            width: 0.0,
            height: None,
            rtl: false,
        };
        let (width, _) = replaced_size(block, replaced, cb);
        return (width, width);
    }
    match &block.contents {
        BlockContents::Inline(items) => inline::intrinsic(items, &block.style),
        BlockContents::Table(table) => crate::table::intrinsic(table),
        BlockContents::Flex(items) => crate::flex::intrinsic(block, items),
        BlockContents::Grid(items) => crate::grid::intrinsic(block, items),
        // Out-of-flow boxes take no space in flow, and floats in lines count with the line.
        BlockContents::OutOfFlow(_) | BlockContents::Float(_) => (0.0, 0.0),
        BlockContents::Turned { inner, outer } => {
            crate::writing::intrinsic(&block.style, inner, *outer)
        }
        BlockContents::Ruby(ruby) => crate::ruby::intrinsic(block, ruby),
        BlockContents::Blocks(segments) if crate::multicol::is_multicol(&block.style) => {
            crate::multicol::intrinsic(block, segments)
        }
        BlockContents::Blocks(blocks) => blocks
            .iter()
            .map(intrinsic_outer)
            .fold((0.0, 0.0), |(a, b), (c, d)| (a.max(c), b.max(d))),
    }
}

/// The max-content width of a box's content box.
pub(crate) fn intrinsic_content_width(block: &BlockBox) -> f32 {
    intrinsic_content(block).1
}

/// Minimum and maximum widths of a box's margin box, ignoring percentages.
pub(crate) fn intrinsic_outer(block: &BlockBox) -> (f32, f32) {
    let style = &block.style;
    let (padding, border) = padding_and_border(style, 0.0);
    let fixed = |m: &MaybeAuto<LengthPercentage>| match m {
        MaybeAuto::Value(LengthPercentage::Length(px)) => *px,
        _ => 0.0,
    };
    let extra = padding.horizontal()
        + border.horizontal()
        + fixed(&style.margin_left)
        + fixed(&style.margin_right);
    let (min, max) = match &style.width {
        Size::Value(LengthPercentage::Length(px)) => {
            let content = if style.box_sizing == BoxSizing::BorderBox {
                (px - padding.horizontal() - border.horizontal()).max(0.0)
            } else {
                *px
            };
            (content, content)
        }
        _ => intrinsic_content(block),
    };
    (min + extra, max + extra)
}
