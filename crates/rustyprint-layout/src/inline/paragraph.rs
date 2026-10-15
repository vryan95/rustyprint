//! Building a paragraph from inline content: white space processing, text transformation,
//! inline box edges, bidi levels and shaping (CSS Text 3 §§ 4, 2; CSS Writing Modes 4 § 2).

use std::ops::Range;
use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{
    BaselineSource, Direction, FontKerning, FontMetric, FontSizeAdjust, FontVariantCaps,
    FontVariantPosition, LengthPercentage, MaybeAuto, Overflow, Synthesis, TextAutospace, TextCase,
    TextCombineUpright, TextOrientation, TextRendering, UnicodeBidi, WhiteSpaceCollapse,
    WritingMode,
};
use rustyprint_dom::NodeId;
use unicode_bidi::{BidiClass, BidiInfo, Level, bidi_class};
use unicode_segmentation::UnicodeSegmentation;

use crate::boxes::{BlockBox, InlineItem};
use crate::display::Upright;
use crate::fonts::{FontChoice, select};
use crate::fragment::{BoxFragment, Sides};
use crate::text::{Feature, Glyph, ShapeOptions, shape_range};
use crate::{Font, flow};

/// The object replacement character, standing in for atomic inlines.
pub(crate) const OBJECT: char = '\u{FFFC}';

/// Inline content flattened into one string with everything layout needs about it.
pub(crate) struct Paragraph {
    /// The text after white space processing and text transformation. Atomic inlines are
    /// U+FFFC and forced line breaks are `\n`.
    pub text: Arc<str>,
    /// Pieces of the text in one style, in order, covering all of it.
    pub spans: Vec<Span>,
    /// Inline boxes; the first is the root inline box of the block container.
    pub boxes: Vec<InlineBox>,
    /// Where inline boxes start and end, in document order.
    pub edges: Vec<Edge>,
    pub atomics: Vec<Atomic>,
    /// The bidi embedding level of each byte of the text.
    pub levels: Vec<u8>,
    pub base_rtl: bool,
    /// Shaped text, in logical order.
    pub runs: Vec<Run>,
    /// Advances summed up to each byte offset, without tabs.
    prefix: Vec<f32>,
    /// Byte offsets of preserved tabs.
    pub tabs: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpanKind {
    Text,
    Atomic(usize),
    /// A forced line break.
    Break,
}

#[derive(Debug, Clone)]
pub(crate) struct Span {
    pub range: Range<usize>,
    pub style: Arc<ComputedValues>,
    /// The innermost inline box containing the span.
    pub inline_box: usize,
    pub kind: SpanKind,
    pub lang: Option<Arc<str>>,
}

#[derive(Debug, Clone)]
pub(crate) struct InlineBox {
    pub style: Arc<ComputedValues>,
    pub node: Option<NodeId>,
    pub parent: usize,
    /// The boxes from the outermost non-root box down to this one.
    pub chain: Vec<usize>,
    pub start: usize,
    pub end: usize,
    pub margin: Sides,
    pub border: Sides,
    pub padding: Sides,
}

impl InlineBox {
    pub fn rtl(&self) -> bool {
        self.style.direction == Direction::Rtl
    }

    /// Margin, border and padding on the left (`true`) or right side.
    pub fn side(&self, left: bool) -> f32 {
        if left {
            self.margin.left + self.border.left + self.padding.left
        } else {
            self.margin.right + self.border.right + self.padding.right
        }
    }

