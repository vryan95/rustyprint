//! Vertical writing modes (CSS Writing Modes 4).
//!
//! Layout always runs in line-relative axes, where lines run left to right and stack from top
//! to bottom. Boxes in a vertical writing mode get styles turned into those axes
//! ([`ComputedValues::in_line_axes`]) and are laid out like horizontal text; their fragments
//! are turned into their parent's axes afterwards, and the root's into the page's.
//!
//! A box whose writing mode turns its axes differently from its parent's establishes an
//! independent formatting context. Its style is turned into its parent's axes, so its parent
//! sizes and places it, and its content moves into an anonymous inner box styled in its own
//! axes ([`BlockContents::Turned`]).

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

use rustyprint_css::properties::types::{
    BoxSizing, Direction, Display, DisplayInside, DisplayInternal, DisplayOutside,
    LengthPercentage, Position, Size as SizeValue, WritingMode,
};
use rustyprint_css::{ComputedValues, Styles};
use rustyprint_dom::{Document, NodeId, local_name};

use crate::boxes::{BlockBox, BlockContents, InlineItem};
use crate::display::{Rect, Size};
use crate::flow::{self, BlockKind, ContainingBlock};
use crate::fragment::{BoxFragment, Fragment, LineFragment, LineItem, Turn};

/// The turn from the line-relative axes of `writing_mode` to physical axes.
pub(crate) fn axes(writing_mode: WritingMode) -> Turn {
    match writing_mode {
        WritingMode::HorizontalTb => Turn::IDENTITY,
        WritingMode::VerticalRl | WritingMode::SidewaysRl => Turn::CLOCKWISE,
        WritingMode::VerticalLr => Turn::TRANSPOSE,
        WritingMode::SidewaysLr => Turn::COUNTER_CLOCKWISE,
    }
}

/// The turn of the content of a line of `writing_mode`, from the axes it was laid out in to
/// the line-relative axes its boxes are in. Lines are turned rather than reflected, so in
/// `vertical-lr`, where lines stack left to right, the over side of each line is still on the
/// right.
fn line_content(writing_mode: WritingMode) -> Turn {
    let physical = match writing_mode {
        WritingMode::HorizontalTb => Turn::IDENTITY,
        WritingMode::SidewaysLr => Turn::COUNTER_CLOCKWISE,
        _ => Turn::CLOCKWISE,
    };
    physical.then(axes(writing_mode).inverse())
}

/// Whether the physical right side of a line of vertical text in `writing_mode` is its over
/// side, as it is everywhere but in `sideways-lr`. Underlines and emphasis marks placed
/// `left` or `right` go on the under or over side accordingly.
pub(crate) fn right_is_over(writing_mode: WritingMode) -> bool {
    writing_mode != WritingMode::SidewaysLr
}

/// Whether text in `writing_mode` placed on the physical `left` (or else right) side goes
/// on the under side of its line. `None` in horizontal text, where those sides do not apply.
pub(crate) fn left_is_under(writing_mode: WritingMode, left: bool) -> Option<bool> {
    writing_mode
        .is_vertical()
        .then(|| left == right_is_over(writing_mode))
}

/// Whether the content of lines in `writing_mode` is flipped across them relative to the
/// boxes around: in `vertical-lr`, lines stack left to right but their over side is on the
/// right. Atomic inlines on such lines stack their content the other way from the line's.
pub(crate) fn flips_lines(writing_mode: WritingMode) -> bool {
    line_content(writing_mode) != Turn::IDENTITY
}

/// The turn from the axes of a box in `inner` writing mode to those of its parent in `outer`.
fn relative(outer: WritingMode, inner: WritingMode) -> Turn {
    axes(inner).then(axes(outer).inverse())
}

thread_local! {
    /// The root's writing mode and the page area in its axes, which orthogonal flows fall
    /// back to when their parent's size along their inline axis is not definite.
    static VIEWPORT: Cell<(WritingMode, Size)> =
        const { Cell::new((WritingMode::HorizontalTb, Size::new(0.0, 0.0))) };
}

