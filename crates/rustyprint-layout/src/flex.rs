//! Flex layout (CSS Flexible Box Layout 1 § 9), with balanced wrapping (CSS Flexbox 2
//! § 5.2) and breaking across pages (§ 12) by line or, in columns, by item (see
//! [`crate::units`]).

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    AlignPosition, BoxSizing, ContentAlignment, ContentDistribution, Direction, FlexBasis,
    ItemAlignment, LengthPercentage, MaybeAuto, Overflow, OverflowPosition, Size, Wrap,
};

use crate::boxes::{BlockBox, BlockContents};
use crate::flow::{self, BlockKind, BreakToken, ContainingBlock, EPSILON, Fragmentainer, Laid};
use crate::fragment::{Fragment, Sides};
use crate::units::{ContentLaid, ContentSpace, Placed, Unit, paginate};

/// The directions of a container's axes. The main axis runs along rows or columns; each
/// axis is reversed when its start is the right or bottom edge.
#[derive(Debug, Clone, Copy)]
struct Axes {
    column: bool,
    main_reverse: bool,
    cross_reverse: bool,
    /// Whether `flex-direction` is reversed, which puts main-start at the writing mode's end.
    direction_reverse: bool,
    wrap_reverse: bool,
}

impl Axes {
    fn of(style: &ComputedValues) -> Self {
        let rtl = style.direction == Direction::Rtl;
        let column = style.flex_direction.is_column();
        let direction_reverse = style.flex_direction.is_reverse();
        let wrap_reverse = style.flex_wrap.wrap == Wrap::WrapReverse;
        Axes {
            column,
            main_reverse: if column {
                direction_reverse
            } else {
                direction_reverse != rtl
            },
            cross_reverse: if column {
                rtl != wrap_reverse
            } else {
                wrap_reverse
            },
            direction_reverse,
            wrap_reverse,
        }
    }
}

/// An in-flow child of the container, as the algorithm sizes and places it.
struct Item<'a> {
    block: &'a BlockBox,
    style: &'a ComputedValues,
    padding: Sides,
    border: Sides,
    /// Margins on the main-start, main-end, cross-start and cross-end sides; `None` when
    /// `auto`.
    margin: [Option<f32>; 4],
    grow: f32,
    shrink: f32,
    /// Flex base size and hypothetical main size, as content-box sizes.
    base: f32,
    hypothetical: f32,
    min_main: f32,
    max_main: f32,
    min_cross: f32,
    max_cross: f32,
    /// The content-box main size, once flexed.
    main: f32,
    frozen: bool,
    /// The content-box cross size.
    cross: f32,
    /// Whether the cross size comes from the line, by stretching.
    stretched: bool,
    align: ItemAlignment,
    laid: Option<Laid>,
    /// Border-box offsets from the main-start and cross-start edges of the content box.
    main_pos: f32,
    cross_pos: f32,
    /// How much a fieldset's rendered legend makes its top border area taller.
    legend: f32,
}

impl Item<'_> {
    /// Padding, borders and legend along the main axis.
    fn main_pb(&self, axes: Axes) -> f32 {
        if axes.column {
            self.vertical_pb()
        } else {
            self.padding.horizontal() + self.border.horizontal()
        }
    }

    fn cross_pb(&self, axes: Axes) -> f32 {
        if axes.column {
            self.padding.horizontal() + self.border.horizontal()
        } else {
            self.vertical_pb()
        }
    }

    fn vertical_pb(&self) -> f32 {
        self.padding.vertical() + self.border.vertical() + self.legend
    }

    /// Notes what a layout of the item found out about its legend.
    fn laid_out(&mut self, laid: Laid) {
        self.legend = laid.fragment.legend.map_or(0.0, |gap| gap.y * 2.0);
        self.laid = Some(laid);
    }

    /// The main-axis margins with `auto` as zero.
    fn main_margins(&self) -> f32 {
        self.margin[0].unwrap_or(0.0) + self.margin[1].unwrap_or(0.0)
    }

    fn cross_margins(&self) -> f32 {
        self.margin[2].unwrap_or(0.0) + self.margin[3].unwrap_or(0.0)
    }

    fn outer_main(&self, content: f32, axes: Axes) -> f32 {
        content + self.main_pb(axes) + self.main_margins()
    }

    fn outer_cross(&self, axes: Axes) -> f32 {
        self.cross + self.cross_pb(axes) + self.cross_margins()
    }

    /// The content-box width and height.
    fn size(&self, axes: Axes) -> (f32, f32) {
        if axes.column {
            (self.cross, self.main)
        } else {
            (self.main, self.cross)
        }
    }

    fn has_auto_cross_margin(&self) -> bool {
        self.margin[2].is_none() || self.margin[3].is_none()
    }
}

/// A content-box height from a sizing property, against a definite containing block height
/// if it has a percentage.
fn height_value(
    style: &ComputedValues,
    size: &Size<LengthPercentage>,
    basis: Option<f32>,
    pb: f32,
) -> Option<f32> {
    let Size::Value(value) = size else {
        return None;
    };
    let height = match (value, basis) {
        (LengthPercentage::Length(px), _) => *px,
        (value, Some(basis)) => value.resolve(basis),
        (_, None) => return None,
    };
    Some(if style.box_sizing == BoxSizing::BorderBox {
        (height - pb).max(0.0)
    } else {
        height
    })
}