    /// Whether the start edge is on the left.
    pub fn start_is_left(&self) -> bool {
        !self.rtl()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Edge {
    pub pos: usize,
    pub inline_box: usize,
    pub start: bool,
    /// Whether a line break at `pos` leaves the edge on the line before it.
    pub sticky_prev: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Atomic {
    pub fragment: BoxFragment,
    /// Margin box size.
    pub width: f32,
    pub height: f32,
    /// Distance from the top of the margin box to the baseline.
    pub baseline: f32,
    pub margin_left: f32,
    pub margin_top: f32,
    /// For a `leader()`, its pattern and style. Leaders have no size until lines are
    /// placed, when they take the space left.
    pub leader: Option<(Arc<str>, Arc<ComputedValues>)>,
}

/// Text shaped in one font, size and direction.
#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub range: Range<usize>,
    pub span: usize,
    pub font: Font,
    pub size: f32,
    pub level: u8,
    /// Glyphs in visual order; their text ranges are offsets into the paragraph text.
    pub glyphs: Vec<Glyph>,
    /// Baseline offset of synthesized superscripts and subscripts, positive downwards.
    pub shift: f32,
    pub oblique: f32,
    pub embolden: bool,
    /// How the glyphs stand in vertical text, if upright.
    pub upright: Option<Upright>,
}

impl Paragraph {
    /// Builds the paragraph for inline `items`. Atomic inlines are laid out with
    /// `available` as their containing block width; percentages of inline boxes' margins,
    /// borders and padding refer to `cb_width`.
    pub fn build(
        items: &[InlineItem],
        container: &Arc<ComputedValues>,
        available: f32,
        cb_width: f32,
    ) -> Paragraph {
        let mut builder = Builder {
            text: String::new(),
            spans: Vec::new(),
            boxes: vec![InlineBox {
                style: container.clone(),
                node: None,
                parent: 0,
                chain: Vec::new(),
                start: 0,
                end: 0,
                margin: Sides::default(),
                border: Sides::default(),
                padding: Sides::default(),
            }],
            edges: Vec::new(),
            atomics: Vec::new(),
            stack: vec![0],
            pending: None,
            word_start: true,
            cb_width,
        };
        let tokens = tokenize(items);
        builder.process(&tokens, available);
        builder.finish()
    }

    /// The span containing byte `pos`.
    pub fn span_at(&self, pos: usize) -> &Span {
        let index = self
            .spans
            .partition_point(|s| s.range.end <= pos)
            .min(self.spans.len().saturating_sub(1));
        &self.spans[index]
    }

    /// Whether the paragraph has nothing that would make a line box.
    pub fn is_empty(&self) -> bool {
        let has_text = self.spans.iter().any(|span| match span.kind {
            SpanKind::Text => self.text[span.range.clone()]
                .chars()
                .any(|c| !is_collapsible_space(c, &span.style)),
            SpanKind::Atomic(_) | SpanKind::Break => true,
        });
        let has_edges = self.boxes[1..]
            .iter()
            .any(|b| b.side(true) != 0.0 || b.side(false) != 0.0);
        !has_text && !has_edges
    }

    /// The advance of `[start, end)`, without tabs, including the inline box edges a line
    /// over that range contains.
    pub fn width(&self, start: usize, end: usize) -> f32 {
        let mut width = self.prefix[end] - self.prefix[start];
        for edge in self.edges_between(start, end) {
            width += self.edge_width(edge);
        }
        width
    }

    /// The width of an inline box edge.
    pub fn edge_width(&self, edge: &Edge) -> f32 {
        let b = &self.boxes[edge.inline_box];
        b.side(edge.start == b.start_is_left())
    }

    /// The edges a line over `[start, end)` contains.
    pub fn edges_between(&self, start: usize, end: usize) -> impl Iterator<Item = &Edge> {
        let at_end = end == self.text.len();
        let first = self.edges.partition_point(|e| e.pos < start);
        self.edges[first..]
            .iter()
            .take_while(move |e| e.pos <= end)
            .filter(move |e| {
                if e.sticky_prev {
                    e.pos > start && e.pos <= end
                } else {
                    e.pos < end || (at_end && e.pos == end)
                }
            })
    }

    /// The x position after laying out `[start, end)` from `x`, measured from the start of
    /// the line's content box, with tabs advancing to the next tab stop.
    pub fn measure(&self, start: usize, end: usize, x: f32) -> f32 {
        let mut x = x;
        let mut from = start;
        let first = self.tabs.partition_point(|&t| t < start);
        for &tab in self.tabs[first..].iter().take_while(|&&t| t < end) {
            x += self.width(from, tab);
            x = self.tab_stop(tab, x);
            from = tab + 1;
        }
        x + self.width(from, end)
    }

    /// Where a tab at `pos` reached at `x` ends (CSS Text 3 § 4.2).
    pub fn tab_stop(&self, pos: usize, x: f32) -> f32 {
        let span = self.span_at(pos);
        let interval = tab_interval(&span.style);
        if interval <= 0.0 {
            return x;
        }
        let mut stop = ((x / interval).floor() + 1.0) * interval;
        // A stop closer than half a space width is skipped.
        let space = span.style.font_size * 0.25;
        if stop - x < space * 0.5 {
            stop += interval;
        }
        stop
    }

    /// The style of the character at `pos`.
    pub fn style_at(&self, pos: usize) -> &Arc<ComputedValues> {
        &self.span_at(pos).style
    }
}

/// The distance between tab stops for a style.
fn tab_interval(style: &ComputedValues) -> f32 {
    use rustyprint_css::properties::types::TabSize;
    match style.tab_size {
        TabSize::Length(px) => px,
        TabSize::Spaces(n) => {
            let choice = select(style);
            let space = advance_of(&choice.font, ' ', style.font_size);
            let letter = spacing(&style.letter_spacing, style.font_size);
            let word = style
                .word_spacing
                .as_ref()
                .map_or(0.0, |w| w.resolve(space));
            n * (space + letter + word)
        }
    }
}

/// The advance of a character in a font at a size.
pub(crate) fn advance_of(font: &Font, c: char, size: f32) -> f32 {
    font.glyph(c)
        .and_then(|g| font.advance(g))
        .map_or(size * 0.25, |a| f32::from(a) * font.em_scale(size))
}

/// `letter-spacing` or `word-spacing` in pixels; percentages refer to `basis`.
pub(crate) fn spacing(value: &Option<LengthPercentage>, basis: f32) -> f32 {
    value.as_ref().map_or(0.0, |v| v.resolve(basis))
}

/// Whether `c` is a space that collapses in `style`.
pub(crate) fn is_collapsible_space(c: char, style: &ComputedValues) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
        && matches!(
            style.white_space_collapse,
            WhiteSpaceCollapse::Collapse
                | WhiteSpaceCollapse::PreserveBreaks
                | WhiteSpaceCollapse::Discard
        )
}

// ---------------------------------------------------------------------------------------------
// Tokens

/// Inline content as a flat stream.
enum Token<'a> {
    Char {
        c: char,
        item: usize,
        style: &'a Arc<ComputedValues>,
        lang: &'a Option<Arc<str>>,
        /// Whether the text item is a single character, for `text-transform: math-auto`.
        single: bool,
    },
    Start(&'a Arc<ComputedValues>, Option<NodeId>),
    End,
    Atomic(&'a BlockBox),
    Leader(&'a Arc<str>, &'a Arc<ComputedValues>),
    Break,
}

fn tokenize(items: &[InlineItem]) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match item {
            InlineItem::Text { text, style, lang } => {
                let single = text.trim().chars().count() == 1;
                for c in text.chars() {
                    tokens.push(Token::Char {
                        c,
                        item: index,
                        style,
                        lang,
                        single,
                    });
                }
            }
            InlineItem::Start { style, node } => tokens.push(Token::Start(style, *node)),
            InlineItem::End => tokens.push(Token::End),
            InlineItem::Atomic(block) => tokens.push(Token::Atomic(block)),
            InlineItem::Leader { pattern, style } => tokens.push(Token::Leader(pattern, style)),
            InlineItem::LineBreak => tokens.push(Token::Break),
        }
    }
    tokens
}

// ---------------------------------------------------------------------------------------------
// Building

struct Builder {
    text: String,
    spans: Vec<Span>,
    boxes: Vec<InlineBox>,
    edges: Vec<Edge>,
    atomics: Vec<Atomic>,
    /// Open inline boxes, innermost last.
    stack: Vec<usize>,
    /// The text item of the open span, to tell when a new span starts.
    pending: Option<usize>,
    /// Whether the next letter starts a word, for `text-transform: capitalize`.
    word_start: bool,
    cb_width: f32,
}

/// A white space character waiting in a sequence that may collapse.
struct Space<'a> {
    c: char,
    item: usize,
    style: &'a Arc<ComputedValues>,
    lang: &'a Option<Arc<str>>,
}

/// Boxes opened or closed inside a white space sequence, replayed after it.
enum Marker<'a> {
    Start(&'a Arc<ComputedValues>, Option<NodeId>),
    End,
}