/// Records the page area that the root is laid out in, in its axes.
pub(crate) fn set_viewport(root: WritingMode, area: Size) {
    VIEWPORT.with(|v| v.set((root, area)));
}

/// The extent of the page area along the inline axis of `writing_mode`.
fn viewport_inline(writing_mode: WritingMode) -> f32 {
    let (root, area) = VIEWPORT.with(Cell::get);
    if relative(root, writing_mode).swaps_axes() {
        area.height
    } else {
        area.width
    }
}

// ---------------------------------------------------------------------------------------------
// Box construction

/// Styles turned into the axes each box is laid out in, and the elements whose content is in
/// other axes than their parent's.
pub(crate) struct Turned {
    pub styles: Styles,
    /// Each such element: its computed style, with its own writing mode, and the writing
    /// mode of the axes its parent lays it out in.
    pub boundaries: HashMap<NodeId, (Arc<ComputedValues>, WritingMode)>,
}

/// Whether a box with `display` passes its parent's axes to its content: inline boxes and
/// table-internal boxes other than cells use the writing mode of the box around them.
fn passes_through(display: Display) -> bool {
    match display {
        Display::None | Display::Contents => true,
        Display::Normal {
            outside: DisplayOutside::Inline,
            inside: DisplayInside::Flow | DisplayInside::Ruby,
            ..
        } => true,
        // Tables keep the axes of the box around them.
        Display::Normal {
            inside: DisplayInside::Table,
            ..
        } => true,
        Display::Internal(DisplayInternal::TableCell) => false,
        Display::Internal(_) => true,
        Display::Normal { .. } => false,
    }
}

/// Turns the styles of a document into the axes its boxes are laid out in, or `None` if
/// nothing in it is vertical.
pub(crate) fn turn_styles(document: &Document, styles: &Styles) -> Option<Turned> {
    let root = document.document_element()?;
    let vertical = document
        .descendants(root)
        .filter_map(|node| styles.get(node))
        .any(|style| style.writing_mode.is_vertical());
    if !vertical {
        return None;
    }
    // The principal writing mode comes from `<body>` when there is one (CSS Writing Modes 4
    // § 8).
    let body = document.element_children(root).find(|&child| {
        document
            .element(child)
            .is_some_and(|e| e.is_html(&local_name!("body")))
    });
    let principal = body
        .or(Some(root))
        .and_then(|node| styles.get(node))
        .map_or(WritingMode::HorizontalTb, |style| style.writing_mode);

    // The axes each element's box is in, and those its content is in.
    let mut spaces: HashMap<NodeId, (WritingMode, WritingMode, bool)> = HashMap::new();
    let mut boundaries = HashMap::new();
    let mut stack = vec![(root, principal)];
    while let Some((node, space)) = stack.pop() {
        let Some(style) = styles.get(node) else {
            continue;
        };
        let own = if node == root {
            principal
        } else {
            style.writing_mode
        };
        let through = node == root || passes_through(style.display);
        let boundary = !through && axes(own) != axes(space);
        if boundary {
            boundaries.insert(node, (style.clone(), space));
        }
        let inner = if through { space } else { own };
        spaces.insert(node, (space, inner, through));
        for child in document.element_children(node) {
            stack.push((child, inner));
        }
    }

    let turned = styles.map(|node, pseudo, style| {
        let Some(&(space, inner, through)) = spaces.get(&node) else {
            return style.clone();
        };
        // Pseudo-elements are in their element's content, and boxes whose writing mode
        // does not apply use the one around them.
        let (space, through) = match pseudo {
            Some(_) => (inner, axes(style.writing_mode) != axes(inner)),
            None => (space, through),
        };
        if through && style.writing_mode != space {
            let mut adjusted = (**style).clone();
            adjusted.writing_mode = space;
            Arc::new(adjusted).in_line_axes(space)
        } else {
            style.in_line_axes(space)
        }
    });
    Some(Turned {
        styles: turned,
        boundaries,
    })
}

