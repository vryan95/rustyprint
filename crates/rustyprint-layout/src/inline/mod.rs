//! Inline layout: turning inline content into line boxes.
//!
//! [`paragraph`] flattens the content into text with styles, inline box edges, bidi levels
//! and shaped runs; [`breaking`] finds where lines end; [`line`] places each line.

mod breaking;
mod hyphenate;
mod line;
mod orientation;
mod paragraph;

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    BlockEllipsis, Direction, Overflow, TextBoxTrim, TextEdge, TextEdgeMetric, TextOverflowSide,
};

use std::collections::HashMap;

use crate::boxes::{FirstLine, InlineItem};
use crate::display::{Point, Rect};
use crate::fonts::select;
use crate::fragment::{InlineFragment, LineFragment, LineItem, Sides, TextFragment};
use crate::text::{ShapeOptions, shape_range};
use breaking::{Breaker, LineBreak};
use line::{BoxMetrics, LineContext, LineGeometry, LineOptions, is_opening};
use paragraph::Paragraph;

/// Lines laid out in an inline formatting context.
pub(crate) struct InlineLayout {
    pub lines: Vec<LineFragment>,
    pub height: f32,
}

/// Available width standing in for "unlimited" when measuring max-content sizes.
pub(crate) const UNLIMITED: f32 = 1.0e7;

/// How far floats reach into a band of the block container from `top` down `height`, from
/// its content box's left and right edges.
pub(crate) type Exclusion<'a> = &'a dyn Fn(f32, f32) -> (f32, f32);

/// How many times lines are laid out again around floats, with the positions the lines had
/// the time before.
const FLOAT_PASSES: usize = 4;

/// Lays out inline content in a block container of `width`. `decorators` are the block
/// boxes whose text decorations propagate to the content, outermost first.
///
/// `::first-line` styles change how much fits on the first line, so with `first_line` the
/// first line comes from the content in those styles, and the rest continue from where it
/// ends in the content's own styles.
///
/// Lines beside floats are shortened by `exclusion`. Where a line goes depends on the
/// lines above it, so layout runs again with the positions the lines had, until they stay.
pub(crate) fn layout(
    items: &[InlineItem],
    container: &Arc<ComputedValues>,
    width: f32,
    decorators: &[Arc<ComputedValues>],
    first_line: &[FirstLine],
    exclusion: Option<Exclusion>,
) -> InlineLayout {
    let Some(exclusion) = exclusion else {
        return layout_pass(items, container, width, decorators, first_line, None);
    };
    let mut estimates: Vec<(f32, f32)> = Vec::new();
    let mut pass = 0;
    loop {
        let laid = layout_pass(
            items,
            container,
            width,
            decorators,
            first_line,
            Some((exclusion, &estimates)),
        );
        pass += 1;
        let actual: Vec<(f32, f32)> = laid
            .lines
            .iter()
            .map(|line| (line.rect.y, line.rect.height))
            .collect();
        let settled = actual.len() <= estimates.len()
            && actual
                .iter()
                .zip(&estimates)
                .all(|(a, e)| (a.0 - e.0).abs() < 0.01 && (a.1 - e.1).abs() < 0.01);
        if settled || pass == FLOAT_PASSES {
            return laid;
        }
        estimates = actual;
    }
}