impl Builder {
    fn process(&mut self, tokens: &[Token<'_>], available: f32) {
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Char {
                    c,
                    item,
                    style,
                    lang,
                    single,
                } => {
                    if is_collapsible_space(*c, style) {
                        i = self.collapse(tokens, i);
                        continue;
                    }
                    self.preserved(*c, *item, style, lang, *single);
                }
                Token::Start(style, node) => self.start_box(style, *node),
                Token::End => self.end_box(),
                Token::Atomic(block) => self.atomic(block, available),
                Token::Leader(pattern, style) => self.leader(pattern, style),
                Token::Break => {
                    let style = self.boxes[*self.stack.last().expect("root")].style.clone();
                    self.push_special('\n', style, SpanKind::Break);
                    self.word_start = true;
                }
            }
            i += 1;
        }
    }

    /// Handles a sequence of collapsible white space starting at `tokens[start]`, and any
    /// boxes that open or close inside it. Returns the index after the sequence.
    fn collapse(&mut self, tokens: &[Token<'_>], start: usize) -> usize {
        let mut spaces: Vec<Space<'_>> = Vec::new();
        let mut markers: Vec<(usize, Marker<'_>)> = Vec::new();
        let mut i = start;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Char {
                    c,
                    item,
                    style,
                    lang,
                    ..
                } if is_collapsible_space(*c, style) => spaces.push(Space {
                    c: *c,
                    item: *item,
                    style,
                    lang,
                }),
                Token::Start(style, node) => {
                    markers.push((spaces.len(), Marker::Start(style, *node)))
                }
                Token::End => markers.push((spaces.len(), Marker::End)),
                _ => break,
            }
            i += 1;
        }
        // Markers before the first space stay where they are.
        let next = tokens[i..].iter().find_map(|t| match t {
            Token::Char { c, .. } => Some(*c),
            Token::Atomic(_) => Some(OBJECT),
            Token::Break => Some('\n'),
            _ => None,
        });
        let kept: Vec<&Space<'_>> = spaces
            .iter()
            .filter(|s| s.style.white_space_collapse != WhiteSpaceCollapse::Discard)
            .collect();
        let breaks: Vec<&Space<'_>> = kept
            .iter()
            .copied()
            .filter(|s| {
                s.c == '\n' && s.style.white_space_collapse == WhiteSpaceCollapse::PreserveBreaks
            })
            .collect();
        let mut emitted = false;
        let mut emit_at = |builder: &mut Builder, markers: &mut Vec<(usize, Marker<'_>)>| {
            // Replay the markers that came before the first space.
            if !emitted {
                emitted = true;
                let first = markers.iter().take_while(|(at, _)| *at == 0).count();
                for (_, marker) in markers.drain(..first) {
                    builder.marker(marker);
                }
            }
        };
        if !breaks.is_empty() {
            emit_at(self, &mut markers);
            for space in breaks {
                self.push_char('\n', space.item, space.style, space.lang, SpanKind::Break);
            }
            self.word_start = true;
        } else if let Some(first) = kept.first() {
            // Floats and absolutely positioned boxes leave placeholders in the text, which
            // white space collapses across.
            let previous = self
                .last_in_flow()
                .and_then(|span| self.text[span.range.clone()].chars().next_back());
            let at_line_start = previous.is_none_or(|c| c == '\n');
            let segment_break = kept.iter().any(|s| s.c == '\n');
            let remove = at_line_start
                || previous == Some(' ') && self.previous_collapsible()
                || segment_break && removes_segment_break(previous, next);
            emit_at(self, &mut markers);
            if !remove {
                let c = if first.style.text_transform.full_width {
                    '\u{3000}'
                } else {
                    ' '
                };
                self.push_char(c, first.item, first.style, first.lang, SpanKind::Text);
                self.word_start = true;
            }
        }
        for (_, marker) in markers {
            self.marker(marker);
        }
        i
    }

    /// The last span that is not the placeholder of a float or an absolutely positioned
    /// box.
    fn last_in_flow(&self) -> Option<&Span> {
        self.spans.iter().rev().find(|span| match span.kind {
            SpanKind::Atomic(index) => self.atomics[index].fragment.out_of_flow.is_none(),
            _ => true,
        })
    }

    /// Whether the last character came from text whose spaces collapse.
    fn previous_collapsible(&self) -> bool {
        self.last_in_flow().is_some_and(|span| {
            span.kind == SpanKind::Text
                && matches!(
                    span.style.white_space_collapse,
                    WhiteSpaceCollapse::Collapse | WhiteSpaceCollapse::PreserveBreaks
                )
        })
    }

    fn marker(&mut self, marker: Marker<'_>) {
        match marker {
            Marker::Start(style, node) => self.start_box(style, node),
            Marker::End => self.end_box(),
        }
    }

    /// A character that does not collapse: kept, transformed, or turned into a break.
    fn preserved(
        &mut self,
        c: char,
        item: usize,
        style: &Arc<ComputedValues>,
        lang: &Option<Arc<str>>,
        single: bool,
    ) {
        let mode = style.white_space_collapse;
        match c {
            '\n' if matches!(
                mode,
                WhiteSpaceCollapse::Preserve | WhiteSpaceCollapse::BreakSpaces
            ) =>
            {
                self.push_char('\n', item, style, lang, SpanKind::Break);
                self.word_start = true;
            }
            '\n' | '\r' | '\u{c}' => self.push_char(' ', item, style, lang, SpanKind::Text),
            '\t' | ' ' => {
                let c = if c == ' ' && style.text_transform.full_width {
                    '\u{3000}'
                } else {
                    c
                };
                self.push_char(c, item, style, lang, SpanKind::Text);
                self.word_start = true;
            }
            _ => {
                let lang_str = lang.as_deref().unwrap_or("");
                for t in transform(c, style, lang_str, self.word_start, single) {
                    self.push_char(t, item, style, lang, SpanKind::Text);
                }
                self.word_start = !is_word_char(c);
            }
        }
    }

    fn push_char(
        &mut self,
        c: char,
        item: usize,
        style: &Arc<ComputedValues>,
        lang: &Option<Arc<str>>,
        kind: SpanKind,
    ) {
        let pos = self.text.len();
        self.text.push(c);
        let inline_box = *self.stack.last().expect("root");
        let continues = self.pending == Some(item)
            && self.spans.last().is_some_and(|s| {
                s.kind == kind && s.range.end == pos && s.inline_box == inline_box
            });
        if continues {
            self.spans.last_mut().expect("checked").range.end = self.text.len();
        } else {
            self.spans.push(Span {
                range: pos..self.text.len(),
                style: style.clone(),
                inline_box,
                kind,
                lang: lang.clone(),
            });
        }
        self.pending = if kind == SpanKind::Break {
            None
        } else {
            Some(item)
        };
    }

    fn push_special(&mut self, c: char, style: Arc<ComputedValues>, kind: SpanKind) {
        let pos = self.text.len();
        self.text.push(c);
        self.spans.push(Span {
            range: pos..self.text.len(),
            style,
            inline_box: *self.stack.last().expect("root"),
            kind,
            lang: None,
        });
        self.pending = None;
    }

    fn start_box(&mut self, style: &Arc<ComputedValues>, node: Option<NodeId>) {
        let parent = *self.stack.last().expect("root");
        let index = self.boxes.len();
        let mut chain = self.boxes[parent].chain.clone();
        chain.push(index);
        let pos = self.text.len();
        let cb = self.cb_width;
        let resolve = |v: &LengthPercentage| v.resolve(cb);
        let margin = |v: &MaybeAuto<LengthPercentage>| match v {
            MaybeAuto::Auto => 0.0,
            MaybeAuto::Value(v) => v.resolve(cb),
        };
        self.boxes.push(InlineBox {
            style: style.clone(),
            node,
            parent,
            chain,
            start: pos,
            end: pos,
            margin: Sides {
                top: margin(&style.margin_top),
                right: margin(&style.margin_right),
                bottom: margin(&style.margin_bottom),
                left: margin(&style.margin_left),
            },
            border: Sides {
                top: style.border_top_width,
                right: style.border_right_width,
                bottom: style.border_bottom_width,
                left: style.border_left_width,
            },
            padding: Sides {
                top: resolve(&style.padding_top),
                right: resolve(&style.padding_right),
                bottom: resolve(&style.padding_bottom),
                left: resolve(&style.padding_left),
            },
        });
        self.edges.push(Edge {
            pos,
            inline_box: index,
            start: true,
            sticky_prev: false,
        });
        self.stack.push(index);
        self.pending = None;
    }

    fn end_box(&mut self) {
        if self.stack.len() <= 1 {
            return;
        }
        let index = self.stack.pop().expect("checked");
        let pos = self.text.len();
        self.boxes[index].end = pos;
        let empty = self.boxes[index].start == pos;
        self.edges.push(Edge {
            pos,
            inline_box: index,
            start: false,
            sticky_prev: !empty,
        });
        self.pending = None;
    }

    fn atomic(&mut self, block: &BlockBox, available: f32) {
        let laid = flow::layout_atomic(block, available);
        let height = laid.fragment.rect.height + laid.margin.vertical();
        // The baseline is the last line's, unless there is none or overflow is not visible;
        // then it is the bottom margin edge.
        let style = &laid.fragment.style;
        let visible =
            style.overflow_x == Overflow::Visible && style.overflow_y == Overflow::Visible;
        // `baseline-source: auto` means the last baseline for inline blocks.
        let first = style.baseline_source == BaselineSource::First;
        let source = if crate::writing::flips_lines(self.boxes[0].style.writing_mode) {
            // The box's content turns like blocks, not like the line it is on.
            laid.fragment.flipped_baseline(first)
        } else if first {
            laid.fragment.first_baseline()
        } else {
            laid.fragment.last_baseline()
        };
        let baseline = source
            .filter(|_| visible)
            .map_or(height, |baseline| laid.margin.top + baseline);
        let index = self.atomics.len();
        self.atomics.push(Atomic {
            width: laid.fragment.rect.width + laid.margin.horizontal(),
            height,
            baseline,
            margin_left: laid.margin.left,
            margin_top: laid.margin.top,
            fragment: laid.fragment,
            leader: None,
        });
        let style = self.boxes[*self.stack.last().expect("root")].style.clone();
        self.push_special(OBJECT, style, SpanKind::Atomic(index));
        self.word_start = true;
    }

    /// Adds a `leader()` as an empty object that line placement fills.
    fn leader(&mut self, pattern: &Arc<str>, style: &Arc<ComputedValues>) {
        let index = self.atomics.len();
        self.atomics.push(Atomic {
            fragment: BoxFragment {
                node: None,
                style: style.clone(),
                rect: crate::display::Rect::new(0.0, 0.0, 0.0, 0.0),
                border: Sides::default(),
                padding: Sides::default(),
                children: Vec::new(),
                replaced: None,
                legend: None,
                out_of_flow: None,
            },
            width: 0.0,
            height: 0.0,
            baseline: 0.0,
            margin_left: 0.0,
            margin_top: 0.0,
            leader: Some((pattern.clone(), style.clone())),
        });
        self.push_special(OBJECT, style.clone(), SpanKind::Atomic(index));
    }

    fn finish(mut self) -> Paragraph {
        // Close boxes left open.
        while self.stack.len() > 1 {
            self.end_box();
        }
        let text: Arc<str> = Arc::from(self.text.as_str());
        let base_rtl = self.boxes[0].rtl();
        let levels = bidi_levels(&text, &self.boxes, &self.edges);
        let tabs: Vec<usize> = text
            .char_indices()
            .filter(|(_, c)| *c == '\t')
            .map(|(i, _)| i)
            .collect();
        let mut paragraph = Paragraph {
            text,
            spans: self.spans,
            boxes: self.boxes,
            edges: self.edges,
            atomics: self.atomics,
            levels,
            base_rtl,
            runs: Vec::new(),
            prefix: Vec::new(),
            tabs,
        };
        paragraph.runs = shape_spans(&paragraph);
        paragraph.prefix = advances(&paragraph);
        paragraph
    }
}

