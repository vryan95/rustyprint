//! Line breaking: soft wrap opportunities (CSS Text 3 § 5, UAX #14), hyphenation and
//! choosing where each line ends.

use std::cell::RefCell;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    Hyphens, LineBreak as LineBreakKeyword, OverflowWrap, TextWrapMode, TextWrapStyle,
    WhiteSpaceCollapse, WordBreak,
};
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

use super::hyphenate;
use super::paragraph::{Paragraph, SpanKind, advance_of};
use crate::fonts::select;

/// A place where a line may end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Opportunity {
    pub pos: usize,
    pub mandatory: bool,
    /// Whether breaking here inserts a hyphen.
    pub hyphen: bool,
}

/// Where a line starts and ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineBreak {
    pub start: usize,
    pub end: usize,
    pub hyphen: bool,
    /// Whether the line ends with a forced break (or the end of the paragraph).
    pub forced: bool,
}

/// Lines can be this much wider than the space without overflowing, for rounding.
const EPSILON: f32 = 0.01;

/// Soft wrap opportunities and forced breaks of a paragraph, sorted, ending with the end of
/// the text.
pub(crate) fn opportunities(p: &Paragraph) -> Vec<Opportunity> {
    let text = &*p.text;
    let mut out: Vec<Opportunity> = Vec::new();
    for (pos, kind) in linebreaks(text) {
        if pos == 0 {
            continue;
        }
        let mandatory = kind == BreakOpportunity::Mandatory;
        let before = char_before(text, pos);
        let shy = before == Some('\u{AD}');
        if !mandatory && pos < text.len() {
            let style_before = p.style_at(pos - 1);
            let style_after = p.style_at(pos);
            if !wraps(style_before) || !wraps(style_after) {
                continue;
            }
            if shy && style_before.hyphens == Hyphens::None {
                continue;
            }
            let after = text[pos..].chars().next();
            let keep =
                |s: &ComputedValues| matches!(s.word_break, WordBreak::KeepAll | WordBreak::Manual);
            if keep(style_before)
                && before.is_some_and(char::is_alphanumeric)
                && after.is_some_and(char::is_alphanumeric)
            {
                continue;
            }
        }
        out.push(Opportunity {
            pos,
            mandatory: mandatory && pos < text.len() || pos == text.len(),
            hyphen: shy && !mandatory,
        });
    }
    extra_opportunities(p, &mut out);
    out.sort_by_key(|o| o.pos);
    out.dedup_by(|later, earlier| {
        if later.pos != earlier.pos {
            return false;
        }
        earlier.mandatory |= later.mandatory;
        earlier.hyphen &= later.hyphen;
        true
    });
    out
}

fn char_before(text: &str, pos: usize) -> Option<char> {
    text[..pos].chars().next_back()
}

/// Whether text in a style may wrap.
fn wraps(style: &ComputedValues) -> bool {
    style.text_wrap_mode == TextWrapMode::Wrap
}

