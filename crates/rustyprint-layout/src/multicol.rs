//! Multi-column layout (CSS Multi-column Layout 1 and 2).
//!
//! A multi-column container's children are grouped into column flows and the spanners
//! between them (see `boxes::column_contents`). A column flow breaks into columns: each
//! column is a fragmentainer as tall as the column height, which the flow fills from where
//! the column before it broke. Balanced columns take the shortest height at which the
//! content fits in the columns of a row. Content that does not fit continues on the next
//! page, in a new row of columns with `column-wrap`, or in overflow columns beside the
//! container when its own height constrains the columns.

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    BorderStyle, ColumnFill, ColumnSpan, ColumnWrap, Direction, Display, DisplayInside,
    DisplayInternal, Float,
};

use crate::boxes::{BlockBox, BlockContents};
use crate::display::Rect;
use crate::flow::{self, BlockKind, BreakToken, ContainingBlock, EPSILON, Fragmentainer, Resume};
use crate::fragment::{BoxFragment, Fragment, Sides};
use crate::units::{ContentLaid, ContentSpace};

/// The most columns a row gets, overflow columns included.
const MAX_COLUMNS: usize = 1000;

/// Whether a style makes its box a multi-column container: a block container, or a table
/// cell, with a column count or width.
pub(crate) fn is_multicol(style: &ComputedValues) -> bool {
    (style.column_count.is_some() || style.column_width.is_some())
        && matches!(
            style.display,
            Display::Normal {
                inside: DisplayInside::Flow | DisplayInside::FlowRoot,
                ..
            } | Display::Internal(DisplayInternal::TableCell)
        )
}

/// Whether a child of a multi-column container spans all its columns: an in-flow
/// block-level box with `column-span: all`.
pub(crate) fn is_spanner(block: &BlockBox) -> bool {
    block.node.is_some()
        && block.style.column_span == ColumnSpan::All
        && block.style.float == Float::None
        && !matches!(
            block.contents,
            BlockContents::OutOfFlow(_) | BlockContents::Float(_)
        )
}

/// The gap between columns: `normal` is 1em (CSS Box Alignment 3 § 8.1).
fn column_gap(style: &ComputedValues, basis: f32) -> f32 {
    style
        .column_gap
        .as_ref()
        .map_or(style.font_size, |gap| gap.resolve(basis).max(0.0))
}

/// The number of columns and their width in a content box `available` wide (CSS
/// Multi-column 1 § 3.4).
fn columns(style: &ComputedValues, available: f32, gap: f32) -> (usize, f32) {
    let count = match (style.column_width, style.column_count) {
        (Some(width), count) => {
            let fit = if width + gap > 0.0 {
                ((available + gap) / (width + gap)).floor().max(1.0) as usize
            } else {
                MAX_COLUMNS
            };
            count.map_or(fit, |count| fit.min(count as usize))
        }
        (None, Some(count)) => count as usize,
        (None, None) => 1,
    }
    .clamp(1, MAX_COLUMNS);
    let width = ((available - (count - 1) as f32 * gap) / count as f32).max(0.0);
    (count, width)
}

/// Minimum and maximum content widths of a multi-column container (CSS Multi-column 1
/// § 3.4): its columns side by side, or its widest spanner.
pub(crate) fn intrinsic(block: &BlockBox, segments: &[BlockBox]) -> (f32, f32) {
    let style = &block.style;
    let (mut content_min, mut content_max) = (0.0f32, 0.0f32);
    let (mut span_min, mut span_max) = (0.0f32, 0.0f32);
    for segment in segments {
        if is_spanner(segment) {
            let (min, max) = flow::intrinsic_outer(segment);
            span_min = span_min.max(min);
            span_max = span_max.max(max);
        } else {
            let (min, max) = flow::intrinsic_content(segment);
            content_min = content_min.max(min);
            content_max = content_max.max(max);
        }
    }
    let count = style.column_count.unwrap_or(1) as f32;
    let gap = column_gap(style, 0.0) * (count - 1.0);
    let min = match style.column_width {
        Some(width) => width.min(content_min),
        None => content_min * count + gap,
    };
    let column = style
        .column_width
        .map_or(content_max, |width| width.max(content_min));
    let max = (column * count + gap).max(min);
    (min.max(span_min), max.max(span_max))
}