/// Whether a collapsible segment break between `before` and `after` is removed rather than
/// turned into a space (CSS Text 3 § 4.1.3).
fn removes_segment_break(before: Option<char>, after: Option<char>) -> bool {
    let (Some(before), Some(after)) = (before, after) else {
        return false;
    };
    if before == '\u{200B}' || after == '\u{200B}' {
        return true;
    }
    is_wide(before) && is_wide(after) && !is_hangul(before) && !is_hangul(after)
}

/// East Asian Wide and Fullwidth characters, roughly.
fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7A3)
}

/// Ideographic characters, for `text-autospace`.
pub(crate) fn is_ideograph(c: char) -> bool {
    matches!(c as u32,
        0x3041..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFD)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '\u{2019}' | '\u{AD}') || is_mark(c)
}

/// Combining marks, which belong to the preceding letter.
fn is_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

// ---------------------------------------------------------------------------------------------
// Text transformation (CSS Text 3 § 2.1)

fn transform(
    c: char,
    style: &ComputedValues,
    lang: &str,
    word_start: bool,
    single: bool,
) -> Vec<char> {
    let t = &style.text_transform;
    if t.math_auto {
        return vec![if single { math_italic(c) } else { c }];
    }
    let turkic = lang.starts_with("tr") || lang.starts_with("az");
    let mut out: Vec<char> = match t.case {
        TextCase::None => vec![c],
        TextCase::Uppercase => upper(c, turkic),
        TextCase::Lowercase => lower(c, turkic),
        TextCase::Capitalize if word_start && c.is_alphanumeric() => upper(c, turkic),
        TextCase::Capitalize => vec![c],
    };
    if t.full_width {
        for c in &mut out {
            if ('\u{21}'..='\u{7E}').contains(c) {
                *c = char::from_u32(*c as u32 + 0xFEE0).unwrap_or(*c);
            }
        }
    }
    if t.full_size_kana {
        for c in &mut out {
            *c = full_size_kana(*c);
        }
    }
    out
}

fn upper(c: char, turkic: bool) -> Vec<char> {
    match c {
        'i' if turkic => vec!['\u{130}'],
        _ => c.to_uppercase().collect(),
    }
}

fn lower(c: char, turkic: bool) -> Vec<char> {
    match c {
        'I' if turkic => vec!['\u{131}'],
        '\u{130}' if turkic => vec!['i'],
        _ => c.to_lowercase().collect(),
    }
}

/// Mathematical italic forms of Latin letters, for `text-transform: math-auto`.
fn math_italic(c: char) -> char {
    match c {
        'h' => '\u{210E}',
        'A'..='Z' => char::from_u32(0x1D434 + (c as u32 - 'A' as u32)).unwrap_or(c),
        'a'..='z' => char::from_u32(0x1D44E + (c as u32 - 'a' as u32)).unwrap_or(c),
        _ => c,
    }
}

fn full_size_kana(c: char) -> char {
    const SMALL: &str =
        "ぁぃぅぇぉっゃゅょゎゕゖァィゥェォッャュョヮヵヶㇰㇱㇲㇳㇴㇵㇶㇷㇸㇹㇺㇻㇼㇽㇾㇿｧｨｩｪｫｬｭｮｯ";
    const FULL: &str =
        "あいうえおつやゆよわかけアイウエオツヤユヨワカケクシストヌハヒフヘホムラリルレロｱｲｳｴｵﾔﾕﾖﾂ";
    SMALL
        .chars()
        .position(|s| s == c)
        .and_then(|i| FULL.chars().nth(i))
        .unwrap_or(c)
}

// ---------------------------------------------------------------------------------------------
// Bidi (CSS Writing Modes 4 § 2.4)

