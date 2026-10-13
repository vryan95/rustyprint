//! Grid layout (CSS Grid Layout 2): placing items in the grid (§ 8), sizing its tracks
//! (§ 11), aligning items in their areas (§ 10), subgrids (§ 9), and breaking across pages
//! between rows (§ 12, through [`crate::units`]).
//!
//! Lines and tracks are numbered from zero once placement is done: line `i` is before
//! track `i`. Before that, line numbers are relative to the start of the explicit grid
//! and can be negative, for implicit tracks before it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    AlignPosition, BoxSizing, ContentAlignment, ContentDistribution, Direction, GridLine,
    GridTemplate, ItemAlignment, LengthPercentage, MaybeAuto, Overflow, OverflowPosition, Position,
    RepeatCount, Size, SubgridNames, TrackBreadth, TrackEntry, TrackSize,
};

use crate::boxes::{BlockBox, BlockContents};
use crate::display::Rect;
use crate::flow::{self, BlockKind, BreakToken, ContainingBlock, Fragmentainer, Laid};
use crate::fragment::{Fragment, OutOfFlow, Sides};
use crate::units::{ContentLaid, ContentSpace, Placed, Unit, paginate};

/// The axes, as indices into `[rows, columns]` pairs.
const ROWS: usize = 0;
const COLUMNS: usize = 1;

/// A track's minimum sizing function.
#[derive(Debug, Clone, Copy, PartialEq)]
enum MinSize {
    Fixed(f32),
    MinContent,
    MaxContent,
    Auto,
}

/// A track's maximum sizing function.
#[derive(Debug, Clone, Copy, PartialEq)]
enum MaxSize {
    Fixed(f32),
    MinContent,
    MaxContent,
    Auto,
    FitContent(f32),
    Flex(f32),
}

impl MaxSize {
    fn is_intrinsic(self) -> bool {
        matches!(
            self,
            MaxSize::MinContent | MaxSize::MaxContent | MaxSize::Auto | MaxSize::FitContent(_)
        )
    }
}

/// A grid track as it is sized (§ 11.4).
#[derive(Debug, Clone, Copy)]
struct Track {
    min: MinSize,
    max: MaxSize,
    base: f32,
    /// Infinite until something limits it.
    growth: f32,
    /// An empty track of `repeat(auto-fit)`, which takes no space.
    collapsed: bool,
}

impl Track {
    /// A track sized by `size`, with percentages of `basis`; without one they are `auto`.
    fn new(size: &TrackSize<LengthPercentage>, basis: Option<f32>) -> Track {
        let length = |value: &LengthPercentage| match (value, basis) {
            (LengthPercentage::Length(px), _) => Some(*px),
            (value, Some(basis)) => Some(value.resolve(basis)),
            (_, None) => None,
        };
        let min_of = |breadth: &TrackBreadth<LengthPercentage>| match breadth {
            TrackBreadth::Length(value) => length(value).map_or(MinSize::Auto, MinSize::Fixed),
            TrackBreadth::MinContent => MinSize::MinContent,
            TrackBreadth::MaxContent => MinSize::MaxContent,
            TrackBreadth::Auto | TrackBreadth::Flex(_) => MinSize::Auto,
        };
        let max_of = |breadth: &TrackBreadth<LengthPercentage>| match breadth {
            TrackBreadth::Length(value) => length(value).map_or(MaxSize::Auto, MaxSize::Fixed),
            TrackBreadth::Flex(fr) => MaxSize::Flex(*fr),
            TrackBreadth::MinContent => MaxSize::MinContent,
            TrackBreadth::MaxContent => MaxSize::MaxContent,
            TrackBreadth::Auto => MaxSize::Auto,
        };
        let (min, max) = match size {
            TrackSize::Breadth(breadth) => (min_of(breadth), max_of(breadth)),
            TrackSize::MinMax(min, max) => (min_of(min), max_of(max)),
            TrackSize::FitContent(limit) => (
                MinSize::Auto,
                length(limit).map_or(MaxSize::MaxContent, MaxSize::FitContent),
            ),
        };
        Track::sized(min, max)
    }

    fn sized(min: MinSize, max: MaxSize) -> Track {
        let base = match min {
            MinSize::Fixed(px) => px,
            _ => 0.0,
        };
        let growth = match max {
            MaxSize::Fixed(px) => px.max(base),
            _ => f32::INFINITY,
        };
        Track {
            min,
            max,
            base,
            growth,
            collapsed: false,
        }
    }

    /// A track of a fixed size, as a subgrid takes from its parent.
    fn fixed(size: f32) -> Track {
        Track::sized(MinSize::Fixed(size), MaxSize::Fixed(size))
    }

    fn flex(&self) -> Option<f32> {
        match self.max {
            MaxSize::Flex(fr) => Some(fr),
            _ => None,
        }
    }

    /// The growth limit, or the base size while it is infinite.
    fn limit(&self) -> f32 {
        if self.growth.is_finite() {
            self.growth
        } else {
            self.base
        }
    }
}

/// The explicit grid along one axis: its tracks and the names of its lines.
struct Explicit {
    tracks: Vec<TrackSize<LengthPercentage>>,
    /// The names of each line, one more than there are tracks.
    names: Vec<Vec<Arc<str>>>,
    /// The tracks that `repeat(auto-fit)` made, which collapse when empty.
    auto_fit: Option<Range<usize>>,
}

/// The size of a track that does not depend on content, for counting how many repeats fit.
fn fixed_size(size: &TrackSize<LengthPercentage>, basis: Option<f32>) -> f32 {
    let track = Track::new(size, basis);
    match (track.max, track.min) {
        (MaxSize::Fixed(px), _) => px,
        (_, MinSize::Fixed(px)) => px,
        _ => 0.0,
    }
}

/// Expands a template's repeats into the explicit grid along one axis (§ 7.2.3). A repeat
/// that fills the space repeats as often as fits in `available`, or else as often as
/// `minimum` needs, and once without either.
fn explicit_grid(
    template: &GridTemplate<LengthPercentage>,
    gap: f32,
    available: Option<f32>,
    minimum: Option<f32>,
) -> Explicit {
    let mut out = Explicit {
        tracks: Vec::new(),
        names: vec![Vec::new()],
        auto_fit: None,
    };
    let GridTemplate::Tracks(list) = template else {
        return out;
    };
    let basis = available;
    // How many times a repeat that fills the space repeats.
    let auto_count = list.entries.iter().find_map(|entry| match entry {
        TrackEntry::Repeat(repeat) if !matches!(repeat.count, RepeatCount::Count(_)) => {
            let others: Vec<&TrackSize<LengthPercentage>> = list
                .entries
                .iter()
                .flat_map(|e| match e {
                    TrackEntry::Track(size) => vec![size],
                    TrackEntry::Repeat(r) => match r.count {
                        RepeatCount::Count(n) => r
                            .tracks
                            .iter()
                            .cycle()
                            .take(r.tracks.len() * n as usize)
                            .collect(),
                        _ => Vec::new(),
                    },
                })
                .collect();
            let fixed: f32 = others.iter().map(|s| fixed_size(s, basis)).sum();
            let each: f32 = repeat.tracks.iter().map(|s| fixed_size(s, basis)).sum();
            let total = |n: usize| {
                let count = others.len() + n * repeat.tracks.len();
                fixed + each * n as f32 + gap * count.saturating_sub(1) as f32
            };
            let mut n = 1;
            if let Some(available) = available {
                while n < 1000 && total(n + 1) <= available + 0.01 {
                    n += 1;
                }
            } else if let Some(minimum) = minimum {
                while n < 1000 && total(n) < minimum - 0.01 {
                    n += 1;
                }
            }
            Some(n)
        }
        _ => None,
    });
    let push = |out: &mut Explicit, before: &[Arc<str>], size: &TrackSize<LengthPercentage>| {
        out.names
            .last_mut()
            .expect("a slot")
            .extend(before.iter().cloned());
        out.tracks.push(size.clone());
        out.names.push(Vec::new());
    };
    for (i, entry) in list.entries.iter().enumerate() {
        out.names
            .last_mut()
            .expect("a slot")
            .extend(list.names[i].iter().cloned());
        match entry {
            TrackEntry::Track(size) => {
                out.tracks.push(size.clone());
                out.names.push(Vec::new());
            }
            TrackEntry::Repeat(repeat) => {
                let times = match repeat.count {
                    RepeatCount::Count(n) => n as usize,
                    _ => auto_count.unwrap_or(1),
                };
                let first = out.tracks.len();
                for _ in 0..times {
                    for (k, size) in repeat.tracks.iter().enumerate() {
                        push(&mut out, &repeat.names[k], size);
                    }
                    out.names
                        .last_mut()
                        .expect("a slot")
                        .extend(repeat.names[repeat.tracks.len()].iter().cloned());
                }
                if repeat.count == RepeatCount::AutoFit {
                    out.auto_fit = Some(first..out.tracks.len());
                }
            }
        }
    }
    if let Some(last) = list.names.last() {
        out.names
            .last_mut()
            .expect("a slot")
            .extend(last.iter().cloned());
    }
    out
}