/// A content-box width from a sizing property.
fn width_value(
    block: &BlockBox,
    size: &Size<LengthPercentage>,
    cb: ContainingBlock,
    pb: f32,
) -> Option<f32> {
    flow::content_width(block, size, cb, pb)
}

/// Whether the box's automatic minimum size comes from its content: it is not a scroll
/// container (CSS Flexbox 1 § 4.5).
fn content_minimum(style: &ComputedValues) -> bool {
    [style.overflow_x, style.overflow_y]
        .iter()
        .all(|o| matches!(o, Overflow::Visible | Overflow::Clip))
}

/// The gap between items along the main axis and between lines, in that order.
fn gaps(style: &ComputedValues, axes: Axes, space: ContentSpace) -> (f32, f32) {
    let resolve = |gap: &Option<LengthPercentage>, basis: Option<f32>| match (gap, basis) {
        (None, _) => 0.0,
        (Some(LengthPercentage::Length(px)), _) => *px,
        (Some(value), Some(basis)) => value.resolve(basis),
        (Some(_), None) => 0.0,
    };
    let column_gap = resolve(&style.column_gap, Some(space.width));
    let row_gap = resolve(&style.row_gap, space.height);
    if axes.column {
        (row_gap, column_gap)
    } else {
        (column_gap, row_gap)
    }
}

/// The in-flow items of a container in order-modified document order (CSS Flexbox 1
/// § 5.4), and its out-of-flow placeholders.
fn split(items: &[BlockBox]) -> (Vec<&BlockBox>, Vec<&BlockBox>) {
    let (mut in_flow, placeholders): (Vec<&BlockBox>, Vec<&BlockBox>) = items
        .iter()
        .partition(|item| !matches!(item.contents, BlockContents::OutOfFlow(_)));
    in_flow.sort_by_key(|item| item.style.order);
    (in_flow, placeholders)
}

/// Minimum and maximum content widths of a flex container's content box (CSS Flexbox 1
/// § 9.9.1, roughly: each item contributes its own intrinsic widths).
pub(crate) fn intrinsic(block: &BlockBox, items: &[BlockBox]) -> (f32, f32) {
    let style = &block.style;
    let axes = Axes::of(style);
    let (in_flow, _) = split(items);
    let gap = match &style.column_gap {
        Some(LengthPercentage::Length(px)) => *px,
        _ => 0.0,
    };
    let contributions = in_flow.iter().map(|item| {
        let (min, max) = flow::intrinsic_outer(item);
        // A fixed basis is the item's preferred width in a row.
        match &item.style.flex_basis {
            FlexBasis::Size(Size::Value(LengthPercentage::Length(px))) if !axes.column => {
                let (padding, border) = flow::padding_and_border(&item.style, 0.0);
                let pb = padding.horizontal() + border.horizontal();
                let content = if item.style.box_sizing == BoxSizing::BorderBox {
                    (px - pb).max(0.0)
                } else {
                    *px
                };
                let basis = min - (flow::intrinsic_content(item).0) + content;
                let basis = if matches!(item.style.width, Size::Value(LengthPercentage::Length(_)))
                {
                    basis.max(0.0)
                } else {
                    (max - flow::intrinsic_content(item).1 + content).max(0.0)
                };
                let shrinks = item.style.flex_shrink > 0.0;
                (if shrinks { min.min(basis) } else { basis }, basis)
            }
            _ => (min, max),
        }
    });
    let (mut min, mut max, mut count) = (0.0f32, 0.0f32, 0usize);
    let mut min_sum = 0.0f32;
    for (item_min, item_max) in contributions {
        count += 1;
        if axes.column {
            min = min.max(item_min);
            max = max.max(item_max);
        } else {
            min = min.max(item_min);
            min_sum += item_min;
            max += item_max;
        }
    }
    let gaps = gap * count.saturating_sub(1) as f32;
    if axes.column {
        (min, max)
    } else if style.flex_wrap.wrap == Wrap::NoWrap {
        (min_sum + gaps, max + gaps)
    } else {
        (min, max + gaps)
    }
}

/// Lays an item out at a content-box width, and height if it has one.
fn lay(
    item: &Item<'_>,
    cb: ContainingBlock,
    (width, height): (f32, Option<f32>),
    decorators: &[Arc<ComputedValues>],
    fragmentainer: Option<Fragmentainer>,
    resume: Option<&BreakToken>,
) -> Laid {
    flow::layout_block(
        item.block,
        cb,
        BlockKind::Item {
            width,
            height,
            at_least: false,
        },
        decorators,
        fragmentainer,
        resume,
    )
}

/// The content-box height of an item, from its last layout.
fn content_height(item: &Item<'_>) -> f32 {
    let laid = item.laid.as_ref().expect("the item is laid out");
    (laid.fragment.rect.height - item.vertical_pb()).max(0.0)
}