/// Embedding levels of each byte of `text`, with inline boxes' `unicode-bidi` applied as
/// explicit embeddings, isolates and overrides.
fn bidi_levels(text: &str, boxes: &[InlineBox], edges: &[Edge]) -> Vec<u8> {
    let root = &boxes[0].style;
    let base = if root.direction == Direction::Rtl {
        Level::rtl()
    } else {
        Level::ltr()
    };
    let has_controls = boxes[1..]
        .iter()
        .any(|b| b.style.unicode_bidi != UnicodeBidi::Normal);
    let root_override = matches!(
        root.unicode_bidi,
        UnicodeBidi::BidiOverride | UnicodeBidi::IsolateOverride
    );
    let strong_rtl = text
        .chars()
        .any(|c| matches!(bidi_class(c), BidiClass::R | BidiClass::AL | BidiClass::AN));
    if !strong_rtl && !has_controls && !root_override && !base.is_rtl() {
        return vec![0; text.len()];
    }
    // Insert the controls each box needs and map text offsets to the controlled text.
    let mut controlled = String::with_capacity(text.len() + 16);
    let mut map = vec![0usize; text.len()];
    let rtl_of = |style: &ComputedValues| style.direction == Direction::Rtl;
    if root_override {
        controlled.push(if rtl_of(root) { '\u{202E}' } else { '\u{202D}' });
    }
    let mut edge = 0;
    let emit_edges = |pos: usize, controlled: &mut String, edge: &mut usize| {
        while *edge < edges.len() && edges[*edge].pos == pos {
            let e = edges[*edge];
            let style = &boxes[e.inline_box].style;
            let rtl = rtl_of(style);
            if e.start {
                match style.unicode_bidi {
                    UnicodeBidi::Normal => {}
                    UnicodeBidi::Embed => {
                        controlled.push(if rtl { '\u{202B}' } else { '\u{202A}' })
                    }
                    UnicodeBidi::Isolate => {
                        controlled.push(if rtl { '\u{2067}' } else { '\u{2066}' })
                    }
                    UnicodeBidi::BidiOverride => {
                        controlled.push(if rtl { '\u{202E}' } else { '\u{202D}' })
                    }
                    UnicodeBidi::IsolateOverride => {
                        controlled.push(if rtl { '\u{2067}' } else { '\u{2066}' });
                        controlled.push(if rtl { '\u{202E}' } else { '\u{202D}' });
                    }
                    UnicodeBidi::Plaintext => controlled.push('\u{2068}'),
                }
            } else {
                match style.unicode_bidi {
                    UnicodeBidi::Normal => {}
                    UnicodeBidi::Embed | UnicodeBidi::BidiOverride => controlled.push('\u{202C}'),
                    UnicodeBidi::Isolate | UnicodeBidi::Plaintext => controlled.push('\u{2069}'),
                    UnicodeBidi::IsolateOverride => {
                        controlled.push('\u{202C}');
                        controlled.push('\u{2069}');
                    }
                }
            }
            *edge += 1;
        }
    };
    for (i, c) in text.char_indices() {
        emit_edges(i, &mut controlled, &mut edge);
        let at = controlled.len();
        // Forced breaks end bidi paragraphs, as segment breaks do.
        controlled.push(c);
        for b in 0..c.len_utf8() {
            map[i + b] = at + b;
        }
    }
    emit_edges(text.len(), &mut controlled, &mut edge);
    let level = if root.unicode_bidi == UnicodeBidi::Plaintext {
        None
    } else {
        Some(base)
    };
    let info = BidiInfo::new(&controlled, level);
    map.iter().map(|&i| info.levels[i].number()).collect()
}

// ---------------------------------------------------------------------------------------------
// Shaping

/// The OpenType features a style asks for (CSS Fonts 4 § 6 and § 7).
pub(crate) fn features(style: &ComputedValues, font: &Font) -> Vec<Feature> {
    let mut out = Vec::new();
    let mut set = |tag: &[u8; 4], value: u32| out.push(Feature::new(tag, value));
    // The `@font-face` rule's features come first; the properties override them.
    for &(tag, value) in font.instance().features.iter() {
        set(&tag, value);
    }
    match style.font_kerning {
        FontKerning::Auto => {}
        FontKerning::Normal => set(b"kern", 1),
        FontKerning::None => set(b"kern", 0),
    }
    if style.text_rendering == TextRendering::OptimizeSpeed {
        set(b"kern", 0);
        set(b"liga", 0);
    }
    let ligatures = &style.font_variant_ligatures;
    let spaced = spacing(&style.letter_spacing, style.font_size) != 0.0;
    if ligatures.none || spaced {
        for tag in [b"liga", b"clig", b"dlig", b"hlig", b"calt"] {
            if !(spaced && tag == b"calt") {
                set(tag, 0);
            }
        }
    } else {
        let toggle = |value: Option<bool>| value.map(u32::from);
        if let Some(v) = toggle(ligatures.common) {
            set(b"liga", v);
            set(b"clig", v);
        }
        if let Some(v) = toggle(ligatures.discretionary) {
            set(b"dlig", v);
        }
        if let Some(v) = toggle(ligatures.historical) {
            set(b"hlig", v);
        }
        if let Some(v) = toggle(ligatures.contextual) {
            set(b"calt", v);
        }
    }
    match style.font_variant_caps {
        FontVariantCaps::Normal => {}
        FontVariantCaps::SmallCaps => set(b"smcp", 1),
        FontVariantCaps::AllSmallCaps => {
            set(b"c2sc", 1);
            set(b"smcp", 1);
        }
        FontVariantCaps::PetiteCaps => set(
            if font.has_feature(b"pcap") {
                b"pcap"
            } else {
                b"smcp"
            },
            1,
        ),
        FontVariantCaps::AllPetiteCaps => {
            set(
                if font.has_feature(b"c2pc") {
                    b"c2pc"
                } else {
                    b"c2sc"
                },
                1,
            );
            set(
                if font.has_feature(b"pcap") {
                    b"pcap"
                } else {
                    b"smcp"
                },
                1,
            );
        }
        FontVariantCaps::Unicase => set(b"unic", 1),
        FontVariantCaps::TitlingCaps => set(b"titl", 1),
    }
    let numeric = &style.font_variant_numeric;
    match numeric.lining {
        Some(true) => set(b"lnum", 1),
        Some(false) => set(b"onum", 1),
        None => {}
    }
    match numeric.proportional {
        Some(true) => set(b"pnum", 1),
        Some(false) => set(b"tnum", 1),
        None => {}
    }
    match numeric.diagonal {
        Some(true) => set(b"frac", 1),
        Some(false) => set(b"afrc", 1),
        None => {}
    }
    if numeric.ordinal {
        set(b"ordn", 1);
    }
    if numeric.slashed_zero {
        set(b"zero", 1);
    }
    let east = &style.font_variant_east_asian;
    if let Some(variant) = east.variant {
        let tag: &[u8; 4] = match variant {
            "jis78" => b"jp78",
            "jis83" => b"jp83",
            "jis90" => b"jp90",
            "jis04" => b"jp04",
            "simplified" => b"smpl",
            _ => b"trad",
        };
        set(tag, 1);
    }
    match east.width {
        Some("full-width") => set(b"fwid", 1),
        Some(_) => set(b"pwid", 1),
        None => {}
    }
    if east.ruby {
        set(b"ruby", 1);
    }
    match style.font_variant_position {
        FontVariantPosition::Normal => {}
        FontVariantPosition::Sub => set(b"subs", 1),
        FontVariantPosition::Super => set(b"sups", 1),
    }
    alternates(style, font, &mut set);
    for setting in style.font_feature_settings.iter() {
        set(&setting.tag, setting.value);
    }
    out
}