/// The names of a grid's lines along one axis, and where its explicit grid ends.
struct Lines<'a> {
    names: &'a [Vec<Arc<str>>],
    /// The number of explicit tracks: the explicit lines are `0..=end`.
    end: i32,
}

impl Lines<'_> {
    fn has(&self, line: i32, name: &str) -> bool {
        line >= 0 && line <= self.end && self.names[line as usize].iter().any(|n| &**n == name)
    }

    fn any(&self, name: &str) -> bool {
        (0..=self.end).any(|line| self.has(line, name))
    }

    /// The `k`th line named `name`, from the start if `k` is positive or the end if
    /// negative. Without that many, the implicit lines past the explicit grid count as
    /// named (§ 8.3.1).
    fn nth(&self, name: &str, k: i32) -> i32 {
        let named: Vec<i32> = (0..=self.end).filter(|&l| self.has(l, name)).collect();
        let count = named.len() as i32;
        if k > 0 {
            if k <= count {
                named[(k - 1) as usize]
            } else {
                self.end + (k - count)
            }
        } else if -k <= count {
            named[(count + k) as usize]
        } else {
            -(-k - count)
        }
    }

    /// The `k`th line after (or before) `from` named `name`, or any line without a name.
    fn search(&self, from: i32, name: Option<&str>, k: u32, forward: bool) -> i32 {
        let Some(name) = name else {
            return if forward {
                from + k as i32
            } else {
                from - k as i32
            };
        };
        let step = if forward { 1 } else { -1 };
        let mut line = from;
        let mut found = 0;
        loop {
            line += step;
            // Implicit lines count as having every name.
            if line < 0 || line > self.end || self.has(line, name) {
                found += 1;
                if found == k {
                    return line;
                }
            }
        }
    }
}

/// One side of a placement before auto-placement: a line, a span, or `auto`.
enum Side {
    Line(i32),
    Span(u32, Option<Arc<str>>),
    Auto,
}

fn side(value: &GridLine, start: bool, lines: &Lines<'_>) -> Side {
    match value {
        GridLine::Auto => Side::Auto,
        GridLine::Line {
            span: true,
            integer,
            name,
        } => Side::Span(integer.unwrap_or(1).max(1) as u32, name.clone()),
        GridLine::Line {
            integer: Some(i),
            name: None,
            ..
        } => Side::Line(if *i > 0 { i - 1 } else { lines.end + 1 + i }),
        GridLine::Line {
            integer: Some(i),
            name: Some(name),
            ..
        } => Side::Line(lines.nth(name, *i)),
        GridLine::Line {
            integer: None,
            name: Some(name),
            ..
        } => {
            // A name alone is the edge of the area of that name, or else the first line
            // of that name.
            let edge = format!("{name}-{}", if start { "start" } else { "end" });
            if lines.any(&edge) {
                Side::Line(lines.nth(&edge, 1))
            } else {
                Side::Line(lines.nth(name, 1))
            }
        }
        GridLine::Line {
            integer: None,
            name: None,
            ..
        } => Side::Auto,
    }
}

/// An item's placement along one axis: definite lines, or a span to auto-place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Span {
    Lines(i32, i32),
    Auto(u32),
}

impl Span {
    fn span(self) -> u32 {
        match self {
            Span::Lines(start, end) => (end - start) as u32,
            Span::Auto(span) => span,
        }
    }
}

/// Resolves an item's start and end along one axis (§ 8.3.1).
fn placement(start: &GridLine, end: &GridLine, lines: &Lines<'_>) -> Span {
    let named_span = |k: u32, name: &Option<Arc<str>>| if name.is_some() { 1 } else { k };
    match (side(start, true, lines), side(end, false, lines)) {
        (Side::Line(a), Side::Line(b)) => {
            if a == b {
                Span::Lines(a, a + 1)
            } else {
                Span::Lines(a.min(b), a.max(b))
            }
        }
        (Side::Line(a), Side::Span(k, name)) => {
            Span::Lines(a, lines.search(a, name.as_deref(), k, true))
        }
        (Side::Line(a), Side::Auto) => Span::Lines(a, a + 1),
        (Side::Span(k, name), Side::Line(b)) => {
            Span::Lines(lines.search(b, name.as_deref(), k, false), b)
        }
        (Side::Auto, Side::Line(b)) => Span::Lines(b - 1, b),
        (Side::Span(k, name), _) | (Side::Auto, Side::Span(k, name)) => {
            Span::Auto(named_span(k, &name))
        }
        (Side::Auto, Side::Auto) => Span::Auto(1),
    }
}

/// A start and end line along one axis.
type Lines2 = (i32, i32);

/// Places items in the grid (§ 8.5): first those with definite positions, then those
/// locked to a row (or column), then the rest, along `grid-auto-flow`. Returns each item's
/// rows and columns, from zero, and how far the lines moved to make room for implicit
/// tracks before the explicit grid.
fn auto_place(
    spans: &[[Span; 2]],
    explicit: [usize; 2],
    column_flow: bool,
    dense: bool,
) -> (Vec<[Range<usize>; 2]>, [i32; 2]) {
    let (major, minor) = if column_flow {
        (COLUMNS, ROWS)
    } else {
        (ROWS, COLUMNS)
    };
    let mut shift = [0i32; 2];
    for item in spans {
        for axis in [ROWS, COLUMNS] {
            if let Span::Lines(start, _) = item[axis] {
                shift[axis] = shift[axis].max(-start);
            }
        }
    }
    let definite = |span: Span, axis: usize| match span {
        Span::Lines(start, end) => Some((start + shift[axis], end + shift[axis])),
        Span::Auto(_) => None,
    };
    let mut minor_count = explicit[minor] as i32 + shift[minor];
    for item in spans {
        minor_count = minor_count.max(match definite(item[minor], minor) {
            Some((_, end)) => end,
            None => item[minor].span() as i32,
        });
    }
    let mut occupied: HashSet<(i32, i32)> = HashSet::new();
    let fits = |occupied: &HashSet<(i32, i32)>, a: (i32, i32), b: (i32, i32)| {
        (a.0..a.1).all(|i| (b.0..b.1).all(|j| !occupied.contains(&(i, j))))
    };
    let occupy = |occupied: &mut HashSet<(i32, i32)>, a: (i32, i32), b: (i32, i32)| {
        for i in a.0..a.1 {
            for j in b.0..b.1 {
                occupied.insert((i, j));
            }
        }
    };
    // Positions as (major, minor) line pairs.
    let mut placed: Vec<Option<(Lines2, Lines2)>> = vec![None; spans.len()];
    for (i, item) in spans.iter().enumerate() {
        if let (Some(a), Some(b)) = (definite(item[major], major), definite(item[minor], minor)) {
            occupy(&mut occupied, a, b);
            placed[i] = Some((a, b));
        }
    }
    // Items locked to a major line.
    let mut cursors: HashMap<i32, i32> = HashMap::new();
    for (i, item) in spans.iter().enumerate() {
        if placed[i].is_some() {
            continue;
        }
        let Some(a) = definite(item[major], major) else {
            continue;
        };
        let span = item[minor].span() as i32;
        let mut start = if dense {
            0
        } else {
            cursors.get(&a.0).copied().unwrap_or(0)
        };
        while !fits(&occupied, a, (start, start + span)) {
            start += 1;
        }
        let b = (start, start + span);
        occupy(&mut occupied, a, b);
        cursors.insert(a.0, b.1);
        minor_count = minor_count.max(b.1);
        placed[i] = Some((a, b));
    }
    // The rest, from the auto-placement cursor.
    let mut cursor = (0i32, 0i32);
    for (i, item) in spans.iter().enumerate() {
        if placed[i].is_some() {
            continue;
        }
        let major_span = item[major].span() as i32;
        if let Some(b) = definite(item[minor], minor) {
            if dense {
                cursor.0 = 0;
            } else if b.0 < cursor.1 {
                cursor.0 += 1;
            }
            cursor.1 = b.0;
            while !fits(&occupied, (cursor.0, cursor.0 + major_span), b) {
                cursor.0 += 1;
            }
            let a = (cursor.0, cursor.0 + major_span);
            occupy(&mut occupied, a, b);
            placed[i] = Some((a, b));
            continue;
        }
        let minor_span = item[minor].span() as i32;
        if dense {
            cursor = (0, 0);
        }
        loop {
            while cursor.1 + minor_span <= minor_count {
                let a = (cursor.0, cursor.0 + major_span);
                let b = (cursor.1, cursor.1 + minor_span);
                if fits(&occupied, a, b) {
                    occupy(&mut occupied, a, b);
                    placed[i] = Some((a, b));
                    break;
                }
                cursor.1 += 1;
            }
            if placed[i].is_some() {
                break;
            }
            cursor = (cursor.0 + 1, 0);
        }
    }
    let areas = placed
        .into_iter()
        .map(|place| {
            let (a, b) = place.expect("every item is placed");
            let range =
                |(start, end): (i32, i32)| start.max(0) as usize..end.max(start + 1) as usize;
            let mut out = [0..0, 0..0];
            out[major] = range(a);
            out[minor] = range(b);
            out
        })
        .collect();
    (areas, shift)
}

