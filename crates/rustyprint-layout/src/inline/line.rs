//! Placing one line: bidi reordering, alignment and justification, vertical alignment and
//! building the line's fragments (CSS 2.1 § 10.8, CSS Inline 3, CSS Text 3 §§ 7–8).

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    AlignmentBaseline, BaselineShift, DecorationLength, Direction, LineHeight, Paint, TextAlign,
    TextAlignLast, TextDecorationSkipInk, TextDecorationStyle, TextJustify,
};
use rustyprint_css::values::AbsoluteColor;

use super::breaking::{Breaker, LineBreak};
use super::paragraph::{Paragraph, SpanKind, is_ideograph, is_word_separator, spacing};
use crate::Font;
use crate::display::{Color, Point, Rect, TextRun, TextStroke};
use crate::fonts::select;
use crate::fragment::{
    Decoration, DecorationKind, InlineFragment, LineFragment, LineItem, Sides, TextFragment,
};
use crate::text::{Glyph, shape};

/// Font metrics of an inline box, in pixels.
#[derive(Debug, Clone)]
pub(crate) struct BoxMetrics {
    pub font: Font,
    pub size: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
    pub x_height: f32,
    pub cap_height: f32,
    pub subscript: f32,
    pub superscript: f32,
}

impl BoxMetrics {
    pub fn of(style: &ComputedValues) -> Self {
        let choice = select(style);
        let font = choice.font;
        let size = super::paragraph::adjusted_size(style, &font);
        let m = font.metrics();
        let px = |units: i16| font.scale(units, size);
        let ascent = px(m.ascender);
        let descent = -px(m.descender);
        let line_height = match style.line_height {
            LineHeight::Normal => ascent + descent + px(m.line_gap),
            ref other => other.to_px(style.font_size),
        };
        Self {
            size,
            ascent,
            descent,
            line_height,
            x_height: px(m.x_height),
            cap_height: px(m.cap_height),
            subscript: px(m.subscript_offset),
            superscript: px(m.superscript_offset),
            font,
        }
    }

    /// The top and bottom of the box's layout bounds around its baseline (CSS 2.1 § 10.8.1).
    fn bounds(&self) -> (f32, f32) {
        let half_leading = (self.line_height - (self.ascent + self.descent)) / 2.0;
        (-(self.ascent + half_leading), self.descent + half_leading)
    }
}

/// What every line of a paragraph shares.
pub(crate) struct LineContext<'a> {
    pub p: &'a Paragraph,
    pub breaker: &'a Breaker<'a>,
    pub container: &'a Arc<ComputedValues>,
    /// Block-level boxes whose text decorations apply to the paragraph, outermost first.
    pub decorators: &'a [Arc<ComputedValues>],
    /// Metrics of each inline box; the first is the root inline box.
    pub metrics: Vec<BoxMetrics>,
    /// The width of the block container's content box.
    pub width: f32,
}

/// Where a line goes horizontally.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LineGeometry {
    /// The left edge of the space for content, from the content box's left edge.
    pub left: f32,
    pub width: f32,
    /// Where the line starts for tab stops, measured from the start edge.
    pub x0: f32,
    /// Width of punctuation that hangs before the start edge.
    pub hang_start: f32,
}

/// Instructions for the end of a line.
pub(crate) struct LineOptions<'a> {
    /// Whether the line is the last before a forced break or the end of the paragraph.
    pub last: bool,
    /// Whether this is the last line of the paragraph.
    pub paragraph_end: bool,
    /// An ellipsis to put at the end, and whether to put it there even if the line fits.
    pub ellipsis: Option<(&'a str, bool)>,
}

#[derive(Debug, Clone)]
enum Kind {
    Text {
        run: usize,
        glyphs: Vec<Glyph>,
    },
    /// Text that is not in the paragraph, such as a hyphen or an ellipsis.
    Extra {
        text: Arc<str>,
        font: Font,
        size: f32,
        glyphs: Vec<Glyph>,
        style: Arc<ComputedValues>,
    },
    Tab,
    Atomic(usize),
    Edge {
        start: bool,
    },
}

#[derive(Debug, Clone)]
struct Piece {
    kind: Kind,
    pos: usize,
    level: u8,
    /// The innermost inline box: for edges, the box the edge belongs to.
    inline_box: usize,
    width: f32,
    x: f32,
}

impl Piece {
    fn is_content(&self) -> bool {
        !matches!(self.kind, Kind::Edge { .. })
    }

    fn glyphs_mut(&mut self) -> Option<&mut Vec<Glyph>> {
        match &mut self.kind {
            Kind::Text { glyphs, .. } | Kind::Extra { glyphs, .. } => Some(glyphs),
            _ => None,
        }
    }
}

const EPSILON: f32 = 0.01;