/// Opportunities that UAX #14 does not give: `word-break: break-all`, `line-break: anywhere`,
/// `white-space: break-spaces` and automatic hyphenation.
fn extra_opportunities(p: &Paragraph, out: &mut Vec<Opportunity>) {
    let text = &*p.text;
    for span in &p.spans {
        if span.kind != SpanKind::Text || !wraps(&span.style) {
            continue;
        }
        let style = &span.style;
        let anywhere = style.line_break == LineBreakKeyword::Anywhere;
        let break_all = style.word_break == WordBreak::BreakAll;
        if anywhere || break_all {
            let piece = &text[span.range.clone()];
            let mut previous: Option<char> = None;
            for (offset, grapheme) in piece.grapheme_indices(true) {
                let first = grapheme.chars().next();
                let pos = span.range.start + offset;
                let allowed = anywhere
                    || (previous.is_some_and(char::is_alphanumeric)
                        && first.is_some_and(char::is_alphanumeric));
                if pos > 0 && allowed {
                    out.push(Opportunity {
                        pos,
                        mandatory: false,
                        hyphen: false,
                    });
                }
                previous = grapheme.chars().next_back();
            }
        }
        if style.white_space_collapse == WhiteSpaceCollapse::BreakSpaces {
            for (offset, c) in text[span.range.clone()].char_indices() {
                if c == ' ' {
                    let pos = span.range.start + offset + 1;
                    if pos < text.len() {
                        out.push(Opportunity {
                            pos,
                            mandatory: false,
                            hyphen: false,
                        });
                    }
                }
            }
        }
        if style.hyphens == Hyphens::Auto
            && let Some(lang) = &span.lang
        {
            let [word_limit, before, after] = style.hyphenate_limit_chars;
            let word_limit = word_limit.unwrap_or(5) as usize;
            let before = before.unwrap_or(2) as usize;
            let after = after.map_or(before, |a| a as usize);
            let piece = &text[span.range.clone()];
            for (offset, word) in piece.split_word_bound_indices() {
                if word.chars().count() < word_limit || !word.chars().all(char::is_alphabetic) {
                    continue;
                }
                for point in hyphenate::points(word, lang, before, after) {
                    out.push(Opportunity {
                        pos: span.range.start + offset + point,
                        mandatory: false,
                        hyphen: true,
                    });
                }
            }
        }
    }
}

/// Measures lines for breaking.
pub(crate) struct Breaker<'a> {
    pub p: &'a Paragraph,
    pub opportunities: Vec<Opportunity>,
    /// The width of the hyphen string of each span, once measured.
    hyphens: RefCell<Vec<Option<f32>>>,
}

/// The start offset and the available width of a line, by line index and whether the line
/// starts the paragraph or follows a forced break.
pub(crate) type Geometry<'a> = &'a dyn Fn(usize, bool) -> (f32, f32);

impl<'a> Breaker<'a> {
    pub fn new(p: &'a Paragraph) -> Self {
        Self {
            p,
            opportunities: opportunities(p),
            hyphens: RefCell::new(vec![None; p.spans.len()]),
        }
    }

    /// The hyphen inserted when breaking at `pos`: its text.
    pub fn hyphen_text(style: &ComputedValues) -> String {
        match &style.hyphenate_character {
            Some(s) => s.to_string(),
            None => {
                let font = select(style).font;
                if font.coverage("\u{2010}")[0] {
                    "\u{2010}".into()
                } else {
                    "-".into()
                }
            }
        }
    }

    /// The width of the hyphen inserted when breaking at `pos`.
    pub fn hyphen_width(&self, pos: usize) -> f32 {
        let index = self
            .p
            .spans
            .partition_point(|s| s.range.end < pos)
            .min(self.p.spans.len() - 1);
        if let Some(width) = self.hyphens.borrow()[index] {
            return width;
        }
        let style = &self.p.spans[index].style;
        let font = select(style).font;
        let width = Self::hyphen_text(style)
            .chars()
            .map(|c| advance_of(&font, c, style.font_size))
            .sum();
        self.hyphens.borrow_mut()[index] = Some(width);
        width
    }

    /// The end of `[start, end)` without the white space that hangs at the end of a line
    /// and the forced break character.
    pub fn trim_end(&self, start: usize, end: usize) -> usize {
        let text = &*self.p.text;
        let mut end = end;
        while end > start {
            let Some(c) = char_before(text, end) else {
                break;
            };
            let style = self.p.style_at(end - c.len_utf8());
            let hangs = match c {
                '\n' => true,
                ' ' | '\t' | '\u{3000}' => match style.white_space_collapse {
                    WhiteSpaceCollapse::Collapse
                    | WhiteSpaceCollapse::PreserveBreaks
                    | WhiteSpaceCollapse::Discard => true,
                    WhiteSpaceCollapse::Preserve | WhiteSpaceCollapse::PreserveSpaces => {
                        style.text_wrap_mode == TextWrapMode::Wrap
                    }
                    WhiteSpaceCollapse::BreakSpaces => false,
                },
                _ => false,
            };
            if !hangs {
                break;
            }
            end -= c.len_utf8();
        }
        end
    }

