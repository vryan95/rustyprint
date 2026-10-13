//! The fragment tree: positioned boxes and lines, the output of layout.
//!
//! Box and line positions are relative to the parent box fragment's border box. Everything
//! inside a line box (text, atomic inlines and the fragments of inline boxes) is positioned
//! relative to the line box's top-left corner.
//!
//! Lines of vertical text keep their content in the line-relative axes it was laid out in,
//! and a [`Turn`] maps it onto the line box; atomic inlines on those lines are turned
//! already.

use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::TextDecorationStyle;
use rustyprint_dom::NodeId;

use crate::display::{Color, Rect, Size, TextRun};

#[derive(Debug, Clone)]
pub enum Fragment {
    Box(BoxFragment),
    Line(LineFragment),
}

#[derive(Debug, Clone)]
pub struct BoxFragment {
    pub node: Option<NodeId>,
    pub style: Arc<ComputedValues>,
    /// The border box, relative to the parent's border box.
    pub rect: Rect,
    pub border: Sides,
    pub padding: Sides,
    pub children: Vec<Fragment>,
    /// The content of a replaced element, painted in the content box.
    pub replaced: Option<crate::boxes::Replaced>,
    /// The border box of a `<fieldset>`'s rendered legend, relative to this border box. The
    /// top border is drawn through the legend's middle, with a gap behind it.
    pub legend: Option<Rect>,
    /// For the placeholder of an out-of-flow box, the box, until its containing block lays
    /// it out. The placeholder's position is the box's static position.
    pub out_of_flow: Option<OutOfFlow>,
}

/// The box of an out-of-flow placeholder.
#[derive(Debug, Clone)]
pub enum OutOfFlow {
    /// An absolutely positioned box, which its containing block places.
    Positioned(Arc<crate::boxes::Positioned>),
    /// A float in a line, which goes at the top of its line.
    Float(Arc<crate::boxes::BlockBox>),
    /// An absolutely positioned box whose containing block is the grid area that the
    /// placeholder covers, as when its grid container is positioned.
    InArea(Arc<crate::boxes::Positioned>),
}

/// A line box. Its items are positioned relative to the line's top-left corner.
#[derive(Debug, Clone)]
pub struct LineFragment {
    pub rect: Rect,
    /// Distance from the line's top to its baseline, in the line's own axes.
    pub baseline: f32,
    /// Text, atomic inlines and inline box fragments in visual order.
    pub items: Vec<LineItem>,
    /// For a line turned out of the axes it was laid out in, how its text and inline boxes
    /// map onto `rect`. Its atomic inlines are positioned in `rect`'s axes already.
    pub turn: Option<Turn>,
}

impl LineFragment {
    /// The turn of the line's content, if it is not upright.
    pub fn turned(&self) -> Option<Turn> {
        self.turn.filter(|turn| *turn != Turn::IDENTITY)
    }

    /// The size of the line in the axes its content was laid out in.
    pub fn content_size(&self) -> Size {
        match self.turned() {
            Some(turn) => turn.size(Size::new(self.rect.width, self.rect.height)),
            None => Size::new(self.rect.width, self.rect.height),
        }
    }

    /// The baseline relative to the line's top, unless the line is turned.
    pub fn horizontal_baseline(&self) -> Option<f32> {
        match self.turned() {
            None => Some(self.baseline),
            Some(_) => None,
        }
    }

    /// A rectangle of the line's content, such as an inline box, in the line's axes.
    pub fn content_rect(&self, rect: Rect) -> Rect {
        match self.turned() {
            Some(turn) => turn.rect(rect, self.content_size()),
            None => rect,
        }
    }
}

/// A rotation by quarter turns, or a reflection, that maps one set of axes onto another, as
/// the line-relative axes of a writing mode onto the physical ones: `(x, y)` maps to
/// `(a x + b y, c x + d y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    a: i8,
    b: i8,
    c: i8,
    d: i8,
}

impl Turn {
    pub const IDENTITY: Turn = Turn::new(1, 0, 0, 1);
    /// A quarter turn clockwise: lines that ran right now run down.
    pub const CLOCKWISE: Turn = Turn::new(0, -1, 1, 0);
    /// A quarter turn counter-clockwise: lines that ran right now run up.
    pub const COUNTER_CLOCKWISE: Turn = Turn::new(0, 1, -1, 0);
    /// A reflection across the diagonal: lines that ran right now run down, and lines that
    /// stacked down now stack right.
    pub const TRANSPOSE: Turn = Turn::new(0, 1, 1, 0);

