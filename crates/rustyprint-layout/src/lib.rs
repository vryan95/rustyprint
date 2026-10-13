//! Box tree, layout, text shaping, fragmentation into pages, and display lists.
//!
//! [`layout`] runs the pipeline: box tree ([`boxes`]), block and inline layout into fragments
//! ([`fragment`]), painting into display items, and pagination into [`display::Document`].

mod background;
mod border_image;
pub mod boxes;
mod color_glyphs;
pub mod display;
mod effects;
mod filter;
mod flex;
mod floats;
mod flow;
mod font;
mod fonts;
pub mod fragment;
mod gradient;
mod grid;
pub mod images;
mod inline;
mod links;
pub mod margins;
mod mask;
mod multicol;
pub mod paginate;
mod paint;
pub mod picture;
mod position;
mod raster;
mod rounded;
mod ruby;
mod shadow;
mod shapes;
pub mod table;
mod text;
mod transform;
mod units;
mod writing;

use rustyprint_css::page::PageOrientation;
use rustyprint_css::{ContainerSizes, StyleOptions, Styles, compute_styles_in};
use rustyprint_dom::{Document, local_name};

pub use display::{
    Bookmark, Color, Destination, DisplayItem, FillRule, Link, LinkTarget, Page, Path, PathSegment,
    Point, Rect, Size, TextRun, TextStroke, Upright,
};
pub use font::{Font, FontError, VerticalMetrics};
pub use fonts::{FontChoice, font_metrics, select, select_font};
pub use fragment::{Sides, Turn};
pub use images::{Image, ImageData, Images};
pub use margins::MarginBoxFragment;
pub use paginate::LaidPage;
pub use text::{Glyph, shape};

/// The page box: its size and margins, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageOptions {
    pub size: Size,
    pub margin: Sides,
}

impl PageOptions {
    /// The page area, where content goes.
    pub fn area(&self) -> Size {
        Size::new(
            (self.size.width - self.margin.horizontal()).max(1.0),
            (self.size.height - self.margin.vertical()).max(1.0),
        )
    }
}

/// Lays out a styled document as one continuous page area, without paginating. Fragment
/// positions start at the top-left corner of the page area.
pub fn layout_tree(
    document: &Document,
    styles: &Styles,
    page: &PageOptions,
    images: &Images,
) -> Option<fragment::BoxFragment> {
    let area = page.area();
    let built = boxes::build_with_targets(document, styles, images, &boxes::Targets::default())?;
    // Page numbers of link targets come from the paginated layout.
    let root = if built.uses_targets {
        paginated(document, styles, page, images).0.root
    } else {
        built.root
    };
    // A vertical root turns about the page area, so what fits on the first page is where
    // it would be there.
    let logical = writing::page_area(&root, area);
    let mut fragment = flow::layout_root(&root, logical.width, logical.height, None, None).0;
    writing::root_to_page(&mut fragment, logical);
    Some(fragment)
}

/// How many times styles are computed again for container queries, at most.
const CONTAINER_PASSES: usize = 8;

/// Computes the styles of a document. Container queries and container units depend on the
/// sizes of their containers, so a document with size containers is laid out, and styled
/// again with their sizes, until the sizes stop changing.
pub fn style(
    document: &Document,
    options: &StyleOptions,
    page: &PageOptions,
    images: &Images,
) -> Styles {
    let mut sizes = ContainerSizes::new();
    let mut styles = compute_styles_in(document, options, &sizes);
    for _ in 0..CONTAINER_PASSES {
        if !styles.size_containers {
            break;
        }
        let Some(tree) = layout_tree(document, &styles, page, images) else {
            break;
        };
        let mut laid = ContainerSizes::new();
        for (node, _, fragment) in tree.element_boxes() {
            if let fragment::ElementBox::Block(block) = fragment
                && block.style.container_type.size.is_some()
            {
                let content = block.content_box();
                laid.entry(node)
                    .or_insert((content.width.max(0.0), content.height.max(0.0)));
            }
        }
        if laid == sizes {
            break;
        }
        sizes = laid;
        styles = compute_styles_in(document, options, &sizes);
    }
    styles
}

/// How many times layout runs to let `target-counter()` page numbers settle.
const TARGET_PASSES: usize = 3;