/// The features `font-variant-alternates` turns on, with the values its names stand for in
/// the `@font-feature-values` rules of the font's family (CSS Fonts 4 § 6.8). Names without
/// values are ignored.
fn alternates(style: &ComputedValues, font: &Font, set: &mut dyn FnMut(&[u8; 4], u32)) {
    use rustyprint_css::font_rules::{FeatureBlock, FeatureValues};
    let alternates = &style.font_variant_alternates;
    if alternates.is_normal() {
        return;
    }
    let family = &font.instance().family;
    let rules = &style.font_rules.feature_values;
    let lookup = |kind, name: &str| FeatureValues::lookup(rules, family, kind, name);
    let numbered = |prefix: &[u8; 2], n: u32| -> Option<[u8; 4]> {
        (1..=99).contains(&n).then(|| {
            let digits = format!("{n:02}");
            let d = digits.as_bytes();
            [prefix[0], prefix[1], d[0], d[1]]
        })
    };
    let single = [
        (&alternates.stylistic, FeatureBlock::Stylistic, b"salt"),
        (&alternates.swash, FeatureBlock::Swash, b"swsh"),
        (&alternates.ornaments, FeatureBlock::Ornaments, b"ornm"),
        (&alternates.annotation, FeatureBlock::Annotation, b"nalt"),
    ];
    for (name, kind, tag) in single {
        if let Some(&[value]) = name.as_deref().and_then(|n| lookup(kind, n)) {
            set(tag, value);
        }
    }
    for name in &alternates.styleset {
        for &n in lookup(FeatureBlock::Styleset, name).unwrap_or_default() {
            if let Some(tag) = numbered(b"ss", n) {
                set(&tag, 1);
            }
        }
    }
    for name in &alternates.character_variant {
        if let Some(values) = lookup(FeatureBlock::CharacterVariant, name)
            && let Some(tag) = numbered(b"cv", values[0])
        {
            set(&tag, values.get(1).copied().unwrap_or(1));
        }
    }
    if alternates.historical_forms {
        set(b"hist", 1);
    }
}

/// The font size after `font-size-adjust` (CSS Fonts 5 § 2.6).
pub(crate) fn adjusted_size(style: &ComputedValues, font: &Font) -> f32 {
    let size = style.font_size;
    let FontSizeAdjust::Adjust(metric, Some(value)) = style.font_size_adjust else {
        return size;
    };
    let m = font.metrics();
    let em = f32::from(font.units_per_em());
    let aspect = match metric {
        FontMetric::ExHeight => f32::from(m.x_height) / em,
        FontMetric::CapHeight => f32::from(m.cap_height) / em,
        FontMetric::ChWidth => m.zero_advance.map_or(0.5, |a| f32::from(a) / em),
        FontMetric::IcWidth | FontMetric::IcHeight => {
            m.ideograph_advance.map_or(1.0, |a| f32::from(a) / em)
        }
    };
    if aspect > 0.0 {
        size * value / aspect
    } else {
        size
    }
}

/// Shapes every text span, split where the bidi level, font or synthesis changes.
fn shape_spans(p: &Paragraph) -> Vec<Run> {
    let mut runs = Vec::new();
    for (index, span) in p.spans.iter().enumerate() {
        if span.kind != SpanKind::Text || span.range.is_empty() {
            continue;
        }
        let style = &span.style;
        let choice = select(style);
        let size = adjusted_size(style, &choice.font);
        let language = match &style.font_language_override {
            Some(tag) => Some(format!("x-hbot{tag}")),
            None => span.lang.as_deref().map(str::to_owned),
        };
        for piece in itemize(p, span, &choice) {
            shape_piece(
                p,
                index,
                span,
                &choice,
                size,
                language.as_deref(),
                piece,
                &mut runs,
            );
        }
    }
    // Spacing applies after shaping.
    for run in &mut runs {
        apply_spacing(p, run);
    }
    runs
}

/// A range of a span to shape in one go.
#[derive(Debug, Clone)]
struct Piece {
    range: Range<usize>,
    font: Font,
    level: u8,
    /// Lowercase letters set as synthesized small capitals.
    small_caps: bool,
    upright: Option<Upright>,
}

/// How each character of vertical text in `style` stands: whether it is upright, given
/// `text-orientation` (CSS Writing Modes 4 § 5.1). Sideways writing modes set everything
/// sideways.
fn stands_upright(style: &ComputedValues, c: char) -> bool {
    match style.writing_mode {
        WritingMode::VerticalRl | WritingMode::VerticalLr => match style.text_orientation {
            TextOrientation::Mixed => super::orientation::is_upright(c),
            TextOrientation::Upright => true,
            TextOrientation::Sideways => false,
        },
        _ => false,
    }
}

/// Which bytes of `text`, the text of a span in `style`, are combined upright into the
/// space of one character (CSS Writing Modes 4 § 9.1): all of it, or runs of up to the
/// given number of ASCII digits.
fn combined_bytes(style: &ComputedValues, text: &str) -> Vec<bool> {
    let vertical = matches!(
        style.writing_mode,
        WritingMode::VerticalRl | WritingMode::VerticalLr
    );
    match style.text_combine_upright {
        _ if !vertical => vec![false; text.len()],
        TextCombineUpright::None => vec![false; text.len()],
        TextCombineUpright::All => vec![true; text.len()],
        TextCombineUpright::Digits(most) => {
            let bytes = text.as_bytes();
            let mut out = vec![false; text.len()];
            let mut start = 0;
            while start < bytes.len() {
                if !bytes[start].is_ascii_digit() {
                    start += 1;
                    continue;
                }
                let end = start
                    + bytes[start..]
                        .iter()
                        .take_while(|b| b.is_ascii_digit())
                        .count();
                if end - start <= usize::from(most) {
                    out[start..end].fill(true);
                }
                start = end;
            }
            out
        }
    }
}

/// Whether a grapheme asks to be drawn in color (`Some(true)`), as text (`Some(false)`), or
/// leaves it to the font order (`None`).
fn emoji_presentation(
    variant: rustyprint_css::properties::types::FontVariantEmoji,
    grapheme: &str,
) -> Option<bool> {
    use rustyprint_css::properties::types::FontVariantEmoji as Emoji;
    let first = grapheme.chars().next()?;
    if !could_be_emoji(first) {
        return None;
    }
    match variant {
        Emoji::Normal => None,
        Emoji::Text => Some(false),
        Emoji::Emoji => Some(true),
        // Variation selectors decide, and otherwise the character's default presentation.
        Emoji::Unicode => Some(if grapheme.contains('\u{FE0F}') {
            true
        } else if grapheme.contains('\u{FE0E}') {
            false
        } else {
            emoji_by_default(first)
        }),
    }
}