/// What share of the free space goes before a box: 0 at the start edge of the axis, 1 at
/// the end, for an alignment along the main axis.
fn main_fraction(position: AlignPosition, axes: Axes) -> f32 {
    let flipped = |at_end: bool| if at_end { 1.0 } else { 0.0 };
    match position {
        AlignPosition::FlexStart => 0.0,
        AlignPosition::FlexEnd => 1.0,
        AlignPosition::Center => 0.5,
        // The writing mode's start is main-start unless the direction is reversed.
        AlignPosition::Start | AlignPosition::SelfStart => flipped(axes.direction_reverse),
        AlignPosition::End | AlignPosition::SelfEnd => flipped(!axes.direction_reverse),
        // Left and right are physical along a row, and the start along a column.
        AlignPosition::Left if !axes.column => flipped(axes.main_reverse),
        AlignPosition::Right if !axes.column => flipped(!axes.main_reverse),
        AlignPosition::Left | AlignPosition::Right => flipped(axes.direction_reverse),
    }
}

/// The share of free space before a box along the cross axis, for a box whose direction
/// is `item_rtl`.
fn cross_fraction(position: AlignPosition, axes: Axes, item_rtl: bool) -> f32 {
    let flipped = |at_end: bool| if at_end { 1.0 } else { 0.0 };
    match position {
        AlignPosition::FlexStart => 0.0,
        AlignPosition::FlexEnd => 1.0,
        AlignPosition::Center => 0.5,
        // The writing mode's start is cross-start unless lines wrap in reverse.
        AlignPosition::Start | AlignPosition::Left | AlignPosition::Right => {
            flipped(axes.wrap_reverse)
        }
        AlignPosition::End => flipped(!axes.wrap_reverse),
        // A box's own start differs across a column when its direction does.
        AlignPosition::SelfStart if axes.column => flipped(axes.cross_reverse != item_rtl),
        AlignPosition::SelfEnd if axes.column => flipped(axes.cross_reverse == item_rtl),
        AlignPosition::SelfStart => flipped(axes.wrap_reverse),
        AlignPosition::SelfEnd => flipped(!axes.wrap_reverse),
    }
}

/// The space before the first box and between boxes, for `free` space shared among `n`
/// boxes by a `*-content` value.
fn distribute(
    value: ContentAlignment,
    free: f32,
    n: usize,
    fraction: impl Fn(AlignPosition) -> f32,
) -> (f32, f32) {
    let start = fraction(AlignPosition::Start);
    match value {
        ContentAlignment::Normal
        | ContentAlignment::Baseline { .. }
        | ContentAlignment::Distribution(ContentDistribution::Stretch) => (0.0, 0.0),
        ContentAlignment::Distribution(distribution) => {
            if free <= 0.0 || n == 0 {
                // `space-between` falls back to `flex-start`, the others to `safe center`.
                return match distribution {
                    ContentDistribution::SpaceBetween => (0.0, 0.0),
                    _ if free < 0.0 => (free * start, 0.0),
                    _ => (free / 2.0, 0.0),
                };
            }
            match distribution {
                ContentDistribution::SpaceBetween if n == 1 => (0.0, 0.0),
                ContentDistribution::SpaceBetween => (0.0, free / (n - 1) as f32),
                ContentDistribution::SpaceAround => (free / n as f32 / 2.0, free / n as f32),
                ContentDistribution::SpaceEvenly => (free / (n + 1) as f32, free / (n + 1) as f32),
                ContentDistribution::Stretch => (0.0, 0.0),
            }
        }
        ContentAlignment::Position { position, overflow } => {
            let fraction = if overflow == OverflowPosition::Safe && free < 0.0 {
                start
            } else {
                fraction(position)
            };
            (free * fraction, 0.0)
        }
    }
}

/// Splits items with outer main sizes `sizes` into lines no longer than `available`, each
/// filled in turn, with `gap` between items. Returns where each line starts.
fn greedy_lines(sizes: &[f32], gap: f32, available: f32) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut length = 0.0;
    for (index, &size) in sizes.iter().enumerate() {
        if starts.is_empty() || length + gap + size > available + EPSILON {
            starts.push(index);
            length = size;
        } else {
            length += gap + size;
        }
    }
    starts
}

/// Splits items into `count` lines whose lengths are as similar as possible: the sum of
/// their squared lengths is least, among splits whose lines fit in `available` where any
/// does (CSS Flexbox 2 § 6.1).
fn balanced_lines(sizes: &[f32], gap: f32, available: f32, count: usize) -> Vec<usize> {
    let n = sizes.len();
    let mut prefix = vec![0.0f32; n + 1];
    for (i, size) in sizes.iter().enumerate() {
        prefix[i + 1] = prefix[i] + size;
    }
    let length = |from: usize, to: usize| prefix[to] - prefix[from] + gap * (to - from - 1) as f32;
    let cost = |from: usize, to: usize| {
        let len = length(from, to);
        // A line too long costs more than any that fits, unless it holds one item.
        let over = if len > available + EPSILON && to - from > 1 {
            1.0e12
        } else {
            0.0
        };
        f64::from(len) * f64::from(len) + over
    };
    // best[k][i]: the least cost of the first `i` items in `k` lines.
    let mut best = vec![vec![f64::INFINITY; n + 1]; count + 1];
    let mut from = vec![vec![0usize; n + 1]; count + 1];
    best[0][0] = 0.0;
    for k in 1..=count {
        for i in k..=n {
            for j in (k - 1)..i {
                if best[k - 1][j].is_finite() {
                    // Ties go to the split that puts more items on earlier lines.
                    let total = best[k - 1][j] + cost(j, i);
                    if total <= best[k][i] {
                        best[k][i] = total;
                        from[k][i] = j;
                    }
                }
            }
        }
    }
    let mut starts = Vec::with_capacity(count);
    let mut end = n;
    for k in (1..=count).rev() {
        let start = from[k][end];
        starts.push(start);
        end = start;
    }
    starts.reverse();
    starts
}