/// A row of columns laid out at one height.
struct Row {
    /// The columns' fragments, with their border boxes at the origin.
    columns: Vec<BoxFragment>,
    /// Where the flow continues after the row.
    rest: Option<BreakToken>,
    /// The tallest column.
    height: f32,
    /// Whether every column's content fit in the column height.
    fits: bool,
    /// Whether the row ended at a forced page break.
    page_break: bool,
    /// Whether nothing fit in the first column, which has content before it on the page.
    deferred: bool,
}

/// What the columns of a container share.
struct Columns<'a> {
    decorators: &'a [Arc<ComputedValues>],
    count: usize,
    width: f32,
    gap: f32,
    rtl: bool,
    /// Whether the container breaks across pages, so forced page breaks end its columns
    /// on the page.
    paged: bool,
}

impl Columns<'_> {
    /// Fills up to `max` columns of `height` with the flow, from `from`. The first column
    /// moves to the next page if nothing fits in it and it is not `at_top`.
    fn fill(
        &self,
        flow: &BlockBox,
        from: Option<&BreakToken>,
        height: f32,
        max: usize,
        at_top: bool,
    ) -> Row {
        let cb = ContainingBlock {
            width: self.width,
            height: None,
            rtl: self.rtl,
        };
        let mut row = Row {
            columns: Vec::new(),
            rest: from.cloned(),
            height: 0.0,
            fits: true,
            page_break: false,
            deferred: false,
        };
        while row.columns.len() < max {
            let laid = flow::layout_block(
                flow,
                cb,
                BlockKind::Item {
                    width: self.width,
                    height: None,
                    at_least: false,
                },
                self.decorators,
                Some(Fragmentainer {
                    available: height,
                    at_top: at_top || !row.columns.is_empty(),
                    columns: true,
                }),
                row.rest.as_ref(),
            );
            if laid.deferred {
                row.deferred = row.columns.is_empty();
                break;
            }
            let fragment = laid.fragment;
            row.height = row.height.max(fragment.rect.height);
            row.fits &= fragment.rect.height <= height + EPSILON;
            row.columns.push(fragment);
            row.rest = laid.break_token;
            match &row.rest {
                None => break,
                Some(token) if self.paged && token.forced && flow::breaks_page(flow, token) => {
                    row.page_break = true;
                    break;
                }
                Some(_) => {}
            }
        }
        row.fits &= row.rest.is_none() && !row.deferred;
        row
    }

    /// The shortest row of at most `limit` (or of any height) whose columns take all the
    /// flow, found by bisection, or `None` if the flow does not fit in a row.
    fn balance(
        &self,
        flow: &BlockBox,
        from: Option<&BreakToken>,
        limit: Option<f32>,
    ) -> Option<Row> {
        let mut best = self.fill(flow, from, limit.unwrap_or(1.0e7), self.count, true);
        if !best.fits {
            return None;
        }
        let mut hi = best.height;
        let total: f32 = best.columns.iter().map(|c| c.rect.height).sum();
        // Content that breaks anywhere fits in columns of an equal share of it.
        let share = total / self.count as f32;
        let mut lo = 0.0f32;
        if share < hi - EPSILON {
            let row = self.fill(flow, from, share, self.count, true);
            if row.fits {
                return Some(row);
            }
            lo = share;
        }
        for _ in 0..24 {
            if hi - lo <= 0.1 {
                break;
            }
            let mid = (lo + hi) / 2.0;
            let row = self.fill(flow, from, mid, self.count, true);
            if row.fits {
                hi = row.height.min(mid);
                best = row;
            } else {
                lo = mid;
            }
        }
        Some(best)
    }

    /// The left edge of column `index`, which runs from the inline start of a content box
    /// `available` wide.
    fn x(&self, index: usize, available: f32) -> f32 {
        let offset = index as f32 * (self.width + self.gap);
        if self.rtl {
            available - self.width - offset
        } else {
            offset
        }
    }
}