/// What an item contributes to the size of the tracks it spans (§ 11.5), as outer sizes.
#[derive(Debug, Clone, Copy, Default)]
struct Contribution {
    min_content: f32,
    max_content: f32,
    /// The size the item needs at least: its automatic minimum or `min-*` size.
    minimum: f32,
}

/// Shares `extra` among the tracks at `targets` (§ 11.5.1): equally, each up to `limit`,
/// then the rest equally among `overflow`, or all of them.
fn distribute(tracks: &mut [Track], targets: &[usize], overflow: &[usize], extra: f32, base: bool) {
    if extra <= 0.0 || targets.is_empty() {
        return;
    }
    let size = |t: &Track| if base { t.base } else { t.limit() };
    let limit = |t: &Track| {
        if base {
            t.limit().max(t.base)
        } else {
            f32::INFINITY
        }
    };
    let mut planned: Vec<f32> = targets.iter().map(|&i| size(&tracks[i])).collect();
    let mut left = extra;
    let mut open: Vec<usize> = (0..targets.len()).collect();
    while left > 0.001 && !open.is_empty() {
        let share = left / open.len() as f32;
        let mut next = Vec::new();
        for &k in &open {
            let room = limit(&tracks[targets[k]]) - planned[k];
            let add = share.min(room.max(0.0));
            planned[k] += add;
            left -= add;
            if add < share - 0.0001 {
                continue;
            }
            next.push(k);
        }
        if next.len() == open.len() {
            break;
        }
        open = next;
    }
    if left > 0.001 {
        // Past the limits, into the tracks that may grow further.
        let past: Vec<usize> = if overflow.is_empty() {
            (0..targets.len()).collect()
        } else {
            (0..targets.len())
                .filter(|&k| overflow.contains(&targets[k]))
                .collect()
        };
        let past = if past.is_empty() {
            (0..targets.len()).collect()
        } else {
            past
        };
        let share = left / past.len() as f32;
        for k in past {
            planned[k] += share;
        }
    }
    for (k, &i) in targets.iter().enumerate() {
        if base {
            tracks[i].base = tracks[i].base.max(planned[k]);
        } else {
            tracks[i].growth = planned[k].max(if tracks[i].growth.is_finite() {
                tracks[i].growth
            } else {
                0.0
            });
        }
    }
}

/// How the free space is known when sizing tracks.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Space {
    Definite(f32),
    /// Sizing for the grid's min-content size: no free space, and `fr` adds nothing.
    MinContent,
    /// Sizing for its max-content size: as much free space as tracks can take.
    MaxContent,
}

/// The track sizing algorithm along one axis (§ 11.3–§ 11.8), for items spanning
/// `spans` with their contributions.
fn size_tracks(
    tracks: &mut [Track],
    items: &[(Range<usize>, Contribution)],
    gap: f32,
    space: Space,
    stretch: bool,
) {
    let gaps = |range: &Range<usize>| gap * range.len().saturating_sub(1) as f32;
    let spans_flex = |range: &Range<usize>, tracks: &[Track]| {
        tracks[range.clone()].iter().any(|t| t.flex().is_some())
    };
    // Items spanning one track, then more, smaller spans first (§ 11.5).
    let mut order: Vec<usize> = (0..items.len())
        .filter(|&i| !spans_flex(&items[i].0, tracks))
        .collect();
    order.sort_by_key(|&i| items[i].0.len());
    for &i in &order {
        let (range, c) = &items[i];
        if range.is_empty() || tracks[range.clone()].iter().all(|t| t.collapsed) {
            continue;
        }
        if range.len() == 1 {
            let track = &mut tracks[range.start];
            match track.min {
                MinSize::MinContent => track.base = track.base.max(c.min_content),
                MinSize::MaxContent => track.base = track.base.max(c.max_content),
                MinSize::Auto => track.base = track.base.max(c.minimum),
                MinSize::Fixed(_) => {}
            }
            let grow = |track: &mut Track, size: f32| {
                track.growth = if track.growth.is_finite() {
                    track.growth.max(size)
                } else {
                    size
                };
            };
            match track.max {
                MaxSize::MinContent => grow(track, c.min_content),
                MaxSize::MaxContent | MaxSize::Auto => grow(track, c.max_content),
                MaxSize::FitContent(limit) => grow(
                    track,
                    c.max_content.min(limit).max(c.min_content.min(limit)),
                ),
                MaxSize::Fixed(_) | MaxSize::Flex(_) => {}
            }
            if track.growth < track.base {
                track.growth = track.base;
            }
            continue;
        }
        let indices: Vec<usize> = range.clone().filter(|&t| !tracks[t].collapsed).collect();
        let intrinsic_min: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&t| !matches!(tracks[t].min, MinSize::Fixed(_)))
            .collect();
        let growable: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&t| matches!(tracks[t].max, MaxSize::MaxContent | MaxSize::Auto))
            .collect();
        let base_sum: f32 = indices.iter().map(|&t| tracks[t].base).sum::<f32>() + gaps(range);
        distribute(
            tracks,
            &intrinsic_min,
            &growable,
            c.minimum - base_sum,
            true,
        );
        for &t in &indices {
            if tracks[t].growth < tracks[t].base {
                tracks[t].growth = tracks[t].base;
            }
        }
        let intrinsic_max: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&t| tracks[t].max.is_intrinsic())
            .collect();
        let limit_sum: f32 = indices.iter().map(|&t| tracks[t].limit()).sum::<f32>() + gaps(range);
        distribute(
            tracks,
            &intrinsic_max,
            &[],
            c.max_content - limit_sum,
            false,
        );
    }
    // Items spanning flexible tracks grow those tracks' base sizes (§ 11.5 step 4).
    let flex_items: Vec<usize> = (0..items.len())
        .filter(|&i| spans_flex(&items[i].0, tracks))
        .collect();
    for i in flex_items {
        let (range, c) = &items[i];
        let flexible: Vec<usize> = range
            .clone()
            .filter(|&t| tracks[t].flex().is_some() && tracks[t].min == MinSize::Auto)
            .collect();
        let base_sum: f32 = tracks[range.clone()].iter().map(|t| t.base).sum::<f32>() + gaps(range);
        distribute(tracks, &flexible, &flexible, c.minimum - base_sum, true);
    }
    for track in tracks.iter_mut() {
        if !track.growth.is_finite() {
            track.growth = track.base;
        }
        if track.collapsed {
            track.base = 0.0;
            track.growth = 0.0;
        }
    }

    let total_gaps = gap
        * tracks
            .iter()
            .filter(|t| !t.collapsed)
            .count()
            .saturating_sub(1) as f32;
    let used = |tracks: &[Track]| tracks.iter().map(|t| t.base).sum::<f32>() + total_gaps;

    // Maximize tracks (§ 11.6).
    match space {
        Space::Definite(available) => {
            let free = available - used(tracks);
            if free > 0.0 {
                let all: Vec<usize> = (0..tracks.len())
                    .filter(|&t| !tracks[t].collapsed)
                    .collect();
                let mut planned: Vec<f32> = tracks.iter().map(|t| t.base).collect();
                let mut left = free;
                let mut open = all;
                while left > 0.001 && !open.is_empty() {
                    let share = left / open.len() as f32;
                    let mut next = Vec::new();
                    for &t in &open {
                        let add = share.min((tracks[t].growth - planned[t]).max(0.0));
                        planned[t] += add;
                        left -= add;
                        if add >= share - 0.0001 {
                            next.push(t);
                        }
                    }
                    if next.len() == open.len() {
                        break;
                    }
                    open = next;
                }
                for (track, size) in tracks.iter_mut().zip(planned) {
                    track.base = size;
                }
            }
        }
        Space::MaxContent => {
            for track in tracks.iter_mut() {
                track.base = track.base.max(track.growth);
            }
        }
        Space::MinContent => {}
    }

    // Expand flexible tracks (§ 11.7).
    if tracks.iter().any(|t| t.flex().is_some()) {
        let fr = match space {
            Space::Definite(available) => find_fr(tracks, 0..tracks.len(), available - total_gaps),
            Space::MinContent => 0.0,
            Space::MaxContent => {
                let mut fr = 0.0f32;
                for track in tracks.iter() {
                    if let Some(flex) = track.flex() {
                        fr = fr.max(if flex > 1.0 {
                            track.base / flex
                        } else {
                            track.base
                        });
                    }
                }
                for (range, c) in items.iter().filter(|(r, _)| spans_flex(r, tracks)) {
                    let space = c.max_content - gaps(range);
                    fr = fr.max(find_fr(tracks, range.clone(), space));
                }
                fr
            }
        };
        for track in tracks.iter_mut() {
            if let Some(flex) = track.flex() {
                track.base = track.base.max(fr * flex);
            }
        }
    }

    // Stretch `auto` tracks (§ 11.8).
    if stretch && let Space::Definite(available) = space {
        let free = available - used(tracks);
        let autos: Vec<usize> = (0..tracks.len())
            .filter(|&t| tracks[t].max == MaxSize::Auto && !tracks[t].collapsed)
            .collect();
        if free > 0.0 && !autos.is_empty() {
            let share = free / autos.len() as f32;
            for t in autos {
                tracks[t].base += share;
            }
        }
    }
}