/// Resolves the flexible lengths of a line's items to fill `size` (CSS Flexbox 1 § 9.7).
fn resolve_flexible(items: &mut [Item<'_>], size: f32, gap: f32, axes: Axes) {
    let gaps = gap * items.len().saturating_sub(1) as f32;
    let used: f32 = items
        .iter()
        .map(|item| item.outer_main(item.hypothetical, axes))
        .sum::<f32>()
        + gaps;
    let grow = used < size;
    for item in items.iter_mut() {
        let factor = if grow { item.grow } else { item.shrink };
        item.main = item.hypothetical;
        item.frozen = factor == 0.0
            || (grow && item.base > item.hypothetical)
            || (!grow && item.base < item.hypothetical);
    }
    let free_space = |items: &[Item<'_>]| {
        size - gaps
            - items
                .iter()
                .map(|item| {
                    let content = if item.frozen { item.main } else { item.base };
                    item.outer_main(content, axes)
                })
                .sum::<f32>()
    };
    let initial = free_space(items);
    // Each round freezes at least one item.
    for _ in 0..=items.len() {
        if items.iter().all(|item| item.frozen) {
            break;
        }
        let mut free = free_space(items);
        let factors: f32 = items
            .iter()
            .filter(|item| !item.frozen)
            .map(|item| if grow { item.grow } else { item.shrink })
            .sum();
        if factors < 1.0 && (initial * factors).abs() < free.abs() {
            free = initial * factors;
        }
        if grow {
            for item in items.iter_mut().filter(|item| !item.frozen) {
                item.main = item.base + free * item.grow / factors;
            }
        } else {
            let scaled: f32 = items
                .iter()
                .filter(|item| !item.frozen)
                .map(|item| item.shrink * item.base)
                .sum();
            for item in items.iter_mut().filter(|item| !item.frozen) {
                item.main = if scaled > 0.0 {
                    item.base - free.abs() * item.shrink * item.base / scaled
                } else {
                    item.base
                };
            }
        }
        // Clamp to the limits, then freeze the items that broke the way the total did.
        let mut total = 0.0;
        let mut violations = Vec::with_capacity(items.len());
        for item in items.iter_mut() {
            if item.frozen {
                violations.push(0.0);
                continue;
            }
            let clamped = item.main.min(item.max_main).max(item.min_main).max(0.0);
            violations.push(clamped - item.main);
            total += clamped - item.main;
            item.main = clamped;
        }
        for (item, violation) in items.iter_mut().zip(violations) {
            if item.frozen {
                continue;
            }
            if total.abs() < EPSILON
                || (total > 0.0 && violation > 0.0)
                || (total < 0.0 && violation < 0.0)
            {
                item.frozen = true;
            }
        }
    }
}

/// Lays out the content of a flex container: its items in lines, placed and aligned.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout(
    container: &BlockBox,
    children: &[BlockBox],
    space: ContentSpace,
    decorators: &[Arc<ComputedValues>],
    fragmentainer: Option<Fragmentainer>,
    resume: Option<&BreakToken>,
) -> ContentLaid {
    let style = &container.style;
    let axes = Axes::of(style);
    let (in_flow, placeholders) = split(children);
    let cb = ContainingBlock {
        width: space.width,
        height: space.height,
        rtl: style.direction == Direction::Rtl,
    };
    let (main_gap, cross_gap) = gaps(style, axes, space);
    // Main and cross sizes that do not depend on the content.
    let definite_main = if axes.column {
        space.height
    } else {
        Some(space.width)
    };
    let definite_cross = if axes.column {
        Some(space.width)
    } else {
        space.height
    };
    let multi_line = style.flex_wrap.wrap != Wrap::NoWrap;

    let mut items: Vec<Item<'_>> = in_flow
        .iter()
        .map(|block| new_item(block, style, axes, cb, multi_line, decorators))
        .collect();

    // Lines (§ 9.3): by the outer hypothetical main sizes. Without a definite main size,
    // a column's lines are as long as `max-height` allows.
    let available = definite_main.unwrap_or_else(|| {
        if axes.column {
            space.max_height.unwrap_or(f32::INFINITY)
        } else {
            f32::INFINITY
        }
    });
    let sizes: Vec<f32> = items
        .iter()
        .map(|item| item.outer_main(item.hypothetical, axes))
        .collect();
    let mut starts = if multi_line {
        greedy_lines(&sizes, main_gap, available)
    } else {
        vec![0]
    };
    if items.is_empty() {
        starts.clear();
    }
    if multi_line && style.flex_wrap.balance && !items.is_empty() {
        let count = starts
            .len()
            .max((style.flex_line_count as usize).min(items.len()));
        if items.len() <= 400 {
            starts = balanced_lines(&sizes, main_gap, available, count);
        }
    }
    let lines: Vec<std::ops::Range<usize>> = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| start..starts.get(i + 1).copied().unwrap_or(items.len()))
        .collect();

    // `margin-trim` drops the margins of items against the container's edges.
    trim_margins(style, axes, &mut items, &lines);

    // The main size of a container whose size depends on its content.
    let line_length = |items: &[Item<'_>]| {
        items
            .iter()
            .map(|item| item.outer_main(item.hypothetical, axes))
            .sum::<f32>()
            + main_gap * items.len().saturating_sub(1) as f32
    };
    let main_size = definite_main.unwrap_or_else(|| {
        let longest = lines
            .iter()
            .map(|line| line_length(&items[line.clone()]))
            .fold(0.0, f32::max);
        longest
            .min(space.max_height.unwrap_or(f32::INFINITY))
            .max(space.min_height)
    });
    for line in &lines {
        resolve_flexible(&mut items[line.clone()], main_size, main_gap, axes);
    }

    // Hypothetical cross sizes (§ 9.4): lay out each item at its main size.
    for item in &mut items {
        if axes.column {
            let laid = lay(
                item,
                cb,
                (item.cross, Some(item.main)),
                decorators,
                None,
                None,
            );
            item.laid_out(laid);
        } else {
            let height = height_value(
                item.style,
                &item.style.height,
                space.height,
                item.cross_pb(axes),
            );
            let laid = lay(item, cb, (item.main, None), decorators, None, None);
            item.laid_out(laid);
            item.cross = height
                .map(|h| h.min(item.max_cross).max(item.min_cross))
                .unwrap_or_else(|| content_height(item));
        }
    }

    // Line cross sizes, with baseline-aligned items sharing a baseline.
    let baselines = |item: &Item<'_>| {
        let fragment = &item.laid.as_ref().expect("items are laid out").fragment;
        let height = fragment.rect.height;
        let top = item.margin[2].unwrap_or(0.0);
        (
            top + fragment.first_baseline().unwrap_or(height),
            top + fragment.last_baseline().unwrap_or(height),
        )
    };
    let baseline_aligned = |item: &Item<'_>, last: bool| {
        !axes.column
            && !item.has_auto_cross_margin()
            && matches!(item.align, ItemAlignment::Baseline { last: l } if l == last)
    };
    let mut line_cross: Vec<f32> = lines
        .iter()
        .map(|line| {
            let mut size = 0.0f32;
            let (mut ascent, mut descent) = (0.0f32, 0.0f32);
            let (mut last_ascent, mut last_descent) = (0.0f32, 0.0f32);
            for item in &items[line.clone()] {
                let outer = item.outer_cross(axes);
                if baseline_aligned(item, false) {
                    let (first, _) = baselines(item);
                    ascent = ascent.max(first);
                    descent = descent.max(outer - first);
                } else if baseline_aligned(item, true) {
                    let (_, last) = baselines(item);
                    last_ascent = last_ascent.max(last);
                    last_descent = last_descent.max(outer - last);
                } else {
                    size = size.max(outer);
                }
            }
            size.max(ascent + descent).max(last_ascent + last_descent)
        })
        .collect();
    if !multi_line && let Some(line) = line_cross.first_mut() {
        *line = match definite_cross {
            Some(cross) => cross,
            None => line
                .min(space.max_height.unwrap_or(f32::INFINITY))
                .max(space.min_height),
        };
    }

    // The container's cross size, then lines stretched to fill it (§ 9.4 step 15).
    let cross_gaps = cross_gap * lines.len().saturating_sub(1) as f32;
    let cross_size = definite_cross.unwrap_or_else(|| {
        (line_cross.iter().sum::<f32>() + cross_gaps)
            .min(space.max_height.unwrap_or(f32::INFINITY))
            .max(space.min_height)
    });
    let cross_free = cross_size - line_cross.iter().sum::<f32>() - cross_gaps;
    let stretch_lines = multi_line
        && matches!(
            style.align_content,
            ContentAlignment::Normal | ContentAlignment::Distribution(ContentDistribution::Stretch)
        );
    if stretch_lines && cross_free > 0.0 && !line_cross.is_empty() {
        let share = cross_free / line_cross.len() as f32;
        for line in &mut line_cross {
            *line += share;
        }
    }

    // Stretched items take the line's cross size, and are laid out again at it.
    for (line, &size) in lines.iter().zip(&line_cross) {
        for item in &mut items[line.clone()] {
            let auto_cross = if axes.column {
                matches!(item.style.width, Size::Auto)
            } else {
                height_value(item.style, &item.style.height, space.height, 0.0).is_none()
            };
            let stretches = matches!(item.align, ItemAlignment::Stretch | ItemAlignment::Normal)
                && auto_cross
                && !item.has_auto_cross_margin();
            if !stretches {
                continue;
            }
            let cross = (size - item.cross_margins() - item.cross_pb(axes))
                .min(item.max_cross)
                .max(item.min_cross)
                .max(0.0);
            item.stretched = true;
            item.cross = cross;
            let (width, height) = item.size(axes);
            let laid = lay(item, cb, (width, Some(height)), decorators, None, None);
            item.laid_out(laid);
        }
    }
    // A column item's cross size is its laid-out width; a row item's its laid-out height.
    for item in &mut items {
        if !axes.column && !item.stretched {
            item.cross = content_height(item);
        }
    }

    // Main-axis alignment (§ 9.5): auto margins, then `justify-content`.
    for line in &lines {
        let line_items = &mut items[line.clone()];
        let used: f32 = line_items
            .iter()
            .map(|item| item.outer_main(item.main, axes))
            .sum::<f32>()
            + main_gap * line_items.len().saturating_sub(1) as f32;
        let mut free = main_size - used;
        let autos = line_items
            .iter()
            .map(|item| item.margin[..2].iter().filter(|m| m.is_none()).count())
            .sum::<usize>();
        if autos > 0 {
            let share = (free / autos as f32).max(0.0);
            for item in line_items.iter_mut() {
                for margin in &mut item.margin[..2] {
                    margin.get_or_insert(share);
                }
            }
            free = if free > 0.0 { 0.0 } else { free };
        }
        let (before, between) = distribute(style.justify_content, free, line_items.len(), |p| {
            main_fraction(p, axes)
        });
        let mut cursor = before;
        for item in line_items.iter_mut() {
            cursor += item.margin[0].unwrap_or(0.0);
            item.main_pos = cursor;
            cursor +=
                item.main + item.main_pb(axes) + item.margin[1].unwrap_or(0.0) + main_gap + between;
        }
    }

    // Lines along the cross axis (`align-content`), and items in their lines.
    let (lines_before, lines_between) = if multi_line {
        let free = if stretch_lines && cross_free > 0.0 {
            0.0
        } else {
            cross_free
        };
        distribute(style.align_content, free, lines.len(), |p| {
            cross_fraction(p, axes, axes.cross_reverse)
        })
    } else {
        (0.0, 0.0)
    };
    let mut line_start = lines_before;
    for (line, &size) in lines.iter().zip(&line_cross) {
        let line_items = &mut items[line.clone()];
        let first_baseline = line_items
            .iter()
            .filter(|item| baseline_aligned(item, false))
            .map(|item| baselines(item).0)
            .fold(0.0, f32::max);
        let last_descent = line_items
            .iter()
            .filter(|item| baseline_aligned(item, true))
            .map(|item| item.outer_cross(axes) - baselines(item).1)
            .fold(0.0, f32::max);
        for item in line_items.iter_mut() {
            let outer = item.outer_cross(axes);
            let mut free = size - outer;
            let autos = item.margin[2..].iter().filter(|m| m.is_none()).count();
            if autos > 0 {
                let share = (free / autos as f32).max(0.0);
                for margin in &mut item.margin[2..] {
                    margin.get_or_insert(share);
                }
                free = 0.0;
            }
            let item_rtl = item.style.direction == Direction::Rtl;
            let offset = if autos > 0 {
                0.0
            } else if baseline_aligned(item, false) {
                first_baseline - baselines(item).0
            } else if baseline_aligned(item, true) {
                size - last_descent - baselines(item).1
            } else {
                match item.align {
                    ItemAlignment::Position { position, overflow } => {
                        let fraction = if overflow == OverflowPosition::Safe && free < 0.0 {
                            cross_fraction(AlignPosition::Start, axes, item_rtl)
                        } else {
                            cross_fraction(position, axes, item_rtl)
                        };
                        free * fraction
                    }
                    ItemAlignment::AnchorCenter => free / 2.0,
                    // Baselines across a column fall back to `start`.
                    ItemAlignment::Baseline { last } => {
                        let position = if last {
                            AlignPosition::End
                        } else {
                            AlignPosition::Start
                        };
                        let fraction = cross_fraction(position, axes, item_rtl);
                        if free < 0.0 && last {
                            0.0
                        } else {
                            free * fraction
                        }
                    }
                    _ => 0.0,
                }
            };
            item.cross_pos = line_start + offset + item.margin[2].unwrap_or(0.0);
        }
        line_start += size + cross_gap + lines_between;
    }

    // Physical positions in the content box.
    let (width, height) = if axes.column {
        (space.width, main_size)
    } else {
        (space.width, cross_size)
    };
    let mut placed: Vec<Placed> = Vec::with_capacity(items.len());
    for (order, item) in items.iter().enumerate() {
        let mut fragment = item
            .laid
            .as_ref()
            .expect("items are laid out")
            .fragment
            .clone();
        let (w, h) = (fragment.rect.width, fragment.rect.height);
        let (main_len, cross_len) = if axes.column { (h, w) } else { (w, h) };
        let main = if axes.main_reverse {
            (if axes.column { height } else { width }) - item.main_pos - main_len
        } else {
            item.main_pos
        };
        let cross = if axes.cross_reverse {
            (if axes.column { width } else { height }) - item.cross_pos - cross_len
        } else {
            item.cross_pos
        };
        let (x, y) = if axes.column {
            (cross, main)
        } else {
            (main, cross)
        };
        let (dx, dy) = crate::position::relative_offset(item.style, cb);
        fragment.rect.x = x + dx;
        fragment.rect.y = y + dy;
        let (content_width, content_height) = item.size(axes);
        placed.push(Placed {
            block: item.block,
            fragment,
            order,
            width: content_width,
            height: (axes.column || item.stretched).then_some(content_height),
            offset: 0.0,
        });
    }

    // Units that pages break between, in visual order from the top.
    let margin_box = |index: usize| {
        let item = &items[index];
        let rect = placed[index].fragment.rect;
        let (top, bottom) = if axes.column {
            let (start, end) = if axes.main_reverse { (1, 0) } else { (0, 1) };
            (item.margin[start], item.margin[end])
        } else {
            let (start, end) = if axes.cross_reverse { (3, 2) } else { (2, 3) };
            (item.margin[start], item.margin[end])
        };
        (
            rect.y - top.unwrap_or(0.0),
            rect.bottom() + bottom.unwrap_or(0.0),
        )
    };
    let mut units: Vec<Unit> = if axes.column && multi_line {
        vec![Unit {
            items: (0..items.len()).collect(),
            top: 0.0,
            bottom: height,
        }]
    } else if axes.column {
        (0..items.len())
            .map(|index| {
                let (top, bottom) = margin_box(index);
                Unit {
                    items: vec![index],
                    top,
                    bottom,
                }
            })
            .collect()
    } else {
        lines
            .iter()
            .map(|line| {
                let (top, bottom) = line.clone().map(margin_box).fold(
                    (f32::INFINITY, f32::NEG_INFINITY),
                    |(t, b), (top, bottom)| (t.min(top), b.max(bottom)),
                );
                Unit {
                    items: line.clone().collect(),
                    top,
                    bottom,
                }
            })
            .collect()
    };
    units.retain(|unit| !unit.items.is_empty());
    units.sort_by(|a, b| a.top.total_cmp(&b.top));
    Placed::set_offsets(&mut placed, &units);

    let mut out = match fragmentainer {
        None => ContentLaid {
            children: Vec::new(),
            height,
            break_token: None,
            deferred: false,
        },
        Some(f) => paginate(
            &placed,
            &units,
            !axes.column,
            cb,
            decorators,
            f,
            resume,
            height,
        ),
    };
    if fragmentainer.is_none() {
        out.children = placed
            .into_iter()
            .map(|p| Fragment::Box(p.fragment))
            .collect();
    }
    // Out-of-flow boxes have their static position at the content box's start, on the
    // first page.
    if resume.is_none() && !out.deferred {
        for block in placeholders {
            let laid = flow::layout_block(block, cb, BlockKind::InFlow, decorators, None, None);
            out.children.push(Fragment::Box(laid.fragment));
        }
    }
    out
}