    const fn new(a: i8, b: i8, c: i8, d: i8) -> Turn {
        Turn { a, b, c, d }
    }

    /// This turn followed by `next`.
    pub fn then(self, next: Turn) -> Turn {
        Turn::new(
            next.a * self.a + next.b * self.c,
            next.a * self.b + next.b * self.d,
            next.c * self.a + next.d * self.c,
            next.c * self.b + next.d * self.d,
        )
    }

    /// The turn that undoes this one.
    pub fn inverse(self) -> Turn {
        // Rotations and reflections are orthogonal: the inverse is the transpose.
        Turn::new(self.a, self.c, self.b, self.d)
    }

    /// Whether horizontal lengths become vertical ones.
    pub fn swaps_axes(self) -> bool {
        self.a == 0
    }

    fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            f32::from(self.a) * x + f32::from(self.b) * y,
            f32::from(self.c) * x + f32::from(self.d) * y,
        )
    }

    /// The size of a box of `size` once turned.
    pub fn size(self, size: Size) -> Size {
        if self.swaps_axes() {
            Size::new(size.height, size.width)
        } else {
            size
        }
    }

    /// Where `rect`, inside a container of `container` at the origin, goes when the
    /// container turns in place, keeping its top-left corner at the origin.
    pub fn rect(self, rect: Rect, container: Size) -> Rect {
        let (ox, oy) = self.origin(container);
        let (x0, y0) = self.apply(rect.x, rect.y);
        let (x1, y1) = self.apply(rect.right(), rect.bottom());
        Rect::new(
            x0.min(x1) - ox,
            y0.min(y1) - oy,
            (x1 - x0).abs(),
            (y1 - y0).abs(),
        )
    }

    /// The top-left corner of a container of `size` once turned about the origin.
    fn origin(self, size: Size) -> (f32, f32) {
        let (x0, y0) = self.apply(0.0, 0.0);
        let (x1, y1) = self.apply(size.width, size.height);
        (x0.min(x1), y0.min(y1))
    }

    /// The affine matrix `[a, b, c, d, e, f]` that turns a container of `size` in place, as
    /// [`crate::DisplayItem::PushTransform`] takes it.
    pub fn matrix(self, size: Size) -> [f32; 6] {
        let (ox, oy) = self.origin(size);
        [
            f32::from(self.a),
            f32::from(self.c),
            f32::from(self.b),
            f32::from(self.d),
            -ox,
            -oy,
        ]
    }

    /// Side lengths after the turn: each goes to the side its direction turns to.
    pub fn sides(self, sides: Sides) -> Sides {
        let mut out = Sides::default();
        for (value, (x, y)) in [
            (sides.top, (0.0, -1.0)),
            (sides.right, (1.0, 0.0)),
            (sides.bottom, (0.0, 1.0)),
            (sides.left, (-1.0, 0.0)),
        ] {
            match self.apply(x, y) {
                (_, y) if y < 0.0 => out.top = value,
                (x, _) if x > 0.0 => out.right = value,
                (_, y) if y > 0.0 => out.bottom = value,
                _ => out.left = value,
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
pub enum LineItem {
    Text(TextFragment),
    /// An atomic inline, such as an `inline-block`.
    Box(BoxFragment),
    /// The part of an inline box on this line.
    Inline(InlineFragment),
}

/// A run of text on a line.
#[derive(Debug, Clone)]
pub struct TextFragment {
    /// The glyphs, with the origin relative to the line.
    pub run: TextRun,
    pub style: Arc<ComputedValues>,
    /// Lines drawn across the text by it and its decorating ancestors.
    pub decorations: Vec<Decoration>,
}

/// An underline, overline or line-through across a text fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoration {
    pub kind: DecorationKind,
    /// The vertical center of the line, relative to the line box.
    pub y: f32,
    pub thickness: f32,
    pub color: Color,
    pub style: TextDecorationStyle,
    /// Whether the line skips glyph ink that crosses it.
    pub skip_ink: bool,
    /// Whether the line leaves out spaces at the start and end of the text.
    pub skip_spaces: bool,
    /// Insets at the start and end of the decorating box's line.
    pub inset: (f32, f32),
    /// Identifies the decorating box, so its lines join across fragments.
    pub source: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecorationKind {
    Underline,
    Overline,
    LineThrough,
}

/// The part of an inline box on one line. Only the sides where the box starts or ends have
/// borders and padding.
#[derive(Debug, Clone)]
pub struct InlineFragment {
    pub node: Option<NodeId>,
    pub style: Arc<ComputedValues>,
    /// The border box, relative to the line.
    pub rect: Rect,
    pub border: Sides,
    pub padding: Sides,
    /// Content in visual order, positioned relative to the line.
    pub children: Vec<LineItem>,
}

/// Lengths for the four sides of a box.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sides {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Sides {
    /// The same length on every side.
    pub const fn all(length: f32) -> Self {
        Self {
            top: length,
            right: length,
            bottom: length,
            left: length,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// A box an element generated: a block-level or atomic box, or part of an inline box.
#[derive(Debug, Clone, Copy)]
pub enum ElementBox<'a> {
    Block(&'a BoxFragment),
    Inline(&'a InlineFragment),
}

impl ElementBox<'_> {
    /// The number of line boxes inside the box; inline boxes have none.
    pub fn line_count(&self) -> usize {
        match self {
            ElementBox::Block(fragment) => fragment.line_count(),
            ElementBox::Inline(_) => 0,
        }
    }

    /// The text painted inside the box in visual order, with lines separated by `|`.
    pub fn text(&self) -> String {
        match self {
            ElementBox::Block(fragment) => fragment.text(),
            ElementBox::Inline(fragment) => items_text(&fragment.children),
        }
    }
}

impl BoxFragment {
    /// The padding box, relative to the parent's border box.
    pub fn padding_box(&self) -> Rect {
        self.rect.deflate(
            self.border.top,
            self.border.right,
            self.border.bottom,
            self.border.left,
        )
    }

    /// The baseline of the box's last line, relative to its border box top (CSS 2.1 § 10.8.1).
    pub fn last_baseline(&self) -> Option<f32> {
        self.children.iter().rev().find_map(|child| match child {
            Fragment::Line(line) => line.horizontal_baseline().map(|b| line.rect.y + b),
            Fragment::Box(child) => child
                .last_baseline()
                .map(|baseline| child.rect.y + baseline),
        })
    }

    /// The baseline of the box's first line, relative to its border box top.
    pub fn first_baseline(&self) -> Option<f32> {
        self.children.iter().find_map(|child| match child {
            Fragment::Line(line) => line.horizontal_baseline().map(|b| line.rect.y + b),
            Fragment::Box(child) => child
                .first_baseline()
                .map(|baseline| child.rect.y + baseline),
        })
    }

    /// The first or last baseline of a box whose lines' content is flipped across them, as
    /// in `vertical-lr`, where lines stack towards their under side: the distance from the
    /// box's bottom, where the over side of its lines is, to the baseline.
    pub fn flipped_baseline(&self, first: bool) -> Option<f32> {
        // The baseline relative to the top, in flipped lines.
        fn within(fragment: &BoxFragment, first: bool) -> Option<f32> {
            let find = |child: &Fragment| match child {
                Fragment::Line(line) => line
                    .horizontal_baseline()
                    .map(|b| line.rect.y + line.rect.height - b),
                Fragment::Box(child) => within(child, first).map(|b| child.rect.y + b),
            };
            if first {
                fragment.children.iter().find_map(find)
            } else {
                fragment.children.iter().rev().find_map(find)
            }
        }
        within(self, first).map(|b| self.rect.height - b)
    }

    /// Every element's boxes with border boxes in the root's coordinate space, in tree order.
    /// Atomic inlines and inline box fragments are included; anonymous boxes are not.
    pub fn element_boxes(&self) -> Vec<(NodeId, Rect, ElementBox<'_>)> {
        let mut out = Vec::new();
        collect_boxes(self, 0.0, 0.0, &mut out);
        out
    }

    /// The number of line boxes in this box, including nested block boxes.
    pub fn line_count(&self) -> usize {
        self.children
            .iter()
            .map(|child| match child {
                Fragment::Line(_) => 1,
                Fragment::Box(child) => child.line_count(),
            })
            .sum()
    }

    /// The text painted in the box's lines in visual order, with lines separated by `|`.
    pub fn text(&self) -> String {
        let mut lines = Vec::new();
        collect_lines(self, &mut lines);
        lines.join("|")
    }

    /// The content box, relative to the parent's border box.
    pub fn content_box(&self) -> Rect {
        self.padding_box().deflate(
            self.padding.top,
            self.padding.right,
            self.padding.bottom,
            self.padding.left,
        )
    }
}

fn collect_lines(fragment: &BoxFragment, out: &mut Vec<String>) {
    for child in &fragment.children {
        match child {
            Fragment::Line(line) => out.push(items_text(&line.items)),
            Fragment::Box(child) => collect_lines(child, out),
        }
    }
}

/// The text of line items in visual order. Each cluster's text appears once.
fn items_text(items: &[LineItem]) -> String {
    let mut out = String::new();
    for item in items {
        match item {
            LineItem::Text(text) => {
                let run = &text.run;
                let mut last = None;
                for glyph in &run.glyphs {
                    if last.as_ref() != Some(&glyph.text) {
                        // Invisible format characters, such as soft hyphens, are left out.
                        out.extend(run.text[glyph.text.clone()].chars().filter(|c| {
                            !matches!(
                                c,
                                '\u{AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}'
                            )
                        }));
                        last = Some(glyph.text.clone());
                    }
                }
            }
            LineItem::Box(atomic) => out.push_str(&atomic.text()),
            LineItem::Inline(inline) => out.push_str(&items_text(&inline.children)),
        }
    }
    out
}

fn collect_boxes<'a>(
    fragment: &'a BoxFragment,
    x: f32,
    y: f32,
    out: &mut Vec<(NodeId, Rect, ElementBox<'a>)>,
) {
    let rect = fragment.rect.translated(x, y);
    if let Some(node) = fragment.node {
        out.push((node, rect, ElementBox::Block(fragment)));
    }
    for child in &fragment.children {
        match child {
            Fragment::Box(child) => collect_boxes(child, rect.x, rect.y, out),
            Fragment::Line(line) => collect_line_items(
                line,
                &line.items,
                rect.x + line.rect.x,
                rect.y + line.rect.y,
                out,
            ),
        }
    }
}

fn collect_line_items<'a>(
    line: &LineFragment,
    items: &'a [LineItem],
    x: f32,
    y: f32,
    out: &mut Vec<(NodeId, Rect, ElementBox<'a>)>,
) {
    for item in items {
        match item {
            LineItem::Text(_) => {}
            LineItem::Box(atomic) => collect_boxes(atomic, x, y, out),
            LineItem::Inline(inline) => {
                if let Some(node) = inline.node {
                    out.push((
                        node,
                        line.content_rect(inline.rect).translated(x, y),
                        ElementBox::Inline(inline),
                    ));
                }
                collect_line_items(line, &inline.children, x, y, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_compose_and_invert() {
        let turns = [
            Turn::IDENTITY,
            Turn::CLOCKWISE,
            Turn::COUNTER_CLOCKWISE,
            Turn::TRANSPOSE,
        ];
        for turn in turns {
            assert_eq!(turn.then(turn.inverse()), Turn::IDENTITY);
        }
        assert_eq!(
            Turn::CLOCKWISE.then(Turn::CLOCKWISE).then(Turn::CLOCKWISE),
            Turn::COUNTER_CLOCKWISE
        );
        assert!(Turn::TRANSPOSE.swaps_axes() && !Turn::IDENTITY.swaps_axes());
    }

    #[test]
    fn rects_turn_with_their_container() {
        let container = Size::new(100.0, 30.0);
        let rect = Rect::new(10.0, 0.0, 20.0, 5.0);
        // Turned clockwise, the top of the container is its right side.
        assert_eq!(
            Turn::CLOCKWISE.rect(rect, container),
            Rect::new(25.0, 10.0, 5.0, 20.0)
        );
        assert_eq!(
            Turn::TRANSPOSE.rect(rect, container),
            Rect::new(0.0, 10.0, 5.0, 20.0)
        );
        assert_eq!(
            Turn::COUNTER_CLOCKWISE.rect(rect, container),
            Rect::new(0.0, 70.0, 5.0, 20.0)
        );
        let sides = Sides {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        };
        let turned = Turn::CLOCKWISE.sides(sides);
        assert_eq!(
            (turned.top, turned.right, turned.bottom, turned.left),
            (4.0, 1.0, 2.0, 3.0)
        );
    }

    #[test]
    fn matrices_turn_in_place() {
        // The far corner of a 100 by 30 box turned clockwise lands at its bottom-left.
        let [a, b, c, d, e, f] = Turn::CLOCKWISE.matrix(Size::new(100.0, 30.0));
        let (x, y) = (100.0, 30.0);
        assert_eq!((a * x + c * y + e, b * x + d * y + f), (0.0, 100.0));
    }
}