/// The size of `1fr` when the tracks in `range` fill `space` (§ 11.7.1): flexible tracks
/// whose base size is more than their share keep it, and the rest share what is left.
fn find_fr(tracks: &[Track], range: Range<usize>, space: f32) -> f32 {
    let mut inflexible: HashSet<usize> = HashSet::new();
    loop {
        let mut left = space;
        let mut flex_sum = 0.0;
        for t in range.clone() {
            match tracks[t].flex() {
                Some(flex) if !inflexible.contains(&t) => flex_sum += flex,
                _ => left -= tracks[t].base,
            }
        }
        let fr = left.max(0.0) / flex_sum.max(1.0);
        let grown: Vec<usize> = range
            .clone()
            .filter(|&t| {
                !inflexible.contains(&t)
                    && tracks[t]
                        .flex()
                        .is_some_and(|flex| fr * flex < tracks[t].base)
            })
            .collect();
        if grown.is_empty() {
            return fr;
        }
        inflexible.extend(grown);
    }
}

/// Where each track starts and ends, with the gaps and the space `*-content` puts before
/// and between them.
fn positions(tracks: &[Track], gap: f32, before: f32, between: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(tracks.len());
    let mut at = before;
    for (i, track) in tracks.iter().enumerate() {
        out.push((at, at + track.base));
        at += track.base;
        // Collapsed tracks lose the gap after them.
        if i + 1 < tracks.len() && !track.collapsed {
            at += gap + between;
        }
    }
    out
}

/// The tracks of a parent grid that a subgrid spans along one axis: their edges relative to
/// the subgrid's content box, and the names of its lines there.
#[derive(Debug, Clone)]
struct Inherited {
    edges: Vec<(f32, f32)>,
    names: Vec<Vec<Arc<str>>>,
}

thread_local! {
    /// The parent tracks of the subgrid being laid out, rows then columns, set by its
    /// parent around the layout of the subgrid.
    static SUBGRID: RefCell<[Option<Inherited>; 2]> = const { RefCell::new([None, None]) };
}

/// Runs `f` with the tracks a subgrid inherits from its parent.
fn with_subgrid<T>(inherited: [Option<Inherited>; 2], f: impl FnOnce() -> T) -> T {
    let saved = SUBGRID.with(|s| std::mem::replace(&mut *s.borrow_mut(), inherited));
    let out = f();
    SUBGRID.with(|s| *s.borrow_mut() = saved);
    out
}

/// A grid item as layout sizes and places it.
struct Item<'a> {
    block: &'a BlockBox,
    style: &'a ComputedValues,
    /// Its rows and columns.
    area: [Range<usize>; 2],
    padding: Sides,
    border: Sides,
    /// Margins in the order top, right, bottom, left; `None` for `auto`.
    margin: [Option<f32>; 4],
    /// The content-box width it lays out at, and its height if that is not `auto`.
    width: f32,
    height: Option<f32>,
    laid: Option<Laid>,
    /// Its border box in the content box, in the container's inline direction.
    x: f32,
    y: f32,
}

impl Item<'_> {
    fn horizontal(&self) -> f32 {
        self.padding.horizontal() + self.border.horizontal()
    }

    /// Vertical padding and borders, with the extra height of a fieldset's legend.
    fn vertical(&self) -> f32 {
        let legend = self
            .laid
            .as_ref()
            .and_then(|laid| laid.fragment.legend)
            .map_or(0.0, |gap| gap.y * 2.0);
        self.padding.vertical() + self.border.vertical() + legend
    }

    fn margins(&self, axis: usize) -> f32 {
        let m = |i: usize| self.margin[i].unwrap_or(0.0);
        if axis == COLUMNS {
            m(1) + m(3)
        } else {
            m(0) + m(2)
        }
    }

    /// Whether `normal` alignment stretches it: it has no preferred aspect ratio.
    fn stretches_normally(&self) -> bool {
        self.style.aspect_ratio.value().is_none()
            && !self
                .block
                .replaced
                .as_ref()
                .is_some_and(|r| r.size.is_some_and(|(w, h)| w > 0.0 && h > 0.0))
    }

    fn stretches(&self, alignment: ItemAlignment, axis: usize) -> bool {
        let auto_margins = if axis == COLUMNS {
            self.margin[1].is_none() || self.margin[3].is_none()
        } else {
            self.margin[0].is_none() || self.margin[2].is_none()
        };
        !auto_margins
            && match alignment {
                ItemAlignment::Stretch => true,
                ItemAlignment::Normal => self.stretches_normally(),
                _ => false,
            }
    }
}

/// A box's width or height property as a content-box size, when it does not need a
/// percentage of an unknown size.
fn content_size(
    block: &BlockBox,
    size: &Size<LengthPercentage>,
    basis: Option<f32>,
    pb: f32,
    width: bool,
) -> Option<f32> {
    let style = &block.style;
    if width {
        let cb = ContainingBlock {
            width: basis?,
            height: None,
            rtl: false,
        };
        return match size {
            Size::Value(LengthPercentage::Length(_))
            | Size::MinContent
            | Size::MaxContent
            | Size::FitContent(_) => flow::content_width(block, size, cb, pb),
            Size::Value(_) => basis.and_then(|_| flow::content_width(block, size, cb, pb)),
            _ => None,
        };
    }
    let Size::Value(value) = size else {
        return None;
    };
    let px = match (value, basis) {
        (LengthPercentage::Length(px), _) => *px,
        (value, Some(basis)) => value.resolve(basis),
        (_, None) => return None,
    };
    Some(if style.box_sizing == BoxSizing::BorderBox {
        (px - pb).max(0.0)
    } else {
        px
    })
}

/// Whether a box's automatic minimum size comes from its content: it does not scroll.
fn content_minimum(style: &ComputedValues) -> bool {
    [style.overflow_x, style.overflow_y]
        .iter()
        .all(|o| matches!(o, Overflow::Visible | Overflow::Clip))
}

/// What an item contributes to the widths of its columns.
fn column_contribution(item: &Item<'_>) -> Contribution {
    let block = item.block;
    let style = item.style;
    let pb = item.horizontal();
    let outer = pb + item.margins(COLUMNS);
    let fixed = |size: &Size<LengthPercentage>| match size {
        Size::Value(LengthPercentage::Length(_)) | Size::MinContent | Size::MaxContent => {
            content_size(block, size, Some(0.0), pb, true)
        }
        _ => None,
    };
    let (min_content, max_content) = flow::intrinsic_content(block);
    let width = fixed(&style.width);
    let min = fixed(&style.min_width).unwrap_or(0.0);
    let max = fixed(&style.max_width).unwrap_or(f32::INFINITY);
    let clamp = |w: f32| w.min(max).max(min);
    let min_c = clamp(width.unwrap_or(min_content));
    let max_c = clamp(width.unwrap_or(max_content));
    let minimum = if matches!(style.min_width, Size::Auto) {
        if content_minimum(style) {
            clamp(width.map_or(min_content, |w| w.min(min_content)))
        } else {
            0.0
        }
    } else {
        min
    };
    Contribution {
        min_content: min_c + outer,
        max_content: max_c + outer,
        minimum: minimum + outer,
    }
}

