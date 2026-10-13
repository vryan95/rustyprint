//! Pagination: laying the root out page after page, each continuing where the page before
//! broke, on pages styled by `@page` rules (CSS Fragmentation 3, CSS Paged Media 3).

use std::sync::Arc;

use rustyprint_css::Styles;
use rustyprint_css::page::{PageKey, PageSize, PageStyle};
use rustyprint_css::properties::types::{
    Direction, LengthPercentage, MaybeAuto, PageSide, Position, WritingMode,
};
use rustyprint_dom::budget;

use crate::PageOptions;
use crate::boxes::BlockBox;
use crate::display::{Rect, Size};
use crate::flow::{self, BreakToken, Fragmentainer};
use crate::fragment::{BoxFragment, Fragment, Sides};
use crate::margins::MarginBoxFragment;

/// More pages than any document should need, in case layout stops making progress.
const MAX_PAGES: usize = 100_000;

/// A laid-out page.
#[derive(Debug, Clone)]
pub struct LaidPage {
    /// What the page's `@page` rules matched.
    pub key: PageKey,
    pub style: Arc<PageStyle>,
    /// The page's size: the trim size, without bleed.
    pub size: Size,
    /// The page box's border box: the page without its margins.
    pub page_box: Rect,
    pub border: Sides,
    pub padding: Sides,
    /// Where content goes, in page coordinates.
    pub area: Rect,
    /// The root's fragment, positioned from the top-left corner of the page area; `None` on
    /// blank pages.
    pub root: Option<BoxFragment>,
    /// The margin boxes, positioned in page coordinates.
    pub margin_boxes: Vec<MarginBoxFragment>,
}

/// Lays out `root` page by page.
pub(crate) fn pages(root: &BlockBox, styles: &Styles, defaults: &PageOptions) -> Vec<LaidPage> {
    // Pages progress in the root's inline direction, or its block flow direction in
    // vertical writing modes: the first page is a right page when they progress left to
    // right (CSS Paged Media 3 § 4.2).
    let right_to_left = match root.style.writing_mode {
        WritingMode::HorizontalTb => root.style.direction == Direction::Rtl,
        WritingMode::VerticalRl | WritingMode::SidewaysRl => true,
        WritingMode::VerticalLr | WritingMode::SidewaysLr => false,
    };
    let first_side = if right_to_left {
        PageSide::Left
    } else {
        PageSide::Right
    };
    let side_of = |index: usize| match (index.is_multiple_of(2), first_side) {
        (true, side) => side,
        (false, PageSide::Right) => PageSide::Left,
        (false, PageSide::Left) => PageSide::Right,
    };
    let mut pages: Vec<LaidPage> = Vec::new();
    let mut token: Option<BreakToken> = None;
    loop {
        let name = flow::resume_page(root, token.as_ref());
        if let Some(side) = token.as_ref().and_then(|t| t.side)
            && side_of(pages.len()) != side
        {
            // A forced break to a left or right page leaves a blank page in between.
            let key = PageKey {
                name: name.clone(),
                first: pages.is_empty(),
                side: side_of(pages.len()),
                blank: true,
            };
            pages.push(setup(styles, defaults, key));
            budget::check_pages(pages.len());
        }
        let key = PageKey {
            name,
            first: pages.is_empty(),
            side: side_of(pages.len()),
            blank: false,
        };
        let mut page = setup(styles, defaults, key);
        // Vertical roots fill the page in their own axes.
        let area = crate::writing::page_area(root, Size::new(page.area.width, page.area.height));
        let fragmentainer = Fragmentainer {
            available: area.height,
            at_top: true,
            columns: false,
        };
        let (mut fragment, next) = flow::layout_root(
            root,
            area.width,
            area.height,
            Some(fragmentainer),
            token.as_ref(),
        );
        crate::writing::root_to_page(&mut fragment, area);
        page.root = Some(fragment);
        pages.push(page);
        budget::check_pages(pages.len());
        budget::check_deadline();
        match next {
            // A page that consumed nothing would repeat forever.
            Some(next) if pages.len() < MAX_PAGES && token.as_ref() != Some(&next) => {
                token = Some(next);
            }
            _ => break,
        }
    }
    repeat_fixed(&mut pages);
    pages
}

