//! Breaking the content of flex and grid containers across pages.
//!
//! The container lays its items out whole, then groups them into units that pages break
//! between: a line of a row flex container, an item of a single-line column flex
//! container, or a run of grid rows that items span together. A unit that does not fit
//! breaks inside its items, each from where it is.

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::PageSide;

use crate::boxes::BlockBox;
use crate::flow::{
    self, BlockKind, BreakToken, ContainingBlock, EPSILON, Fragmentainer, ItemResume, Resume,
};
use crate::fragment::{BoxFragment, Fragment, Sides};

/// The space the content of a flex or grid container gets.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContentSpace {
    /// The content-box width.
    pub width: f32,
    /// The content-box height, when it does not depend on the content.
    pub height: Option<f32>,
    pub min_height: f32,
    pub max_height: Option<f32>,
    /// The container's padding, whose edges are the containing block of absolutely
    /// positioned children placed in a grid with `auto` lines.
    pub padding: Sides,
}

/// The content of a flex or grid container, laid out on one page.
pub(crate) struct ContentLaid {
    /// Items and placeholders, relative to the content box, in order-modified document
    /// order.
    pub children: Vec<Fragment>,
    /// The content height on this page.
    pub height: f32,
    pub break_token: Option<BreakToken>,
    /// Whether nothing fits on this page and the container should move to the next.
    pub deferred: bool,
}

/// A unit that pages break between: a line of a row container, an item of a single-line
/// column container, or all of a multi-line column container, whose columns break side by
/// side.
pub(crate) struct Unit {
    /// Its items, as indices into the placed items, and its top and bottom in the content
    /// box, margins included.
    pub items: Vec<usize>,
    pub top: f32,
    pub bottom: f32,
}

/// An item positioned in the container's content box.
pub(crate) struct Placed<'a> {
    pub block: &'a BlockBox,
    pub fragment: BoxFragment,
    /// Where it is in order-modified document order.
    pub order: usize,
    /// The content width and height it was laid out at; `None` for an auto height.
    pub width: f32,
    pub height: Option<f32>,
    /// Its top's offset in its unit, from the unit's top.
    pub offset: f32,
}

impl Placed<'_> {
    /// Sets each item's offset in the unit it is in.
    pub(crate) fn set_offsets(placed: &mut [Placed<'_>], units: &[Unit]) {
        for unit in units {
            for &index in &unit.items {
                placed[index].offset = placed[index].fragment.rect.y - unit.top;
            }
        }
    }
}

/// Whether a break before the items of `after` is forced, by them or by the items of
/// `before`, and the side the next page must be on.
fn forced_between(
    before: &[&BlockBox],
    after: &[&BlockBox],
    columns: bool,
) -> Option<Option<PageSide>> {
    let values = before
        .iter()
        .map(|b| flow::break_after(b))
        .chain(after.iter().map(|b| flow::break_before(b)));
    let mut found = None;
    for value in values {
        let forced = if columns {
            value.forces_column()
        } else {
            value.forces_page()
        };
        if forced {
            let side = value.side();
            found = Some(match found {
                Some(Some(side)) => Some(side),
                _ => side,
            });
        }
    }
    found
}