/// How grid items are aligned: `justify-self` and `align-self`, with `auto` from the
/// container.
fn alignments(item: &ComputedValues, container: &ComputedValues) -> (ItemAlignment, ItemAlignment) {
    (
        item.justify_self.resolve(container.justify_items),
        item.align_self.resolve(container.align_items),
    )
}

/// The share of the free space before a box for an alignment `position`: 0 at the start,
/// 1 at the end. `left_is_start` says whether the axis starts on the left; along rows it
/// is `None`, where left and right mean start.
fn fraction(position: AlignPosition, left_is_start: Option<bool>, own_flipped: bool) -> f32 {
    match position {
        AlignPosition::Center => 0.5,
        AlignPosition::Start | AlignPosition::FlexStart => 0.0,
        AlignPosition::End | AlignPosition::FlexEnd => 1.0,
        AlignPosition::SelfStart => {
            if own_flipped {
                1.0
            } else {
                0.0
            }
        }
        AlignPosition::SelfEnd => {
            if own_flipped {
                0.0
            } else {
                1.0
            }
        }
        AlignPosition::Left => match left_is_start {
            Some(false) => 1.0,
            _ => 0.0,
        },
        AlignPosition::Right => match left_is_start {
            Some(false) => 0.0,
            None => 0.0,
            _ => 1.0,
        },
    }
}

/// Where a box goes in `free` space along an axis for an alignment, before auto margins.
fn self_offset(
    alignment: ItemAlignment,
    free: f32,
    left_is_start: Option<bool>,
    flipped: bool,
) -> f32 {
    match alignment {
        ItemAlignment::Position { position, overflow } => {
            if overflow == OverflowPosition::Safe && free < 0.0 {
                0.0
            } else {
                free * fraction(position, left_is_start, flipped)
            }
        }
        ItemAlignment::Legacy(position) => free * fraction(position, left_is_start, flipped),
        ItemAlignment::AnchorCenter => free / 2.0,
        // `last baseline` falls back to `safe end` without a baseline group.
        ItemAlignment::Baseline { last: true } => free.max(0.0),
        _ => 0.0,
    }
}

/// The space before the first track and between tracks for `*-content` (§ 10.5).
fn content_offsets(
    value: ContentAlignment,
    free: f32,
    count: usize,
    left_is_start: Option<bool>,
) -> (f32, f32) {
    match value {
        ContentAlignment::Normal
        | ContentAlignment::Baseline { .. }
        | ContentAlignment::Distribution(ContentDistribution::Stretch) => (0.0, 0.0),
        ContentAlignment::Distribution(distribution) => {
            if free <= 0.0 || count == 0 {
                return match distribution {
                    ContentDistribution::SpaceBetween => (0.0, 0.0),
                    _ if free < 0.0 => (0.0, 0.0),
                    _ => (free / 2.0, 0.0),
                };
            }
            let n = count as f32;
            match distribution {
                ContentDistribution::SpaceBetween if count == 1 => (0.0, 0.0),
                ContentDistribution::SpaceBetween => (0.0, free / (n - 1.0)),
                ContentDistribution::SpaceAround => (free / n / 2.0, free / n),
                ContentDistribution::SpaceEvenly => (free / (n + 1.0), free / (n + 1.0)),
                ContentDistribution::Stretch => (0.0, 0.0),
            }
        }
        ContentAlignment::Position { position, overflow } => {
            if overflow == OverflowPosition::Safe && free < 0.0 {
                (0.0, 0.0)
            } else {
                (free * fraction(position, left_is_start, false), 0.0)
            }
        }
    }
}

/// A grid container's items placed in its tracks, before the rows are sized.
struct Plan<'a> {
    items: Vec<Item<'a>>,
    /// Rows, then columns.
    tracks: [Vec<Track>; 2],
    /// Edges inherited from a parent grid, along the axes the grid is a subgrid in.
    inherited: [Option<Inherited>; 2],
    gaps: [f32; 2],
    /// The names of each line, rows then columns, for the subgrids among the items.
    names: [Vec<Vec<Arc<str>>>; 2],
}