/// Moves the content of each box in another writing mode than its parent into an inner box
/// laid out in its own axes.
pub(crate) fn wrap_boundaries(block: &mut BlockBox, turned: &Turned) {
    wrap_children(block, turned);
    let Some(node) = block.node else {
        return;
    };
    let Some((computed, outer)) = turned.boundaries.get(&node) else {
        return;
    };
    if block.replaced.is_some()
        || matches!(
            block.contents,
            BlockContents::OutOfFlow(_) | BlockContents::Float(_) | BlockContents::Table(_)
        )
    {
        return;
    }
    let (computed, outer) = (computed.clone(), *outer);
    let inner = BlockBox {
        node: None,
        style: inner_style(&computed).in_line_axes(computed.writing_mode),
        contents: std::mem::replace(&mut block.contents, BlockContents::Blocks(Vec::new())),
        page: block.page.clone(),
        marker: block.marker.take(),
        first_line: std::mem::take(&mut block.first_line),
        legend: block.legend.take(),
        center_contents: block.center_contents,
        shape_image: None,
        replaced: None,
    };
    block.contents = BlockContents::Turned {
        inner: Box::new(inner),
        outer,
    };
}

fn wrap_children(block: &mut BlockBox, turned: &Turned) {
    for nested in [&mut block.marker, &mut block.legend].into_iter().flatten() {
        wrap_boundaries(nested, turned);
    }
    match &mut block.contents {
        BlockContents::Blocks(children)
        | BlockContents::Flex(children)
        | BlockContents::Grid(children) => {
            for child in children {
                wrap_boundaries(child, turned);
            }
        }
        BlockContents::Inline(items) => {
            for item in items {
                if let InlineItem::Atomic(atomic) = item {
                    wrap_boundaries(atomic, turned);
                }
            }
        }
        BlockContents::Table(table) => {
            for caption in &mut table.captions {
                wrap_boundaries(caption, turned);
            }
            for group in &mut table.groups {
                for row in &mut group.rows {
                    for cell in &mut row.cells {
                        wrap_boundaries(&mut cell.block, turned);
                    }
                }
            }
        }
        BlockContents::OutOfFlow(positioned) => {
            wrap_boundaries(&mut Arc::make_mut(positioned).block, turned);
        }
        BlockContents::Float(float) => wrap_boundaries(Arc::make_mut(float), turned),
        BlockContents::Turned { inner, .. } => wrap_boundaries(inner, turned),
        BlockContents::Ruby(ruby) => {
            wrap_boundaries(&mut ruby.base, turned);
            for annotation in &mut ruby.annotations {
                wrap_boundaries(&mut annotation.block, turned);
            }
        }
    }
}

/// The style of the inner box of an element in another writing mode than its parent: the
/// element's inherited properties and those that lay out its content. Its outer box keeps
/// the rest, such as sizes, margins, borders and backgrounds.
fn inner_style(element: &ComputedValues) -> Arc<ComputedValues> {
    let mut style = ComputedValues::inherit_from(element);
    style.display = Display::Normal {
        outside: DisplayOutside::Block,
        inside: match element.display {
            Display::Normal {
                inside: inside @ (DisplayInside::Flex | DisplayInside::Grid),
                ..
            } => inside,
            _ => DisplayInside::FlowRoot,
        },
        list_item: false,
    };
    style.position = Position::Static;
    macro_rules! copy {
        ($($field:ident),+ $(,)?) => {$(style.$field = element.$field.clone();)+};
    }
    copy!(
        text_box_trim,
        line_clamp,
        max_lines,
        text_overflow,
        text_fit,
        margin_trim,
        align_content,
        justify_content,
        align_items,
        justify_items,
        row_gap,
        column_gap,
        flex_direction,
        flex_wrap,
        flex_line_count,
        reading_flow,
        grid_template_columns,
        grid_template_rows,
        grid_template_areas,
        grid_auto_columns,
        grid_auto_rows,
        grid_auto_flow,
        column_count,
        column_width,
        column_height,
        column_fill,
        column_wrap,
        column_rule_width,
        column_rule_style,
        column_rule_color,
    );
    Arc::new(style)
}

