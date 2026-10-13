//! The box tree: what layout works on (CSS 2.1 § 9.2, CSS Display 3).
//!
//! Elements become block boxes, inline content or atomic inlines. Inline content mixed with
//! blocks is wrapped in anonymous block boxes, and inline elements that contain blocks are
//! split around them. The children of a flex or grid container are its items, each a
//! block box.
//!
//! The builder also generates `::before`, `::after` and `::marker` boxes, which need the
//! counters and quote depth at their place in the tree (CSS Lists 3 § 4, CSS Generated
//! Content 3 § 3), and `::first-letter` boxes. `::first-line` styles go to the block whose
//! inline content has the first line, for inline layout (CSS Pseudo 4 § 2).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use rustyprint_css::counter_style::CounterStyleRef;
use rustyprint_css::properties::types::{
    BaselineSource, LengthPercentage, RubyMerge, RubyOverhang, RubyPosition, TextAlign,
    TextAlignLast,
};
use rustyprint_css::properties::types::{
    Content, ContentItem, Display, DisplayInside, DisplayInternal, DisplayOutside, Float,
    ImageValue, ListStylePosition, ListStyleType, Position, ShapeOutside, TargetTextKind,
    TryFallback, TryTactic, WhiteSpaceCollapse, WritingMode,
};
use rustyprint_css::selector::PseudoElement;
use rustyprint_css::{ComputedValues, StyleOptions, Styles, compute_styles};
use rustyprint_dom::forms::InputType;
use rustyprint_dom::{Document, Element, NodeData, NodeId, local_name, ns};

mod controls;

use crate::images::{Image, Images, natural_size};
use crate::picture::{Frame, Picture};
use crate::table;
use controls::Control;

/// An image in generated content: an atomic inline replaced box. Without a natural `size`,
/// it takes the default size of replaced content.
pub(crate) fn image_item(
    picture: Picture,
    size: Option<(f32, f32)>,
    style: &Arc<ComputedValues>,
) -> InlineItem {
    let style = Arc::new(ComputedValues::inherit_from(style));
    InlineItem::Atomic(Box::new(BlockBox {
        node: None,
        contents: BlockContents::Blocks(Vec::new()),
        page: None,
        marker: None,
        first_line: Vec::new(),
        legend: None,
        center_contents: false,
        shape_image: None,
        replaced: Some(Replaced {
            size,
            picture: Some(picture),
            widget: None,
        }),
        style,
    }))
}

/// How deeply `iframe` documents nest; deeper frames are empty, which also stops a document
/// that frames itself.
const MAX_FRAME_DEPTH: usize = 4;

/// A block-level box, or the block container of an atomic inline.
#[derive(Debug, Clone)]
pub struct BlockBox {
    /// The element that generated the box; `None` for anonymous boxes.
    pub node: Option<NodeId>,
    pub style: Arc<ComputedValues>,
    pub contents: BlockContents,
    /// The used page name: the box's `page`, or its parent's (CSS Paged Media 3 § 9.2).
    pub page: Option<Arc<str>>,
    /// An outside `::marker`, placed beside the box's first line.
    pub marker: Option<Box<BlockBox>>,
    /// The `::first-line` styles of the box's first line, outermost first. Only boxes with
    /// inline content have them.
    pub first_line: Vec<FirstLine>,
    /// The content of a replaced element, such as an image, which has no box contents.
    pub replaced: Option<Replaced>,
    /// The rendered legend of a `<fieldset>`, which sits in its top border (HTML § 15.3.13).
    pub legend: Option<Box<BlockBox>>,
    /// Whether the contents are centered vertically when the box is taller than they are,
    /// as form controls center their value.
    pub center_contents: bool,
    /// The image of a float's `shape-outside`, whose alpha channel is the shape.
    pub shape_image: Option<Picture>,
}

/// The content of a replaced element (CSS 2.1 § 3.1).
#[derive(Debug, Clone)]
pub struct Replaced {
    /// The image, or `None` for content that paints nothing, such as a video without a
    /// poster.
    pub picture: Option<Picture>,
    /// The natural size in CSS pixels, if the content has one.
    pub size: Option<(f32, f32)>,
    /// A control drawn in the content box, such as a checkbox or a media control bar.
    pub widget: Option<Widget>,
}

/// A form or media control that paints itself in its content box, in its static state.
#[derive(Debug, Clone, PartialEq)]
pub enum Widget {
    /// The control bar of `<audio controls>`.
    MediaControls,
    Checkbox {
        checked: bool,
        disabled: bool,
    },
    Radio {
        checked: bool,
        disabled: bool,
    },
    /// A `<progress>` bar filled to `position`, from 0 to 1; `None` when indeterminate.
    Progress {
        position: Option<f32>,
    },
    /// A `<meter>` gauge filled to `fraction`, coloured by how good its value is.
    Meter {
        fraction: f32,
        level: MeterLevel,
    },
    /// An `<input type=range>` slider with its thumb at `fraction`.
    Range {
        fraction: f32,
        disabled: bool,
    },
    /// The swatch of an `<input type=color>`, as `[r, g, b]`.
    Color([u8; 3]),
    /// The drop-down arrow of a `<select>`.
    PickerIcon,
    /// The check mark of a checked checkbox or a selected option, drawn by `::checkmark`.
    Checkmark,
    /// The dot of a checked radio button, drawn by `::checkmark`.
    Dot,
}

/// An absolutely positioned box and the other positions to try if it overflows.
#[derive(Debug, Clone)]
pub struct Positioned {
    pub block: BlockBox,
    /// From `position-try-fallbacks`, in order.
    pub fallbacks: Vec<Fallback>,
}

/// An option of `position-try-fallbacks`: the box's style with its `@position-try` rule or
/// `position-area` applied, and the tactics that flip it.
#[derive(Debug, Clone)]
pub struct Fallback {
    pub style: Arc<ComputedValues>,
    pub tactics: Vec<TryTactic>,
}

/// Which region of a `<meter>`'s range its value is in (HTML § 4.10.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterLevel {
    Optimum,
    Suboptimum,
    EvenLessGood,
}

/// What an element that can embed content renders as.
enum Embedding {
    Replaced(Replaced),
    /// The `alt` text of an image that is not available, as inline text.
    Text(String),
    /// Nothing at all.
    Nothing,
    /// The element's children, such as an `<object>`'s fallback content.
    Children,
}

/// A `::first-line` style and the style of the element it belongs to.
#[derive(Debug, Clone)]
pub struct FirstLine {
    pub origin: Arc<ComputedValues>,
    pub style: Arc<ComputedValues>,
}

#[derive(Debug, Clone)]
pub enum BlockContents {
    /// Block-level children.
    Blocks(Vec<BlockBox>),
    /// An inline formatting context.
    Inline(Vec<InlineItem>),
    /// A table's grid and captions.
    Table(Box<crate::table::Table>),
    /// A flex container's items, and the placeholders of its out-of-flow children, in
    /// document order (CSS Flexbox 1 § 4).
    Flex(Vec<BlockBox>),
    /// A grid container's items and placeholders, in document order (CSS Grid 2 § 6).
    Grid(Vec<BlockBox>),
    /// The placeholder of an absolutely positioned box at its static position. The box is
    /// laid out by its containing block (CSS Positioned Layout 3 § 4).
    OutOfFlow(Arc<Positioned>),
    /// The placeholder of a float in a line, which goes at the top of the line it is on
    /// (CSS 2.1 § 9.5.1).
    Float(Arc<BlockBox>),
    /// The content of a box in another writing mode than its parent, `outer`: an anonymous
    /// box laid out in the axes of the box's own writing mode (see [`crate::writing`]).
    Turned {
        inner: Box<BlockBox>,
        outer: WritingMode,
    },
    /// A ruby segment: bases and their annotations, laid out as an atomic inline.
    Ruby(Box<Ruby>),
}

/// A segment of a ruby container: base content with annotations over or under it (CSS
/// Ruby 1 § 2). Ruby containers are inline boxes holding a segment per base.
#[derive(Debug, Clone)]
pub struct Ruby {
    /// An anonymous block holding the base content.
    pub base: BlockBox,
    /// Anonymous blocks holding the annotations, in order: the first is nearest the base on
    /// its side.
    pub annotations: Vec<Annotation>,
    /// Whether annotations wider than the base may hang over the content before and after
    /// the segment (`ruby-overhang`).
    pub overhang: (bool, bool),
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub block: BlockBox,
    pub side: AnnotationSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationSide {
    Over,
    Under,
    /// Beside the base, after it in the inline direction, set in vertical text.
    InterCharacter,
}

/// The flattened contents of an inline formatting context.
#[derive(Debug, Clone)]
pub enum InlineItem {
    Text {
        text: String,
        style: Arc<ComputedValues>,
        /// The language from the nearest `lang` attribute, if any.
        lang: Option<Arc<str>>,
    },
    /// The start of an inline box.
    Start {
        style: Arc<ComputedValues>,
        node: Option<NodeId>,
    },
    /// The end of the most recently started inline box.
    End,
    /// A forced line break (`<br>`).
    LineBreak,
    /// An atomic inline, such as `inline-block`.
    Atomic(Box<BlockBox>),
    /// `leader()`: the pattern repeated to fill the space left on the line.
    Leader {
        pattern: Arc<str>,
        style: Arc<ComputedValues>,
    },
}

/// What `target-counter()`, `target-counters()` and `target-text()` know about the elements
/// that links point to, from an earlier layout of the document (CSS GCPM 3 § 4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Targets {
    pub elements: HashMap<NodeId, Target>,
    /// The number of pages, for `target-counter(url, pages)`.
    pub pages: usize,
}

/// An element that links can point to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Target {
    /// The counters in scope at the element, with the value of each instance, outermost
    /// first.
    pub counters: BTreeMap<Arc<str>, Vec<i32>>,
    /// The page the element starts on, from one.
    pub page: Option<usize>,
    /// The text of its `::before` and `::after`.
    pub before: String,
    pub after: String,
}

/// A box tree and what came out of building it.
pub struct Built {
    pub root: BlockBox,
    pub running: Vec<RunningElement>,
    /// The elements links can point to, without their pages.
    pub targets: HashMap<NodeId, Target>,
    /// Whether the content uses `target-*()`, so that layout has to run again once the
    /// pages are known.
    pub uses_targets: bool,
}

/// An element taken out of the flow by `position: running(name)`, for `element(name)` in
/// page-margin boxes (CSS GCPM 3 § 2).
#[derive(Debug, Clone)]
pub struct RunningElement {
    pub name: Arc<str>,
    pub block: BlockBox,
}

/// Builds the box tree for `document`, with the images it embeds from `images`. Returns
/// `None` if the root element is not rendered.
pub fn build(document: &Document, styles: &Styles, images: &Images) -> Option<BlockBox> {
    build_with_targets(document, styles, images, &Targets::default()).map(|built| built.root)
}