/// Lays out the content of a multi-column container: its column flows and spanners, from
/// where the container broke on the page before.
pub(crate) fn layout(
    container: &BlockBox,
    segments: &[BlockBox],
    space: ContentSpace,
    decorators: &[Arc<ComputedValues>],
    fragmentainer: Option<Fragmentainer>,
    resume: Option<&BreakToken>,
) -> ContentLaid {
    let style = &container.style;
    let gap = column_gap(style, space.width);
    let (count, width) = columns(style, space.width, gap);
    let row_gap = style.row_gap.as_ref().map_or(style.font_size, |g| {
        g.resolve(space.height.unwrap_or(0.0)).max(0.0)
    });
    let rtl = style.direction == Direction::Rtl;
    let set = Columns {
        decorators,
        count,
        width,
        gap,
        rtl,
        paged: fragmentainer.is_some(),
    };
    // The height the content may take: a definite height or `max-height`, less what
    // earlier pages took.
    let consumed = resume.map_or(0.0, |token| token.consumed);
    let own_limit = space
        .height
        .or(space.max_height)
        .map(|h| (h - consumed).max(0.0));
    let wrap = match style.column_wrap {
        ColumnWrap::Auto => style.column_height.is_some(),
        ColumnWrap::Nowrap => false,
        ColumnWrap::Wrap => true,
    };
    let rule = rule_style(style);

    let (start, mut start_token) = match resume.map(|token| &token.resume) {
        None => (0, None),
        Some(Resume::Child { index, token }) => (*index, token.as_deref().cloned()),
        // Anything else means the content is done, as when a definite height broke.
        Some(_) => (segments.len(), None),
    };
    let mut rules = Vec::new();
    let mut children = Vec::new();
    let mut y = 0.0f32;
    let mut placed_any = false;
    let mut break_token = None;
    // Whether nothing comes before content on the page, so it has to go there.
    let at_top = |placed_any: bool| !placed_any && fragmentainer.is_none_or(|f| f.at_top);
    let columns_context = fragmentainer.is_some_and(|f| f.columns);
    'segments: for (s, segment) in segments.iter().enumerate().skip(start) {
        let from = if s == start { start_token.take() } else { None };
        if s > start
            && let Some(side) = flow::forced_between(&segments[s - 1], segment, columns_context)
        {
            break_token = Some(BreakToken {
                consumed: 0.0,
                resume: Resume::Child {
                    index: s,
                    token: None,
                },
                forced: true,
                side,
            });
            break;
        }
        let stop = |token: Option<BreakToken>| {
            Some(BreakToken {
                consumed: 0.0,
                forced: token.as_ref().is_some_and(|t| t.forced),
                side: token.as_ref().and_then(|t| t.side),
                resume: Resume::Child {
                    index: s,
                    token: token.map(Box::new),
                },
            })
        };

        if is_spanner(segment) {
            // A spanner goes across all the columns, below the columns before it.
            let cb = ContainingBlock {
                width: space.width,
                height: space.height,
                rtl,
            };
            let page = fragmentainer.map(|f| Fragmentainer {
                available: f.available - y,
                at_top: at_top(placed_any),
                columns: f.columns,
            });
            let laid = flow::layout_block(
                segment,
                cb,
                BlockKind::InFlow,
                decorators,
                page,
                from.as_ref(),
            );
            let top = y + if from.is_some() {
                0.0
            } else {
                laid.collapsed_top.value()
            };
            let overflows = page.is_some_and(|f| {
                laid.break_token.is_none()
                    && top - y + laid.fragment.rect.height > f.available + EPSILON
            });
            if laid.deferred || (overflows && !at_top(placed_any)) {
                // A spanner that does not fit moves to the next page, with what follows.
                if placed_any {
                    break_token = stop(from);
                    break;
                }
                return deferred();
            }
            let mut fragment = laid.fragment;
            fragment.rect.x = laid.margin.left;
            fragment.rect.y = top;
            y = top + fragment.rect.height;
            if laid.break_token.is_none() {
                y += laid.collapsed_bottom.value();
            }
            children.push(Fragment::Box(fragment));
            placed_any = true;
            if laid.break_token.is_some() {
                break_token = stop(laid.break_token);
                break;
            }
            continue;
        }

        // A column flow, in rows of columns.
        let before_spanner = segments.get(s + 1).is_some_and(is_spanner);
        let mut token = from;
        let mut first_row = true;
        // The height the container gives each row: `column-height`, within what is left
        // of its own height for the first row. Later rows repeat it.
        let own = match (
            style.column_height,
            own_limit.map(|limit| (limit - y).max(0.0)),
        ) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        for _ in 0..MAX_COLUMNS {
            let row_top = if first_row { y } else { y + row_gap };
            let page_left = fragmentainer.map(|f| f.available - row_top);
            let cap = match (own, page_left) {
                (Some(a), Some(b)) => Some(a.min(b.max(0.0))),
                (a, b) => a.or(b.map(|b| b.max(0.0))),
            };
            let by_page = page_left.is_some_and(|page| own.is_none_or(|own| page < own - EPSILON));
            let balances = before_spanner
                || match style.column_fill {
                    ColumnFill::Balance | ColumnFill::BalanceAll => true,
                    ColumnFill::Auto => own.is_none() && page_left.is_none(),
                };
            let first_at_top = at_top(placed_any) || !by_page;
            let (mut row, balanced) = match cap {
                None => match set.balance(segment, token.as_ref(), None) {
                    Some(row) => (row, true),
                    // More forced breaks than columns: the rest overflows beside them.
                    None => (
                        set.fill(segment, token.as_ref(), 1.0e7, MAX_COLUMNS, true),
                        false,
                    ),
                },
                Some(cap) => {
                    let full = set.fill(segment, token.as_ref(), cap, count, first_at_top);
                    match full.fits && balances {
                        true => (
                            set.balance(segment, token.as_ref(), Some(cap))
                                .unwrap_or(full),
                            true,
                        ),
                        false => (full, false),
                    }
                }
            };
            if row.deferred {
                if placed_any {
                    break_token = stop(token);
                    break 'segments;
                }
                return deferred();
            }
            // A height of its own that the columns do not fit in makes overflow columns
            // beside the container, unless they wrap into a new row.
            if let Some(cap) = cap
                && row.rest.is_some()
                && !row.page_break
                && !wrap
                && !by_page
            {
                let more = set.fill(segment, row.rest.as_ref(), cap, MAX_COLUMNS, true);
                row.height = row.height.max(more.height);
                row.columns.extend(more.columns);
                row.rest = more.rest;
                row.page_break = more.page_break;
            }
            let row_height = match cap {
                Some(cap) if row.rest.is_some() => cap.max(row.height),
                Some(cap) if !balanced && own.is_some() => cap.max(row.height),
                _ => row.height,
            };
            if let Some(rule) = &rule {
                for i in 1..row.columns.len() {
                    let (a, b) = (set.x(i - 1, space.width), set.x(i, space.width));
                    let center = (a.min(b) + width + a.max(b)) / 2.0;
                    rules.push(Fragment::Box(rule_fragment(
                        rule, center, row_top, row_height,
                    )));
                }
            }
            for (i, mut column) in row.columns.into_iter().enumerate() {
                column.rect.x = set.x(i, space.width);
                column.rect.y = row_top;
                children.push(Fragment::Box(column));
            }
            y = row_top + row_height;
            placed_any = true;
            first_row = false;
            token = row.rest;
            if token.is_none() {
                continue 'segments;
            }
            if row.page_break || by_page || !wrap {
                break_token = stop(token);
                if let Some(t) = &mut break_token
                    && !row.page_break
                {
                    t.forced = false;
                    t.side = None;
                }
                break 'segments;
            }
        }
    }
    // Rules paint below the content of the columns.
    rules.extend(children);
    ContentLaid {
        children: rules,
        height: y,
        break_token,
        deferred: false,
    }
}