impl LineContext<'_> {
    /// Places a line and returns its fragment, positioned at the top of the content box.
    pub fn place(
        &self,
        line: &LineBreak,
        geometry: &LineGeometry,
        options: &LineOptions<'_>,
    ) -> LineFragment {
        let p = self.p;
        let mut pieces = self.pieces(line);
        self.assign_levels(&mut pieces);

        // Tabs advance to tab stops measured along the line in logical order.
        let mut x = geometry.x0;
        for piece in &mut pieces {
            if let Kind::Tab = piece.kind {
                piece.width = p.tab_stop(piece.pos, x) - x;
            }
            x += piece.width;
        }
        self.trim_letter_spacing(&mut pieces);
        if let Some((ellipsis, force)) = options.ellipsis {
            self.add_ellipsis(&mut pieces, ellipsis, force, geometry.width);
        }
        self.fill_leaders(&mut pieces, geometry.width);

        // Alignment and justification.
        let content: f32 = pieces.iter().map(|piece| piece.width).sum();
        let hang_end = self.hanging_end(&pieces, options, content, geometry.width);
        let free = geometry.width - (content - hang_end);
        let rtl = p.base_rtl;
        let align = self.alignment(options.last);
        let mut offset = match align {
            TextAlign::Left => 0.0,
            TextAlign::Right => free,
            TextAlign::Center => free / 2.0,
            TextAlign::End => {
                if rtl {
                    0.0
                } else {
                    free
                }
            }
            TextAlign::Justify => {
                if self.justify(&mut pieces, free) || !rtl {
                    0.0
                } else {
                    free
                }
            }
            TextAlign::Start | TextAlign::MatchParent | TextAlign::JustifyAll => {
                if rtl {
                    free
                } else {
                    0.0
                }
            }
        };
        // Lines too long for the space are start-aligned.
        if free < 0.0 {
            offset = if rtl { free } else { 0.0 };
        }
        offset -= if rtl { 0.0 } else { geometry.hang_start };

        // Visual order and horizontal positions.
        let order = visual_order(&pieces.iter().map(|piece| piece.level).collect::<Vec<_>>());
        let mut x = geometry.left + offset;
        let mut visual: Vec<Piece> = Vec::with_capacity(pieces.len());
        for index in order {
            let mut piece = pieces[index].clone();
            piece.x = x;
            x += piece.width;
            visual.push(piece);
        }

        // Vertical alignment.
        let baselines = self.baselines(&visual);
        let (top, bottom) = baselines.extent;
        let items = self.build(&visual, 0, &baselines.boxes, &baselines.atomics, -top);
        LineFragment {
            rect: Rect::new(0.0, 0.0, self.width, bottom - top),
            baseline: -top,
            items,
            turn: None,
        }
    }

    /// The pieces of a line in logical order.
    fn pieces(&self, line: &LineBreak) -> Vec<Piece> {
        let p = self.p;
        let end = self.breaker.trim_end(line.start, line.end);
        let mut pieces = Vec::new();
        for edge in p.edges_between(line.start, line.end) {
            pieces.push(Piece {
                kind: Kind::Edge { start: edge.start },
                pos: edge.pos.min(end.max(line.start)),
                level: 0,
                inline_box: edge.inline_box,
                width: p.edge_width(edge),
                x: 0.0,
            });
        }
        let first_run = p.runs.partition_point(|r| r.range.end <= line.start);
        for (index, run) in p.runs.iter().enumerate().skip(first_run) {
            if run.range.start >= end {
                break;
            }
            let from = run.range.start.max(line.start);
            let to = run.range.end.min(end);
            let glyphs: Vec<Glyph> = run
                .glyphs
                .iter()
                .filter(|g| g.text.start >= from && g.text.start < to)
                .cloned()
                .collect();
            if glyphs.is_empty() {
                continue;
            }
            pieces.push(Piece {
                width: glyphs.iter().map(|g| g.x_advance).sum(),
                kind: Kind::Text { run: index, glyphs },
                pos: from,
                level: run.level,
                inline_box: p.spans[run.span].inline_box,
                x: 0.0,
            });
        }
        let first_tab = p.tabs.partition_point(|&t| t < line.start);
        for &tab in p.tabs[first_tab..].iter().take_while(|&&t| t < end) {
            pieces.push(Piece {
                kind: Kind::Tab,
                pos: tab,
                level: 0,
                inline_box: p.span_at(tab).inline_box,
                width: 0.0,
                x: 0.0,
            });
        }
        let first_span = p.spans.partition_point(|s| s.range.end <= line.start);
        for span in p.spans[first_span..]
            .iter()
            .take_while(|s| s.range.start < end)
        {
            if let SpanKind::Atomic(index) = span.kind {
                pieces.push(Piece {
                    kind: Kind::Atomic(index),
                    pos: span.range.start,
                    level: p.levels[span.range.start],
                    inline_box: span.inline_box,
                    width: p.atomics[index].width,
                    x: 0.0,
                });
            }
        }
        // Edges come before content at the same position; the sort is stable, so edges keep
        // their document order.
        pieces.sort_by_key(|piece| (piece.pos, piece.is_content()));
        if line.hyphen && end > line.start {
            let span = p.span_at(end - 1);
            let style = span.style.clone();
            let text = Breaker::hyphen_text(&style);
            let piece = self.extra(&text, &style, p.levels[end - 1], span.inline_box);
            let at = pieces
                .iter()
                .rposition(Piece::is_content)
                .map_or(pieces.len(), |i| i + 1);
            pieces.insert(at, piece);
        }
        pieces
    }

    /// A piece of text that is not in the paragraph.
    fn extra(
        &self,
        text: &str,
        style: &Arc<ComputedValues>,
        level: u8,
        inline_box: usize,
    ) -> Piece {
        let font = select(style).font;
        let size = super::paragraph::adjusted_size(style, &font);
        let glyphs = shape(&font, size, text);
        Piece {
            width: glyphs.iter().map(|g| g.x_advance).sum(),
            kind: Kind::Extra {
                text: Arc::from(text),
                font,
                size,
                glyphs,
                style: style.clone(),
            },
            pos: self.p.text.len(),
            level,
            inline_box,
            x: 0.0,
        }
    }

    /// Fills `leader()`s with their pattern, sharing the space left on the line between
    /// them (CSS GCPM 3 § 5). The pattern repeats a whole number of times and sits at the
    /// end of the space, so the content after a leader lines up at the end of the line.
    fn fill_leaders(&self, pieces: &mut Vec<Piece>, width: f32) {
        let p = self.p;
        let leader = |piece: &Piece| match piece.kind {
            Kind::Atomic(index) => p.atomics[index].leader.clone(),
            _ => None,
        };
        let count = pieces
            .iter()
            .filter(|piece| leader(piece).is_some())
            .count();
        if count == 0 {
            return;
        }
        let content: f32 = pieces.iter().map(|piece| piece.width).sum();
        let share = ((width - content) / count as f32).max(0.0);
        let mut i = 0;
        while i < pieces.len() {
            let Some((pattern, style)) = leader(&pieces[i]) else {
                i += 1;
                continue;
            };
            let (level, inline_box) = (pieces[i].level, pieces[i].inline_box);
            let unit = self.extra(&pattern, &style, level, inline_box).width;
            let copies = if unit > 0.0 {
                (share / unit + EPSILON).floor() as usize
            } else {
                0
            };
            let fill = self.extra(&pattern.repeat(copies), &style, level, inline_box);
            let gap = Piece {
                kind: Kind::Tab,
                pos: pieces[i].pos,
                level,
                inline_box,
                width: (share - fill.width).max(0.0),
                x: 0.0,
            };
            pieces.splice(i..=i, [gap, fill]);
            i += 2;
        }
    }

    /// Bidi levels of pieces that have no text of their own: tabs take the paragraph level
    /// (UAX #9 rule L1) and box edges take the level of the content next to them.
    fn assign_levels(&self, pieces: &mut [Piece]) {
        let base = u8::from(self.p.base_rtl);
        let content_levels: Vec<Option<u8>> = pieces
            .iter()
            .map(|piece| match piece.kind {
                Kind::Tab => Some(base),
                Kind::Edge { .. } => None,
                _ => Some(piece.level),
            })
            .collect();
        for (i, piece) in pieces.iter_mut().enumerate() {
            piece.level = match piece.kind {
                Kind::Tab => base,
                Kind::Edge { start } => {
                    let found = if start {
                        content_levels[i..].iter().flatten().next()
                    } else {
                        content_levels[..i].iter().rev().flatten().next()
                    };
                    found.copied().unwrap_or(base).max(base)
                }
                _ => piece.level,
            };
        }
    }

    /// Letter spacing is not applied after the last character of a line.
    fn trim_letter_spacing(&self, pieces: &mut [Piece]) {
        let p = self.p;
        let Some(last) = pieces.iter().rposition(Piece::is_content) else {
            return;
        };
        let Kind::Text { run, .. } = pieces[last].kind else {
            return;
        };
        let style = &p.spans[p.runs[run].span].style;
        let letter = spacing(&style.letter_spacing, style.font_size);
        if letter == 0.0 {
            return;
        }
        let piece = &mut pieces[last];
        if let Some(glyphs) = piece.glyphs_mut()
            && let Some(glyph) = glyphs.iter_mut().max_by_key(|g| g.text.start)
        {
            glyph.x_advance -= letter;
            piece.width -= letter;
        }
    }

    /// Truncates the line to make room for an ellipsis and adds it.
    fn add_ellipsis(&self, pieces: &mut Vec<Piece>, ellipsis: &str, force: bool, width: f32) {
        let total = |pieces: &[Piece]| pieces.iter().map(|piece| piece.width).sum::<f32>();
        if !force && total(pieces) <= width + EPSILON {
            return;
        }
        let base = u8::from(self.p.base_rtl);
        let mark = self.extra(ellipsis, self.container, base, 0);
        while total(pieces) + mark.width > width + EPSILON {
            let Some(last) = pieces.last_mut() else {
                break;
            };
            let removable = match last.glyphs_mut() {
                Some(glyphs) if glyphs.len() > 1 => {
                    let cluster = glyphs
                        .iter()
                        .map(|g| g.text.start)
                        .max()
                        .expect("not empty");
                    glyphs.retain(|g| g.text.start != cluster);
                    last.width = glyphs.iter().map(|g| g.x_advance).sum();
                    false
                }
                _ => true,
            };
            if removable {
                pieces.pop();
            }
        }
        pieces.push(mark);
    }

    /// Width of punctuation that hangs past the end of the line (`hanging-punctuation`).
    fn hanging_end(
        &self,
        pieces: &[Piece],
        options: &LineOptions<'_>,
        content: f32,
        width: f32,
    ) -> f32 {
        let hanging = self.container.hanging_punctuation;
        let Some(last) = pieces.iter().rev().find(|piece| piece.is_content()) else {
            return 0.0;
        };
        let Kind::Text { glyphs, .. } = &last.kind else {
            return 0.0;
        };
        let Some(glyph) = glyphs.iter().max_by_key(|g| g.text.start) else {
            return 0.0;
        };
        let c = self.p.text[glyph.text.clone()]
            .chars()
            .next()
            .unwrap_or(' ');
        let stop = matches!(
            c,
            ',' | '.'
                | '\u{60C}'
                | '\u{6D4}'
                | '\u{3001}'
                | '\u{3002}'
                | '\u{FF0C}'
                | '\u{FF0E}'
                | '\u{FE50}'
                | '\u{FE51}'
                | '\u{FE52}'
                | '\u{FF61}'
                | '\u{FF64}'
        );
        let hangs = match hanging.force_end {
            Some(true) => stop,
            Some(false) => stop && content > width + EPSILON,
            None => false,
        } || (hanging.last && options.paragraph_end && is_closing(c));
        if hangs { glyph.x_advance } else { 0.0 }
    }

    /// The alignment of a line (CSS Text 3 §§ 7.1–7.2).
    fn alignment(&self, last: bool) -> TextAlign {
        let style = self.container;
        let align = style.text_align;
        if !last {
            return match align {
                TextAlign::JustifyAll => TextAlign::Justify,
                other => other,
            };
        }
        match style.text_align_last {
            TextAlignLast::Auto => match align {
                TextAlign::JustifyAll => TextAlign::Justify,
                TextAlign::Justify => TextAlign::Start,
                other => other,
            },
            TextAlignLast::Start => TextAlign::Start,
            TextAlignLast::End => TextAlign::End,
            TextAlignLast::Left => TextAlign::Left,
            TextAlignLast::Right => TextAlign::Right,
            TextAlignLast::Center => TextAlign::Center,
            TextAlignLast::Justify => TextAlign::Justify,
            TextAlignLast::MatchParent => TextAlign::MatchParent,
        }
    }

    /// Spreads `free` space over justification opportunities. Returns `false` when there
    /// are none, and the line is start-aligned instead.
    fn justify(&self, pieces: &mut [Piece], free: f32) -> bool {
        let justify = self.container.text_justify;
        if free <= 0.0 || justify == TextJustify::None {
            return false;
        }
        let text = &*self.p.text;
        let first_char = |g: &Glyph| text[g.text.clone()].chars().next().unwrap_or(' ');
        // The glyph that ends each cluster gets the space after it.
        let ends: Vec<(usize, usize, char)> = pieces
            .iter()
            .enumerate()
            .filter_map(|(i, piece)| match &piece.kind {
                Kind::Text { glyphs, .. } => Some((i, glyphs)),
                _ => None,
            })
            .flat_map(|(i, glyphs)| {
                glyphs
                    .iter()
                    .enumerate()
                    .filter(move |(j, g)| glyphs.get(j + 1).is_none_or(|n| n.text != g.text))
                    .map(move |(j, g)| (i, j, first_char(g)))
            })
            .collect();
        let separators: Vec<(usize, usize)> = ends
            .iter()
            .filter(|(_, _, c)| is_word_separator(*c))
            .map(|&(i, j, _)| (i, j))
            .collect();
        let ideographic = ends.iter().any(|(_, _, c)| is_ideograph(*c));
        let targets: Vec<(usize, usize)> = match justify {
            TextJustify::InterWord => separators,
            TextJustify::Auto if !separators.is_empty() || !ideographic => separators,
            _ => ends[..ends.len().saturating_sub(1)]
                .iter()
                .map(|&(i, j, _)| (i, j))
                .collect(),
        };
        if targets.is_empty() {
            return false;
        }
        let each = free / targets.len() as f32;
        for (i, j) in targets {
            if let Some(glyphs) = pieces[i].glyphs_mut() {
                glyphs[j].x_advance += each;
            }
            pieces[i].width += each;
        }
        true
    }

    /// Baselines of every inline box and atomic inline on the line, relative to the root
    /// inline box's baseline, and the line's extent (CSS 2.1 § 10.8, CSS Inline 3 § 4).
    fn baselines(&self, pieces: &[Piece]) -> Baselines {
        let p = self.p;
        let root = &self.metrics[0];
        // Units are the inline boxes and atomic inlines on the line.
        let mut boxes: Vec<usize> = Vec::new();
        for piece in pieces {
            for &b in &p.boxes[piece.inline_box].chain {
                if !boxes.contains(&b) {
                    boxes.push(b);
                }
            }
        }
        boxes.sort_by_key(|&b| p.boxes[b].chain.len());

        #[derive(Clone, Copy, PartialEq)]
        enum Anchor {
            Top,
            Bottom,
            Center,
        }
        let mut box_rel = vec![0.0f32; p.boxes.len()];
        let mut box_anchor: Vec<Option<usize>> = vec![None; p.boxes.len()];
        // Special anchors (top, bottom, center), indexed by unit: (unit kind, its anchor).
        let mut anchors: Vec<(Anchor, f32, f32)> = Vec::new();
        let mut root_extent = root.bounds();

        let align = |style: &ComputedValues,
                     parent: &BoxMetrics,
                     ascent: f32,
                     descent: f32,
                     line_height: f32|
         -> Result<f32, Anchor> {
            let base = match style.alignment_baseline {
                AlignmentBaseline::Baseline | AlignmentBaseline::Alphabetic => 0.0,
                AlignmentBaseline::TextTop => -parent.ascent + ascent,
                AlignmentBaseline::TextBottom | AlignmentBaseline::Ideographic => {
                    parent.descent - descent
                }
                AlignmentBaseline::Middle | AlignmentBaseline::Mathematical => {
                    -parent.x_height / 2.0 + (ascent - descent) / 2.0
                }
                AlignmentBaseline::Central => {
                    -(parent.ascent - parent.descent) / 2.0 + (ascent - descent) / 2.0
                }
            };
            match &style.baseline_shift {
                BaselineShift::Value(value) => Ok(base - value.resolve(line_height)),
                BaselineShift::Sub => Ok(base + parent.subscript),
                BaselineShift::Super => Ok(base - parent.superscript),
                BaselineShift::Top => Err(Anchor::Top),
                BaselineShift::Bottom => Err(Anchor::Bottom),
                BaselineShift::Center => Err(Anchor::Center),
            }
        };

        for &b in &boxes {
            let ib = &p.boxes[b];
            let m = &self.metrics[b];
            let parent = ib.parent;
            let (top, bottom) = m.bounds();
            match align(
                &ib.style,
                &self.metrics[parent],
                m.ascent,
                m.descent,
                m.line_height,
            ) {
                Ok(rel) => {
                    box_rel[b] = box_rel[parent] + rel;
                    box_anchor[b] = box_anchor[parent];
                }
                Err(kind) => {
                    box_rel[b] = 0.0;
                    box_anchor[b] = Some(anchors.len());
                    anchors.push((kind, top, bottom));
                }
            }
            let rel = box_rel[b];
            match box_anchor[b] {
                None => {
                    root_extent.0 = root_extent.0.min(rel + top);
                    root_extent.1 = root_extent.1.max(rel + bottom);
                }
                Some(a) => {
                    anchors[a].1 = anchors[a].1.min(rel + top);
                    anchors[a].2 = anchors[a].2.max(rel + bottom);
                }
            }
        }

        // Atomic inlines, by piece index.
        let mut atomic_rel: Vec<(usize, f32, Option<usize>)> = Vec::new();
        for (i, piece) in pieces.iter().enumerate() {
            let Kind::Atomic(index) = piece.kind else {
                continue;
            };
            let atomic = &p.atomics[index];
            let style = &atomic.fragment.style;
            let parent = piece.inline_box;
            let ascent = atomic.baseline;
            let descent = atomic.height - atomic.baseline;
            let line_height = style.line_height.to_px(style.font_size);
            let (top, bottom) = (-ascent, descent);
            match align(style, &self.metrics[parent], ascent, descent, line_height) {
                Ok(rel) => {
                    let rel = box_rel[parent] + rel;
                    let anchor = box_anchor[parent];
                    match anchor {
                        None => {
                            root_extent.0 = root_extent.0.min(rel + top);
                            root_extent.1 = root_extent.1.max(rel + bottom);
                        }
                        Some(a) => {
                            anchors[a].1 = anchors[a].1.min(rel + top);
                            anchors[a].2 = anchors[a].2.max(rel + bottom);
                        }
                    }
                    atomic_rel.push((i, rel, anchor));
                }
                Err(kind) => {
                    atomic_rel.push((i, 0.0, Some(anchors.len())));
                    anchors.push((kind, top, bottom));
                }
            }
        }

        // Top- and bottom-aligned subtrees can make the line taller.
        let (mut top, mut bottom) = root_extent;
        for pass in 0..3 {
            for (kind, t, b) in &anchors {
                let height = b - t;
                let fits = bottom - top;
                if height <= fits {
                    continue;
                }
                match (pass, kind) {
                    (0, Anchor::Top) => bottom = top + height,
                    (1, Anchor::Bottom) => top = bottom - height,
                    (2, Anchor::Center) => {
                        let grow = (height - fits) / 2.0;
                        top -= grow;
                        bottom += grow;
                    }
                    _ => {}
                }
            }
        }
        let step = self.container.line_height_step;
        if step > 0.0 {
            let height = bottom - top;
            let stepped = (height / step - 1e-4).ceil() * step;
            let extra = stepped - height;
            top -= extra / 2.0;
            bottom += extra / 2.0;
        }
        let anchor_baseline: Vec<f32> = anchors
            .iter()
            .map(|(kind, t, b)| match kind {
                Anchor::Top => top - t,
                Anchor::Bottom => bottom - b,
                Anchor::Center => (top + bottom) / 2.0 - (t + b) / 2.0,
            })
            .collect();
        let resolve = |rel: f32, anchor: Option<usize>| match anchor {
            None => rel,
            Some(a) => anchor_baseline[a] + rel,
        };
        let box_baselines: Vec<f32> = (0..p.boxes.len())
            .map(|b| resolve(box_rel[b], box_anchor[b]))
            .collect();
        let mut atomics = vec![0.0f32; pieces.len()];
        for (i, rel, anchor) in atomic_rel {
            atomics[i] = resolve(rel, anchor);
        }
        Baselines {
            boxes: box_baselines,
            atomics,
            extent: (top, bottom),
        }
    }

    /// Builds line items from pieces in visual order, nesting inline box fragments.
    /// `baseline` converts baselines relative to the root baseline into line coordinates.
    fn build(
        &self,
        pieces: &[Piece],
        depth: usize,
        boxes: &[f32],
        atomics: &[f32],
        root_y: f32,
    ) -> Vec<LineItem> {
        let p = self.p;
        let chain = |piece: &Piece| &p.boxes[piece.inline_box].chain;
        let mut items = Vec::new();
        let mut i = 0;
        while i < pieces.len() {
            let piece_chain = chain(&pieces[i]);
            if piece_chain.len() > depth {
                let b = piece_chain[depth];
                let mut j = i;
                while j < pieces.len() && chain(&pieces[j]).get(depth) == Some(&b) {
                    j += 1;
                }
                let group = &pieces[i..j];
                let children = self.build(group, depth + 1, boxes, atomics, root_y);
                let mut inline = self.inline_fragment(b, group, children, boxes, root_y);
                let (dx, dy) = self.relative_offset(&inline.style);
                if dx != 0.0 || dy != 0.0 {
                    inline.rect = inline.rect.translated(dx, dy);
                    offset_items(&mut inline.children, dx, dy);
                }
                items.push(LineItem::Inline(inline));
                i = j;
                continue;
            }
            let piece = &pieces[i];
            let baseline = root_y + boxes[piece.inline_box];
            match &piece.kind {
                Kind::Text { run, glyphs } => {
                    let r = &p.runs[*run];
                    let span = &p.spans[r.span];
                    let mut run = self.text_run(
                        Point::new(piece.x, baseline + r.shift),
                        r.font.clone(),
                        r.size,
                        glyphs.clone(),
                        p.text.clone(),
                        &span.style,
                        r.oblique,
                        r.embolden,
                    );
                    run.upright = r.upright;
                    items.push(LineItem::Text(TextFragment {
                        decorations: self.decorations(&span.style, piece.inline_box, boxes, root_y),
                        run,
                        style: span.style.clone(),
                    }));
                }
                Kind::Extra {
                    text,
                    font,
                    size,
                    glyphs,
                    style,
                } => {
                    let choice = select(style);
                    let run = self.text_run(
                        Point::new(piece.x, baseline),
                        font.clone(),
                        *size,
                        glyphs.clone(),
                        text.clone(),
                        style,
                        choice.synthetic_oblique,
                        choice.synthetic_bold,
                    );
                    items.push(LineItem::Text(TextFragment {
                        decorations: self.decorations(style, piece.inline_box, boxes, root_y),
                        run,
                        style: style.clone(),
                    }));
                }
                Kind::Atomic(index) => {
                    let atomic = &p.atomics[*index];
                    let mut fragment = atomic.fragment.clone();
                    let (dx, dy) = self.relative_offset(&fragment.style);
                    fragment.rect.x = piece.x + atomic.margin_left + dx;
                    fragment.rect.y =
                        root_y + atomics[i] - atomic.baseline + atomic.margin_top + dy;
                    items.push(LineItem::Box(fragment));
                }
                Kind::Tab | Kind::Edge { .. } => {}
            }
            i += 1;
        }
        items
    }

    /// How far `position: relative` moves an inline-level box. Percentages refer to the
    /// block container's width; vertical ones to its height, which depends on content.
    fn relative_offset(&self, style: &ComputedValues) -> (f32, f32) {
        let cb = crate::flow::ContainingBlock {
            width: self.width,
            height: None,
            rtl: self.container.direction == rustyprint_css::properties::types::Direction::Rtl,
        };
        crate::position::relative_offset(style, cb)
    }

    #[allow(clippy::too_many_arguments)]
    fn text_run(
        &self,
        origin: Point,
        font: Font,
        size: f32,
        glyphs: Vec<Glyph>,
        text: Arc<str>,
        style: &ComputedValues,
        oblique: f32,
        embolden: bool,
    ) -> TextRun {
        let color = color(&style.color);
        let stroke_color = match &style.stroke {
            Paint::Color(c) => Some(c.resolve(&style.color)),
            Paint::Url(_, Some(c)) => Some(c.resolve(&style.color)),
            _ => {
                let c = style.stroke_color.resolve(&style.color);
                (c.alpha() > 0.0).then_some(c)
            }
        };
        let stroke = stroke_color.map(|c| TextStroke {
            color: color_of(&c),
            width: 1.0,
        });
        let order = style.paint_order;
        let stroke_first = order.iter().position(|&o| o == 1) < order.iter().position(|&o| o == 0);
        TextRun {
            origin,
            font,
            size,
            color,
            glyphs,
            text,
            oblique,
            embolden: if embolden { size / 24.0 } else { 0.0 },
            stroke,
            fill: true,
            stroke_first,
            upright: None,
        }
    }

    /// The fragment of inline box `b` made of `group`, which is in visual order.
    fn inline_fragment(
        &self,
        b: usize,
        group: &[Piece],
        children: Vec<LineItem>,
        boxes: &[f32],
        root_y: f32,
    ) -> InlineFragment {
        let ib = &self.p.boxes[b];
        let m = &self.metrics[b];
        let mut left = group
            .iter()
            .map(|piece| piece.x)
            .fold(f32::INFINITY, f32::min);
        let mut right = group
            .iter()
            .map(|piece| piece.x + piece.width)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut border = Sides {
            top: ib.border.top,
            bottom: ib.border.bottom,
            ..Sides::default()
        };
        let mut padding = Sides {
            top: ib.padding.top,
            bottom: ib.padding.bottom,
            ..Sides::default()
        };
        for piece in group {
            if piece.inline_box != b {
                continue;
            }
            if let Kind::Edge { start } = piece.kind {
                if start == ib.start_is_left() {
                    left = piece.x + ib.margin.left;
                    border.left = ib.border.left;
                    padding.left = ib.padding.left;
                } else {
                    right = piece.x + piece.width - ib.margin.right;
                    border.right = ib.border.right;
                    padding.right = ib.padding.right;
                }
            }
        }
        if !left.is_finite() {
            left = 0.0;
            right = 0.0;
        }
        let baseline = root_y + boxes[b];
        let top = baseline - m.ascent - padding.top - border.top;
        let height = m.ascent + m.descent + padding.vertical() + border.vertical();
        InlineFragment {
            node: ib.node,
            style: ib.style.clone(),
            rect: Rect::new(left, top, (right - left).max(0.0), height),
            border,
            padding,
            children,
        }
    }

    /// The decorations drawn across text in inline box `b` by its decorating ancestors.
    fn decorations(
        &self,
        text_style: &ComputedValues,
        b: usize,
        boxes: &[f32],
        root_y: f32,
    ) -> Vec<Decoration> {
        let mut out = Vec::new();
        for style in self.decorators {
            let m = BoxMetrics::of(style);
            decorations_of(
                style,
                text_style,
                &m,
                root_y,
                Arc::as_ptr(style) as usize,
                &mut out,
            );
        }
        for &inner in &self.p.boxes[b].chain {
            let ib = &self.p.boxes[inner];
            if !ib.style.text_decoration_line.is_none() {
                decorations_of(
                    &ib.style,
                    text_style,
                    &self.metrics[inner],
                    root_y + boxes[inner],
                    Arc::as_ptr(&ib.style) as usize,
                    &mut out,
                );
            }
        }
        out
    }
}