    /// Skips the collapsible spaces at the start of a line.
    pub fn skip_start(&self, pos: usize) -> usize {
        let text = &*self.p.text;
        let mut pos = pos;
        while pos < text.len() {
            let c = text[pos..].chars().next().expect("in bounds");
            let style = self.p.style_at(pos);
            let collapsible = c == ' '
                && matches!(
                    style.white_space_collapse,
                    WhiteSpaceCollapse::Collapse | WhiteSpaceCollapse::PreserveBreaks
                );
            if !collapsible {
                break;
            }
            pos += 1;
        }
        pos
    }

    /// The width of a line over `[start, end)` that starts at `x`.
    pub fn line_width(&self, start: usize, end: usize, x: f32, hyphen: bool) -> f32 {
        let trimmed = self.trim_end(start, end);
        let mut width = self.p.measure(start, trimmed, x) - x;
        // Edges at the very end belong to the line even though the trailing spaces hang.
        if trimmed < end {
            width += self
                .p
                .edges_between(start, end)
                .filter(|e| e.pos > trimmed)
                .map(|e| self.p.edge_width(e))
                .sum::<f32>();
        }
        if hyphen {
            width += self.hyphen_width(end);
        }
        width
    }

    /// Greedy line breaking: each line takes as much as fits.
    #[cfg(test)]
    pub fn break_lines(&self, geometry: Geometry<'_>) -> Vec<LineBreak> {
        self.break_from(0, 0, true, geometry, &|_| None)
    }

    /// Breaks lines from `start`, numbering them from `first_line`; `fresh` tells whether
    /// the first follows a forced break. `limit` can narrow the width of a line.
    fn break_from(
        &self,
        start: usize,
        first_line: usize,
        fresh: bool,
        geometry: Geometry<'_>,
        limit: &dyn Fn(usize) -> Option<f32>,
    ) -> Vec<LineBreak> {
        let mut fresh = fresh;
        let len = self.p.text.len();
        let mut lines = Vec::new();
        if len == 0 {
            return lines;
        }
        let mut start = self.skip_start(start);
        let mut k = 0;
        let ops = &self.opportunities;
        loop {
            rustyprint_dom::budget::check_deadline();
            let index = first_line + lines.len();
            let (x0, mut available) = geometry(index, fresh);
            if let Some(width) = limit(index) {
                available = available.min(width);
            }
            while k < ops.len() && ops[k].pos <= start {
                k += 1;
            }
            if k >= ops.len() {
                break;
            }
            let mut best = None;
            let mut j = k;
            while j < ops.len() {
                let o = ops[j];
                let width = self.line_width(start, o.pos, x0, o.hyphen);
                if width <= available + EPSILON {
                    best = Some(j);
                    if o.mandatory {
                        break;
                    }
                    j += 1;
                } else {
                    break;
                }
            }
            let line = match best {
                Some(b) => LineBreak {
                    start,
                    end: ops[b].pos,
                    hyphen: ops[b].hyphen,
                    forced: ops[b].mandatory,
                },
                None => {
                    let first = ops[k];
                    match self.emergency(start, first.pos, x0, available) {
                        Some(end) if end < first.pos => LineBreak {
                            start,
                            end,
                            hyphen: false,
                            forced: false,
                        },
                        _ => LineBreak {
                            start,
                            end: first.pos,
                            hyphen: first.hyphen,
                            forced: first.mandatory,
                        },
                    }
                }
            };
            lines.push(line);
            fresh = line.forced;
            if line.end >= len {
                break;
            }
            start = self.skip_start(line.end);
            if start >= len {
                break;
            }
        }
        lines
    }