/// Content that moves to the next page whole.
fn deferred() -> ContentLaid {
    ContentLaid {
        children: Vec::new(),
        height: 0.0,
        break_token: None,
        deferred: true,
    }
}

/// The style of a container's column rules, drawn as the left border of a box in the
/// middle of each gap, or `None` if they draw nothing (CSS Multi-column 1 § 4.4).
fn rule_style(style: &ComputedValues) -> Option<Arc<ComputedValues>> {
    if style.column_rule_width <= 0.0
        || matches!(
            style.column_rule_style,
            BorderStyle::None | BorderStyle::Hidden
        )
    {
        return None;
    }
    let mut rule = ComputedValues::inherit_from(style);
    rule.border_left_width = style.column_rule_width;
    rule.border_left_style = style.column_rule_style;
    rule.border_left_color = style.column_rule_color.clone();
    Some(Arc::new(rule))
}

/// A column rule centered on `center`, from `top` down `height`.
fn rule_fragment(style: &Arc<ComputedValues>, center: f32, top: f32, height: f32) -> BoxFragment {
    let width = style.border_left_width;
    BoxFragment {
        node: None,
        style: style.clone(),
        rect: Rect::new(center - width / 2.0, top, width, height),
        border: Sides {
            left: width,
            ..Sides::default()
        },
        padding: Sides::default(),
        children: Vec::new(),
        replaced: None,
        legend: None,
        out_of_flow: None,
    }
}