/// Builds the box tree and paginates it. Content that uses `target-*()` is built again with
/// the pages its targets landed on, until they stop changing.
fn paginated(
    document: &Document,
    styles: &Styles,
    page: &PageOptions,
    images: &Images,
) -> (boxes::Built, Vec<LaidPage>) {
    let mut targets = boxes::Targets::default();
    let mut pass = 0;
    loop {
        pass += 1;
        let built =
            boxes::build_with_targets(document, styles, images, &targets).unwrap_or_else(|| {
                boxes::Built {
                    root: boxes::BlockBox {
                        node: None,
                        style: std::sync::Arc::new(rustyprint_css::ComputedValues::initial()),
                        contents: boxes::BlockContents::Blocks(Vec::new()),
                        page: None,
                        marker: None,
                        first_line: Vec::new(),
                        legend: None,
                        center_contents: false,
                        shape_image: None,
                        replaced: None,
                    },
                    running: Vec::new(),
                    targets: Default::default(),
                    uses_targets: false,
                }
            });
        let pages = paginate::pages(&built.root, styles, page);
        if !built.uses_targets || pass == TARGET_PASSES {
            return (built, pages);
        }
        let first = links::first_pages(&pages);
        let mut found = boxes::Targets {
            elements: built.targets.clone(),
            pages: pages.len(),
        };
        for (node, target) in &mut found.elements {
            target.page = first.get(node).copied();
        }
        if found == targets {
            return (built, pages);
        }
        targets = found;
    }
}

/// Lays out a styled document page by page, on pages that `@page` rules style. `page` is
/// the size and margins of pages that the rules leave alone.
pub fn layout_pages(
    document: &Document,
    styles: &Styles,
    page: &PageOptions,
    images: &Images,
) -> Vec<LaidPage> {
    let (built, mut pages) = paginated(document, styles, page, images);
    let running = built.running;
    let order = document
        .descendants(document.root())
        .enumerate()
        .map(|(index, node)| (node, index))
        .collect();
    margins::all(&mut pages, &running, &order, &styles.counter_styles, images);
    pages
}

/// The link areas of each laid-out page, and the document's bookmarks from its headings.
pub fn links_and_bookmarks(
    document: &Document,
    pages: &[LaidPage],
) -> (Vec<Vec<Link>>, Vec<Bookmark>) {
    links::collect(document, pages)
}

/// The display items a box and its descendants paint, relative to the box's parent. Fixed
/// backgrounds are positioned in the box's border box.
pub fn paint_fragment(fragment: &fragment::BoxFragment, images: &Images) -> Vec<DisplayItem> {
    effects::resolve_backdrops(paint::paint(fragment, images, fragment.rect).items)
}

/// The document's title for PDF metadata: the text of its first `<title>`, with whitespace
/// collapsed, unless that is empty.
pub fn document_title(document: &Document) -> Option<String> {
    document
        .find_html(&local_name!("title"))
        .map(|title| {
            document
                .text_content(title)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|title| !title.is_empty())
}

/// Lays out a styled document into pages, loading the images it uses from `images`.
pub fn layout(
    document: &Document,
    styles: &Styles,
    page: &PageOptions,
    images: &Images,
) -> display::Document {
    let title = document_title(document);
    let laid_pages = layout_pages(document, styles, page, images);
    let (mut links, bookmarks) = links_and_bookmarks(document, &laid_pages);
    picture::register_elements(document, &laid_pages, images);
    let pages = laid_pages
        .iter()
        .zip(links.iter_mut())
        .map(|(laid, links)| {
            let style = &laid.style;
            let bleed = style.used_bleed();
            let mut items = paint::paint_page(laid, images, bleed);
            if let Some(root) = &laid.root {
                let area = laid.area;
                let viewport = Rect::new(0.0, 0.0, area.width, area.height);
                items.extend(
                    paint::paint(root, images, viewport)
                        .items
                        .iter()
                        .map(|item| item.translated(area.x, area.y)),
                );
            }
            let sheet = Rect::new(0.0, 0.0, laid.size.width, laid.size.height);
            for margin_box in &laid.margin_boxes {
                items.extend(paint::paint(&margin_box.fragment, images, sheet).items);
            }
            Page {
                size: laid.size,
                items: effects::resolve_backdrops(items),
                bleed,
                marks: display::PrintMarks {
                    crop: style.marks.crop,
                    cross: style.marks.cross,
                },
                rotate: match style.orientation {
                    PageOrientation::Upright => 0,
                    PageOrientation::RotateLeft => 270,
                    PageOrientation::RotateRight => 90,
                },
                links: std::mem::take(links),
            }
        })
        .collect();
    let color_profiles = styles
        .color_profiles
        .iter()
        .map(|profile| display::ColorProfile {
            data: profile.data.clone(),
            components: profile.components,
        })
        .collect();
    let device_cmyk = styles
        .color_profiles
        .iter()
        .position(|profile| &*profile.name == "device-cmyk" && profile.components == 4)
        .map(|index| index as u32);
    display::Document {
        title,
        pages,
        bookmarks,
        color_profiles,
        device_cmyk,
    }
}