/// Builds the box tree with what an earlier layout found out about link targets.
pub fn build_with_targets(
    document: &Document,
    styles: &Styles,
    images: &Images,
    targets: &Targets,
) -> Option<Built> {
    let root = document.document_element()?;
    // Boxes in vertical writing modes are built with styles in their line-relative axes.
    let turned = crate::writing::turn_styles(document, styles);
    let styles = turned.as_ref().map_or(styles, |turned| &turned.styles);
    let style = styles.get(root)?.clone();
    if style.display == Display::None {
        return None;
    }
    let anchors = crate::links::anchors(document);
    let mut builder = Builder {
        document,
        styles,
        images,
        running: Vec::new(),
        counters: Counters::default(),
        quote_depth: 0,
        targetable: anchors.values().copied().collect(),
        anchors,
        previous: targets,
        targets: HashMap::new(),
        uses_targets: false,
        legends: Vec::new(),
    };
    builder.counters.enter();
    builder.update_counters(Some(root), &style);
    builder.record_target(root);
    let mut root = builder.block(root, style);
    assign_pages(&mut root, None);
    crate::writing::wrap_tree(&mut root, turned.as_ref());
    Some(Built {
        root,
        running: builder.running,
        targets: builder.targets,
        uses_targets: builder.uses_targets,
    })
}

/// Sets the used page name of a box and its block-level descendants.
fn assign_pages(block: &mut BlockBox, inherited: Option<&Arc<str>>) {
    block.page = block.style.page.clone().or_else(|| inherited.cloned());
    if let BlockContents::Blocks(children)
    | BlockContents::Flex(children)
    | BlockContents::Grid(children) = &mut block.contents
    {
        for child in children {
            assign_pages(child, block.page.as_ref());
        }
    }
}

struct Builder<'a> {
    document: &'a Document,
    styles: &'a Styles,
    images: &'a Images<'a>,
    running: Vec<RunningElement>,
    counters: Counters,
    /// How many quotes are open (CSS Generated Content 3 § 3.2).
    quote_depth: usize,
    /// The elements links can point to, by anchor name and as a set.
    anchors: HashMap<String, NodeId>,
    targetable: HashSet<NodeId>,
    /// Link targets from the layout before, and those recorded in this tree.
    previous: &'a Targets,
    targets: HashMap<NodeId, Target>,
    uses_targets: bool,
    /// The rendered legends of the fieldsets being built, innermost last, with their boxes
    /// once built.
    legends: Vec<(NodeId, Option<BlockBox>)>,
}

/// The counters in scope at the current point of the tree (CSS Lists 3 § 4.5).
#[derive(Default)]
struct Counters {
    /// The instances of each counter, outermost first.
    values: HashMap<Arc<str>, Vec<Instance>>,
    /// The counters created at each open sibling level, innermost last. A counter that an
    /// element creates is in scope for the element, its following siblings and their
    /// descendants, so it ends with the level.
    levels: Vec<Vec<Arc<str>>>,
    /// The levels where style containment starts, innermost last: increments and sets
    /// inside create counters of their own instead of changing those from outside.
    contained: Vec<usize>,
}

#[derive(Debug, Clone, Copy)]
struct Instance {
    value: i32,
    reversed: bool,
    /// The level that created it.
    level: usize,
}

impl Counters {
    fn enter(&mut self) {
        self.levels.push(Vec::new());
    }

    fn leave(&mut self) {
        for name in self.levels.pop().unwrap_or_default() {
            if let Some(stack) = self.values.get_mut(&name) {
                stack.pop();
            }
        }
    }

    /// Creates a counter at the current level, replacing one a preceding sibling created.
    fn reset(&mut self, name: &Arc<str>, value: i32, reversed: bool) {
        if self.levels.is_empty() {
            self.enter();
        }
        let created = self.levels.len() - 1;
        let level = self.levels.last_mut().expect("a level is open");
        let stack = self.values.entry(name.clone()).or_default();
        if let Some(index) = level.iter().position(|n| n == name) {
            level.remove(index);
            stack.pop();
        }
        stack.push(Instance {
            value,
            reversed,
            level: created,
        });
        level.push(name.clone());
    }

    /// Starts style containment below the current level.
    fn contain(&mut self) {
        self.contained.push(self.levels.len());
    }

    fn uncontain(&mut self) {
        self.contained.pop();
    }

    /// The innermost instance of a counter, created at 0 if none is in scope, or if the one
    /// in scope comes from outside style containment.
    fn innermost(&mut self, name: &Arc<str>) -> &mut Instance {
        let floor = self.contained.last().copied().unwrap_or(0);
        let outside = self
            .values
            .get(name)
            .and_then(|stack| stack.last())
            .is_none_or(|instance| instance.level < floor);
        if outside {
            self.reset(name, 0, false);
        }
        self.values
            .get_mut(name)
            .and_then(|stack| stack.last_mut())
            .expect("the counter was created")
    }

    fn value(&self, name: &str) -> i32 {
        self.values
            .get(name)
            .and_then(|stack| stack.last())
            .map_or(0, |i| i.value)
    }

    /// The value of every instance in scope, outermost first, for `counters()`.
    fn all(&self, name: &str) -> Vec<i32> {
        match self.values.get(name) {
            Some(stack) if !stack.is_empty() => stack.iter().map(|i| i.value).collect(),
            _ => vec![0],
        }
    }
}

/// The URL and pixel density of the first candidate in a `srcset` attribute.
fn first_srcset(srcset: &str) -> Option<(&str, f32)> {
    let mut parts = srcset.split(',').next()?.split_whitespace();
    let url = parts.next()?;
    let density = parts
        .next()
        .and_then(|d| d.strip_suffix('x'))
        .and_then(|d| d.parse::<f32>().ok())
        .filter(|d| *d > 0.0)
        .unwrap_or(1.0);
    Some((url, density))
}

