//! Page-margin boxes (CSS Paged Media 3 § 5): running headers, footers and page numbers in
//! the margins of a page.

use rustyprint_css::ComputedValues;
use rustyprint_css::counter_style::CounterStyles;
use rustyprint_css::page::MarginBox;
use rustyprint_css::properties::types::{
    AlignmentBaseline, BaselineShift, Content, ContentItem, ElementPolicy, ImageValue,
    Size as CssSize,
};

use std::collections::HashMap;

use rustyprint_dom::NodeId;

use crate::boxes::{BlockBox, BlockContents, InlineItem, RunningElement, image_item};
use crate::display::Rect;
use crate::flow::{self, BlockKind, ContainingBlock};
use crate::fragment::{BoxFragment, Fragment};
use crate::images::Images;
use crate::paginate::LaidPage;
use crate::picture::Picture;

/// A laid-out margin box, positioned in page coordinates.
#[derive(Debug, Clone)]
pub struct MarginBoxFragment {
    pub name: MarginBox,
    pub fragment: BoxFragment,
}

/// A piece of a margin box's content.
#[derive(Debug, PartialEq)]
enum Piece {
    Text(String),
    Image(ImageValue),
}

/// The text and images of a margin box's `content`, or `None` if the box is not generated.
/// `number` is the page's number and `total` the number of pages, for `counter(page)` and
/// `counter(pages)`; other counters are zero.
fn content_pieces(
    style: &ComputedValues,
    number: usize,
    total: usize,
    counter_styles: &CounterStyles,
) -> Option<Vec<Piece>> {
    let Content::Items { items, .. } = &style.content else {
        return None;
    };
    let mut pieces = Vec::new();
    let counter = |name: &str| match name {
        "page" => number as i32,
        "pages" => total as i32,
        _ => 0,
    };
    let mut text = String::new();
    for item in items.iter() {
        match item {
            ContentItem::String(s) => text.push_str(s),
            ContentItem::Counter { name, style } | ContentItem::Counters { name, style, .. } => {
                let formatted = counter_styles.format(counter(name), style);
                text.push_str(&counter_styles.without_images(&formatted));
            }
            ContentItem::Image(image) => {
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Image(image.clone()));
            }
            // Quotes have no nesting in a margin box, and running elements replace the text.
            // Targets and leaders only apply in the document.
            ContentItem::Element { .. }
            | ContentItem::TargetCounter { .. }
            | ContentItem::TargetCounters { .. }
            | ContentItem::TargetText { .. }
            | ContentItem::Leader(_)
            | ContentItem::OpenQuote
            | ContentItem::CloseQuote
            | ContentItem::NoOpenQuote
            | ContentItem::NoCloseQuote => {}
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Some(pieces)
}

/// A generated margin box before it is placed.
struct Generated {
    name: MarginBox,
    block: BlockBox,
}

/// Finds the running element a page shows for `element(name, policy)`.
type FindRunning<'a> = &'a dyn Fn(&str, ElementPolicy) -> Option<&'a BlockBox>;

/// Lays out the margin boxes of a page.
fn margin_boxes(
    page: &LaidPage,
    number: usize,
    total: usize,
    running: FindRunning<'_>,
    counter_styles: &CounterStyles,
    images: &Images,
) -> Vec<MarginBoxFragment> {
    let generated: Vec<Generated> = page
        .style
        .margin_boxes
        .iter()
        .filter_map(|(name, style)| {
            let pieces = content_pieces(style, number, total, counter_styles)?;
            // `element()` puts a running element in the box instead of text.
            let element = match &style.content {
                Content::Items { items, .. } => items.iter().find_map(|item| match item {
                    ContentItem::Element { name, policy } => Some(running(name, *policy)),
                    _ => None,
                }),
                _ => None,
            };
            let contents = match element {
                Some(Some(block)) => BlockContents::Blocks(vec![block.clone()]),
                Some(None) => BlockContents::Blocks(Vec::new()),
                None => BlockContents::Inline(
                    pieces
                        .into_iter()
                        .filter_map(|piece| match piece {
                            Piece::Text(text) => Some(InlineItem::Text {
                                text,
                                style: style.clone(),
                                lang: None,
                            }),
                            Piece::Image(image) => {
                                let picture = Picture::resolve(&image, images, style)?;
                                let size = picture.natural_size(style);
                                Some(image_item(picture, size, style))
                            }
                        })
                        .collect(),
                ),
            };
            Some(Generated {
                name: *name,
                block: BlockBox {
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
            })
        })
        .collect();
    if generated.is_empty() {
        return Vec::new();
    }
    let find = |name: MarginBox| generated.iter().find(|g| g.name == name);

    let (width, height) = (page.size.width, page.size.height);
    let pb = page.page_box;
    let (top, left) = (pb.y, pb.x);
    let right = width - pb.right();
    let bottom = height - pb.bottom();
    let mut out = Vec::new();
    let mut place = |g: &Generated, rect: Rect| {
        out.push(MarginBoxFragment {
            name: g.name,
            fragment: lay_out(&g.block, rect),
        });
    };

    use MarginBox::*;
    // The corners.
    for (name, rect) in [
        (TopLeftCorner, Rect::new(0.0, 0.0, left, top)),
        (TopRightCorner, Rect::new(width - right, 0.0, right, top)),
        (
            BottomRightCorner,
            Rect::new(width - right, height - bottom, right, bottom),
        ),
        (
            BottomLeftCorner,
            Rect::new(0.0, height - bottom, left, bottom),
        ),
    ] {
        if let Some(g) = find(name) {
            place(g, rect);
        }
    }

    // The top and bottom edges divide their width between three boxes.
    let span = pb.width;
    for (names, y, h) in [
        ([TopLeft, TopCenter, TopRight], 0.0, top),
        (
            [BottomLeft, BottomCenter, BottomRight],
            height - bottom,
            bottom,
        ),
    ] {
        let boxes = names.map(find);
        let sizes = boxes.map(|g| g.map(|g| outer_width(&g.block, span)));
        let [a, b, c] = distribute(span, sizes);
        let mut x = left;
        for (g, w) in boxes.iter().zip([a, b, c]) {
            if let Some(g) = g {
                place(g, Rect::new(x, y, w, h));
            }
            x += w;
        }
    }

    // The left and right edges divide their height, by the height each box's content
    // needs at the margin's width.
    let span = pb.height;
    for (names, x, w) in [
        ([LeftTop, LeftMiddle, LeftBottom], 0.0, left),
        ([RightTop, RightMiddle, RightBottom], width - right, right),
    ] {
        let boxes = names.map(find);
        let sizes = boxes.map(|g| g.map(|g| content_height(&g.block, w)));
        let [a, b, c] = distribute(span, sizes);
        let mut y = top;
        for (g, h) in boxes.iter().zip([a, b, c]) {
            if let Some(g) = g {
                place(g, Rect::new(x, y, w, h));
            }
            y += h;
        }
    }
    out
}

/// Divides `span` between the start, middle and end boxes of an edge (§ 5.3.2), given the
/// size each generated box's content wants. A middle box is centered with the room it
/// needs, and the boxes beside it share the rest equally; without one, the start and end
/// boxes share the span in proportion to what they want.
fn distribute(span: f32, sizes: [Option<f32>; 3]) -> [f32; 3] {
    match sizes {
        [_, Some(middle), _] => {
            let middle = middle.min(span);
            let side = (span - middle) / 2.0;
            [side, middle, side]
        }
        [Some(a), None, Some(c)] => {
            let total = a + c;
            let a = if total > 0.0 {
                span * a / total
            } else {
                span / 2.0
            };
            [a, 0.0, span - a]
        }
        [Some(_), None, None] => [span, 0.0, 0.0],
        [None, None, Some(_)] => [0.0, 0.0, span],
        [None, None, None] => [0.0; 3],
    }
}

/// The width a margin box wants along a top or bottom edge of `span`: a percentage width
/// refers to the edge, other widths are the content's max-content width.
fn outer_width(block: &BlockBox, span: f32) -> f32 {
    match &block.style.width {
        CssSize::Value(value) if value.as_length().is_none() => {
            let extra = flow::intrinsic_outer(block).1 - flow::intrinsic_content_width(block);
            value.resolve(span) + extra
        }
        _ => flow::intrinsic_outer(block).1,
    }
}

/// The margin-box height a box's content needs at `width`.
fn content_height(block: &BlockBox, width: f32) -> f32 {
    let cb = ContainingBlock {
        width,
        height: None,
        rtl: false,
    };
    let laid = flow::layout_block(block, cb, BlockKind::InFlow, &[], None, None);
    laid.fragment.rect.height + laid.margin.vertical()
}

/// Lays out a margin box in `rect`: its margin box fills the rectangle, and its content is
/// aligned vertically as `vertical-align` asks.
fn lay_out(block: &BlockBox, rect: Rect) -> BoxFragment {
    let style = &block.style;
    let cb = ContainingBlock {
        width: rect.width,
        height: Some(rect.height),
        rtl: false,
    };
    let laid = flow::layout_block(block, cb, BlockKind::InFlow, &[], None, None);
    let mut fragment = laid.fragment;
    let margin = laid.margin;
    fragment.rect.x = rect.x + margin.left;
    fragment.rect.y = rect.y + margin.top;
    if matches!(style.height, CssSize::Auto) {
        let content = fragment.rect.height;
        let available = (rect.height - margin.vertical()).max(content);
        let fraction = match (&style.baseline_shift, style.alignment_baseline) {
            (BaselineShift::Top, _) => 0.0,
            (BaselineShift::Bottom, _) => 1.0,
            (_, AlignmentBaseline::Middle | AlignmentBaseline::Central) => 0.5,
            (BaselineShift::Center, _) => 0.5,
            _ => 0.0,
        };
        let shift = (available - content) * fraction;
        for child in &mut fragment.children {
            match child {
                Fragment::Line(line) => line.rect.y += shift,
                Fragment::Box(child) => child.rect.y += shift,
            }
        }
        fragment.rect.height = available;
    }
    fragment
}

/// Lays out the margin boxes of every page, numbered from one. `running` are the running
/// elements in document order, and `order` gives each element's position in the document.
pub(crate) fn all(
    pages: &mut [LaidPage],
    running: &[RunningElement],
    order: &HashMap<NodeId, usize>,
    counter_styles: &CounterStyles,
    images: &Images,
) {
    let total = pages.len();
    let position = |node: Option<NodeId>| node.and_then(|n| order.get(&n).copied());
    // A running element is assigned on the page where the content after it is laid out:
    // after the last element of the page before, up to the last element of this page.
    let mut previous_end = None;
    for (index, page) in pages.iter_mut().enumerate() {
        let end = page
            .root
            .as_ref()
            .and_then(|root| {
                root.element_boxes()
                    .iter()
                    .filter_map(|(node, ..)| order.get(node).copied())
                    .max()
            })
            .or(previous_end);
        let on_page = |p: usize| previous_end.is_none_or(|s| p > s) && end.is_some_and(|e| p <= e);
        let before = |p: usize| previous_end.is_some_and(|s| p <= s);
        let find = |name: &str, policy: ElementPolicy| -> Option<&BlockBox> {
            let named = || running.iter().filter(|r| &*r.name == name);
            let here: Vec<&RunningElement> = named()
                .filter(|r| position(r.block.node).is_some_and(on_page))
                .collect();
            let earlier = named().rfind(|r| position(r.block.node).is_some_and(before));
            let chosen = match policy {
                ElementPolicy::First | ElementPolicy::Start => here.first().copied().or(earlier),
                ElementPolicy::Last => here.last().copied().or(earlier),
                ElementPolicy::FirstExcept if here.is_empty() => earlier,
                ElementPolicy::FirstExcept => None,
            };
            chosen.map(|r| &r.block)
        };
        page.margin_boxes = margin_boxes(page, index + 1, total, &find, counter_styles, images);
        previous_end = end;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rustyprint_css::counter_style::CounterStyleRef;

    use super::*;

    #[test]
    fn middle_boxes_are_centered() {
        assert_eq!(
            distribute(100.0, [Some(10.0), Some(20.0), None]),
            [40.0, 20.0, 40.0]
        );
        assert_eq!(
            distribute(100.0, [Some(30.0), None, Some(10.0)]),
            [75.0, 0.0, 25.0]
        );
        assert_eq!(
            distribute(100.0, [None, None, Some(5.0)]),
            [0.0, 0.0, 100.0]
        );
    }

    #[test]
    fn page_counters() {
        let mut style = ComputedValues::initial();
        let items: Arc<[ContentItem]> = Arc::from([
            ContentItem::String(Arc::from("Page ")),
            ContentItem::Counter {
                name: Arc::from("page"),
                style: CounterStyleRef::Name(Arc::from("upper-roman")),
            },
            ContentItem::String(Arc::from(" of ")),
            ContentItem::Counter {
                name: Arc::from("pages"),
                style: CounterStyleRef::decimal(),
            },
        ]);
        style.content = Content::Items { items, alt: None };
        let counter_styles = CounterStyles::predefined();
        assert_eq!(
            content_pieces(&style, 4, 9, &counter_styles),
            Some(vec![Piece::Text("Page IV of 9".into())])
        );
        assert_eq!(
            content_pieces(&ComputedValues::initial(), 1, 1, &counter_styles),
            None
        );
    }
}