    /// An emergency break inside an unbreakable piece that overflows, when `overflow-wrap`
    /// or `word-break` allow one: the last grapheme boundary that fits, but at least one
    /// grapheme.
    fn emergency(&self, start: usize, end: usize, x0: f32, available: f32) -> Option<usize> {
        let style = self.p.style_at(start);
        let allowed = matches!(
            style.overflow_wrap,
            OverflowWrap::Anywhere | OverflowWrap::BreakWord
        ) || style.word_break == WordBreak::BreakWord;
        if !allowed || !wraps(style) {
            return None;
        }
        let text = &self.p.text[start..end];
        let mut best = None;
        for (offset, _) in text.grapheme_indices(true).skip(1) {
            let pos = start + offset;
            if self.line_width(start, pos, x0, false) <= available + EPSILON {
                best = Some(pos);
            } else {
                break;
            }
        }
        best.or_else(|| text.grapheme_indices(true).nth(1).map(|(o, _)| start + o))
    }

    /// Breaks lines from `start` and applies `text-wrap-style`.
    pub fn break_styled(
        &self,
        start: usize,
        geometry: Geometry<'_>,
        style: &ComputedValues,
    ) -> Vec<LineBreak> {
        let lines = self.break_from(start, 0, true, geometry, &|_| None);
        match style.text_wrap_style {
            TextWrapStyle::Balance => self.balance(lines, start, geometry),
            TextWrapStyle::Pretty | TextWrapStyle::AvoidOrphans => self.pretty(lines, geometry),
            TextWrapStyle::Auto | TextWrapStyle::Stable => lines,
        }
    }

    /// Greedy line breaking from `start`, numbering the lines from `index`; `fresh` tells
    /// whether the first follows a forced break.
    pub fn break_rest(
        &self,
        start: usize,
        index: usize,
        fresh: bool,
        geometry: Geometry<'_>,
    ) -> Vec<LineBreak> {
        self.break_from(start, index, fresh, geometry, &|_| None)
    }

    /// Makes the lines as even as possible without adding one (CSS Text 4 § 6.1.1).
    fn balance(
        &self,
        lines: Vec<LineBreak>,
        start: usize,
        geometry: Geometry<'_>,
    ) -> Vec<LineBreak> {
        let count = lines.len();
        let soft = lines[..count.saturating_sub(1)].iter().all(|l| !l.forced);
        if !(2..=10).contains(&count) || !soft {
            return lines;
        }
        let (_, full) = geometry(0, true);
        let (mut low, mut high) = (0.0f32, full);
        for _ in 0..16 {
            let mid = (low + high) / 2.0;
            let trial = self.break_from(start, 0, true, geometry, &|_| Some(mid));
            if trial.len() <= count {
                high = mid;
            } else {
                low = mid;
            }
        }
        let balanced = self.break_from(start, 0, true, geometry, &|_| Some(high));
        if balanced.len() == count {
            balanced
        } else {
            lines
        }
    }

    /// Avoids a last line with a single short word by moving a word down to it.
    fn pretty(&self, lines: Vec<LineBreak>, geometry: Geometry<'_>) -> Vec<LineBreak> {
        let count = lines.len();
        if count < 2 {
            return lines;
        }
        let last = lines[count - 1];
        let penultimate = lines[count - 2];
        if penultimate.forced {
            return lines;
        }
        let inside = |l: &LineBreak| {
            self.opportunities
                .iter()
                .filter(|o| o.pos > l.start && o.pos < l.end && !o.hyphen)
                .count()
        };
        if inside(&last) > 0 || inside(&penultimate) < 2 {
            return lines;
        }
        let target = count - 2;
        let fresh = target == 0 || lines[target - 1].forced;
        let (x0, available) = geometry(target, fresh);
        let used = self.line_width(penultimate.start, penultimate.end, x0, penultimate.hyphen);
        let retry = self.break_from(penultimate.start, target, fresh, geometry, &|i| {
            (i == target).then_some(used - 0.5)
        });
        let shorter = retry
            .first()
            .map_or(0.0, |l| self.line_width(l.start, l.end, x0, l.hyphen));
        if retry.len() == 2 && shorter >= available * 0.6 {
            let mut out = lines[..count - 2].to_vec();
            out.extend(retry);
            out
        } else {
            lines
        }
    }