/// The text of inline items, without leaders.
fn text_of(items: &[InlineItem]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            InlineItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// The block box, this one or a descendant, whose inline content has the first line.
fn first_formatted(block: &mut BlockBox) -> Option<&mut BlockBox> {
    if let BlockContents::Inline(items) = &block.contents {
        let empty = is_collapsible(items);
        return (!empty).then_some(block);
    }
    // Flex containers and tables are not block containers, which have a first line.
    let BlockContents::Blocks(children) = &mut block.contents else {
        return None;
    };
    children.iter_mut().find_map(first_formatted)
}

/// The byte range of the first letter in `text`: punctuation before it, its first letter or
/// digit with any combining marks, and punctuation after it. `None` if the text has nothing
/// but white space and punctuation.
fn letter_range(text: &str) -> Option<std::ops::Range<usize>> {
    let mut chars = text
        .char_indices()
        .skip_while(|(_, c)| c.is_whitespace())
        .peekable();
    let start = chars.peek()?.0;
    while chars.peek().is_some_and(|&(_, c)| is_punctuation(c)) {
        chars.next();
    }
    let (_, letter) = chars.next()?;
    if letter.is_whitespace() {
        return None;
    }
    while chars
        .peek()
        .is_some_and(|&(_, c)| is_combining(c) || is_punctuation(c))
    {
        chars.next();
    }
    let end = chars.peek().map_or(text.len(), |&(i, _)| i);
    Some(start..end)
}

/// Unicode punctuation: roughly the general categories Ps, Pe, Pi, Pf and Po.
fn is_punctuation(c: char) -> bool {
    (c.is_ascii_punctuation() && !matches!(c, '$' | '+' | '<' | '=' | '>' | '^' | '`' | '|' | '~'))
        || matches!(
            c,
            '\u{A1}'
                | '\u{A7}'
                | '\u{AB}'
                | '\u{B6}'
                | '\u{B7}'
                | '\u{BB}'
                | '\u{BF}'
                | '\u{2010}'..='\u{2027}'
                | '\u{2030}'..='\u{205E}'
                | '\u{3001}'..='\u{3003}'
                | '\u{3008}'..='\u{3011}'
        )
}

fn is_combining(c: char) -> bool {
    matches!(
        c,
        '\u{300}'..='\u{36F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

/// Whether a style makes its element a list item, which has a marker and increments the
/// `list-item` counter.
fn is_list_item(style: &ComputedValues) -> bool {
    matches!(
        style.display,
        Display::Normal {
            list_item: true,
            ..
        }
    )
}

/// How an element participates in its parent's formatting context.
enum Level {
    Block,
    Inline,
    Atomic,
}

fn level(display: Display) -> Option<Level> {
    match display {
        Display::None | Display::Contents => None,
        Display::Normal {
            outside: DisplayOutside::Inline,
            inside: DisplayInside::Flow | DisplayInside::Ruby,
            ..
        } => Some(Level::Inline),
        Display::Normal {
            outside: DisplayOutside::Inline,
            ..
        } => Some(Level::Atomic),
        Display::Normal { .. } | Display::Internal(_) => Some(Level::Block),
    }
}

impl Builder<'_> {
    /// The block box of an element and its contents.
    fn block(&mut self, node: NodeId, style: Arc<ComputedValues>) -> BlockBox {
        let mut contents = Contents {
            parent_style: style.clone(),
            blocks: Vec::new(),
            inline: Vec::new(),
            open: Vec::new(),
        };
        let legend = self.rendered_legend(node);
        if let Some(legend) = legend {
            self.legends.push((legend, None));
        }
        let marker = self.children(&mut contents, node, &style, true);
        let legend = legend
            .and_then(|_| self.legends.pop())
            .and_then(|(_, block)| block);
        let contents = column_contents(
            &style,
            flex_contents(&style, self.table_contents(&style, contents.finish())),
        );
        let mut block = BlockBox {
            node: Some(node),
            contents,
            style: style.clone(),
            page: None,
            marker,
            first_line: Vec::new(),
            legend: legend.map(Box::new),
            // Buttons center their content, like other controls.
            center_contents: self
                .document
                .element(node)
                .is_some_and(|e| e.is_html(&local_name!("button"))),
            shape_image: None,
            replaced: None,
        };
        let letter = self.styles.pseudo(node, &PseudoElement::FirstLetter);
        let line = self.styles.pseudo(node, &PseudoElement::FirstLine);
        if (letter.is_some() || line.is_some())
            && let Some(first) = first_formatted(&mut block)
        {
            if let Some(letter) = letter {
                self.first_letter(first, letter);
            }
            if let Some(line) = line {
                // Descendants added theirs first, and they nest inside this one.
                first.first_line.insert(
                    0,
                    FirstLine {
                        origin: style,
                        style: line.clone(),
                    },
                );
            }
        }
        block
    }

    /// The contents of a table become its grid, and table parts outside a table are wrapped
    /// in anonymous tables (CSS 2.1 § 17.2.1). Rows, row groups and column groups keep their
    /// children for their table to sort.
    fn table_contents(
        &self,
        style: &Arc<ComputedValues>,
        contents: BlockContents,
    ) -> BlockContents {
        let spans = |node: NodeId, name: &str| -> Option<u32> {
            let element = self.document.element(node)?;
            if element.name.ns != ns!(html) {
                return None;
            }
            let value = element.attr(name)?.trim_start();
            let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        };
        let take = |mut block: BlockBox| {
            let contents =
                std::mem::replace(&mut block.contents, BlockContents::Blocks(Vec::new()));
            let children = blocks_of(contents, &block.style);
            (block, children)
        };
        if matches!(
            style.display,
            Display::Normal {
                inside: DisplayInside::Table,
                ..
            }
        ) {
            let children = blocks_of(contents, style);
            return BlockContents::Table(Box::new(table::build(style, children, &take, &spans)));
        }
        if matches!(
            table::part(style.display),
            Some(
                DisplayInternal::TableRowGroup
                    | DisplayInternal::TableHeaderGroup
                    | DisplayInternal::TableFooterGroup
                    | DisplayInternal::TableRow
                    | DisplayInternal::TableColumnGroup
            )
        ) {
            return contents;
        }
        let BlockContents::Blocks(blocks) = contents else {
            return contents;
        };
        if !blocks
            .iter()
            .any(|b| table::part(b.style.display).is_some())
        {
            return BlockContents::Blocks(blocks);
        }
        let mut out = Vec::new();
        let mut run = Vec::new();
        let wrap = |run: &mut Vec<BlockBox>, out: &mut Vec<BlockBox>| {
            if run.is_empty() {
                return;
            }
            let mut table_style = ComputedValues::inherit_from(style);
            table_style.display = Display::Normal {
                outside: DisplayOutside::Block,
                inside: DisplayInside::Table,
                list_item: false,
            };
            let table_style = Arc::new(table_style);
            let grid = table::build(&table_style, std::mem::take(run), &take, &spans);
            out.push(BlockBox {
                node: None,
                style: table_style,
                contents: BlockContents::Table(Box::new(grid)),
                page: None,
                marker: None,
                first_line: Vec::new(),
                legend: None,
                center_contents: false,
                shape_image: None,
                replaced: None,
            });
        };
        for block in blocks {
            if table::part(block.style.display).is_some() {
                run.push(block);
            } else {
                wrap(&mut run, &mut out);
                out.push(block);
            }
        }
        wrap(&mut run, &mut out);
        BlockContents::Blocks(out)
    }

    /// Wraps the first letter of a block's inline content, with the punctuation around it,
    /// in a `::first-letter` box (CSS Pseudo 4 § 2.4). An inside marker does not count, and
    /// content that starts with something other than text has no first letter.
    fn first_letter(&self, block: &mut BlockBox, style: &Arc<ComputedValues>) {
        let marker = block
            .node
            .and_then(|node| self.styles.pseudo(node, &PseudoElement::Marker));
        let BlockContents::Inline(items) = &mut block.contents else {
            return;
        };
        let mut index = 0;
        if let (Some(InlineItem::Start { style: first, .. }), Some(marker)) =
            (items.first(), marker)
            && Arc::ptr_eq(first, marker)
        {
            // Skip the marker's box.
            let mut depth = 0;
            for (i, item) in items.iter().enumerate() {
                match item {
                    InlineItem::Start { .. } => depth += 1,
                    InlineItem::End => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    index = i + 1;
                    break;
                }
            }
        }
        while index < items.len() {
            let (text, text_style, lang) = match &items[index] {
                InlineItem::Start { .. } | InlineItem::End => {
                    index += 1;
                    continue;
                }
                InlineItem::LineBreak | InlineItem::Atomic(_) | InlineItem::Leader { .. } => {
                    return;
                }
                InlineItem::Text { text, style, lang } => {
                    (text.clone(), style.clone(), lang.clone())
                }
            };
            let Some(range) = letter_range(&text) else {
                index += 1;
                continue;
            };
            let piece = |s: &str, style: &Arc<ComputedValues>| InlineItem::Text {
                text: s.to_owned(),
                style: style.clone(),
                lang: lang.clone(),
            };
            let mut replacement = Vec::new();
            if range.start > 0 {
                replacement.push(piece(&text[..range.start], &text_style));
            }
            replacement.push(InlineItem::Start {
                style: style.clone(),
                node: None,
            });
            replacement.push(piece(&text[range.clone()], style));
            replacement.push(InlineItem::End);
            if range.end < text.len() {
                replacement.push(piece(&text[range.end..], &text_style));
            }
            items.splice(index..=index, replacement);
            return;
        }
    }

    /// Adds an element's children with its `::marker`, `::before` and `::after` boxes.
    /// Returns the outside marker of a block container, which its layout places.
    fn children(
        &mut self,
        contents: &mut Contents,
        node: NodeId,
        style: &Arc<ComputedValues>,
        block: bool,
    ) -> Option<Box<BlockBox>> {
        // `content-visibility: hidden` skips the contents, markers and pseudo-elements
        // included.
        if style.content_visibility == rustyprint_css::properties::types::ContentVisibility::Hidden
        {
            return None;
        }
        // Style containment keeps counter changes and quote depth inside the subtree.
        let contained = style.used_contain().style;
        let quote_depth = self.quote_depth;
        self.counters.enter();
        if contained {
            self.counters.contain();
        }
        let mut outside = None;
        if is_list_item(style)
            && let Some((marker_style, items)) = self.marker(node, style)
        {
            if block && style.list_style_position == ListStylePosition::Outside {
                outside = Some(Box::new(BlockBox {
                    node: None,
                    style: marker_style,
                    contents: BlockContents::Inline(items),
                    page: None,
                    marker: None,
                    first_line: Vec::new(),
                    legend: None,
                    center_contents: false,
                    shape_image: None,
                    replaced: None,
                }));
            } else {
                contents.inline.push(InlineItem::Start {
                    style: marker_style,
                    node: None,
                });
                contents.inline.extend(items);
                contents.inline.push(InlineItem::End);
            }
        }
        self.pseudo(contents, node, PseudoElement::Before);
        let details = self
            .document
            .element(node)
            .is_some_and(|e| e.is_html(&local_name!("details")));
        if details {
            self.details_children(contents, node, style);
        } else {
            for child in self.document.children(node) {
                self.add(contents, child);
            }
        }
        self.pseudo(contents, node, PseudoElement::After);
        if contained {
            self.counters.uncontain();
            self.quote_depth = quote_depth;
        }
        self.counters.leave();
        outside
    }

    fn add(&mut self, contents: &mut Contents, node: NodeId) {
        match self.document.node(node).data() {
            NodeData::Text(text) => {
                // Text takes the style of its parent element.
                let style = contents.open.last().map_or_else(
                    || {
                        self.parent_style(node)
                            .unwrap_or_else(|| contents.parent_style.clone())
                    },
                    |(style, _)| style.clone(),
                );
                contents.inline.push(InlineItem::Text {
                    text: text.clone(),
                    style,
                    lang: self.language(node),
                });
            }
            NodeData::Element(element) => {
                let Some(style) = self.styles.get(node).cloned() else {
                    return;
                };
                // Elements without boxes do not affect counters.
                if style.display == Display::None {
                    return;
                }
                self.update_counters(Some(node), &style);
                self.record_target(node);
                if let Position::Running(name) = &style.position {
                    let block = self.block(node, style.clone());
                    self.running.push(RunningElement {
                        name: name.clone(),
                        block,
                    });
                    return;
                }
                // Flex items do not float (CSS Flexbox 1 § 4).
                let floats = style.float != Float::None && !is_flex(&contents.parent_style);
                if (is_out_of_flow(&style) || floats) && style.display != Display::Contents {
                    let mut own = Contents {
                        parent_style: style.clone(),
                        blocks: Vec::new(),
                        inline: Vec::new(),
                        open: Vec::new(),
                    };
                    self.add_element(&mut own, node, element, style.clone());
                    let block = match own.finish() {
                        BlockContents::Blocks(mut blocks) if blocks.len() == 1 => blocks.remove(0),
                        contents => BlockBox {
                            node: None,
                            style: style.clone(),
                            contents,
                            page: None,
                            marker: None,
                            first_line: Vec::new(),
                            legend: None,
                            center_contents: false,
                            shape_image: None,
                            replaced: None,
                        },
                    };
                    if !floats {
                        let fallbacks = self.fallbacks(Some(node), &style);
                        contents.push_out_of_flow(block, fallbacks);
                        return;
                    }
                    {
                        let mut block = block;
                        if let ShapeOutside::Image(image) = &style.shape_outside {
                            block.shape_image = Picture::resolve(image, self.images, &style);
                        }
                        contents.push_float(block);
                    }
                    return;
                }
                self.add_element(contents, node, element, style);
            }
            _ => {}
        }
    }

    /// Adds the box of an element that is in flow, or of an out-of-flow element on its own.
    fn add_element(
        &mut self,
        contents: &mut Contents,
        node: NodeId,
        element: &Element,
        style: Arc<ComputedValues>,
    ) {
        {
            {
                if style.display == Display::Contents {
                    self.children(contents, node, &style, false);
                    return;
                }
                if self
                    .legends
                    .last()
                    .is_some_and(|(legend, _)| *legend == node)
                {
                    let block = self.block(node, style);
                    if let Some(slot) = self.legends.last_mut() {
                        slot.1 = Some(block);
                    }
                    return;
                }
                if element.name.ns == ns!(html)
                    && let Some(control) = self.control(node, element, &style)
                {
                    let block = match control {
                        Control::Block(block) => block,
                        Control::Replaced(replaced) => BlockBox {
                            node: Some(node),
                            style: style.clone(),
                            contents: BlockContents::Blocks(Vec::new()),
                            page: None,
                            marker: None,
                            first_line: Vec::new(),
                            legend: None,
                            center_contents: false,
                            shape_image: None,
                            replaced: Some(replaced),
                        },
                    };
                    // Controls are atomic, even with `display: inline`.
                    match level(style.display) {
                        None => {}
                        Some(Level::Block) => contents.push_block(block),
                        Some(Level::Inline | Level::Atomic) => {
                            contents.inline.push(InlineItem::Atomic(Box::new(block)));
                        }
                    }
                    return;
                }
                if element.name.ns == ns!(html) {
                    match self.embedding(element, &style) {
                        Embedding::Children => {}
                        Embedding::Nothing => return,
                        Embedding::Text(alt) => {
                            contents.inline.push(InlineItem::Start {
                                style: style.clone(),
                                node: Some(node),
                            });
                            contents.inline.push(InlineItem::Text {
                                text: alt,
                                style: style.clone(),
                                lang: self.language(node),
                            });
                            contents.inline.push(InlineItem::End);
                            return;
                        }
                        Embedding::Replaced(replaced) => {
                            let block = BlockBox {
                                node: Some(node),
                                style: style.clone(),
                                contents: BlockContents::Blocks(Vec::new()),
                                page: None,
                                marker: None,
                                first_line: Vec::new(),
                                legend: None,
                                center_contents: false,
                                shape_image: None,
                                replaced: Some(replaced),
                            };
                            match level(style.display) {
                                None => {}
                                Some(Level::Block) => contents.push_block(block),
                                Some(Level::Inline | Level::Atomic) => {
                                    contents.inline.push(InlineItem::Atomic(Box::new(block)));
                                }
                            }
                            return;
                        }
                    }
                }
                if element.name.ns == ns!(html) && element.name.local == local_name!("br") {
                    contents.inline.push(InlineItem::LineBreak);
                    return;
                }
                // `<wbr>` is a line break opportunity: a zero-width space.
                if element.name.ns == ns!(html) && element.name.local == local_name!("wbr") {
                    contents.inline.push(InlineItem::Text {
                        text: "\u{200B}".into(),
                        lang: self.language(node),
                        style,
                    });
                    return;
                }
                match level(style.display) {
                    None => {}
                    Some(Level::Inline)
                        if matches!(
                            style.display,
                            Display::Normal {
                                inside: DisplayInside::Ruby,
                                ..
                            }
                        ) =>
                    {
                        self.ruby(contents, node, &style);
                    }
                    Some(Level::Inline) => {
                        contents.inline.push(InlineItem::Start {
                            style: style.clone(),
                            node: Some(node),
                        });
                        contents.open.push((style.clone(), Some(node)));
                        self.children(contents, node, &style, false);
                        contents.open.pop();
                        contents.inline.push(InlineItem::End);
                    }
                    Some(Level::Atomic) => {
                        let block = self.block(node, style);
                        contents.inline.push(InlineItem::Atomic(Box::new(block)));
                    }
                    Some(Level::Block) => {
                        let block = self.block(node, style);
                        contents.push_block(block);
                    }
                }
            }
        }
    }

    /// Adds a ruby container: an inline box holding a segment for each run of base content
    /// and the annotations (`display: ruby-text`) that follow it (CSS Ruby 1 § 2.2).
    fn ruby(&mut self, contents: &mut Contents, node: NodeId, style: &Arc<ComputedValues>) {
        contents.inline.push(InlineItem::Start {
            style: style.clone(),
            node: Some(node),
        });
        contents.open.push((style.clone(), Some(node)));
        self.counters.enter();
        // Runs of base content, each with the annotations after it.
        let mut segments: Vec<(Vec<NodeId>, Vec<RubyText>)> = Vec::new();
        for child in self.document.children(node) {
            let annotation = self.styles.get(child).filter(|s| {
                s.display == Display::Internal(DisplayInternal::RubyText)
                    && self.document.element(child).is_some()
            });
            match (annotation, segments.last_mut()) {
                (Some(rt), Some((_, annotations))) => annotations.push((child, rt.clone())),
                (Some(rt), None) => segments.push((Vec::new(), vec![(child, rt.clone())])),
                (None, Some((base, annotations))) if annotations.is_empty() => base.push(child),
                (None, _) => segments.push((vec![child], Vec::new())),
            }
        }
        // `ruby-merge: merge` sets all the annotations of a level over all the bases.
        if style.ruby_merge == RubyMerge::Merge && segments.len() > 1 {
            let mut merged = (Vec::new(), Vec::new());
            for (base, annotations) in segments {
                merged.0.extend(base);
                merged.1.extend(annotations);
            }
            segments = vec![merged];
        }
        let count = segments.len();
        for (index, (base, annotations)) in segments.into_iter().enumerate() {
            let mut base_contents = Contents {
                parent_style: style.clone(),
                blocks: Vec::new(),
                inline: Vec::new(),
                open: Vec::new(),
            };
            if index == 0 {
                self.pseudo(&mut base_contents, node, PseudoElement::Before);
            }
            for child in base {
                self.add(&mut base_contents, child);
            }
            if index + 1 == count {
                self.pseudo(&mut base_contents, node, PseudoElement::After);
            }
            if annotations.is_empty() {
                // Base content without annotations is ordinary inline content.
                contents.inline.append(&mut base_contents.inline);
                continue;
            }
            let base = BlockBox {
                node: None,
                style: ruby_part_style(style),
                contents: base_contents.finish(),
                page: None,
                marker: None,
                first_line: Vec::new(),
                legend: None,
                center_contents: false,
                shape_image: None,
                replaced: None,
            };
            let annotations = self.annotations(style, annotations);
            let overhang = style.ruby_overhang == RubyOverhang::Auto;
            let text_beside = |sibling: Option<NodeId>| {
                sibling.is_some_and(|s| match self.document.node(s).data() {
                    NodeData::Text(text) => !text.trim().is_empty(),
                    _ => false,
                })
            };
            let ruby = Ruby {
                base,
                annotations,
                overhang: (
                    overhang
                        && index == 0
                        && text_beside(self.document.node(node).previous_sibling()),
                    overhang
                        && index + 1 == count
                        && text_beside(self.document.node(node).next_sibling()),
                ),
            };
            let mut segment = ComputedValues::inherit_from(style);
            segment.display = Display::Normal {
                outside: DisplayOutside::Inline,
                inside: DisplayInside::FlowRoot,
                list_item: false,
            };
            // The base's baseline aligns with the text around.
            segment.baseline_source = BaselineSource::First;
            contents.inline.push(InlineItem::Atomic(Box::new(BlockBox {
                node: None,
                style: Arc::new(segment),
                contents: BlockContents::Ruby(Box::new(ruby)),
                page: None,
                marker: None,
                first_line: Vec::new(),
                legend: None,
                center_contents: false,
                shape_image: None,
                replaced: None,
            })));
        }
        self.counters.leave();
        contents.open.pop();
        contents.inline.push(InlineItem::End);
    }

    /// The annotation boxes of a ruby segment, each holding its `rt` as an inline box.
    /// Annotations of one level are merged into one box; levels alternate sides with
    /// `ruby-position: alternate`.
    fn annotations(
        &mut self,
        ruby: &Arc<ComputedValues>,
        annotations: Vec<RubyText>,
    ) -> Vec<Annotation> {
        let merged = ruby.ruby_merge == RubyMerge::Merge;
        let mut levels: Vec<Vec<RubyText>> = Vec::new();
        for annotation in annotations {
            match levels.last_mut() {
                Some(level) if merged => level.push(annotation),
                _ => levels.push(vec![annotation]),
            }
        }
        let mut out = Vec::new();
        for (level, rts) in levels.into_iter().enumerate() {
            let first = rts[0].1.clone();
            let side = match first.ruby_position {
                RubyPosition::InterCharacter if !first.writing_mode.is_vertical() => {
                    AnnotationSide::InterCharacter
                }
                position if position.under(level) => AnnotationSide::Under,
                _ => AnnotationSide::Over,
            };
            let mut inline = Vec::new();
            for (rt, style) in rts {
                self.update_counters(Some(rt), &style);
                self.record_target(rt);
                let mut rt_contents = Contents {
                    parent_style: style.clone(),
                    blocks: Vec::new(),
                    inline: vec![InlineItem::Start {
                        style: style.clone(),
                        node: Some(rt),
                    }],
                    open: vec![(style.clone(), Some(rt))],
                };
                self.children(&mut rt_contents, rt, &style, false);
                rt_contents.inline.push(InlineItem::End);
                inline.extend(rt_contents.inline);
            }
            let style = ruby_part_style(&first);
            let block = if side == AnnotationSide::InterCharacter {
                // Annotations beside each base character are set vertically.
                let vertical = WritingMode::VerticalRl;
                BlockBox {
                    node: None,
                    style: style.clone(),
                    contents: BlockContents::Turned {
                        inner: Box::new(BlockBox {
                            node: None,
                            style: turned_style(&style, vertical),
                            contents: BlockContents::Inline(turned_items(inline, vertical)),
                            page: None,
                            marker: None,
                            first_line: Vec::new(),
                            legend: None,
                            center_contents: false,
                            shape_image: None,
                            replaced: None,
                        }),
                        outer: first.writing_mode,
                    },
                    page: None,
                    marker: None,
                    first_line: Vec::new(),
                    legend: None,
                    center_contents: false,
                    shape_image: None,
                    replaced: None,
                }
            } else {
                BlockBox {
                    node: None,
                    style,
                    contents: BlockContents::Inline(inline),
                    page: None,
                    marker: None,
                    first_line: Vec::new(),
                    legend: None,
                    center_contents: false,
                    shape_image: None,
                    replaced: None,
                }
            };
            out.push(Annotation { block, side });
        }
        out
    }

    /// What an HTML element that can embed content renders as (HTML § 15.4).
    fn embedding(&self, element: &Element, style: &ComputedValues) -> Embedding {
        let attr = |name: &str| element.attr(name).map(str::trim).filter(|v| !v.is_empty());
        let image = |url: Option<&str>| url.and_then(|url| self.images.get(url));
        let replaced = |image: Arc<Image>| {
            Embedding::Replaced(Replaced {
                size: Some(natural_size(&image, style)),
                picture: Some(Picture::raster(image)),
                widget: None,
            })
        };
        // Only `type`s that name images are loaded as images.
        let image_type =
            |name: &str| attr(name).is_none_or(|t| t.to_ascii_lowercase().starts_with("image/"));
        match element.name.local {
            local_name!("img") => {
                // `src`, or else the first `srcset` candidate, whose density descriptor
                // (such as `2x`) scales its natural size.
                let (src, density) = match attr("src") {
                    Some(src) => (Some(src), 1.0),
                    None => attr("srcset")
                        .and_then(first_srcset)
                        .map_or((None, 1.0), |(url, density)| (Some(url), density)),
                };
                if let Some(image) = image(src) {
                    let (w, h) = natural_size(&image, style);
                    return Embedding::Replaced(Replaced {
                        size: Some((w / density, h / density)),
                        picture: Some(Picture::raster(image)),
                        widget: None,
                    });
                }
                match element.attr("alt") {
                    Some(alt) if !alt.is_empty() => Embedding::Text(alt.to_owned()),
                    // Without an image, an `<img>` with a size is an empty box of that size.
                    _ if attr("width").is_some() || attr("height").is_some() => {
                        Embedding::Replaced(Replaced {
                            picture: None,
                            size: None,
                            widget: None,
                        })
                    }
                    _ => Embedding::Nothing,
                }
            }
            local_name!("video") => match image(attr("poster")) {
                Some(poster) => replaced(poster),
                None => Embedding::Replaced(Replaced {
                    picture: None,
                    size: Some((300.0, 150.0)),
                    widget: None,
                }),
            },
            local_name!("audio") if element.has_attr("controls") => Embedding::Replaced(Replaced {
                picture: None,
                size: Some((300.0, 54.0)),
                widget: Some(Widget::MediaControls),
            }),
            local_name!("audio") => Embedding::Nothing,
            local_name!("iframe") => Embedding::Replaced(Replaced {
                picture: self.frame(element, style).map(Picture::Frame),
                size: None,
                widget: None,
            }),
            local_name!("embed") if image_type("type") => match image(attr("src")) {
                Some(image) => replaced(image),
                None => Embedding::Nothing,
            },
            local_name!("embed") => Embedding::Nothing,
            local_name!("object") if image_type("type") => match image(attr("data")) {
                Some(image) => replaced(image),
                None => Embedding::Children,
            },
            // An image button shows its image, or else its alternative text.
            local_name!("input") if InputType::of(element) == InputType::Image => {
                match image(attr("src")) {
                    Some(image) => replaced(image),
                    None => Embedding::Text(
                        attr("alt")
                            .or_else(|| attr("value"))
                            .unwrap_or("Submit")
                            .to_owned(),
                    ),
                }
            }
            _ => Embedding::Children,
        }
    }

    /// The document an `iframe` shows: its `srcdoc`, or else the file at `src`, styled with
    /// the frame's size as the viewport.
    fn frame(&self, element: &Element, style: &ComputedValues) -> Option<Frame> {
        if self.images.frame_depth() >= MAX_FRAME_DEPTH {
            return None;
        }
        let html = match element.attr("srcdoc") {
            Some(srcdoc) => srcdoc.to_owned(),
            None => {
                let src = element
                    .attr("src")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())?;
                if src.eq_ignore_ascii_case("about:blank") {
                    String::new()
                } else {
                    let text = self.images.text(src);
                    if text.is_none() {
                        self.images.warn(format!("could not load frame `{src}`"));
                    }
                    text?
                }
            }
        };
        let fixed = |size: &rustyprint_css::properties::types::Size<_>, default: f32| match size {
            rustyprint_css::properties::types::Size::Value(
                rustyprint_css::properties::types::LengthPercentage::Length(px),
            ) => *px,
            _ => default,
        };
        let viewport = (fixed(&style.width, 300.0), fixed(&style.height, 150.0));
        let document = Document::parse(&html);
        rustyprint_dom::budget::check_document(&document);
        let load = |url: &str| self.images.text(url);
        let load_font = |url: &str| self.images.bytes(url);
        let styles = compute_styles(
            &document,
            &StyleOptions {
                page_size: viewport,
                stylesheet: None,
                load: &load,
                font_metrics: Some(&crate::font_metrics),
                load_font: Some(&load_font),
            },
        );
        for warning in &styles.warnings {
            self.images.warn(format!(
                "frame {}:{}: {}",
                warning.source, warning.warning.line, warning.warning.message
            ));
        }
        let root = self
            .images
            .in_frame(|| build(&document, &styles, self.images))?;
        Some(Frame {
            root: Arc::new(root),
            laid: None,
        })
    }

    /// The positions to try for an absolutely positioned box that overflows, from its
    /// `position-try-fallbacks`. Pseudo-elements, without a `node`, have no `@position-try`
    /// styles, so options that name a rule are left out for them, as are options that name a
    /// rule that does not exist.
    fn fallbacks(&self, node: Option<NodeId>, style: &Arc<ComputedValues>) -> Vec<Fallback> {
        style
            .position_try_fallbacks
            .iter()
            .filter_map(|fallback| match fallback {
                TryFallback::Rule { name, tactics } => {
                    let style = match name {
                        Some(name) => self.styles.position_try(node?, name)?.clone(),
                        None => style.clone(),
                    };
                    Some(Fallback {
                        style,
                        tactics: tactics.clone(),
                    })
                }
                TryFallback::Area(area) => {
                    let mut style = (**style).clone();
                    style.position_area = Some(area.clone());
                    Some(Fallback {
                        style: Arc::new(style),
                        tactics: Vec::new(),
                    })
                }
            })
            .collect()
    }

    /// The rendered legend of a `<fieldset>`: its first `<legend>` child, unless that is not
    /// rendered or is positioned out of flow (HTML § 15.3.13).
    fn rendered_legend(&self, node: NodeId) -> Option<NodeId> {
        let document = self.document;
        if !document
            .element(node)
            .is_some_and(|e| e.is_html(&local_name!("fieldset")))
        {
            return None;
        }
        let legend = document.element_children(node).find(|&child| {
            document
                .element(child)
                .is_some_and(|e| e.is_html(&local_name!("legend")))
        })?;
        let style = self.styles.get(legend)?;
        let in_flow = !matches!(
            style.position,
            Position::Absolute | Position::Fixed | Position::Running(_)
        );
        (in_flow && !matches!(style.display, Display::None | Display::Contents)).then_some(legend)
    }

    /// The children of a `<details>`: its first `<summary>`, or a default one, and when it
    /// is open, the rest of its children in its `::details-content` box (HTML § 15.5.17).
    fn details_children(
        &mut self,
        contents: &mut Contents,
        node: NodeId,
        style: &Arc<ComputedValues>,
    ) {
        let document = self.document;
        let open = document.element(node).is_some_and(|e| e.has_attr("open"));
        let summary = document.element_children(node).find(|&child| {
            document
                .element(child)
                .is_some_and(|e| e.is_html(&local_name!("summary")))
        });
        match summary {
            Some(summary) => self.add(contents, summary),
            None => self.default_summary(contents, node, style, open),
        }
        if !open {
            return;
        }
        let rest = document
            .children(node)
            .filter(|&child| Some(child) != summary);
        let slot = self
            .styles
            .pseudo(node, &PseudoElement::DetailsContent)
            .cloned()
            .filter(|slot| level(slot.display).is_some());
        let Some(slot) = slot else {
            for child in rest {
                self.add(contents, child);
            }
            return;
        };
        let mut inner = Contents {
            parent_style: slot.clone(),
            blocks: Vec::new(),
            inline: Vec::new(),
            open: Vec::new(),
        };
        for child in rest {
            self.add(&mut inner, child);
        }
        let inner = column_contents(
            &slot,
            flex_contents(&slot, self.table_contents(&slot, inner.finish())),
        );
        let block = BlockBox {
            node: None,
            style: slot.clone(),
            contents: inner,
            page: None,
            marker: None,
            first_line: Vec::new(),
            legend: None,
            center_contents: false,
            shape_image: None,
            replaced: None,
        };
        match level(slot.display) {
            Some(Level::Atomic) => contents.inline.push(InlineItem::Atomic(Box::new(block))),
            _ => contents.push_block(block),
        }
    }

    /// The summary of a `<details>` without one: "Details" after a disclosure triangle.
    fn default_summary(
        &mut self,
        contents: &mut Contents,
        node: NodeId,
        style: &Arc<ComputedValues>,
        open: bool,
    ) {
        let summary = Arc::new(ComputedValues::inherit_from(style));
        let marker = Arc::new(ComputedValues::inherit_from(&summary));
        let triangle = if open {
            "disclosure-open"
        } else {
            "disclosure-closed"
        };
        let text = self
            .styles
            .counter_styles
            .marker(0, &CounterStyleRef::Name(Arc::from(triangle)));
        let lang = self.language(node);
        let items = vec![
            InlineItem::Start {
                style: marker.clone(),
                node: None,
            },
            InlineItem::Text {
                text,
                style: marker,
                lang: lang.clone(),
            },
            InlineItem::End,
            InlineItem::Text {
                text: "Details".into(),
                style: summary.clone(),
                lang,
            },
        ];
        contents.push_block(BlockBox {
            node: None,
            style: summary,
            contents: BlockContents::Inline(items),
            page: None,
            marker: None,
            first_line: Vec::new(),
            legend: None,
            center_contents: false,
            shape_image: None,
            replaced: None,
        });
    }

    /// Applies `counter-reset`, `counter-increment` and `counter-set`, in that order, with
    /// the implicit `list-item` increment of list items (CSS Lists 3 § 4.4).
    fn update_counters(&mut self, node: Option<NodeId>, style: &ComputedValues) {
        for change in style.counter_reset.iter() {
            let value = match (change.value, node) {
                (Some(value), _) => value,
                // A reversed counter without a value starts where its increments end at 1.
                (None, Some(node)) => 1 - self.increments(node, &change.name),
                (None, None) => 0,
            };
            self.counters.reset(&change.name, value, change.reversed);
        }
        let mut increments_list_item = false;
        for change in style.counter_increment.iter() {
            increments_list_item |= &*change.name == "list-item";
            let counter = self.counters.innermost(&change.name);
            counter.value = counter.value.saturating_add(change.value.unwrap_or(1));
        }
        if is_list_item(style) && !increments_list_item {
            let counter = self.counters.innermost(&Arc::from("list-item"));
            let step = if counter.reversed { -1 } else { 1 };
            counter.value = counter.value.saturating_add(step);
        }
        for change in style.counter_set.iter() {
            self.counters.innermost(&change.name).value = change.value.unwrap_or(0);
        }
    }

    /// The sum of the increments of counter `name` by the descendants of `node`, which a
    /// counter it creates covers. List items of a reversed `list-item` count -1 each.
    fn increments(&self, node: NodeId, name: &str) -> i32 {
        let mut sum = 0i32;
        let mut stack: Vec<NodeId> = self.document.children(node).collect();
        stack.reverse();
        while let Some(child) = stack.pop() {
            let Some(style) = self.styles.get(child) else {
                continue;
            };
            // Hidden elements count nothing, and nested resets start counters of their own.
            if style.display == Display::None
                || style.counter_reset.iter().any(|c| &*c.name == name)
            {
                continue;
            }
            match style.counter_increment.iter().find(|c| &*c.name == name) {
                Some(change) => sum = sum.saturating_add(change.value.unwrap_or(1)),
                None if name == "list-item" && is_list_item(style) => sum -= 1,
                None => {}
            }
            let start = stack.len();
            stack.extend(self.document.children(child));
            stack[start..].reverse();
        }
        sum
    }

    /// The style and text of a list item's marker, or `None` without one.
    fn marker(
        &mut self,
        node: NodeId,
        style: &ComputedValues,
    ) -> Option<(Arc<ComputedValues>, Vec<InlineItem>)> {
        let marker_style = self
            .styles
            .pseudo(node, &PseudoElement::Marker)
            .cloned()
            .unwrap_or_else(|| Arc::new(ComputedValues::inherit_from(style)));
        let lang = self.language(node);
        let items = match &marker_style.content {
            Content::None => return None,
            Content::Items { items, .. } => self.content_items(items, &marker_style, lang),
            Content::Normal => {
                // `list-style-image`, followed by a space; the type when it does not load.
                // An image without a natural size is a square of 1em (CSS Lists 3 § 3.2).
                let picture =
                    Picture::resolve(&marker_style.list_style_image, self.images, &marker_style);
                if let Some(picture) = picture {
                    let em = marker_style.font_size;
                    let size = picture.natural_size(&marker_style).unwrap_or((em, em));
                    vec![
                        self.image_box(picture, Some(size), &marker_style),
                        InlineItem::Text {
                            text: " ".into(),
                            style: marker_style.clone(),
                            lang,
                        },
                    ]
                } else {
                    let text = match &marker_style.list_style_type {
                        ListStyleType::None => return None,
                        ListStyleType::String(text) => text.to_string(),
                        ListStyleType::Style(counter_style) => {
                            let value = self.counters.value("list-item");
                            self.styles.counter_styles.marker(value, counter_style)
                        }
                    };
                    if text.is_empty() {
                        return None;
                    }
                    vec![InlineItem::Text {
                        text,
                        style: marker_style.clone(),
                        lang,
                    }]
                }
            }
        };
        let items = self.image_symbols(items);
        (!items.is_empty()).then_some((marker_style, items))
    }

    /// Turns the image symbols of counter styles in text items into image boxes, sized
    /// like marker images: a square of 1em without a natural size.
    fn image_symbols(&self, items: Vec<InlineItem>) -> Vec<InlineItem> {
        let counter_styles = &self.styles.counter_styles;
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let InlineItem::Text { text, style, lang } = item else {
                out.push(item);
                continue;
            };
            if !text.chars().any(|c| counter_styles.image(c).is_some()) {
                out.push(InlineItem::Text { text, style, lang });
                continue;
            }
            let mut run = String::new();
            let flush = |run: &mut String, out: &mut Vec<InlineItem>| {
                if !run.is_empty() {
                    out.push(InlineItem::Text {
                        text: std::mem::take(run),
                        style: style.clone(),
                        lang: lang.clone(),
                    });
                }
            };
            for c in text.chars() {
                let Some(url) = counter_styles.image(c) else {
                    run.push(c);
                    continue;
                };
                flush(&mut run, &mut out);
                let image = ImageValue::Url(url.clone());
                if let Some(picture) = Picture::resolve(&image, self.images, &style) {
                    let em = style.font_size;
                    let size = picture.natural_size(&style).unwrap_or((em, em));
                    out.push(self.image_box(picture, Some(size), &style));
                }
            }
            flush(&mut run, &mut out);
        }
        out
    }

    /// An image in generated content: an atomic inline replaced box. Without a natural
    /// `size`, it takes the default size of replaced content.
    fn image_box(
        &self,
        picture: Picture,
        size: Option<(f32, f32)>,
        style: &Arc<ComputedValues>,
    ) -> InlineItem {
        image_item(picture, size, style)
    }

    /// Adds the `::before` or `::after` box of an element.
    fn pseudo(&mut self, contents: &mut Contents, node: NodeId, kind: PseudoElement) {
        let Some(style) = self.styles.pseudo(node, &kind).cloned() else {
            return;
        };
        // `normal` computes to `none` on `::before` and `::after`.
        let Content::Items { items, .. } = &style.content else {
            return;
        };
        if style.display == Display::None {
            return;
        }
        self.update_counters(None, &style);
        let lang = self.language(node);
        let items = self.content_items(items, &style, lang);
        if self.targetable.contains(&node) {
            let text = text_of(&items);
            if let Some(target) = self.targets.get_mut(&node) {
                match kind {
                    PseudoElement::Before => target.before = text,
                    _ => target.after = text,
                }
            }
        }
        let block = |contents| BlockBox {
            node: None,
            style: style.clone(),
            contents,
            page: None,
            marker: None,
            first_line: Vec::new(),
            legend: None,
            center_contents: false,
            shape_image: None,
            replaced: None,
        };
        if is_out_of_flow(&style) {
            let fallbacks = self.fallbacks(None, &style);
            contents.push_out_of_flow(block(BlockContents::Inline(items)), fallbacks);
            return;
        }
        if style.float != Float::None && !is_flex(&contents.parent_style) {
            contents.push_float(block(BlockContents::Inline(items)));
            return;
        }
        match level(style.display) {
            None => contents.inline.extend(items),
            Some(Level::Inline) => {
                contents.inline.push(InlineItem::Start {
                    style: style.clone(),
                    node: None,
                });
                contents.inline.extend(items);
                contents.inline.push(InlineItem::End);
            }
            Some(Level::Atomic) => {
                let atomic = block(BlockContents::Inline(items));
                contents.inline.push(InlineItem::Atomic(Box::new(atomic)));
            }
            Some(Level::Block) => contents.push_block(block(BlockContents::Inline(items))),
        }
    }

    /// Records the counters at an element that links can point to.
    fn record_target(&mut self, node: NodeId) {
        if !self.targetable.contains(&node) {
            return;
        }
        let counters = self
            .counters
            .values
            .iter()
            .filter(|(_, stack)| !stack.is_empty())
            .map(|(name, stack)| (name.clone(), stack.iter().map(|i| i.value).collect()))
            .collect();
        self.targets.insert(
            node,
            Target {
                counters,
                ..Target::default()
            },
        );
    }

    /// What is known about the element a URL points to: from this tree if it came before,
    /// or else from the layout before.
    fn target(&self, url: &str) -> Option<(NodeId, Option<&Target>)> {
        let fragment = url.trim().strip_prefix('#')?;
        let node = *self.anchors.get(&crate::links::percent_decode(fragment))?;
        let known = self
            .targets
            .get(&node)
            .or_else(|| self.previous.elements.get(&node));
        Some((node, known))
    }

    /// The values of a counter at the element a URL points to; `page` and `pages` come from
    /// the layout before.
    fn target_values(&mut self, url: &str, name: &str) -> Vec<i32> {
        self.uses_targets = true;
        let Some((node, known)) = self.target(url) else {
            return vec![0];
        };
        let previous = self.previous.elements.get(&node);
        let values = match name {
            "page" => previous.and_then(|t| t.page).map(|p| vec![p as i32]),
            "pages" => Some(vec![self.previous.pages as i32]),
            _ => known.and_then(|t| t.counters.get(name).cloned()),
        };
        values.unwrap_or_else(|| vec![0])
    }

    /// The inline items of `content`: text from strings, counters, quotes and targets, with
    /// images and leaders between. `element()` only applies in page-margin boxes.
    fn content_items(
        &mut self,
        items: &[ContentItem],
        style: &Arc<ComputedValues>,
        lang: Option<Arc<str>>,
    ) -> Vec<InlineItem> {
        let items = self.content_pieces(items, style, lang);
        self.image_symbols(items)
    }

    fn content_pieces(
        &mut self,
        items: &[ContentItem],
        style: &Arc<ComputedValues>,
        lang: Option<Arc<str>>,
    ) -> Vec<InlineItem> {
        let mut out = Vec::new();
        let mut text = String::new();
        let flush = |text: &mut String, out: &mut Vec<InlineItem>| {
            if !text.is_empty() {
                out.push(InlineItem::Text {
                    text: std::mem::take(text),
                    style: style.clone(),
                    lang: lang.clone(),
                });
            }
        };
        let quote_lang = lang.as_deref();
        for item in items {
            match item {
                ContentItem::Image(image) => {
                    if let Some(picture) = Picture::resolve(image, self.images, style) {
                        flush(&mut text, &mut out);
                        let size = picture.natural_size(style);
                        out.push(self.image_box(picture, size, style));
                    }
                }
                ContentItem::Leader(pattern) => {
                    flush(&mut text, &mut out);
                    out.push(InlineItem::Leader {
                        pattern: pattern.clone(),
                        style: style.clone(),
                    });
                }
                ContentItem::TargetCounter {
                    target,
                    name,
                    style: counter_style,
                } => {
                    let values = self.target_values(target, name);
                    let value = values.last().copied().unwrap_or(0);
                    text.push_str(&self.styles.counter_styles.format(value, counter_style));
                }
                ContentItem::TargetCounters {
                    target,
                    name,
                    separator,
                    style: counter_style,
                } => {
                    let values: Vec<String> = self
                        .target_values(target, name)
                        .into_iter()
                        .map(|value| self.styles.counter_styles.format(value, counter_style))
                        .collect();
                    text.push_str(&values.join(separator));
                }
                ContentItem::TargetText { target, kind } => {
                    self.uses_targets = true;
                    if let Some((node, known)) = self.target(target) {
                        let content = || {
                            self.document
                                .text_content(node)
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                        };
                        match kind {
                            TargetTextKind::Content => text.push_str(&content()),
                            TargetTextKind::FirstLetter => {
                                let content = content();
                                if let Some(range) = letter_range(&content) {
                                    text.push_str(&content[range]);
                                }
                            }
                            TargetTextKind::Before => {
                                text.push_str(known.map_or("", |t| t.before.as_str()));
                            }
                            TargetTextKind::After => {
                                text.push_str(known.map_or("", |t| t.after.as_str()));
                            }
                        }
                    }
                }
                other => self.content_piece(other, style, quote_lang, &mut text),
            }
        }
        flush(&mut text, &mut out);
        out
    }

    /// Adds the text of a string, counter or quote to `text`.
    fn content_piece(
        &mut self,
        item: &ContentItem,
        style: &ComputedValues,
        lang: Option<&str>,
        text: &mut String,
    ) {
        match item {
            ContentItem::String(s) => text.push_str(s),
            ContentItem::Counter { name, style } => {
                let value = self.counters.value(name);
                text.push_str(&self.styles.counter_styles.format(value, style));
            }
            ContentItem::Counters {
                name,
                separator,
                style,
            } => {
                let values: Vec<String> = self
                    .counters
                    .all(name)
                    .into_iter()
                    .map(|value| self.styles.counter_styles.format(value, style))
                    .collect();
                text.push_str(&values.join(separator));
            }
            ContentItem::OpenQuote => {
                if let Some((open, _)) = style.quotes.pair(self.quote_depth, lang) {
                    text.push_str(&open);
                }
                self.quote_depth += 1;
            }
            ContentItem::CloseQuote => {
                if self.quote_depth > 0 {
                    self.quote_depth -= 1;
                    if let Some((_, close)) = style.quotes.pair(self.quote_depth, lang) {
                        text.push_str(&close);
                    }
                }
            }
            ContentItem::NoOpenQuote => self.quote_depth += 1,
            ContentItem::NoCloseQuote => {
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            ContentItem::Image(_)
            | ContentItem::Element { .. }
            | ContentItem::Leader(_)
            | ContentItem::TargetCounter { .. }
            | ContentItem::TargetCounters { .. }
            | ContentItem::TargetText { .. } => {}
        }
    }

    /// The language of a node from the nearest `lang` or `xml:lang` attribute.
    fn language(&self, node: NodeId) -> Option<Arc<str>> {
        std::iter::once(node)
            .chain(self.document.ancestors(node))
            .filter_map(|n| self.document.element(n))
            .find_map(|e| {
                e.attrs
                    .iter()
                    .find(|a| {
                        a.name.local == local_name!("lang")
                            && (a.name.ns == ns!() || a.name.ns == ns!(xml))
                    })
                    .map(|a| a.value.as_str())
            })
            .filter(|lang| !lang.is_empty())
            .map(Arc::from)
    }

    fn parent_style(&self, node: NodeId) -> Option<Arc<ComputedValues>> {
        let parent = self.document.parent_element(node)?;
        self.styles.get(parent).cloned()
    }
}

/// An annotation element of a ruby container and its style.
type RubyText = (NodeId, Arc<ComputedValues>);

/// Collects a block container's children.
struct Contents {
    parent_style: Arc<ComputedValues>,
    blocks: Vec<BlockBox>,
    /// Inline items not yet placed in a block.
    inline: Vec<InlineItem>,
    /// Inline boxes open at this point, outermost first.
    open: Vec<(Arc<ComputedValues>, Option<NodeId>)>,
}

/// The style of the anonymous block holding a ruby base or annotation, inheriting from
/// `parent`. Ruby layout aligns its content (`ruby-align`), so it is not indented or aligned.
fn ruby_part_style(parent: &ComputedValues) -> Arc<ComputedValues> {
    let mut style = ComputedValues::inherit_from(parent);
    style.display = Display::BLOCK;
    style.text_align = TextAlign::Start;
    style.text_align_last = TextAlignLast::Auto;
    style.text_indent.length = LengthPercentage::ZERO;
    Arc::new(style)
}

/// A style in the axes of `writing_mode`, which it takes.
fn turned_style(style: &Arc<ComputedValues>, writing_mode: WritingMode) -> Arc<ComputedValues> {
    let mut turned = (**style).clone();
    turned.writing_mode = writing_mode;
    turned.unrotated = None;
    Arc::new(turned).in_line_axes(writing_mode)
}

/// Inline content set in `writing_mode` instead.
fn turned_items(items: Vec<InlineItem>, writing_mode: WritingMode) -> Vec<InlineItem> {
    items
        .into_iter()
        .map(|item| match item {
            InlineItem::Text { text, style, lang } => InlineItem::Text {
                text,
                style: turned_style(&style, writing_mode),
                lang,
            },
            InlineItem::Start { style, node } => InlineItem::Start {
                style: turned_style(&style, writing_mode),
                node,
            },
            InlineItem::Leader { pattern, style } => InlineItem::Leader {
                pattern,
                style: turned_style(&style, writing_mode),
            },
            other => other,
        })
        .collect()
}

/// Whether a style makes its box a flex or grid container, whose children are its items.
fn is_flex(style: &ComputedValues) -> bool {
    matches!(
        style.display,
        Display::Normal {
            inside: DisplayInside::Flex | DisplayInside::Grid,
            ..
        }
    )
}

/// The contents of a flex or grid container become its items: each block-level child, and
/// each run of inline content in an anonymous block. Runs of nothing but white space are
/// left out (CSS Flexbox 1 § 4, CSS Grid 2 § 6).
fn flex_contents(style: &Arc<ComputedValues>, contents: BlockContents) -> BlockContents {
    let items = |contents| blocks_of(contents, style);
    match style.display {
        Display::Normal {
            inside: DisplayInside::Flex,
            ..
        } => BlockContents::Flex(items(contents)),
        Display::Normal {
            inside: DisplayInside::Grid,
            ..
        } => BlockContents::Grid(items(contents)),
        _ => contents,
    }
}

/// The contents of a multi-column container: runs of its children in anonymous column
/// flows, which break into columns, and the children that span all columns between them
/// (CSS Multi-column 1 § 6.1). Only children span; deeper boxes stay in their column.
fn column_contents(style: &Arc<ComputedValues>, contents: BlockContents) -> BlockContents {
    if !crate::multicol::is_multicol(style)
        || !matches!(
            contents,
            BlockContents::Blocks(_) | BlockContents::Inline(_)
        )
    {
        return contents;
    }
    let flow = |blocks: Vec<BlockBox>| BlockBox {
        node: None,
        style: Arc::new(ComputedValues::inherit_from(style)),
        contents: BlockContents::Blocks(blocks),
        page: None,
        marker: None,
        first_line: Vec::new(),
        legend: None,
        center_contents: false,
        shape_image: None,
        replaced: None,
    };
    let mut segments = Vec::new();
    let mut run = Vec::new();
    for block in blocks_of(contents, style) {
        if crate::multicol::is_spanner(&block) {
            if !run.is_empty() {
                segments.push(flow(std::mem::take(&mut run)));
            }
            segments.push(block);
        } else {
            run.push(block);
        }
    }
    if !run.is_empty() || segments.is_empty() {
        segments.push(flow(run));
    }
    BlockContents::Blocks(segments)
}

/// Whether a style takes its box out of flow, to be placed by its containing block.
fn is_out_of_flow(style: &ComputedValues) -> bool {
    matches!(style.position, Position::Absolute | Position::Fixed)
}

/// The placeholder of an out-of-flow box, styled as anonymous content of `parent`. Inline
/// placeholders align with the top of their line, where the box's static position is.
fn placeholder(block: BlockBox, fallbacks: Vec<Fallback>, parent: &ComputedValues) -> BlockBox {
    let mut style = ComputedValues::inherit_from(parent);
    style.baseline_shift = rustyprint_css::properties::types::BaselineShift::Top;
    BlockBox {
        node: None,
        style: Arc::new(style),
        contents: BlockContents::OutOfFlow(Arc::new(Positioned { block, fallbacks })),
        page: None,
        marker: None,
        first_line: Vec::new(),
        legend: None,
        center_contents: false,
        shape_image: None,
        replaced: None,
    }
}

/// Whether a box is the placeholder of an out-of-flow box.
fn is_placeholder(item: &InlineItem) -> bool {
    matches!(item, InlineItem::Atomic(block) if matches!(block.contents, BlockContents::OutOfFlow(_)))
}

impl Contents {
    /// Adds a float: as a block between blocks, or in the line after inline content, where
    /// it goes at the top of its line.
    fn push_float(&mut self, block: BlockBox) {
        if !self.open.is_empty() || !is_collapsible(&self.inline) {
            let mut style = ComputedValues::inherit_from(&self.parent_style);
            style.baseline_shift = rustyprint_css::properties::types::BaselineShift::Top;
            self.inline.push(InlineItem::Atomic(Box::new(BlockBox {
                node: None,
                style: Arc::new(style),
                contents: BlockContents::Float(Arc::new(block)),
                page: None,
                marker: None,
                first_line: Vec::new(),
                legend: None,
                center_contents: false,
                shape_image: None,
                replaced: None,
            })));
        } else {
            self.push_block(block);
        }
    }

    /// Adds an out-of-flow box's placeholder: in the line, after inline content, or else
    /// between blocks, where it does not split inline content into anonymous blocks.
    fn push_out_of_flow(&mut self, block: BlockBox, fallbacks: Vec<Fallback>) {
        let placeholder = placeholder(block, fallbacks, &self.parent_style);
        if !self.open.is_empty() || !is_collapsible(&self.inline) {
            self.inline.push(InlineItem::Atomic(Box::new(placeholder)));
        } else {
            self.push_block(placeholder);
        }
    }

    /// Adds a block-level box, wrapping preceding inline content in an anonymous block and
    /// splitting any open inline boxes around the block.
    fn push_block(&mut self, block: BlockBox) {
        for _ in &self.open {
            self.inline.push(InlineItem::End);
        }
        self.flush_inline();
        for (style, node) in &self.open {
            self.inline.push(InlineItem::Start {
                style: style.clone(),
                node: *node,
            });
        }
        self.blocks.push(block);
    }

    /// Wraps pending inline content in an anonymous block, unless it is only collapsible
    /// whitespace.
    fn flush_inline(&mut self) {
        let items = std::mem::take(&mut self.inline);
        if is_collapsible(&items) {
            self.blocks.extend(placeholders(items));
            return;
        }
        let style = Arc::new(ComputedValues::inherit_from(&self.parent_style));
        self.blocks.push(BlockBox {
            node: None,
            style,
            contents: BlockContents::Inline(items),
            page: None,
            marker: None,
            first_line: Vec::new(),
            legend: None,
            center_contents: false,
            shape_image: None,
            replaced: None,
        });
    }

    fn finish(mut self) -> BlockContents {
        // Inline content of nothing but white space and placeholders makes no line: its
        // placeholders become block-level.
        let only_placeholders =
            self.inline.iter().any(is_placeholder) && is_collapsible(&self.inline);
        if self.blocks.is_empty() && !only_placeholders {
            return BlockContents::Inline(self.inline);
        }
        self.flush_inline();
        BlockContents::Blocks(self.blocks)
    }
}

/// The block-level boxes of a box's contents: inline content becomes an anonymous block,
/// unless it is only collapsible white space.
fn blocks_of(contents: BlockContents, style: &Arc<ComputedValues>) -> Vec<BlockBox> {
    match contents {
        BlockContents::Blocks(blocks) => blocks,
        BlockContents::Inline(items) if is_collapsible(&items) => placeholders(items).collect(),
        contents @ (BlockContents::Inline(_)
        | BlockContents::Flex(_)
        | BlockContents::Grid(_)
        | BlockContents::Table(_)
        | BlockContents::OutOfFlow(_)
        | BlockContents::Float(_)
        | BlockContents::Turned { .. }
        | BlockContents::Ruby(_)) => vec![BlockBox {
            node: None,
            style: Arc::new(ComputedValues::inherit_from(style)),
            contents,
            page: None,
            marker: None,
            first_line: Vec::new(),
            legend: None,
            center_contents: false,
            shape_image: None,
            replaced: None,
        }],
    }
}

/// The placeholders among inline items, as block-level boxes.
fn placeholders(items: Vec<InlineItem>) -> impl Iterator<Item = BlockBox> {
    items.into_iter().filter_map(|item| match item {
        InlineItem::Atomic(block) if matches!(block.contents, BlockContents::OutOfFlow(_)) => {
            Some(*block)
        }
        _ => None,
    })
}

/// Whether inline items contain nothing but collapsible white space and the placeholders
/// of out-of-flow boxes.
fn is_collapsible(items: &[InlineItem]) -> bool {
    items.iter().all(|item| match item {
        InlineItem::Text { text, style, .. } => text.chars().all(|c| match c {
            ' ' | '\t' | '\r' | '\u{c}' => matches!(
                style.white_space_collapse,
                WhiteSpaceCollapse::Collapse
                    | WhiteSpaceCollapse::PreserveBreaks
                    | WhiteSpaceCollapse::Discard
            ),
            '\n' => matches!(
                style.white_space_collapse,
                WhiteSpaceCollapse::Collapse | WhiteSpaceCollapse::Discard
            ),
            _ => false,
        }),
        InlineItem::Start { .. } | InlineItem::End => true,
        item @ InlineItem::Atomic(_) => is_placeholder(item),
        InlineItem::LineBreak | InlineItem::Leader { .. } => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyprint_css::{StyleOptions, compute_styles};

    fn tree(html: &str) -> BlockBox {
        let document = Document::parse(html);
        let load = |_: &str| None;
        let styles = compute_styles(
            &document,
            &StyleOptions {
                page_size: (800.0, 1000.0),
                stylesheet: None,
                load: &load,
                font_metrics: None,
                load_font: None,
            },
        );
        build(&document, &styles, &Images::none()).unwrap()
    }

    /// A compact description of the tree: `B[...]` for blocks, `I[...]` for inline content,
    /// `X[...]` for flex items, `M` before a block with an outside marker.
    fn describe(block: &BlockBox) -> String {
        let marker = match &block.marker {
            Some(marker) => format!("M{} ", describe(marker)),
            None => String::new(),
        };
        match &block.contents {
            BlockContents::Blocks(children) => {
                format!(
                    "{marker}B[{}]",
                    children.iter().map(describe).collect::<Vec<_>>().join(" ")
                )
            }
            BlockContents::Inline(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| match item {
                        InlineItem::Text { text, .. } => format!("{:?}", text.trim()),
                        InlineItem::Start { .. } => "<".into(),
                        InlineItem::End => ">".into(),
                        InlineItem::LineBreak => "br".into(),
                        InlineItem::Atomic(block) => format!("A{}", describe(block)),
                        InlineItem::Leader { pattern, .. } => format!("L{pattern:?}"),
                    })
                    .filter(|p| p != "\"\"")
                    .collect();
                format!("{marker}I[{}]", parts.join(" "))
            }
            BlockContents::Flex(items) => {
                format!(
                    "{marker}X[{}]",
                    items.iter().map(describe).collect::<Vec<_>>().join(" ")
                )
            }
            BlockContents::Grid(items) => {
                format!(
                    "{marker}G[{}]",
                    items.iter().map(describe).collect::<Vec<_>>().join(" ")
                )
            }
            BlockContents::OutOfFlow(positioned) => format!("P{}", describe(&positioned.block)),
            BlockContents::Float(block) => format!("F{}", describe(block)),
            BlockContents::Table(table) => {
                let captions: Vec<String> = table.captions.iter().map(describe).collect();
                let groups: Vec<String> = table
                    .groups
                    .iter()
                    .map(|group| {
                        let rows: Vec<String> = group
                            .rows
                            .iter()
                            .map(|row| {
                                let cells: Vec<String> = row
                                    .cells
                                    .iter()
                                    .map(|cell| {
                                        let span = if cell.colspan > 1 || cell.rowspan > 1 {
                                            format!("{}x{}", cell.colspan, cell.rowspan)
                                        } else {
                                            String::new()
                                        };
                                        format!("C{}{span}{}", cell.column, describe(&cell.block))
                                    })
                                    .collect();
                                format!("R[{}]", cells.join(" "))
                            })
                            .collect();
                        let kind = match group.kind {
                            crate::table::GroupKind::Header => "H",
                            crate::table::GroupKind::Body => "G",
                            crate::table::GroupKind::Footer => "F",
                        };
                        format!("{kind}[{}]", rows.join(" "))
                    })
                    .collect();
                let caption = if captions.is_empty() {
                    String::new()
                } else {
                    format!("{} ", captions.join(" "))
                };
                format!(
                    "{marker}T{}[{caption}{}]",
                    table.columns.len(),
                    groups.join(" ")
                )
            }
            BlockContents::Turned { inner, .. } => format!("{marker}W[{}]", describe(inner)),
            BlockContents::Ruby(ruby) => format!(
                "{marker}R[{}{}]",
                describe(&ruby.base),
                ruby.annotations
                    .iter()
                    .map(|a| format!(" {:?}:{}", a.side, describe(&a.block)))
                    .collect::<String>()
            ),
        }
    }

    fn body(html: &str) -> String {
        let root = tree(html);
        let BlockContents::Blocks(children) = &root.contents else {
            panic!()
        };
        describe(children.last().unwrap())
    }

    #[test]
    fn tables_get_a_grid() {
        assert_eq!(
            body("<table><caption>c</caption><tr><td>a<td>b</table>"),
            r#"B[T2[I["c"] G[R[C0I["a"] C1I["b"]]]]]"#
        );
        // The header group goes first and the footer last; whitespace makes no boxes.
        assert_eq!(
            body(
                "<table><tfoot><tr><td>f</tfoot> <tbody><tr><td>b</tbody> <thead><tr><td>h</thead></table>"
            ),
            r#"B[T1[H[R[C0I["h"]]] G[R[C0I["b"]]] F[R[C0I["f"]]]]]"#
        );
    }

    #[test]
    fn cells_skip_slots_of_spanning_cells() {
        assert_eq!(
            body("<table><tr><td rowspan=2>a<td colspan=2>b<tr><td>c<td>d</table>"),
            r#"B[T3[G[R[C01x2I["a"] C12x1I["b"]] R[C1I["c"] C2I["d"]]]]]"#
        );
        // `rowspan=0` reaches the end of the group.
        assert_eq!(
            body("<table><tr><td rowspan=0>a<td>b<tr><td>c<tr><td>d</table>"),
            r#"B[T2[G[R[C01x3I["a"] C1I["b"]] R[C1I["c"]] R[C1I["d"]]]]]"#
        );
    }

    #[test]
    fn misparented_table_parts_get_anonymous_boxes() {
        // Cells outside rows get an anonymous row, and text in a row an anonymous cell.
        assert_eq!(
            body("<div style='display: table'><div style='display: table-cell'>a</div>b</div>"),
            r#"B[T2[G[R[C0I["a"] C1B[I["b"]]]]]]"#
        );
        // Consecutive cells outside a table share an anonymous table.
        assert_eq!(
            body(
                "<div><span style='display: table-cell'>a</span><span style='display: table-cell'>b</span></div>"
            ),
            r#"B[B[T2[G[R[C0I["a"] C1I["b"]]]]]]"#
        );
    }

    #[test]
    fn blocks_and_inline_content() {
        assert_eq!(
            body("<p>Hello <b>world</b></p>"),
            r#"B[I["Hello" < "world" >]]"#
        );
    }

    #[test]
    fn hidden_elements_generate_no_boxes() {
        assert_eq!(
            body("<p>a</p><div hidden>b</div><p style='display:none'>c</p>"),
            r#"B[I["a"]]"#
        );
    }

    #[test]
    fn mixed_content_gets_anonymous_blocks() {
        assert_eq!(
            body("text<p>para</p>more"),
            r#"B[I["text"] I["para"] I["more"]]"#
        );
        // Whitespace between blocks generates nothing.
        assert_eq!(body("<p>a</p>\n  <p>b</p>"), r#"B[I["a"] I["b"]]"#);
    }

    #[test]
    fn flex_items() {
        // Each child element is an item, as is each run of text; white space is not.
        assert_eq!(
            body("<div style='display:flex'>a <span>b</span>\n <img alt='' width=1> c</div>"),
            r#"B[X[I["a"] I["b"] B[] I["c"]]]"#
        );
        // Floats do not float in a flex container.
        assert_eq!(
            body("<div style='display:flex'><span style='float:left'>f</span></div>"),
            r#"B[X[I["f"]]]"#
        );
    }

    #[test]
    fn first_letters() {
        fn letter(text: &str) -> Option<&str> {
            letter_range(text).map(|r| &text[r])
        }
        assert_eq!(letter("  Hello"), Some("H"));
        assert_eq!(letter("“Quote”"), Some("“Q"));
        assert_eq!(letter("(a), b"), Some("(a),"));
        assert_eq!(letter("e\u{301}tude"), Some("e\u{301}"));
        assert_eq!(letter("7 days"), Some("7"));
        assert_eq!(letter(" ... "), None);
        assert_eq!(letter("   "), None);

        let style = "<style>p::first-letter { color: red }</style>";
        assert_eq!(
            body(&format!("{style}<p> <b>Hi</b> there</p>")),
            r#"B[I[< < "H" > "i" > "there"]]"#
        );
        // The first letter can come from `::before`, and an inside marker does not count.
        assert_eq!(
            body(&format!(
                "{style}<style>p::before {{ content: 'Z' }}</style><p>x</p>"
            )),
            r#"B[I[< < "Z" > > "x"]]"#
        );
        assert_eq!(
            body(
                "<style>li::first-letter { color: red }</style>\
                 <ul style='list-style-position: inside'><li>ab</li></ul>"
            ),
            r#"B[B[I[< "•" > < "a" > "b"]]]"#
        );
        // Content that starts with an atomic inline has no first letter.
        assert_eq!(
            body(&format!(
                "{style}<p><span style='display: inline-block'>a</span>b</p>"
            )),
            r#"B[I[AI["a"] "b"]]"#
        );
    }

    #[test]
    fn inline_elements_split_around_blocks() {
        assert_eq!(
            body("<span>a<div>b</div>c</span>"),
            r#"B[I[< "a" >] I["b"] I[< "c" >]]"#
        );
    }

    #[test]
    fn contents_and_atomic_inlines() {
        assert_eq!(
            body("<div style='display:contents'><p>a</p></div>"),
            r#"B[I["a"]]"#
        );
        assert_eq!(
            body("<p>x<span style='display:inline-block'>y</span>br<br>z</p>"),
            r#"B[I["x" AI["y"] "br" br "z"]]"#
        );
    }

    #[test]
    fn out_of_flow_boxes_leave_placeholders() {
        // In a line after inline content, and between blocks otherwise.
        assert_eq!(
            body("<p>x<span style='position:absolute'>y</span>z</p>"),
            r#"B[I["x" API["y"] "z"]]"#
        );
        assert_eq!(
            body("<div><p style='position:absolute'>a</p><p>b</p></div>"),
            r#"B[B[PI["a"] I["b"]]]"#
        );
        // Placeholders alone make no line.
        assert_eq!(
            body("<div> <b style='position:fixed'>f</b> </div>"),
            r#"B[B[PI["f"]]]"#
        );
    }

    #[test]
    fn generated_content() {
        assert_eq!(
            body(
                "<style>p::before { content: 'a' } p::after { content: 'c'; display: block }\
                 </style><p>b</p>"
            ),
            r#"B[B[I[< "a" > "b"] I["c"]]]"#
        );
        assert_eq!(
            body("<p><q>a<q>b</q></q></p>"),
            r#"B[I[< < "“" > "a" < < "‘" > "b" < "’" > > < "”" > >]]"#
        );
    }

    #[test]
    fn list_markers() {
        assert_eq!(
            body("<ol start=3><li>a<li>b</ol>"),
            r#"B[B[MI["3."] I["a"] MI["4."] I["b"]]]"#
        );
        assert_eq!(
            body("<ol reversed><li>a<li style='list-style-position: inside'>b<li>c</ol>"),
            r#"B[B[MI["3."] I["a"] I[< "2." > "b"] MI["1."] I["c"]]]"#
        );
        assert_eq!(
            body("<ul><li>a<li value=7>b</ul>"),
            r#"B[B[MI["•"] I["a"] MI["•"] I["b"]]]"#
        );
    }

    #[test]
    fn counters_nest() {
        let html = "<style>ol { counter-reset: s } li { counter-increment: s; list-style: none }\
                    li::before { content: counters(s, '.') ' ' }</style>\
                    <ol><li>a<ol><li>b<li>c</ol><li>d</ol>";
        assert_eq!(
            body(html),
            r#"B[B[B[I[< "1" > "a"] B[I[< "1.1" > "b"] I[< "1.2" > "c"]]] I[< "2" > "d"]]]"#
        );
    }
}