struct Baselines {
    /// Each inline box's baseline relative to the root baseline.
    boxes: Vec<f32>,
    /// Each atomic inline's baseline, by piece index.
    atomics: Vec<f32>,
    /// The line's top and bottom relative to the root baseline.
    extent: (f32, f32),
}

/// The lines a decorating box draws across text (CSS Text Decoration 4 § 2).
fn decorations_of(
    style: &ComputedValues,
    text_style: &ComputedValues,
    m: &BoxMetrics,
    baseline: f32,
    source: usize,
    out: &mut Vec<Decoration>,
) {
    let lines = style.text_decoration_line;
    if lines.is_none() {
        return;
    }
    let font = &m.font;
    let metrics = font.metrics();
    let px = |units: i16| font.scale(units, m.size);
    let color = color(&style.text_decoration_color.resolve(&style.color));
    let thickness = match &style.text_decoration_thickness {
        DecorationLength::Value(value) => value.resolve(style.font_size),
        _ => px(metrics.underline_thickness).max(0.5),
    };
    let skip = |kind: DecorationKind| {
        kind != DecorationKind::LineThrough
            && text_style.text_decoration_skip_ink != TextDecorationSkipInk::None
    };
    let skip_spaces = {
        let s = text_style.text_decoration_skip;
        s.leading_spaces && s.trailing_spaces
    };
    let inset = match style.text_decoration_inset {
        rustyprint_css::properties::types::TextDecorationInset::Lengths(a, b) => (a, b),
        rustyprint_css::properties::types::TextDecorationInset::Auto => (0.0, 0.0),
    };
    let mut push = |kind: DecorationKind,
                    y: f32,
                    thickness: f32,
                    color: Color,
                    style_: TextDecorationStyle| {
        out.push(Decoration {
            kind,
            y,
            thickness,
            color,
            style: style_,
            skip_ink: skip(kind),
            skip_spaces,
            inset,
            source: source ^ (kind as usize) << 1,
        });
    };
    let deco_style = style.text_decoration_style;
    if lines.underline {
        let position = style.text_underline_position;
        let edge = if position.under { m.descent } else { 0.0 };
        // In vertical text, `left` or `right` can put the underline on the over side.
        let over = position.left.and_then(|left| {
            crate::writing::left_is_under(style.writing_mode, left).map(|under| !under)
        }) == Some(true);
        let top = match &style.text_underline_offset {
            _ if over => baseline - m.ascent,
            DecorationLength::Value(offset) => baseline + edge + offset.resolve(style.font_size),
            _ if position.under => baseline + m.descent,
            _ => baseline - px(metrics.underline_position),
        };
        push(
            DecorationKind::Underline,
            top + thickness / 2.0,
            thickness,
            color,
            deco_style,
        );
    }
    if lines.overline {
        push(
            DecorationKind::Overline,
            baseline - m.ascent + thickness / 2.0,
            thickness,
            color,
            deco_style,
        );
    }
    if lines.line_through {
        let through = match &style.text_decoration_thickness {
            DecorationLength::Value(_) => thickness,
            _ => px(metrics.strikeout_thickness).max(0.5),
        };
        push(
            DecorationKind::LineThrough,
            baseline - px(metrics.strikeout_position) + through / 2.0,
            through,
            color,
            deco_style,
        );
    }
    // Spelling and grammar errors get the usual wavy underlines.
    if lines.spelling_error || lines.grammar_error {
        let color = if lines.spelling_error {
            Color::rgb(255, 0, 0)
        } else {
            Color::rgb(0, 128, 0)
        };
        let top = baseline - px(metrics.underline_position);
        push(
            DecorationKind::Underline,
            top + thickness,
            thickness,
            color,
            TextDecorationStyle::Wavy,
        );
    }
}