/// One layout of inline content. Lines beside floats get the band of `exclusion` at their
/// estimated `(top, height)`: from the layout before, or else from the strut.
fn layout_pass(
    items: &[InlineItem],
    container: &Arc<ComputedValues>,
    width: f32,
    decorators: &[Arc<ComputedValues>],
    first_line: &[FirstLine],
    floats: Option<(Exclusion, &[(f32, f32)])>,
) -> InlineLayout {
    let empty = InlineLayout {
        lines: Vec::new(),
        height: 0.0,
    };
    let p = Paragraph::build(items, container, width, width);
    if p.is_empty() {
        return empty;
    }
    let breaker = Breaker::new(&p);
    let ctx = LineContext {
        p: &p,
        breaker: &breaker,
        container,
        decorators,
        metrics: p.boxes.iter().map(|b| BoxMetrics::of(&b.style)).collect(),
        width,
    };
    let lead_items = (!first_line.is_empty()).then(|| first_line_items(items, first_line));
    let lead_p = lead_items
        .as_ref()
        .map(|items| Paragraph::build(items, container, width, width));
    let lead_breaker = lead_p.as_ref().map(Breaker::new);
    let lead_ctx = lead_p
        .as_ref()
        .zip(lead_breaker.as_ref())
        .map(|(p, breaker)| LineContext {
            p,
            breaker,
            container,
            decorators,
            metrics: p.boxes.iter().map(|b| BoxMetrics::of(&b.style)).collect(),
            width,
        });
    // The context of the first line.
    let first_ctx = lead_ctx.as_ref().unwrap_or(&ctx);
    let letter = initial_letter(first_ctx);
    let start = letter.as_ref().map_or(0, |l| l.end);
    let rtl = container.direction == Direction::Rtl;

    let indent = &container.text_indent;
    let indent_px = indent.length.resolve(width);
    let lead = first_ctx.p;
    let first = first_ctx.breaker.skip_start(start);
    let hang_first = if container.hanging_punctuation.first
        && lead.text[first..].chars().next().is_some_and(is_opening)
    {
        let len = lead.text[first..].chars().next().map_or(0, char::len_utf8);
        lead.width(first, first + len)
    } else {
        0.0
    };
    let exclusion = |index: usize| match &letter {
        Some(letter) if index < letter.lines => letter.width,
        _ => 0.0,
    };
    // Floats beside line `index`, at the place it had the time before, or else where lines
    // of the strut's height would put it.
    let strut = ctx.metrics[0].line_height;
    let beside = |index: usize| -> (f32, f32) {
        let Some((exclude, estimates)) = floats else {
            return (0.0, 0.0);
        };
        let (top, height) = match estimates.get(index) {
            Some(&estimate) => estimate,
            None => {
                let (last_top, last_height) = estimates.last().copied().unwrap_or((0.0, 0.0));
                let below = estimates.len() as f32;
                let skipped = index as f32 - below;
                let top = if estimates.is_empty() {
                    index as f32 * strut
                } else {
                    last_top + last_height + skipped * strut
                };
                (top, strut)
            }
        };
        let (left, right) = exclude(top, height);
        (left.max(0.0), right.max(0.0))
    };
    let geometry = |index: usize, fresh: bool| -> LineGeometry {
        let applies = (index == 0 || (indent.each_line && fresh)) != indent.hanging;
        let indent = if applies { indent_px } else { 0.0 };
        let gap = indent + exclusion(index);
        let hang_start = if index == 0 { hang_first } else { 0.0 };
        let (float_left, float_right) = beside(index);
        LineGeometry {
            left: if rtl { float_left } else { gap + float_left },
            width: (width - gap - float_left - float_right).max(0.0) + hang_start,
            x0: gap,
            hang_start,
        }
    };
    let line_geometry = |index, fresh| {
        let g = geometry(index, fresh);
        (g.x0, g.width)
    };
    let lead_lines = first_ctx
        .breaker
        .break_styled(start, &line_geometry, container);
    let Some(first_break) = lead_lines.first().copied() else {
        return empty;
    };
    // Each line, and whether it comes from the first-line content.
    let lines: Vec<(bool, LineBreak)> = if lead_ctx.is_some() {
        let resume = map_offset(lead, &p, first_break.end);
        let mut lines = vec![(true, first_break)];
        if resume < p.text.len() {
            let rest = breaker.break_rest(resume, 1, first_break.forced, &line_geometry);
            lines.extend(rest.into_iter().map(|line| (false, line)));
        }
        lines
    } else {
        lead_lines.into_iter().map(|line| (false, line)).collect()
    };

    // `line-clamp` and `max-lines` drop the lines after the limit.
    let clamp = container
        .line_clamp
        .lines
        .or(container.max_lines)
        .map(|n| n as usize);
    let clamped = clamp.is_some_and(|n| lines.len() > n);
    let lines: Vec<(bool, LineBreak)> = match clamp {
        Some(n) if clamped => lines[..n].to_vec(),
        _ => lines,
    };
    let clamp_ellipsis = match &container.line_clamp.ellipsis {
        _ if !clamped || container.line_clamp.lines.is_none() => None,
        BlockEllipsis::NoEllipsis => None,
        BlockEllipsis::Auto => Some("\u{2026}".to_owned()),
        BlockEllipsis::String(s) => Some(s.to_string()),
    };
    let overflow_ellipsis = if container.overflow_x == Overflow::Visible {
        None
    } else {
        match &container.text_overflow.end {
            TextOverflowSide::Clip => None,
            TextOverflowSide::Ellipsis => Some("\u{2026}".to_owned()),
            TextOverflowSide::String(s) => Some(s.to_string()),
        }
    };

    let count = lines.len();
    let mut fragments = Vec::with_capacity(count);
    let mut y = 0.0;
    for (index, (lead, line)) in lines.iter().enumerate() {
        let fresh = index == 0 || lines[index - 1].1.forced;
        let last_line = index + 1 == count;
        let ellipsis = if last_line && clamp_ellipsis.is_some() {
            clamp_ellipsis.as_deref().map(|s| (s, true))
        } else {
            overflow_ellipsis.as_deref().map(|s| (s, false))
        };
        let options = LineOptions {
            last: line.forced || last_line,
            paragraph_end: last_line,
            ellipsis,
        };
        let line_ctx = if *lead { first_ctx } else { &ctx };
        let mut fragment = line_ctx.place(line, &geometry(index, fresh), &options);
        fragment.rect.y = y;
        y += fragment.rect.height;
        fragments.push(fragment);
    }

    if let Some(letter) = letter {
        y = place_letter(first_ctx, letter, &mut fragments, y, rtl);
    }
    y = trim_text_box(container, &ctx.metrics[0], &mut fragments, y);
    InlineLayout {
        lines: fragments,
        height: y,
    }
}