// ---------------------------------------------------------------------------------------------
// Layout

/// What the parent of a box in another writing mode gives it, in the parent's axes.
pub(crate) struct Space {
    /// The content width.
    pub width: f32,
    /// The content height, if definite.
    pub height: Option<f32>,
    pub min_height: f32,
    pub max_height: Option<f32>,
    /// The height of the containing block, if definite.
    pub available_height: Option<f32>,
}

/// Lays out `inner`, the content of a box in another writing mode than its parent's
/// `outer`, in its own axes, and turns it into the parent's. The fragment is at the origin.
///
/// Along the parent's inline axis the box is as wide as its parent makes it. When the box's
/// inline axis is the parent's block axis, its inline size is its height if that is definite,
/// or else fits its content into the containing block's height, or the page's (CSS Writing
/// Modes 4 § 7.3).
pub(crate) fn layout(
    inner: &BlockBox,
    outer: WritingMode,
    space: Space,
    decorators: &[Arc<ComputedValues>],
) -> BoxFragment {
    let turn = relative(outer, inner.style.writing_mode);
    let (inline, block) = if turn.swaps_axes() {
        let inline = space.height.unwrap_or_else(|| {
            let available = space
                .available_height
                .unwrap_or_else(|| viewport_inline(inner.style.writing_mode));
            let (min, max) = flow::intrinsic_content(inner);
            available
                .min(max)
                .max(min)
                .min(space.max_height.unwrap_or(f32::INFINITY))
                .max(space.min_height)
        });
        (inline, Some(space.width))
    } else {
        (space.width, space.height)
    };
    let mut fragment = lay(inner, inline, block, decorators);
    let size = Size::new(fragment.rect.width, fragment.rect.height);
    convert_box(&mut fragment, turn, size);
    fragment
}

/// Lays out an inner box at an inline size, and a block size if definite, and places the
/// out-of-flow boxes inside it.
fn lay(
    inner: &BlockBox,
    inline: f32,
    block: Option<f32>,
    decorators: &[Arc<ComputedValues>],
) -> BoxFragment {
    let cb = ContainingBlock {
        width: inline,
        height: block,
        rtl: inner.style.direction == Direction::Rtl,
    };
    let kind = BlockKind::Item {
        width: inline,
        height: block,
        at_least: false,
    };
    let mut fragment = flow::layout_block(inner, cb, kind, decorators, None, None).fragment;
    // Out-of-flow boxes do not leave the axes they are laid out in.
    let area = Rect::new(0.0, 0.0, fragment.rect.width, fragment.rect.height);
    crate::position::place_out_of_flow(&mut fragment, area, crate::position::Takes::All);
    fragment
}

/// The minimum and maximum content widths of a box in `style` whose content is `inner`, in
/// its parent's `outer` writing mode. Across the inner box's lines, both are the inner box's
/// height at the inline size the box's own height gives it, or that fits it into the page.
pub(crate) fn intrinsic(
    style: &ComputedValues,
    inner: &BlockBox,
    outer: WritingMode,
) -> (f32, f32) {
    let turn = relative(outer, inner.style.writing_mode);
    if !turn.swaps_axes() {
        return flow::intrinsic_content(inner);
    }
    // Percentages of the containing block's height are not known here.
    let extra = match style.box_sizing {
        BoxSizing::BorderBox => {
            let (padding, border) = flow::padding_and_border(style, 0.0);
            padding.vertical() + border.vertical()
        }
        BoxSizing::ContentBox => 0.0,
    };
    let length = |size: &SizeValue<LengthPercentage>| match size {
        SizeValue::Value(LengthPercentage::Length(px)) => Some((px - extra).max(0.0)),
        _ => None,
    };
    let inline = length(&style.height).unwrap_or_else(|| {
        let (min, max) = flow::intrinsic_content(inner);
        viewport_inline(inner.style.writing_mode)
            .min(max)
            .max(min)
            .min(length(&style.max_height).unwrap_or(f32::INFINITY))
            .max(length(&style.min_height).unwrap_or(0.0))
    });
    let height = lay(inner, inline, None, &[]).rect.height;
    (height, height)
}