/// Whether a character may have an emoji presentation: pictographs and symbols.
fn could_be_emoji(c: char) -> bool {
    matches!(u32::from(c),
        0x203C | 0x2049 | 0x2122 | 0x2139 | 0x2194..=0x21AA | 0x231A..=0x23FF
        | 0x24C2 | 0x25AA..=0x25FE | 0x2600..=0x27BF | 0x2934 | 0x2935 | 0x2B05..=0x2B55
        | 0x3030 | 0x303D | 0x3297 | 0x3299 | 0x1F000..=0x1FAFF)
}

/// Whether a character shows as an emoji without a variation selector
/// (`Emoji_Presentation`), approximately: the pictographs of the supplementary planes and a
/// few symbols.
fn emoji_by_default(c: char) -> bool {
    matches!(u32::from(c),
        0x231A | 0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3 | 0x25FD | 0x25FE | 0x2614
        | 0x2615 | 0x2648..=0x2653 | 0x267F | 0x2693 | 0x26A1 | 0x26AA | 0x26AB | 0x26BD
        | 0x26BE | 0x26C4 | 0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2..=0x26F5 | 0x26FA
        | 0x26FD | 0x2705 | 0x270A | 0x270B | 0x2728 | 0x274C | 0x274E | 0x2753..=0x2755
        | 0x2757 | 0x2795..=0x2797 | 0x27B0 | 0x27BF | 0x2B1B | 0x2B1C | 0x2B50 | 0x2B55
        | 0x1F004 | 0x1F0CF | 0x1F18E | 0x1F191..=0x1F19A | 0x1F200..=0x1FAFF)
}

/// Whether a font draws a character's glyph in color.
fn draws_in_color(font: &Font, c: char) -> bool {
    font.glyph(c)
        .is_some_and(|glyph| font.is_color_glyph(glyph))
}

/// Splits a span by bidi level, font coverage, tabs and small-caps synthesis.
fn itemize(p: &Paragraph, span: &Span, choice: &FontChoice) -> Vec<Piece> {
    let text = &p.text[span.range.clone()];
    let primary = &choice.font;
    let fallback_fonts = &choice.fallbacks;
    let style = &span.style;
    let synth_caps = match style.font_variant_caps {
        FontVariantCaps::SmallCaps | FontVariantCaps::PetiteCaps => {
            !primary.has_feature(b"smcp") && style.font_synthesis_small_caps == Synthesis::Auto
        }
        FontVariantCaps::AllSmallCaps | FontVariantCaps::AllPetiteCaps => {
            !primary.has_feature(b"c2sc") && style.font_synthesis_small_caps == Synthesis::Auto
        }
        _ => false,
    };
    let all_caps = matches!(
        style.font_variant_caps,
        FontVariantCaps::AllSmallCaps | FontVariantCaps::AllPetiteCaps
    );
    let combined = combined_bytes(style, text);
    let mut pieces: Vec<Piece> = Vec::new();
    for (offset, grapheme) in text.grapheme_indices(true) {
        let start = span.range.start + offset;
        let end = start + grapheme.len();
        let first = grapheme.chars().next().expect("graphemes are not empty");
        let upright = if combined[offset] {
            Some(Upright::Combined)
        } else if stands_upright(style, first) {
            Some(Upright::Each)
        } else {
            None
        };
        // Combined text is shaped in one go, in the first available font.
        let font = if upright == Some(Upright::Combined)
            || first == '\t'
            || first.is_whitespace()
            || first.is_control()
        {
            primary.clone()
        } else {
            let mut covering = std::iter::once(primary)
                .chain(fallback_fonts)
                .filter(|f| f.covers(first));
            // Emoji presentation prefers a font that draws the character in color, text
            // presentation one that does not (CSS Fonts 4 § 6.10).
            let font = match emoji_presentation(style.font_variant_emoji, grapheme) {
                Some(color) => {
                    let covering: Vec<&Font> = covering.collect();
                    covering
                        .iter()
                        .find(|f| draws_in_color(f, first) == color)
                        .or(covering.first())
                        .copied()
                }
                None => covering.next(),
            };
            font.unwrap_or(primary).clone()
        };
        let small_caps = synth_caps && (first.is_lowercase() || (all_caps && first.is_uppercase()));
        let level = p.levels[start];
        let tab = first == '\t';
        match pieces.last_mut() {
            Some(last)
                if !tab
                    && last.range.end == start
                    && last.font == font
                    && last.level == level
                    && last.small_caps == small_caps
                    && last.upright == upright
                    && !p.text[last.range.clone()].ends_with('\t') =>
            {
                last.range.end = end;
            }
            _ => pieces.push(Piece {
                range: start..end,
                font,
                level,
                small_caps,
                upright,
            }),
        }
    }
    // Tabs are positioned by layout, not shaped.
    pieces.retain(|piece| &p.text[piece.range.clone()] != "\t");
    pieces
}

#[allow(clippy::too_many_arguments)]
fn shape_piece(
    p: &Paragraph,
    span_index: usize,
    span: &Span,
    choice: &FontChoice,
    size: f32,
    language: Option<&str>,
    piece: Piece,
    runs: &mut Vec<Run>,
) {
    let style = &span.style;
    let font = &piece.font;
    let mut features = features(style, font);
    if piece.upright == Some(Upright::Each) {
        // Vertical forms, such as of brackets and small kana.
        for tag in [b"vert", b"vrt2"] {
            features.push(Feature::new(tag, 1));
        }
    }
    let rtl = piece.level % 2 == 1;
    let options = ShapeOptions {
        rtl,
        language,
        features: &features,
    };
    // Synthesized superscripts and subscripts are smaller and shifted.
    let synth_position = match style.font_variant_position {
        FontVariantPosition::Super => !font.has_feature(b"sups"),
        FontVariantPosition::Sub => !font.has_feature(b"subs"),
        FontVariantPosition::Normal => false,
    } && style.font_synthesis_position == Synthesis::Auto;
    let m = font.metrics();
    let (mut size, shift) = if synth_position {
        let ratio = f32::from(m.script_size) / f32::from(font.units_per_em());
        let offset = if style.font_variant_position == FontVariantPosition::Super {
            -font.scale(m.superscript_offset, size)
        } else {
            font.scale(m.subscript_offset, size)
        };
        (size * ratio.clamp(0.4, 1.0), offset)
    } else {
        (size, 0.0)
    };
    let glyphs = if piece.small_caps {
        size *= 0.7;
        let upper: String = p.text[piece.range.clone()]
            .chars()
            .flat_map(char::to_uppercase)
            .collect();
        let shaped = shape_range(font, size, &upper, 0..upper.len(), &options);
        let same = upper.len() == piece.range.len();
        shaped
            .into_iter()
            .map(|mut g| {
                g.text = if same {
                    piece.range.start + g.text.start..piece.range.start + g.text.end
                } else {
                    piece.range.clone()
                };
                g
            })
            .collect()
    } else {
        shape_range(font, size, &p.text, piece.range.clone(), &options)
    };
    let _ = choice.synthetic_bold;
    let mut glyphs = glyphs;
    match piece.upright {
        // Upright glyphs advance by their height, centered in it when painted.
        Some(Upright::Each) => {
            let scale = font.em_scale(size);
            for glyph in &mut glyphs {
                if glyph.x_advance != 0.0 {
                    glyph.x_advance = font
                        .vertical_advance(glyph.id)
                        .map_or(size, |advance| f32::from(advance) * scale);
                }
                glyph.x_offset = 0.0;
                glyph.y_offset = 0.0;
            }
        }
        // Combined text takes the space of one ideograph.
        Some(Upright::Combined) => {
            for (index, glyph) in glyphs.iter_mut().enumerate() {
                glyph.x_advance = if index == 0 { size } else { 0.0 };
            }
        }
        None => {}
    }
    runs.push(Run {
        range: piece.range,
        span: span_index,
        font: font.clone(),
        size,
        level: piece.level,
        glyphs,
        shift,
        oblique: choice.synthetic_oblique,
        embolden: choice.synthetic_bold,
        upright: piece.upright,
    });
}