/// Measures an item before its line is known: its margins, limits, flex base size and
/// hypothetical main size (§ 9.2), and for a column its cross size.
fn new_item<'a>(
    block: &'a BlockBox,
    container: &ComputedValues,
    axes: Axes,
    cb: ContainingBlock,
    multi_line: bool,
    decorators: &[Arc<ComputedValues>],
) -> Item<'a> {
    let style = &*block.style;
    let (padding, border) = flow::padding_and_border(style, cb.width);
    let margin = |value: &MaybeAuto<LengthPercentage>| match value {
        MaybeAuto::Auto => None,
        MaybeAuto::Value(value) => Some(value.resolve(cb.width)),
    };
    let (top, right, bottom, left) = (
        margin(&style.margin_top),
        margin(&style.margin_right),
        margin(&style.margin_bottom),
        margin(&style.margin_left),
    );
    let margin = if axes.column {
        let (main_start, main_end) = if axes.main_reverse {
            (bottom, top)
        } else {
            (top, bottom)
        };
        let (cross_start, cross_end) = if axes.cross_reverse {
            (right, left)
        } else {
            (left, right)
        };
        [main_start, main_end, cross_start, cross_end]
    } else {
        let (main_start, main_end) = if axes.main_reverse {
            (right, left)
        } else {
            (left, right)
        };
        let (cross_start, cross_end) = if axes.cross_reverse {
            (bottom, top)
        } else {
            (top, bottom)
        };
        [main_start, main_end, cross_start, cross_end]
    };
    let mut item = Item {
        block,
        style,
        padding,
        border,
        margin,
        grow: style.flex_grow,
        shrink: style.flex_shrink,
        base: 0.0,
        hypothetical: 0.0,
        min_main: 0.0,
        max_main: f32::INFINITY,
        min_cross: 0.0,
        max_cross: f32::INFINITY,
        main: 0.0,
        frozen: false,
        cross: 0.0,
        stretched: false,
        align: style.align_self.resolve(container.align_items),
        laid: None,
        main_pos: 0.0,
        cross_pos: 0.0,
        legend: 0.0,
    };
    let pb_w = padding.horizontal() + border.horizontal();
    let pb_h = padding.vertical() + border.vertical();
    let width_of = |size: &Size<LengthPercentage>| width_value(block, size, cb, pb_w);
    let height_of = |size: &Size<LengthPercentage>| height_value(style, size, cb.height, pb_h);
    let ratio = style.aspect_ratio.value().or_else(|| {
        block
            .replaced
            .as_ref()
            .and_then(|r| r.size)
            .filter(|&(w, h)| w > 0.0 && h > 0.0)
            .map(|(w, h)| w / h)
    });

    if axes.column {
        item.min_cross = width_of(&style.min_width).unwrap_or(0.0);
        item.max_cross = width_of(&style.max_width).unwrap_or(f32::INFINITY);
        item.min_main = height_of(&style.min_height).unwrap_or(0.0);
        item.max_main = height_of(&style.max_height).unwrap_or(f32::INFINITY);
        // The width the item is measured at: its own, the line's when it stretches across
        // a single line, or else shrink-to-fit.
        let available = (cb.width - item.cross_margins() - pb_w).max(0.0);
        let stretches = !multi_line
            && matches!(item.align, ItemAlignment::Stretch | ItemAlignment::Normal)
            && !item.has_auto_cross_margin();
        let width = width_of(&style.width).unwrap_or_else(|| {
            if stretches {
                available
            } else {
                let (min, max) = flow::intrinsic_content(block);
                available.min(max).max(min)
            }
        });
        item.cross = width.min(item.max_cross).max(item.min_cross);
        let measured = lay(&item, cb, (item.cross, None), decorators, None, None);
        item.laid_out(measured);
        let content = content_height(&item);
        let preferred = match &style.flex_basis {
            FlexBasis::Content => None,
            FlexBasis::Size(Size::Auto) => height_of(&style.height),
            FlexBasis::Size(size) => height_of(size),
        };
        item.base = preferred.unwrap_or(content);
        if matches!(style.min_height, Size::Auto) && content_minimum(style) {
            let specified = height_of(&style.height);
            let suggestion = content.min(item.max_main);
            item.min_main = specified.map_or(suggestion, |s| s.min(suggestion));
        }
    } else {
        item.min_main = width_of(&style.min_width).unwrap_or(0.0);
        item.max_main = width_of(&style.max_width).unwrap_or(f32::INFINITY);
        item.min_cross = height_of(&style.min_height).unwrap_or(0.0);
        item.max_cross = height_of(&style.max_height).unwrap_or(f32::INFINITY);
        let preferred = match &style.flex_basis {
            FlexBasis::Content => None,
            FlexBasis::Size(Size::Auto) => width_of(&style.width),
            FlexBasis::Size(Size::Stretch) => None,
            FlexBasis::Size(size) => width_of(size),
        };
        let (min_content, max_content) = flow::intrinsic_content(block);
        // Without a preferred width, a definite height gives one through the aspect ratio.
        let from_ratio = ratio
            .zip(height_of(&style.height))
            .map(|(ratio, height)| height * ratio);
        item.base = preferred.or(from_ratio).unwrap_or(max_content);
        if matches!(style.min_width, Size::Auto) && content_minimum(style) {
            let specified = width_of(&style.width);
            let suggestion = from_ratio.unwrap_or(min_content).min(item.max_main);
            item.min_main = specified.map_or(suggestion, |s| s.min(suggestion));
        }
    }
    item.hypothetical = item.base.min(item.max_main).max(item.min_main);
    item
}

