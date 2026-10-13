//! Ruby layout (CSS Ruby Annotation Layout 1): each segment of a ruby container is an atomic
//! inline holding its base, with its annotations over or under it, or beside it.

use rustyprint_css::properties::types::{Direction, RubyAlign};

use crate::boxes::{AnnotationSide, BlockBox, Ruby};
use crate::flow::{self, CollapsibleMargin, EPSILON, Laid};
use crate::fragment::{BoxFragment, Fragment, LineFragment, LineItem, Sides};

/// Room to lay bases and annotations out on one line: ruby does not wrap inside a segment.
const UNBOUNDED: f32 = 1.0e6;

/// Lays out a ruby segment. The base comes first among the fragment's children, so the
/// segment's first baseline is the base's.
pub(crate) fn layout(block: &BlockBox, ruby: &Ruby) -> Laid {
    let mut base = flow::layout_atomic(&ruby.base, UNBOUNDED).fragment;
    let mut annotations: Vec<(BoxFragment, AnnotationSide)> = ruby
        .annotations
        .iter()
        .map(|a| (flow::layout_atomic(&a.block, UNBOUNDED).fragment, a.side))
        .collect();
    let stacked = |side: AnnotationSide| side != AnnotationSide::InterCharacter;

    // Bases and the annotations over and under them share a width; whichever is narrower
    // spreads across it.
    let base_width = base.rect.width;
    let base_clusters = base
        .children
        .iter()
        .find_map(|child| match child {
            Fragment::Line(line) => Some(clusters(&line.items)),
            Fragment::Box(_) => None,
        })
        .unwrap_or(0);
    let width = annotations
        .iter()
        .filter(|(_, side)| stacked(*side))
        .map(|(a, _)| a.rect.width)
        .fold(base_width, f32::max);
    spread(&mut base, width);
    for (annotation, side) in &mut annotations {
        if stacked(*side) {
            spread(annotation, width);
        }
    }

    // Over annotations stack up from the base, under ones down from it, nearest first. In
    // `vertical-lr` the segment's content turns like blocks, which puts its top on the
    // under side of the line: the sides trade places.
    if crate::writing::flips_lines(block.style.writing_mode) {
        for (_, side) in &mut annotations {
            *side = match side {
                AnnotationSide::Over => AnnotationSide::Under,
                AnnotationSide::Under => AnnotationSide::Over,
                AnnotationSide::InterCharacter => AnnotationSide::InterCharacter,
            };
        }
    }
    let over: f32 = annotations
        .iter()
        .filter(|(_, side)| *side == AnnotationSide::Over)
        .map(|(a, _)| a.rect.height)
        .sum();
    base.rect.x = 0.0;
    base.rect.y = over;
    let (mut above, mut below) = (over, over + base.rect.height);
    let mut beside = width;
    for (annotation, side) in &mut annotations {
        match side {
            AnnotationSide::Over => {
                above -= annotation.rect.height;
                annotation.rect.x = 0.0;
                annotation.rect.y = above;
            }
            AnnotationSide::Under => {
                annotation.rect.x = 0.0;
                annotation.rect.y = below;
                below += annotation.rect.height;
            }
            AnnotationSide::InterCharacter => {
                annotation.rect.x = beside;
                annotation.rect.y = base.rect.y + (base.rect.height - annotation.rect.height) / 2.0;
                beside += annotation.rect.width;
            }
        }
    }
    // Annotations beside a short base may reach above or below the others.
    let top = annotations
        .iter()
        .map(|(a, _)| a.rect.y)
        .fold(0.0, f32::min);
    let bottom = annotations
        .iter()
        .map(|(a, _)| a.rect.bottom())
        .fold(below, f32::max);
    base.rect.y -= top;
    for (annotation, _) in &mut annotations {
        annotation.rect.y -= top;
    }

    // An annotation wider than its base may hang over text beside the ruby, into the room
    // the spread base leaves at its ends, by up to the annotation's font size
    // (`ruby-overhang: auto`).
    let (leading, trailing) = ends(
        ruby.base.style.ruby_align,
        width - base_width,
        base_clusters,
    );
    let font_size = ruby
        .annotations
        .first()
        .map_or(0.0, |a| a.block.style.font_size);
    let hang = |allowed: bool, room: f32| {
        if allowed && room > EPSILON {
            room.min(font_size)
        } else {
            0.0
        }
    };
    let (start, end) = (
        hang(ruby.overhang.0, leading),
        hang(ruby.overhang.1, trailing),
    );
    let (left, right) = if block.style.direction == Direction::Rtl {
        (end, start)
    } else {
        (start, end)
    };

    let mut children = vec![Fragment::Box(base)];
    children.extend(annotations.into_iter().map(|(a, _)| Fragment::Box(a)));
    Laid {
        fragment: BoxFragment {
            node: block.node,
            style: block.style.clone(),
            rect: crate::display::Rect::new(0.0, 0.0, beside, bottom - top),
            border: Sides::default(),
            padding: Sides::default(),
            children,
            replaced: None,
            legend: None,
            out_of_flow: None,
        },
        margin: Sides {
            left: -left,
            right: -right,
            ..Sides::default()
        },
        collapsed_top: CollapsibleMargin::default(),
        collapsed_bottom: CollapsibleMargin::default(),
        through: false,
        break_token: None,
        deferred: false,
    }
}