/// Fixed boxes are drawn on every page (CSS 2.1 § 9.6.1). Each is laid out on the page its
/// placeholder falls on, and copied to the same place in the page area of the other pages.
fn repeat_fixed(pages: &mut [LaidPage]) {
    let is_fixed = |fragment: &BoxFragment| {
        fragment.node.is_some() && fragment.style.position == Position::Fixed
    };
    // Each fixed box once, positioned from the top-left corner of the page area.
    let mut fixed: Vec<BoxFragment> = Vec::new();
    for page in pages.iter() {
        let Some(root) = &page.root else {
            continue;
        };
        for child in &root.children {
            if let Fragment::Box(child) = child
                && is_fixed(child)
                && !fixed.iter().any(|f| f.node == child.node)
            {
                let mut copy = child.clone();
                copy.rect = copy.rect.translated(root.rect.x, root.rect.y);
                fixed.push(copy);
            }
        }
    }
    for page in pages.iter_mut() {
        let Some(root) = &mut page.root else {
            continue;
        };
        for boxed in &fixed {
            let present = root
                .children
                .iter()
                .any(|c| matches!(c, Fragment::Box(c) if c.node == boxed.node && is_fixed(c)));
            if !present {
                let mut copy = boxed.clone();
                copy.rect = copy.rect.translated(-root.rect.x, -root.rect.y);
                root.children.push(Fragment::Box(copy));
            }
        }
    }
}

/// The geometry of a page from its style: size, margins, border and padding.
fn setup(styles: &Styles, defaults: &PageOptions, key: PageKey) -> LaidPage {
    let style = styles.page(&key);
    let size = match style.size {
        PageSize::Auto => defaults.size,
        PageSize::Orientation { landscape } => {
            let (short, long) = (
                defaults.size.width.min(defaults.size.height),
                defaults.size.width.max(defaults.size.height),
            );
            if landscape {
                Size::new(long, short)
            } else {
                Size::new(short, long)
            }
        }
        PageSize::Size(width, height) => Size::new(width, height),
    };
    // Percentages refer to the page's width horizontally and its height vertically.
    let s = &style.style;
    let side = |set: bool, value: &MaybeAuto<LengthPercentage>, basis: f32, default: f32| match (
        set, value,
    ) {
        (false, _) => default,
        (true, MaybeAuto::Value(value)) => value.resolve(basis),
        (true, MaybeAuto::Auto) => 0.0,
    };
    let margin = Sides {
        top: side(
            style.margins_set[0],
            &s.margin_top,
            size.height,
            defaults.margin.top,
        ),
        right: side(
            style.margins_set[1],
            &s.margin_right,
            size.width,
            defaults.margin.right,
        ),
        bottom: side(
            style.margins_set[2],
            &s.margin_bottom,
            size.height,
            defaults.margin.bottom,
        ),
        left: side(
            style.margins_set[3],
            &s.margin_left,
            size.width,
            defaults.margin.left,
        ),
    };
    let padding = Sides {
        top: s.padding_top.resolve(size.height),
        right: s.padding_right.resolve(size.width),
        bottom: s.padding_bottom.resolve(size.height),
        left: s.padding_left.resolve(size.width),
    };
    let border = Sides {
        top: s.border_top_width,
        right: s.border_right_width,
        bottom: s.border_bottom_width,
        left: s.border_left_width,
    };
    let page_box = Rect::new(
        margin.left,
        margin.top,
        (size.width - margin.horizontal()).max(0.0),
        (size.height - margin.vertical()).max(0.0),
    );
    let area = page_box.deflate(
        border.top + padding.top,
        border.right + padding.right,
        border.bottom + padding.bottom,
        border.left + padding.left,
    );
    // Content always gets some room, however large the margins.
    let area = Rect::new(area.x, area.y, area.width.max(1.0), area.height.max(1.0));
    LaidPage {
        key,
        style,
        size,
        page_box,
        border,
        padding,
        area,
        root: None,
        margin_boxes: Vec::new(),
    }
}