/// Inline content restyled for the first line: each `::first-line` box wraps the content,
/// and the content takes the inherited properties it changes.
fn first_line_items(items: &[InlineItem], first_line: &[FirstLine]) -> Vec<InlineItem> {
    let mut cache: HashMap<*const ComputedValues, Arc<ComputedValues>> = HashMap::new();
    let mut restyle = |style: &Arc<ComputedValues>| {
        cache
            .entry(Arc::as_ptr(style))
            .or_insert_with(|| {
                let mut out = (**style).clone();
                for line in first_line {
                    out = out.on_first_line(&line.origin, &line.style);
                }
                Arc::new(out)
            })
            .clone()
    };
    let mut out = Vec::with_capacity(items.len() + 2 * first_line.len());
    for line in first_line {
        out.push(InlineItem::Start {
            style: line.style.clone(),
            node: None,
        });
    }
    for item in items {
        out.push(match item {
            InlineItem::Text { text, style, lang } => InlineItem::Text {
                text: text.clone(),
                style: restyle(style),
                lang: lang.clone(),
            },
            InlineItem::Start { style, node } => InlineItem::Start {
                style: restyle(style),
                node: *node,
            },
            other => other.clone(),
        });
    }
    out.extend(first_line.iter().map(|_| InlineItem::End));
    out
}

/// The offset in paragraph `to` that corresponds to `pos` in paragraph `from`, built from
/// the same content in other styles. Text transforms can change the text's length, so
/// offsets then map by character count.
fn map_offset(from: &Paragraph, to: &Paragraph, pos: usize) -> usize {
    if from.text.len() == to.text.len() && to.text.is_char_boundary(pos) {
        return pos;
    }
    let chars = from.text[..pos].chars().count();
    to.text
        .char_indices()
        .nth(chars)
        .map_or(to.text.len(), |(i, _)| i)
}

/// The used line height of a box with this style, in pixels: `line-height`, or the font's
/// for `normal`.
pub(crate) fn line_height(style: &ComputedValues) -> f32 {
    BoxMetrics::of(style).line_height
}

/// Minimum and maximum content widths of inline content (CSS Sizing 3 § 5).
pub(crate) fn intrinsic(items: &[InlineItem], container: &Arc<ComputedValues>) -> (f32, f32) {
    let indent = container.text_indent.length.resolve(0.0);
    let has_atomics = items.iter().any(|i| matches!(i, InlineItem::Atomic(_)));
    let p = Paragraph::build(items, container, 0.0, 0.0);
    let (min, max) = Breaker::new(&p).intrinsic(indent);
    if !has_atomics {
        return (min, max);
    }
    // Atomic inlines are as narrow as they get for the minimum and unconstrained for the
    // maximum.
    let wide = Paragraph::build(items, container, UNLIMITED, 0.0);
    let (_, max) = Breaker::new(&wide).intrinsic(indent);
    (min, max)
}

/// An initial letter at the start of the paragraph (CSS Inline 3 § 6).
struct Letter {
    inline_box: usize,
    /// Where the paragraph continues after the letter.
    end: usize,
    /// The letter's margin box width, which the lines beside it leave free.
    width: f32,
    /// How many lines the letter sinks into.
    lines: usize,
    size: f32,
    glyphs: Vec<crate::text::Glyph>,
    ascent: f32,
    cap_height: f32,
}