fn color(color: &AbsoluteColor) -> Color {
    color_of(color)
}

fn color_of(color: &AbsoluteColor) -> Color {
    Color::from_css(color)
}

/// Closing brackets and quotes, which `hanging-punctuation: last` lets hang.
fn is_closing(c: char) -> bool {
    matches!(
        c,
        ')' | ']'
            | '}'
            | '"'
            | '\''
            | '\u{BB}'
            | '\u{2019}'
            | '\u{201D}'
            | '\u{203A}'
            | '\u{300D}'
            | '\u{300F}'
            | '\u{FF09}'
    )
}

/// Opening brackets and quotes, which `hanging-punctuation: first` lets hang.
pub(crate) fn is_opening(c: char) -> bool {
    matches!(
        c,
        '(' | '['
            | '{'
            | '"'
            | '\''
            | '\u{AB}'
            | '\u{2018}'
            | '\u{201C}'
            | '\u{2039}'
            | '\u{300C}'
            | '\u{300E}'
            | '\u{FF08}'
    )
}

/// The visual order of items with bidi levels (UAX #9 rule L2).
pub(crate) fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let Some(&max) = levels.iter().max() else {
        return order;
    };
    let Some(min_odd) = levels.iter().copied().filter(|l| l % 2 == 1).min() else {
        return order;
    };
    for level in (min_odd..=max).rev() {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= level {
                let start = i;
                while i < order.len() && levels[order[i]] >= level {
                    i += 1;
                }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }
    order
}

/// The direction of a style, for callers outside this module.
#[allow(dead_code)]
pub(crate) fn is_rtl(style: &ComputedValues) -> bool {
    style.direction == Direction::Rtl
}

/// Moves line items, and the items inside inline boxes, by `(dx, dy)`.
fn offset_items(items: &mut [LineItem], dx: f32, dy: f32) {
    for item in items {
        match item {
            LineItem::Text(text) => {
                text.run.origin.x += dx;
                text.run.origin.y += dy;
                for decoration in &mut text.decorations {
                    decoration.y += dy;
                }
            }
            LineItem::Box(fragment) => fragment.rect = fragment.rect.translated(dx, dy),
            LineItem::Inline(inline) => {
                inline.rect = inline.rect.translated(dx, dy);
                offset_items(&mut inline.children, dx, dy);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorders_levels() {
        assert_eq!(visual_order(&[0, 0, 0]), [0, 1, 2]);
        assert_eq!(visual_order(&[0, 1, 1, 0]), [0, 2, 1, 3]);
        assert_eq!(visual_order(&[1, 1, 2, 2, 1]), [4, 2, 3, 1, 0]);
    }
}