/// Places the in-flow children of a grid container in its grid, and makes its tracks.
fn plan<'a>(
    container: &'a BlockBox,
    children: &[&'a BlockBox],
    space: ContentSpace,
    inherited: [Option<Inherited>; 2],
) -> Plan<'a> {
    let style = &*container.style;
    let gap = |value: &Option<LengthPercentage>, basis: Option<f32>| match (value, basis) {
        (None, _) => 0.0,
        (Some(LengthPercentage::Length(px)), _) => *px,
        (Some(value), Some(basis)) => value.resolve(basis),
        (Some(_), None) => 0.0,
    };
    let gaps = [
        gap(&style.row_gap, space.height),
        gap(&style.column_gap, Some(space.width)),
    ];
    let areas = &style.grid_template_areas;
    let templates = [&style.grid_template_rows, &style.grid_template_columns];
    let available = [space.height.or(space.max_height), Some(space.width)];
    let minimum = [Some(space.min_height), None];
    let mut explicit: Vec<Explicit> = (0..2)
        .map(|axis| match &inherited[axis] {
            Some(parent) => {
                // A subgrid's own line names go on its lines in order (§ 9.2).
                let mut names = parent.names.clone();
                if let GridTemplate::Subgrid(list) = templates[axis] {
                    let mut line = 0;
                    for entry in list {
                        match entry {
                            SubgridNames::Names(own) => {
                                if let Some(slot) = names.get_mut(line) {
                                    slot.extend(own.iter().cloned());
                                }
                                line += 1;
                            }
                            SubgridNames::Repeat(count, sets) => {
                                let times = match count {
                                    RepeatCount::Count(n) => *n as usize,
                                    _ => (names.len().saturating_sub(line)) / sets.len().max(1),
                                };
                                for _ in 0..times {
                                    for set in sets {
                                        if let Some(slot) = names.get_mut(line) {
                                            slot.extend(set.iter().cloned());
                                        }
                                        line += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                Explicit {
                    tracks: Vec::new(),
                    names,
                    auto_fit: None,
                }
            }
            None => explicit_grid(templates[axis], gaps[axis], available[axis], minimum[axis]),
        })
        .collect();
    // Areas add tracks to the explicit grid, and name the lines around them.
    for axis in [ROWS, COLUMNS] {
        if inherited[axis].is_some() {
            continue;
        }
        let count = if axis == ROWS {
            areas.rows.len()
        } else {
            areas.columns()
        };
        let auto = if axis == ROWS {
            &style.grid_auto_rows
        } else {
            &style.grid_auto_columns
        };
        let e = &mut explicit[axis];
        let template = e.tracks.len();
        while e.tracks.len() < count {
            let size = auto[(e.tracks.len() - template) % auto.len().max(1)].clone();
            e.tracks.push(size);
            e.names.push(Vec::new());
        }
        for area in areas.areas.iter() {
            let range = if axis == ROWS {
                &area.rows
            } else {
                &area.columns
            };
            e.names[range.start].push(Arc::from(format!("{}-start", area.name)));
            e.names[range.end].push(Arc::from(format!("{}-end", area.name)));
        }
    }
    let explicit_counts = [0, 1].map(|axis| match &inherited[axis] {
        Some(parent) => parent.edges.len(),
        None => explicit[axis].tracks.len(),
    });

    let mut in_flow = children.to_vec();
    in_flow.sort_by_key(|child| child.style.order);
    let spans: Vec<[Span; 2]> = in_flow
        .iter()
        .map(|child| {
            let s = &child.style;
            let lines = |axis: usize| Lines {
                names: &explicit[axis].names,
                end: explicit_counts[axis] as i32,
            };
            let mut spans = [
                placement(&s.grid_row_start, &s.grid_row_end, &lines(ROWS)),
                placement(&s.grid_column_start, &s.grid_column_end, &lines(COLUMNS)),
            ];
            // A subgrid has no implicit tracks along the axes it inherits (§ 9.3).
            for axis in [ROWS, COLUMNS] {
                if inherited[axis].is_some() {
                    let n = explicit_counts[axis] as i32;
                    spans[axis] = match spans[axis] {
                        Span::Lines(a, b) => {
                            let a = a.clamp(0, (n - 1).max(0));
                            Span::Lines(a, b.clamp(a + 1, n.max(a + 1)))
                        }
                        Span::Auto(k) => Span::Auto(k.min(n.max(1) as u32)),
                    };
                }
            }
            spans
        })
        .collect();
    let flow = style.grid_auto_flow;
    let (areas_of, shift) = auto_place(&spans, explicit_counts, flow.column, flow.dense);

    // Tracks: explicit ones from the template, implicit ones from `grid-auto-*`.
    let count = |axis: usize| {
        let ends = areas_of.iter().map(|a| a[axis].end).max().unwrap_or(0);
        ends.max(explicit_counts[axis] + shift[axis] as usize)
    };
    let tracks: [Vec<Track>; 2] = [ROWS, COLUMNS].map(|axis| {
        if let Some(parent) = &inherited[axis] {
            return parent
                .edges
                .iter()
                .map(|(start, end)| Track::fixed(end - start))
                .collect();
        }
        let auto = if axis == ROWS {
            &style.grid_auto_rows
        } else {
            &style.grid_auto_columns
        };
        let first = shift[axis] as usize;
        let e = &explicit[axis];
        let basis = available[axis];
        (0..count(axis))
            .map(|i| {
                let size = if i >= first && i - first < e.tracks.len() {
                    e.tracks[i - first].clone()
                } else if i >= first {
                    let k = i - first - e.tracks.len();
                    auto[k % auto.len().max(1)].clone()
                } else {
                    let k = first - i - 1;
                    auto[auto.len() - 1 - k % auto.len().max(1)].clone()
                };
                let mut track = Track::new(&size, basis);
                if let Some(fit) = &e.auto_fit {
                    let index = i as isize - first as isize;
                    if index >= fit.start as isize
                        && index < fit.end as isize
                        && !areas_of.iter().any(|a| a[axis].contains(&i))
                    {
                        track = Track::fixed(0.0);
                        track.collapsed = true;
                    }
                }
                track
            })
            .collect()
    });

    let items = in_flow
        .into_iter()
        .zip(areas_of)
        .map(|(block, area)| {
            let style = &*block.style;
            let (padding, border) = flow::padding_and_border(style, space.width);
            let margin = |value: &MaybeAuto<LengthPercentage>| match value {
                MaybeAuto::Auto => None,
                MaybeAuto::Value(value) => Some(value.resolve(space.width)),
            };
            Item {
                block,
                style,
                area,
                padding,
                border,
                margin: [
                    margin(&style.margin_top),
                    margin(&style.margin_right),
                    margin(&style.margin_bottom),
                    margin(&style.margin_left),
                ],
                width: 0.0,
                height: None,
                laid: None,
                x: 0.0,
                y: 0.0,
            }
        })
        .collect();
    let names = [ROWS, COLUMNS].map(|axis| {
        let first = shift[axis] as usize;
        (0..=tracks[axis].len())
            .map(|line| {
                line.checked_sub(first)
                    .and_then(|l| explicit[axis].names.get(l))
                    .cloned()
                    .unwrap_or_default()
            })
            .collect()
    });
    let mut plan = Plan {
        items,
        tracks,
        inherited,
        gaps,
        names,
    };
    trim_margins(style, &mut plan);
    plan
}

/// Applies the container's `margin-trim` to items against its edges: those in the first
/// and last rows and columns.
fn trim_margins(style: &ComputedValues, plan: &mut Plan<'_>) {
    let trim = style.margin_trim;
    let rtl = style.direction == Direction::Rtl;
    let (left, right) = if rtl {
        (trim.inline_end, trim.inline_start)
    } else {
        (trim.inline_start, trim.inline_end)
    };
    let rows = plan.tracks[ROWS].len();
    let columns = plan.tracks[COLUMNS].len();
    // Columns run from the right in a right-to-left grid.
    let (first_side, last_side) = if rtl { (1, 3) } else { (3, 1) };
    for item in &mut plan.items {
        if trim.block_start && item.area[ROWS].start == 0 {
            item.margin[0] = Some(0.0);
        }
        if trim.block_end && item.area[ROWS].end == rows {
            item.margin[2] = Some(0.0);
        }
        let first = if rtl { right } else { left };
        let last = if rtl { left } else { right };
        if first && item.area[COLUMNS].start == 0 {
            item.margin[first_side] = Some(0.0);
        }
        if last && item.area[COLUMNS].end == columns {
            item.margin[last_side] = Some(0.0);
        }
    }
}

/// The span of a range of tracks: from the start of the first to the end of the last.
fn extent(edges: &[(f32, f32)], range: &Range<usize>) -> (f32, f32) {
    match (
        edges.get(range.start),
        edges.get(range.end.saturating_sub(1)),
    ) {
        (Some(first), Some(last)) => (first.0, last.1.max(first.0)),
        _ => (0.0, 0.0),
    }
}

/// The tracks a subgrid item inherits along the axes it subgrids, relative to its content
/// box at `origin` in the parent's content box.
fn inherited_for(
    item: &Item<'_>,
    edges: [&[(f32, f32)]; 2],
    origin: [f32; 2],
    names: [&[Vec<Arc<str>>]; 2],
    rtl: bool,
    item_width: f32,
) -> [Option<Inherited>; 2] {
    let display_grid = matches!(item.block.contents, BlockContents::Grid(_));
    let templates = [
        &item.style.grid_template_rows,
        &item.style.grid_template_columns,
    ];
    [ROWS, COLUMNS].map(|axis| {
        if !display_grid || !matches!(templates[axis], GridTemplate::Subgrid(_)) {
            return None;
        }
        if edges[axis].is_empty() {
            return None;
        }
        let range = item.area[axis].clone();
        let mut edges: Vec<(f32, f32)> = edges[axis][range.clone()]
            .iter()
            .map(|(start, end)| (start - origin[axis], end - origin[axis]))
            .collect();
        // A right-to-left parent's columns run the other way in the subgrid's box only if
        // the subgrid is left-to-right; both measure from their own inline start here.
        if axis == COLUMNS && rtl && item.style.direction != Direction::Rtl {
            edges = edges
                .iter()
                .rev()
                .map(|(start, end)| (item_width - end, item_width - start))
                .collect();
        }
        let names = (range.start..=range.end)
            .map(|line| names[axis].get(line).cloned().unwrap_or_default())
            .collect();
        Some(Inherited { edges, names })
    })
}

/// Lays out the content of a grid container: its items placed in its sized tracks.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout(
    container: &BlockBox,
    children: &[BlockBox],
    space: ContentSpace,
    decorators: &[Arc<ComputedValues>],
    fragmentainer: Option<Fragmentainer>,
    resume: Option<&BreakToken>,
) -> ContentLaid {
    let style = &*container.style;
    let rtl = style.direction == Direction::Rtl;
    let inherited = SUBGRID.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let (in_flow, placeholders): (Vec<&BlockBox>, Vec<&BlockBox>) = children
        .iter()
        .partition(|child| !matches!(child.contents, BlockContents::OutOfFlow(_)));
    let mut plan = plan(container, &in_flow, space, inherited);
    let [row_gap, column_gap] = plan.gaps;

    // Columns: sized by the items' widths (§ 11.1).
    let column_items: Vec<(Range<usize>, Contribution)> = plan
        .items
        .iter()
        .map(|item| (item.area[COLUMNS].clone(), column_contribution(item)))
        .collect();
    let stretch_columns = matches!(
        style.justify_content,
        ContentAlignment::Normal | ContentAlignment::Distribution(ContentDistribution::Stretch)
    );
    let column_edges: Vec<(f32, f32)> = match &plan.inherited[COLUMNS] {
        Some(parent) => parent.edges.clone(),
        None => {
            size_tracks(
                &mut plan.tracks[COLUMNS],
                &column_items,
                column_gap,
                Space::Definite(space.width),
                stretch_columns,
            );
            let used = positions(&plan.tracks[COLUMNS], column_gap, 0.0, 0.0)
                .last()
                .map_or(0.0, |edge| edge.1);
            let (before, between) = content_offsets(
                style.justify_content,
                space.width - used,
                plan.tracks[COLUMNS].len(),
                Some(true),
            );
            positions(&plan.tracks[COLUMNS], column_gap, before, between)
        }
    };

    // Each item's width in its area, then its height there, for the rows (§ 11.1).
    for item in &mut plan.items {
        let (start, end) = extent(&column_edges, &item.area[COLUMNS]);
        let area = end - start;
        let (justify, _) = alignments(item.style, style);
        let pb = item.horizontal();
        let room = (area - item.margins(COLUMNS) - pb).max(0.0);
        let limit =
            |size: &Size<LengthPercentage>| content_size(item.block, size, Some(area), pb, true);
        let min = limit(&item.style.min_width).unwrap_or(0.0);
        let max = limit(&item.style.max_width).unwrap_or(f32::INFINITY);
        item.width = match limit(&item.style.width) {
            Some(width) => width,
            None if item.stretches(justify, COLUMNS) => room,
            None => {
                let (min_content, max_content) = flow::intrinsic_content(item.block);
                room.min(max_content).max(min_content)
            }
        }
        .min(max)
        .max(min);
    }
    let row_cb = |item: &Item<'_>, height: Option<f32>| {
        let (start, end) = extent(&column_edges, &item.area[COLUMNS]);
        ContainingBlock {
            width: end - start,
            height,
            rtl,
        }
    };
    let mut row_items = Vec::with_capacity(plan.items.len());
    for item in &mut plan.items {
        let height = content_size(
            item.block,
            &item.style.height,
            None,
            item.padding.vertical() + item.border.vertical(),
            false,
        );
        let cb = row_cb(item, None);
        let columns = plan.inherited[COLUMNS].is_none().then_some(&column_edges);
        let names = [&plan.names[ROWS][..], &plan.names[COLUMNS][..]];
        let laid = lay_item(
            item,
            cb,
            (item.width, height),
            decorators,
            [None, columns],
            names,
            rtl,
            None,
        );
        item.laid = Some(laid);
        let outer = item.laid.as_ref().map_or(0.0, |l| l.fragment.rect.height) + item.margins(ROWS);
        let min = content_size(
            item.block,
            &item.style.min_height,
            None,
            item.padding.vertical() + item.border.vertical(),
            false,
        );
        let minimum = match min {
            Some(min) => min + item.vertical() + item.margins(ROWS),
            None if content_minimum(item.style) => outer,
            None => item.vertical() + item.margins(ROWS),
        };
        row_items.push((
            item.area[ROWS].clone(),
            Contribution {
                min_content: outer,
                max_content: outer,
                minimum,
            },
        ));
    }

    // Rows: sized by the items' heights, in the container's height if it has one.
    let stretch_rows = matches!(
        style.align_content,
        ContentAlignment::Normal | ContentAlignment::Distribution(ContentDistribution::Stretch)
    );
    let row_edges: Vec<(f32, f32)> = match &plan.inherited[ROWS] {
        Some(parent) => parent.edges.clone(),
        None => {
            let row_space = match space.height {
                Some(height) => Space::Definite(height),
                None => Space::MaxContent,
            };
            size_tracks(
                &mut plan.tracks[ROWS],
                &row_items,
                row_gap,
                row_space,
                stretch_rows,
            );
            let used = positions(&plan.tracks[ROWS], row_gap, 0.0, 0.0)
                .last()
                .map_or(0.0, |edge| edge.1);
            let height = space.height.unwrap_or_else(|| {
                used.min(space.max_height.unwrap_or(f32::INFINITY))
                    .max(space.min_height)
            });
            // Rows stretch into a `min-height` too.
            if space.height.is_none() && stretch_rows && height > used + 0.01 {
                size_tracks(
                    &mut plan.tracks[ROWS],
                    &row_items,
                    row_gap,
                    Space::Definite(height),
                    true,
                );
            }
            let used = positions(&plan.tracks[ROWS], row_gap, 0.0, 0.0)
                .last()
                .map_or(0.0, |edge| edge.1);
            let (before, between) = content_offsets(
                style.align_content,
                height - used,
                plan.tracks[ROWS].len(),
                None,
            );
            positions(&plan.tracks[ROWS], row_gap, before, between)
        }
    };
    let rows_end = row_edges.last().map_or(0.0, |edge| edge.1);
    let height = space.height.unwrap_or_else(|| {
        rows_end
            .min(space.max_height.unwrap_or(f32::INFINITY))
            .max(space.min_height)
    });

    // Items in their areas (§ 10.3–§ 10.4), with baselines shared along each row.
    for item in &mut plan.items {
        let (top, bottom) = extent(&row_edges, &item.area[ROWS]);
        let area = bottom - top;
        let (_, align) = alignments(item.style, style);
        let pb = item.padding.vertical() + item.border.vertical();
        let definite = content_size(item.block, &item.style.height, Some(area), pb, false);
        let min =
            content_size(item.block, &item.style.min_height, Some(area), pb, false).unwrap_or(0.0);
        let max = content_size(item.block, &item.style.max_height, Some(area), pb, false)
            .unwrap_or(f32::INFINITY);
        item.height = definite.or_else(|| {
            item.stretches(align, ROWS).then(|| {
                (area - item.margins(ROWS) - item.vertical())
                    .min(max)
                    .max(min)
                    .max(0.0)
            })
        });
        let cb = row_cb(item, Some(area));
        let (left, right) = extent(&column_edges, &item.area[COLUMNS]);
        let origin = [
            top + item.margin[0].unwrap_or(0.0) + item.border.top + item.padding.top,
            left + item.margin[if rtl { 1 } else { 3 }].unwrap_or(0.0)
                + if rtl {
                    item.border.right + item.padding.right
                } else {
                    item.border.left + item.padding.left
                },
        ];
        let _ = right;
        let columns = plan.inherited[COLUMNS].is_none().then_some(&column_edges);
        let names = [&plan.names[ROWS][..], &plan.names[COLUMNS][..]];
        let laid = lay_item(
            item,
            cb,
            (item.width, item.height),
            decorators,
            [Some(&row_edges), columns],
            names,
            rtl,
            Some(origin),
        );
        item.laid = Some(laid);
    }
    let baseline_of = |item: &Item<'_>, last: bool| {
        let fragment = &item.laid.as_ref().expect("laid out").fragment;
        let own = if last {
            fragment.last_baseline()
        } else {
            fragment.first_baseline()
        };
        item.margin[0].unwrap_or(0.0) + own.unwrap_or(fragment.rect.height)
    };
    let mut first_baselines: HashMap<usize, f32> = HashMap::new();
    let mut last_descents: HashMap<usize, f32> = HashMap::new();
    for item in &plan.items {
        let (_, align) = alignments(item.style, style);
        if item.area[ROWS].len() != 1 || item.margin[0].is_none() || item.margin[2].is_none() {
            continue;
        }
        let row = item.area[ROWS].start;
        match align {
            ItemAlignment::Baseline { last: false } => {
                let b = baseline_of(item, false);
                let entry = first_baselines.entry(row).or_insert(b);
                *entry = entry.max(b);
            }
            ItemAlignment::Baseline { last: true } => {
                let height = item.laid.as_ref().map_or(0.0, |l| l.fragment.rect.height);
                let d = height + item.margins(ROWS) - baseline_of(item, true);
                let entry = last_descents.entry(row).or_insert(d);
                *entry = entry.max(d);
            }
            _ => {}
        }
    }
    for item in &mut plan.items {
        let (justify, align) = alignments(item.style, style);
        let fragment = &item.laid.as_ref().expect("laid out").fragment;
        let (w, h) = (fragment.rect.width, fragment.rect.height);
        let (left, right) = extent(&column_edges, &item.area[COLUMNS]);
        let (top, bottom) = extent(&row_edges, &item.area[ROWS]);
        // Horizontally, from the inline start: the right in a right-to-left grid.
        let (start_margin, end_margin) = if rtl {
            (item.margin[1], item.margin[3])
        } else {
            (item.margin[3], item.margin[1])
        };
        let free = right - left - w - start_margin.unwrap_or(0.0) - end_margin.unwrap_or(0.0);
        let flipped = (item.style.direction == Direction::Rtl) != rtl;
        let inline_offset = match (start_margin, end_margin) {
            (None, None) if free > 0.0 => free / 2.0,
            (None, Some(_)) if free > 0.0 => free,
            (_, None) => 0.0,
            _ => self_offset(justify, free, Some(!rtl), flipped),
        };
        item.x = left + start_margin.unwrap_or(0.0) + inline_offset;
        let free = bottom - top - h - item.margin[0].unwrap_or(0.0) - item.margin[2].unwrap_or(0.0);
        let row = item.area[ROWS].start;
        let block_offset = match (item.margin[0], item.margin[2]) {
            (None, None) if free > 0.0 => free / 2.0,
            (None, Some(_)) if free > 0.0 => free,
            (_, None) => 0.0,
            _ => match align {
                ItemAlignment::Baseline { last: false } if item.area[ROWS].len() == 1 => {
                    first_baselines.get(&row).copied().unwrap_or(0.0) - baseline_of(item, false)
                }
                ItemAlignment::Baseline { last: true } if item.area[ROWS].len() == 1 => {
                    let d = h + item.margins(ROWS) - baseline_of(item, true);
                    free - (last_descents.get(&row).copied().unwrap_or(d) - d)
                }
                _ => self_offset(align, free, None, false),
            },
        };
        item.y = top + item.margin[0].unwrap_or(0.0) + block_offset;
    }

    // Fragments in the content box, columns mirrored in a right-to-left grid.
    let cb = ContainingBlock {
        width: space.width,
        height: Some(height),
        rtl,
    };
    let mut placed: Vec<Placed<'_>> = Vec::with_capacity(plan.items.len());
    for (order, item) in plan.items.iter().enumerate() {
        let mut fragment = item.laid.as_ref().expect("laid out").fragment.clone();
        let x = if rtl {
            space.width - item.x - fragment.rect.width
        } else {
            item.x
        };
        let (dx, dy) = crate::position::relative_offset(item.style, cb);
        fragment.rect.x = x + dx;
        fragment.rect.y = item.y + dy;
        placed.push(Placed {
            block: item.block,
            fragment,
            order,
            width: item.width,
            height: item.height,
            offset: 0.0,
        });
    }

    // Units that pages break between: runs of rows that items span together.
    let mut units: Vec<Unit> = Vec::new();
    let row_count = plan.tracks[ROWS].len();
    let mut start = 0;
    while start < row_count {
        let mut end = start + 1;
        loop {
            let reach = plan
                .items
                .iter()
                .filter(|item| item.area[ROWS].start >= start && item.area[ROWS].start < end)
                .map(|item| item.area[ROWS].end)
                .max()
                .unwrap_or(end);
            if reach <= end {
                break;
            }
            end = reach;
        }
        let members: Vec<usize> = (0..plan.items.len())
            .filter(|&i| (start..end).contains(&plan.items[i].area[ROWS].start))
            .collect();
        let (mut top, mut bottom) = (row_edges[start].0, row_edges[end - 1].1);
        for &i in &members {
            let item = &plan.items[i];
            let rect = placed[i].fragment.rect;
            top = top.min(rect.y - item.margin[0].unwrap_or(0.0));
            bottom = bottom.max(rect.bottom() + item.margin[2].unwrap_or(0.0));
        }
        if !members.is_empty() {
            units.push(Unit {
                items: members,
                top,
                bottom,
            });
        }
        start = end;
    }
    Placed::set_offsets(&mut placed, &units);

    let mut out = match fragmentainer {
        None => ContentLaid {
            children: placed
                .into_iter()
                .map(|p| Fragment::Box(p.fragment))
                .collect(),
            height,
            break_token: None,
            deferred: false,
        },
        Some(f) => paginate(&placed, &units, false, cb, decorators, f, resume, height),
    };

    // Out-of-flow children: in their grid area when the container is their containing
    // block (§ 9.4), or else at the start of the content box.
    if resume.is_none() && !out.deferred {
        let positioned =
            crate::position::is_positioned(style) || crate::effects::contains_fixed(style);
        let lines = |axis: usize| Lines {
            names: &[],
            end: plan.tracks[axis].len() as i32,
        };
        for block in placeholders {
            let laid = flow::layout_block(block, cb, BlockKind::InFlow, decorators, None, None);
            let mut fragment = laid.fragment;
            if let (true, Some(OutOfFlow::Positioned(boxed))) = (positioned, &fragment.out_of_flow)
            {
                let s = &boxed.block.style;
                let placed_in_grid = s.position == Position::Absolute
                    && [
                        &s.grid_row_start,
                        &s.grid_row_end,
                        &s.grid_column_start,
                        &s.grid_column_end,
                    ]
                    .iter()
                    .any(|line| **line != GridLine::Auto);
                if placed_in_grid {
                    let padding = space.padding;
                    // Each side is a line, or the padding edge when `auto`.
                    let edge = |line: &GridLine, axis: usize, start: bool| -> f32 {
                        let edges = if axis == ROWS {
                            &row_edges
                        } else {
                            &column_edges
                        };
                        let outer = if axis == ROWS {
                            if start {
                                -padding.top
                            } else {
                                height + padding.bottom
                            }
                        } else if start {
                            -padding.left
                        } else {
                            space.width + padding.right
                        };
                        match side(line, start, &lines(axis)) {
                            Side::Line(l) => {
                                let l = l.clamp(0, edges.len() as i32) as usize;
                                if l == edges.len() {
                                    edges.last().map_or(outer, |e| e.1)
                                } else {
                                    edges[l].0
                                }
                            }
                            _ => outer,
                        }
                    };
                    let top = edge(&s.grid_row_start, ROWS, true);
                    let bottom = edge(&s.grid_row_end, ROWS, false);
                    let (mut left, mut right) = (
                        edge(&s.grid_column_start, COLUMNS, true),
                        edge(&s.grid_column_end, COLUMNS, false),
                    );
                    if rtl {
                        (left, right) = (space.width - right, space.width - left);
                    }
                    fragment.rect =
                        Rect::new(left, top, (right - left).max(0.0), (bottom - top).max(0.0));
                    fragment.out_of_flow = Some(OutOfFlow::InArea(boxed.clone()));
                }
            }
            out.children.push(Fragment::Box(fragment));
        }
    }
    out
}

/// Lays an item out at its width and height. A subgrid item gets the tracks it spans,
/// relative to its content box at `origin`; columns are known before rows.
#[allow(clippy::too_many_arguments)]
fn lay_item(
    item: &Item<'_>,
    cb: ContainingBlock,
    (width, height): (f32, Option<f32>),
    decorators: &[Arc<ComputedValues>],
    [rows, columns]: [Option<&Vec<(f32, f32)>>; 2],
    names: [&[Vec<Arc<str>>]; 2],
    rtl: bool,
    origin: Option<[f32; 2]>,
) -> Laid {
    let lay = || {
        flow::layout_block(
            item.block,
            cb,
            BlockKind::Item {
                width,
                height,
                at_least: false,
            },
            decorators,
            None,
            None,
        )
    };
    if !matches!(item.block.contents, BlockContents::Grid(_)) {
        return lay();
    }
    let (start, _) = columns.map_or((0.0, 0.0), |edges| extent(edges, &item.area[COLUMNS]));
    let origin = origin.unwrap_or([
        0.0,
        start
            + item.margin[if rtl { 1 } else { 3 }].unwrap_or(0.0)
            + if rtl {
                item.border.right + item.padding.right
            } else {
                item.border.left + item.padding.left
            },
    ]);
    let empty = Vec::new();
    let inherited = inherited_for(
        item,
        [
            rows.map_or(&empty[..], |r| &r[..]),
            columns.map_or(&empty[..], |c| &c[..]),
        ],
        origin,
        names,
        rtl,
        width,
    );
    with_subgrid(inherited, lay)
}

/// Minimum and maximum content widths of a grid container's content box: its columns
/// sized for no free space, and for as much as they can take.
pub(crate) fn intrinsic(block: &BlockBox, children: &[BlockBox]) -> (f32, f32) {
    let in_flow: Vec<&BlockBox> = children
        .iter()
        .filter(|child| !matches!(child.contents, BlockContents::OutOfFlow(_)))
        .collect();
    // Repeats that fill the space count how many fit in `max-width`.
    let (padding, border) = flow::padding_and_border(&block.style, 0.0);
    let width = match &block.style.max_width {
        Size::Value(LengthPercentage::Length(px)) => {
            if block.style.box_sizing == BoxSizing::BorderBox {
                (px - padding.horizontal() - border.horizontal()).max(0.0)
            } else {
                *px
            }
        }
        _ => 0.0,
    };
    let space = ContentSpace {
        width,
        height: None,
        min_height: 0.0,
        max_height: None,
        padding: Sides::default(),
    };
    let plan = plan(block, &in_flow, space, [None, None]);
    let items: Vec<(Range<usize>, Contribution)> = plan
        .items
        .iter()
        .map(|item| (item.area[COLUMNS].clone(), column_contribution(item)))
        .collect();
    let gap = match &block.style.column_gap {
        Some(LengthPercentage::Length(px)) => *px,
        _ => 0.0,
    };
    let total = |space: Space| {
        let mut tracks = plan.tracks[COLUMNS].clone();
        size_tracks(&mut tracks, &items, gap, space, false);
        positions(&tracks, gap, 0.0, 0.0)
            .last()
            .map_or(0.0, |e| e.1)
    };
    let min = total(Space::MinContent);
    (min, total(Space::MaxContent).max(min))
}