fn initial_letter(ctx: &LineContext<'_>) -> Option<Letter> {
    let p = ctx.p;
    let first = ctx.breaker.skip_start(0);
    // The letter's box starts the paragraph, inside nothing but boxes that also start there,
    // such as `::first-line` boxes.
    let (index, b) = p.boxes.iter().enumerate().skip(1).find(|(_, b)| {
        b.start == first
            && b.chain.iter().all(|&i| p.boxes[i].start == first)
            && b.end > b.start
            && b.style.initial_letter.lines().is_some()
    })?;
    let (size_in_lines, sink) = b.style.initial_letter.lines()?;
    let root = &ctx.metrics[0];
    // The letter reaches from the cap height of the first line to the baseline of the
    // line it sinks to.
    let height = (size_in_lines - 1.0) * root.line_height + root.cap_height;
    let font = select(&b.style).font;
    let m = font.metrics();
    let cap_ratio = f32::from(m.cap_height) / f32::from(font.units_per_em());
    let size = if cap_ratio > 0.0 {
        height / cap_ratio
    } else {
        height
    };
    let glyphs = shape_range(
        &font,
        size,
        &p.text,
        b.start..b.end,
        &ShapeOptions::default(),
    );
    let advance: f32 = glyphs.iter().map(|g| g.x_advance).sum();
    Some(Letter {
        inline_box: index,
        end: b.end,
        width: advance + b.side(true) + b.side(false),
        lines: sink.max(1) as usize,
        size,
        glyphs,
        ascent: font.scale(m.ascender, size),
        cap_height: font.scale(m.cap_height, size),
    })
}

/// Puts the initial letter into the first line and returns the paragraph's new height.
fn place_letter(
    ctx: &LineContext<'_>,
    letter: Letter,
    lines: &mut [LineFragment],
    height: f32,
    rtl: bool,
) -> f32 {
    let p = ctx.p;
    let b = &p.boxes[letter.inline_box];
    let root = &ctx.metrics[0];
    let sink = letter.lines - 1;
    let baseline = match lines.get(sink) {
        Some(line) => line.rect.y + line.baseline,
        None => {
            let last = lines.last().expect("lines are not empty");
            let missing = (sink + 1 - lines.len()) as f32;
            last.rect.y + last.baseline + missing * root.line_height
        }
    };
    // A raised letter pushes the lines down so it fits.
    let top = baseline - letter.cap_height;
    let shift = (-top).max(0.0);
    for line in lines.iter_mut() {
        line.rect.y += shift;
    }
    let baseline = baseline + shift;
    let first = &mut lines[0];
    let y = baseline - first.rect.y;
    let x = if rtl {
        ctx.width - letter.width + b.side(true)
    } else {
        b.side(true)
    };
    let choice = select(&b.style);
    let run = crate::display::TextRun {
        origin: Point::new(x, y),
        font: choice.font,
        size: letter.size,
        color: crate::display::Color::from_css(&b.style.color),
        glyphs: letter.glyphs,
        text: p.text.clone(),
        oblique: choice.synthetic_oblique,
        embolden: 0.0,
        stroke: None,
        fill: true,
        stroke_first: false,
        upright: None,
    };
    let advance: f32 = run.glyphs.iter().map(|g| g.x_advance).sum();
    let left = x - b.padding.left - b.border.left;
    let text = LineItem::Text(TextFragment {
        run,
        style: b.style.clone(),
        decorations: Vec::new(),
    });
    first.items.insert(
        0,
        LineItem::Inline(InlineFragment {
            node: b.node,
            style: b.style.clone(),
            rect: Rect::new(
                left,
                y - letter.ascent - b.padding.top - b.border.top,
                advance + b.padding.horizontal() + b.border.horizontal(),
                letter.ascent + b.padding.vertical() + b.border.vertical(),
            ),
            border: Sides {
                top: b.border.top,
                right: b.border.right,
                bottom: b.border.bottom,
                left: b.border.left,
            },
            padding: b.padding,
            children: vec![text],
        }),
    );
    (height + shift).max(baseline)
}

/// Trims the space above the first line and below the last line to the edges
/// `text-box-edge` names (CSS Inline 3 § 5). Returns the new height.
fn trim_text_box(
    container: &ComputedValues,
    root: &BoxMetrics,
    lines: &mut [LineFragment],
    height: f32,
) -> f32 {
    let trim = container.text_box_trim;
    if trim == TextBoxTrim::None || lines.is_empty() {
        return height;
    }
    let edge = container.text_box_edge.unwrap_or(TextEdge::TEXT);
    let over = |metric: TextEdgeMetric| match metric {
        TextEdgeMetric::Cap => root.cap_height,
        TextEdgeMetric::Ex => root.x_height,
        _ => root.ascent,
    };
    let under = |metric: TextEdgeMetric| match metric {
        TextEdgeMetric::Alphabetic => 0.0,
        _ => root.descent,
    };
    let mut height = height;
    if matches!(trim, TextBoxTrim::TrimStart | TextBoxTrim::TrimBoth) {
        let first = &lines[0];
        let cut = first.rect.y + first.baseline - over(edge.over);
        for line in lines.iter_mut() {
            line.rect.y -= cut;
        }
        height -= cut;
    }
    if matches!(trim, TextBoxTrim::TrimEnd | TextBoxTrim::TrimBoth) {
        let last = lines.last().expect("not empty");
        let bottom = last.rect.y + last.baseline + under(edge.under);
        height = bottom;
    }
    height
}