    /// The min-content and max-content widths of the paragraph (CSS Sizing 3 § 5.1).
    pub fn intrinsic(&self, indent: f32) -> (f32, f32) {
        let len = self.p.text.len();
        if len == 0 {
            return (0.0, 0.0);
        }
        let mut min = 0.0f32;
        let mut max = 0.0f32;
        let mut start = self.skip_start(0);
        let mut line_start = start;
        let mut first_line = true;
        for o in &self.opportunities {
            if o.pos <= start {
                continue;
            }
            let x = if first_line { indent } else { 0.0 };
            min = min.max(self.line_width(start, o.pos, x, o.hyphen) + x);
            start = self.skip_start(o.pos);
            if o.mandatory {
                max = max.max(self.line_width(line_start, o.pos, x, false) + x);
                line_start = start;
                first_line = false;
            }
        }
        // `overflow-wrap: anywhere` lets every grapheme break for min-content.
        let anywhere =
            self.p.spans.iter().any(|s| {
                s.kind == SpanKind::Text && s.style.overflow_wrap == OverflowWrap::Anywhere
            });
        if anywhere {
            min = self
                .p
                .text
                .grapheme_indices(true)
                .map(|(i, g)| self.line_width(i, i + g.len(), 0.0, false))
                .fold(0.0, f32::max);
        }
        (min, max.max(min))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::boxes::InlineItem;

    fn paragraph(text: &str, style: ComputedValues) -> Paragraph {
        let style = Arc::new(style);
        Paragraph::build(
            &[InlineItem::Text {
                text: text.into(),
                style: style.clone(),
                lang: Some("en".into()),
            }],
            &style,
            800.0,
            800.0,
        )
    }

    fn lines(text: &str, style: ComputedValues, width: f32) -> Vec<String> {
        let p = paragraph(text, style);
        let breaker = Breaker::new(&p);
        breaker
            .break_lines(&|_, _| (0.0, width))
            .iter()
            .map(|l| p.text[l.start..breaker.trim_end(l.start, l.end)].to_owned())
            .collect()
    }

    #[test]
    fn breaks_at_spaces() {
        let style = ComputedValues::initial();
        assert_eq!(lines("aaa bbb ccc", style.clone(), 1000.0), ["aaa bbb ccc"]);
        let narrow = lines("aaa bbb ccc", style, 60.0);
        assert!(narrow.len() >= 2, "{narrow:?}");
        assert_eq!(narrow.concat().replace(' ', ""), "aaabbbccc");
    }

    #[test]
    fn nowrap_and_anywhere() {
        let mut nowrap = ComputedValues::initial();
        nowrap.text_wrap_mode = TextWrapMode::Nowrap;
        assert_eq!(lines("aaa bbb ccc", nowrap, 10.0).len(), 1);
        let mut anywhere = ComputedValues::initial();
        anywhere.overflow_wrap = OverflowWrap::Anywhere;
        let broken = lines("abcdefghij", anywhere, 30.0);
        assert!(broken.len() > 2, "{broken:?}");
    }

    #[test]
    fn hyphenates_automatically() {
        let mut style = ComputedValues::initial();
        style.hyphens = Hyphens::Auto;
        let broken = lines("hyphenation hyphenation", style, 60.0);
        assert!(broken.len() >= 3, "{broken:?}");
    }

    #[test]
    fn intrinsic_sizes() {
        let p = paragraph("aa bbbb c", ComputedValues::initial());
        let breaker = Breaker::new(&p);
        let (min, max) = breaker.intrinsic(0.0);
        assert!(min > 0.0 && min < max);
        assert!((max - p.width(0, p.text.len())).abs() < 0.01);
    }
}