/// The minimum and maximum widths of a ruby segment: it does not wrap.
pub(crate) fn intrinsic(block: &BlockBox, ruby: &Ruby) -> (f32, f32) {
    let width = layout(block, ruby).fragment.rect.width;
    (width, width)
}

/// Widens a base or annotation box to `width`, spreading the content of each of its lines
/// across the room left as its `ruby-align` says.
fn spread(fragment: &mut BoxFragment, width: f32) {
    let extra = width - fragment.rect.width;
    if extra <= EPSILON {
        return;
    }
    fragment.rect.width = width;
    let align = fragment.style.ruby_align;
    for child in &mut fragment.children {
        if let Fragment::Line(line) = child {
            let content = items_right(&line.items).max(0.0);
            line.rect.width = width;
            spread_line(line, width - content, align);
        }
    }
}

/// Where the content of line items ends.
fn items_right(items: &[LineItem]) -> f32 {
    items
        .iter()
        .map(|item| match item {
            LineItem::Text(text) => text.run.origin.x + text.run.width(),
            LineItem::Box(atomic) => atomic.rect.right(),
            LineItem::Inline(inline) => inline.rect.right(),
        })
        .fold(0.0, f32::max)
}

/// Spreads a line's content by `free` (CSS Ruby 1 § 4.1): `space-around` puts equal space
/// between characters and half of it at the ends, `space-between` puts it all between
/// them, and `start` and `center` keep them together.
fn spread_line(line: &mut LineFragment, free: f32, align: RubyAlign) {
    let count = clusters(&line.items);
    if free <= EPSILON || count == 0 {
        return;
    }
    let (offset, gap) = distribution(align, free, count);
    let mut index = 0;
    shift_items(&mut line.items, offset, gap, &mut index);
}

/// How `ruby-align` spreads `free` space across `count` characters: the space before the
/// first, and between each two.
fn distribution(align: RubyAlign, free: f32, count: usize) -> (f32, f32) {
    let n = count.max(1) as f32;
    match align {
        RubyAlign::Start => (0.0, 0.0),
        RubyAlign::Center => (free / 2.0, 0.0),
        RubyAlign::SpaceBetween if count > 1 => (0.0, free / (n - 1.0)),
        RubyAlign::SpaceBetween => (free / 2.0, 0.0),
        RubyAlign::SpaceAround => (free / (2.0 * n), free / n),
    }
}

/// The space `ruby-align` leaves before the first and after the last of `count` characters.
fn ends(align: RubyAlign, free: f32, count: usize) -> (f32, f32) {
    if free <= EPSILON {
        return (0.0, 0.0);
    }
    let (offset, gap) = distribution(align, free, count);
    let between = gap * count.saturating_sub(1) as f32;
    (offset, (free - offset - between).max(0.0))
}

/// The number of characters (grapheme clusters, and atomic inlines) among line items.
fn clusters(items: &[LineItem]) -> usize {
    items
        .iter()
        .map(|item| match item {
            LineItem::Text(text) => run_clusters(&text.run),
            LineItem::Box(_) => 1,
            LineItem::Inline(inline) => clusters(&inline.children),
        })
        .sum()
}

fn run_clusters(run: &crate::display::TextRun) -> usize {
    let mut count = 0;
    let mut last = None;
    for glyph in &run.glyphs {
        if last != Some(&glyph.text) {
            count += 1;
            last = Some(&glyph.text);
        }
    }
    count
}

/// Moves character `index` (counted across the line) right by `offset + index * gap`.
fn shift_items(items: &mut [LineItem], offset: f32, gap: f32, index: &mut usize) {
    for item in items {
        match item {
            LineItem::Text(text) => {
                let run = &mut text.run;
                run.origin.x += offset + *index as f32 * gap;
                // The gap after each character but the run's last goes in its advance.
                let count = run.glyphs.len();
                for i in 0..count {
                    let ends_cluster =
                        i + 1 == count || run.glyphs[i + 1].text != run.glyphs[i].text;
                    if ends_cluster {
                        *index += 1;
                        if i + 1 < count {
                            run.glyphs[i].x_advance += gap;
                        }
                    }
                }
            }
            LineItem::Box(atomic) => {
                atomic.rect.x += offset + *index as f32 * gap;
                *index += 1;
            }
            LineItem::Inline(inline) => {
                let first = *index;
                shift_items(&mut inline.children, offset, gap, index);
                let last = index.saturating_sub(1).max(first);
                inline.rect.x += offset + first as f32 * gap;
                inline.rect.width += (last - first) as f32 * gap;
            }
        }
    }
}