/// Applies `letter-spacing`, `word-spacing` and `text-autospace` to a run's advances.
fn apply_spacing(p: &Paragraph, run: &mut Run) {
    let style = p.spans[run.span].style.clone();
    let letter = spacing(&style.letter_spacing, style.font_size);
    let word = style.word_spacing.as_ref();
    let autospace = match style.text_autospace {
        TextAutospace::NoAutospace => false,
        TextAutospace::Custom {
            ideograph_alpha,
            ideograph_numeric,
            ..
        } => ideograph_alpha || ideograph_numeric,
        TextAutospace::Normal | TextAutospace::Auto => true,
    };
    if letter == 0.0 && word.is_none() && !autospace {
        return;
    }
    let text = p.text.clone();
    // The glyph that ends each cluster in logical order gets the spacing after it.
    let rtl = run.level % 2 == 1;
    let count = run.glyphs.len();
    for i in 0..count {
        let next_same = if rtl {
            i > 0 && run.glyphs[i - 1].text == run.glyphs[i].text
        } else {
            i + 1 < count && run.glyphs[i + 1].text == run.glyphs[i].text
        };
        if next_same {
            continue;
        }
        let cluster = run.glyphs[i].text.clone();
        let c = text[cluster.clone()].chars().next().unwrap_or(' ');
        let mut extra = letter;
        if let Some(word) = word
            && is_word_separator(c)
        {
            let advance = run.glyphs[i].x_advance;
            extra += word.resolve(advance);
        }
        if autospace {
            let next = text[cluster.end..].chars().next();
            if let Some(next) = next {
                let pair = (is_ideograph(c) && next.is_alphanumeric() && !is_ideograph(next))
                    || (is_ideograph(next) && c.is_alphanumeric() && !is_ideograph(c));
                if pair {
                    extra += style.font_size / 8.0;
                }
            }
        }
        run.glyphs[i].x_advance += extra;
    }
}

/// Characters that `word-spacing` applies to and inter-word justification stretches.
pub(crate) fn is_word_separator(c: char) -> bool {
    matches!(
        c,
        ' ' | '\u{A0}' | '\u{1361}' | '\u{10100}' | '\u{10101}' | '\u{1039F}' | '\u{1091F}'
    )
}

/// Advances summed up to each byte offset: each cluster's advance counts at its first byte,
/// atomic inlines count their margin box.
fn advances(p: &Paragraph) -> Vec<f32> {
    let mut advance = vec![0.0f32; p.text.len() + 1];
    for run in &p.runs {
        for glyph in &run.glyphs {
            advance[glyph.text.start] += glyph.x_advance;
        }
    }
    for span in &p.spans {
        if let SpanKind::Atomic(index) = span.kind {
            advance[span.range.start] += p.atomics[index].width;
        }
    }
    let mut prefix = Vec::with_capacity(advance.len());
    let mut sum = 0.0;
    prefix.push(0.0);
    for a in &advance[..p.text.len()] {
        sum += a;
        prefix.push(sum);
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyprint_css::properties::types::TextTransform;

    fn style() -> Arc<ComputedValues> {
        Arc::new(ComputedValues::initial())
    }

    fn text_item(text: &str, style: &Arc<ComputedValues>) -> InlineItem {
        InlineItem::Text {
            text: text.into(),
            style: style.clone(),
            lang: None,
        }
    }

    fn build(items: &[InlineItem]) -> Paragraph {
        Paragraph::build(items, &style(), 800.0, 800.0)
    }

    #[test]
    fn white_space_collapses_across_boxes() {
        let s = style();
        let p = build(&[
            text_item("  a  \n b ", &s),
            InlineItem::Start {
                style: s.clone(),
                node: None,
            },
            text_item("  c", &s),
            InlineItem::End,
        ]);
        assert_eq!(&*p.text, "a b c");
        // The space before `c` stays outside the box.
        assert_eq!(p.boxes[1].start, 4);
    }

    #[test]
    fn preserved_white_space() {
        let mut pre = ComputedValues::initial();
        pre.white_space_collapse = WhiteSpaceCollapse::Preserve;
        let pre = Arc::new(pre);
        let p = build(&[text_item(" a \tb\n c", &pre)]);
        assert_eq!(&*p.text, " a \tb\n c");
        assert_eq!(p.tabs, vec![3]);
        assert!(p.spans.iter().any(|s| s.kind == SpanKind::Break));
        let mut lines = ComputedValues::initial();
        lines.white_space_collapse = WhiteSpaceCollapse::PreserveBreaks;
        let p = build(&[text_item("a  \n  b", &Arc::new(lines))]);
        assert_eq!(&*p.text, "a\nb");
    }

    #[test]
    fn transforms_text() {
        let mut upper = ComputedValues::initial();
        upper.text_transform = TextTransform {
            case: TextCase::Capitalize,
            ..TextTransform::NONE
        };
        let p = build(&[text_item("hello wide world's end", &Arc::new(upper))]);
        assert_eq!(&*p.text, "Hello Wide World's End");
        let mut upper = ComputedValues::initial();
        upper.text_transform = TextTransform {
            case: TextCase::Uppercase,
            ..TextTransform::NONE
        };
        let p = build(&[text_item("straße", &Arc::new(upper))]);
        assert_eq!(&*p.text, "STRASSE");
    }

    #[test]
    fn rtl_text_gets_odd_levels() {
        let p = build(&[text_item("abc \u{5D0}\u{5D1} def", &style())]);
        assert_eq!(p.levels[0], 0);
        assert_eq!(p.levels[4], 1);
        assert!(p.runs.iter().any(|r| r.level == 1));
    }

    #[test]
    fn widths_include_edges() {
        let s = style();
        let mut padded = ComputedValues::initial();
        padded.padding_left = LengthPercentage::Length(10.0);
        padded.padding_right = LengthPercentage::Length(5.0);
        let padded = Arc::new(padded);
        let p = build(&[
            text_item("a ", &s),
            InlineItem::Start {
                style: padded.clone(),
                node: None,
            },
            text_item("b", &padded),
            InlineItem::End,
        ]);
        let plain = p.width(0, 2);
        let with_box = p.width(0, 3);
        let b = p.width(2, 3);
        assert!((with_box - plain - b).abs() < 0.01);
        assert!(b > 15.0);
    }
}