/// Places the units that fit on this page, from where the container broke before, and
/// breaks inside the first one that does not fit. Its items break each from its own place
/// in the unit; one that has not started yet keeps its offset below the break.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paginate(
    placed: &[Placed<'_>],
    units: &[Unit],
    rows: bool,
    cb: ContainingBlock,
    decorators: &[Arc<ComputedValues>],
    f: Fragmentainer,
    resume: Option<&BreakToken>,
    full_height: f32,
) -> ContentLaid {
    let mut children: Vec<(usize, BoxFragment)> = Vec::new();
    let (start, mut part) = match resume.map(|token| &token.resume) {
        None => (0, None),
        Some(Resume::Row { index }) => (*index, None),
        Some(Resume::FlexPart {
            index,
            items,
            consumed,
        }) => (*index, Some((items.clone(), *consumed))),
        // Anything else means the content is done, as when a definite height broke.
        Some(_) => (units.len(), None),
    };
    let blocks =
        |unit: &Unit| -> Vec<&BlockBox> { unit.items.iter().map(|&i| placed[i].block).collect() };
    let mut y = 0.0f32;
    let mut placed_any = false;
    let mut break_token = None;
    for u in start.min(units.len())..units.len() {
        let unit = &units[u];
        let height = unit.bottom - unit.top;
        if u > start
            && let Some(side) = forced_between(&blocks(&units[u - 1]), &blocks(unit), f.columns)
        {
            break_token = Some(BreakToken {
                consumed: 0.0,
                resume: Resume::Row { index: u },
                forced: true,
                side,
            });
            break;
        }
        // A unit that broke on the page before continues at the top of this one. Space
        // before a unit is kept, unless a break came before it.
        let continued = part.is_some();
        let top = match u {
            _ if u == start && resume.is_some() => y,
            0 => unit.top,
            _ => y + unit.top - units[u - 1].bottom,
        };
        let (states, consumed) = match part.take() {
            Some(part) => part,
            None if top + height <= f.available + EPSILON => {
                for &index in &unit.items {
                    let mut fragment = placed[index].fragment.clone();
                    fragment.rect.y += top - unit.top;
                    children.push((placed[index].order, fragment));
                }
                y = top + height;
                placed_any = true;
                continue;
            }
            None => (vec![ItemResume::NotStarted; unit.items.len()], 0.0),
        };

        // Lay out each item that is not done, from where it is.
        let mut next = Vec::with_capacity(states.len());
        let mut started = false;
        let mut more = false;
        let mut bottom = top;
        for (k, state) in states.into_iter().enumerate() {
            let index = unit.items[k];
            let p = &placed[index];
            let (item_top, from) = match state {
                ItemResume::Done => {
                    next.push(ItemResume::Done);
                    continue;
                }
                ItemResume::NotStarted => (top + (p.offset - consumed).max(0.0), None),
                ItemResume::Continue(token) => (top, Some(token)),
            };
            let space = f.available - item_top;
            // An item with nothing above it on the page has to place some content.
            let first = f.at_top && !placed_any && (rows || item_top <= top + EPSILON);
            let waits = |from: Option<BreakToken>| match from {
                Some(token) => ItemResume::Continue(token),
                None => ItemResume::NotStarted,
            };
            if space <= EPSILON && !first {
                next.push(waits(from));
                more = true;
                continue;
            }
            let laid = flow::layout_block(
                p.block,
                cb,
                BlockKind::Item {
                    width: p.width,
                    height: p.height,
                    at_least: true,
                },
                decorators,
                Some(Fragmentainer {
                    available: space,
                    at_top: first,
                    columns: f.columns,
                }),
                from.as_ref(),
            );
            // An item that cannot break, and does not fit, waits for the next page.
            let overflows =
                laid.break_token.is_none() && laid.fragment.rect.height > space + EPSILON;
            if laid.deferred || (overflows && !first) {
                next.push(waits(from));
                more = true;
                continue;
            }
            let mut fragment = laid.fragment;
            fragment.rect.x = p.fragment.rect.x;
            fragment.rect.y = item_top;
            bottom = bottom.max(fragment.rect.bottom());
            started = true;
            children.push((p.order, fragment));
            match laid.break_token {
                Some(token) => {
                    more = true;
                    next.push(ItemResume::Continue(token));
                }
                None => next.push(ItemResume::Done),
            }
        }
        if !started && !continued {
            // Nothing of the unit fits: it moves to the next page, and the container with
            // it if nothing came before.
            if placed_any {
                break_token = Some(BreakToken {
                    consumed: 0.0,
                    resume: Resume::Row { index: u },
                    forced: false,
                    side: None,
                });
                break;
            }
            if !f.at_top {
                return ContentLaid {
                    children: Vec::new(),
                    height: 0.0,
                    break_token: None,
                    deferred: true,
                };
            }
        }
        placed_any |= started;
        if more {
            y = bottom;
            break_token = Some(BreakToken {
                consumed: 0.0,
                resume: Resume::FlexPart {
                    index: u,
                    items: next,
                    consumed: consumed + (f.available - top).max(0.0),
                },
                forced: false,
                side: None,
            });
            break;
        }
        y = if continued { bottom } else { top + height };
    }
    // Space below the last unit, such as from `align-content`, stays with the last page.
    let height = if break_token.is_some() {
        y
    } else if start >= units.len() && resume.is_some() {
        0.0
    } else {
        let last_bottom = units.last().map_or(0.0, |u| u.bottom);
        y + (full_height - last_bottom).max(0.0)
    };
    children.sort_by_key(|(order, _)| *order);
    ContentLaid {
        children: children
            .into_iter()
            .map(|(_, fragment)| Fragment::Box(fragment))
            .collect(),
        height,
        break_token,
        deferred: false,
    }
}