/// Applies the container's `margin-trim` to the items against its edges: the first and
/// last item of each line along the main axis, and the items of the first and last lines
/// across it.
fn trim_margins(
    style: &ComputedValues,
    axes: Axes,
    items: &mut [Item<'_>],
    lines: &[std::ops::Range<usize>],
) {
    let trim = style.margin_trim;
    let rtl = style.direction == Direction::Rtl;
    // Trimmed physical sides: top, right, bottom, left.
    let (left, right) = if rtl {
        (trim.inline_end, trim.inline_start)
    } else {
        (trim.inline_start, trim.inline_end)
    };
    let sides = [trim.block_start, right, trim.block_end, left];
    if !sides.iter().any(|&s| s) {
        return;
    }
    // The physical side of each logical margin slot: main-start, main-end, cross-start,
    // cross-end.
    let side_of = |slot: usize| -> usize {
        let (start, end) = if axes.column {
            if slot < 2 {
                if axes.main_reverse { (2, 0) } else { (0, 2) }
            } else if axes.cross_reverse {
                (1, 3)
            } else {
                (3, 1)
            }
        } else if slot < 2 {
            if axes.main_reverse { (1, 3) } else { (3, 1) }
        } else if axes.cross_reverse {
            (2, 0)
        } else {
            (0, 2)
        };
        if slot.is_multiple_of(2) { start } else { end }
    };
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let trim_slot = |item: &mut Item<'_>, slot: usize| {
            if sides[side_of(slot)] {
                item.margin[slot] = Some(0.0);
            }
        };
        trim_slot(&mut items[line.start], 0);
        trim_slot(&mut items[line.end - 1], 1);
        for item in &mut items[line.clone()] {
            if index == 0 {
                trim_slot(item, 2);
            }
            if index == lines.len() - 1 {
                trim_slot(item, 3);
            }
        }
    }
}