// ---------------------------------------------------------------------------------------------
// Turning fragments

/// Turns a box laid out in other axes, and everything in it, by `turn`. Its position is
/// relative to a container of `container` in those axes, which turns in place. Boxes take
/// back the styles they had before [`ComputedValues::in_line_axes`].
pub(crate) fn convert_box(fragment: &mut BoxFragment, turn: Turn, container: Size) {
    let own = Size::new(fragment.rect.width, fragment.rect.height);
    let writing_mode = fragment.style.writing_mode;
    for child in &mut fragment.children {
        match child {
            Fragment::Box(child) => convert_box(child, turn, own),
            Fragment::Line(line) => convert_line(line, turn, own, writing_mode),
        }
    }
    if let Some(legend) = &mut fragment.legend {
        *legend = turn.rect(*legend, own);
    }
    fragment.rect = turn.rect(fragment.rect, container);
    fragment.border = turn.sides(fragment.border);
    fragment.padding = turn.sides(fragment.padding);
    if let Some(style) = fragment.style.unrotated.clone() {
        fragment.style = style;
    }
}

/// Turns a line of a box in `writing_mode`. Its text and inline boxes stay in the axes they
/// were laid out in; its atomic inlines turn.
fn convert_line(line: &mut LineFragment, turn: Turn, container: Size, writing_mode: WritingMode) {
    let size = Size::new(line.rect.width, line.rect.height);
    let native = line.turn.is_none();
    let content = line
        .turn
        .unwrap_or_else(|| line_content(writing_mode))
        .then(turn);
    convert_items(&mut line.items, native, content, turn, size);
    line.rect = turn.rect(line.rect, container);
    line.turn = Some(content);
}

/// Turns the atomic inlines among line items. Those of a line that has not turned before
/// are where the line's content puts them; their content turns like the boxes around.
fn convert_items(items: &mut [LineItem], native: bool, content: Turn, turn: Turn, line: Size) {
    for item in items {
        match item {
            LineItem::Text(_) => {}
            LineItem::Box(atomic) => {
                let rect = atomic.rect;
                convert_box(atomic, turn, line);
                if native {
                    atomic.rect = content.rect(rect, line);
                }
            }
            LineItem::Inline(inline) => {
                convert_items(&mut inline.children, native, content, turn, line);
            }
        }
    }
}

/// Turns the root's fragment from the axes of its writing mode into the page's. `area` is
/// the page area in the root's axes.
pub(crate) fn root_to_page(fragment: &mut BoxFragment, area: Size) {
    let turn = axes(fragment.style.writing_mode);
    if turn != Turn::IDENTITY {
        convert_box(fragment, turn, area);
    }
}

/// The page area in the axes of the root's writing mode.
pub(crate) fn page_area(root: &BlockBox, area: Size) -> Size {
    axes(root.style.writing_mode).inverse().size(area)
}

/// Builds the inner boxes of a box tree, if any of its boxes is in another writing mode.
pub(crate) fn wrap_tree(root: &mut BlockBox, turned: Option<&Turned>) {
    if let Some(turned) = turned
        && !turned.boundaries.is_empty()
    {
        wrap_boundaries(root, turned);
    }
}
