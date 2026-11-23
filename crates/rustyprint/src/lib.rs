//! Render HTML and CSS to PDF.
//!
//! ```no_run
//! let output = rustyprint::render("<p>Hello</p>", &rustyprint::RenderOptions::default())?;
//! std::fs::write("hello.pdf", output.pdf)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod assets;
#[cfg(feature = "serde")]
mod json;
mod options;
mod remote;

use std::any::Any;
use std::fmt;
use std::sync::Once;
use std::thread;
use std::time::{Duration, Instant};

use rustyprint_css::StyleOptions;
use rustyprint_dom::Document;
use rustyprint_dom::budget::{self, Budget, Exceeded};
use rustyprint_layout::{Images, PageOptions};

pub use assets::Assets;
pub use options::{page_size, parse_length, parse_margin};
pub use remote::Remote;
pub use rustyprint_layout::{Sides, Size};

/// Options for a single render.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    /// Stylesheet applied after the document's own styles.
    pub stylesheet: Option<String>,
    /// PDF document title. Overrides the document's `<title>`.
    pub title: Option<String>,
    /// Page size in CSS pixels.
    pub page_size: Size,
    /// Page margins in CSS pixels.
    pub margin: Sides,
    /// The scale of the document on the page, from [`MIN_SCALE`] to [`MAX_SCALE`], as the
    /// scale of a browser's print: at 0.5 everything is half the size and the document is
    /// laid out on a page area twice as wide, which is also the width its media queries
    /// see. Page sizes, page margins and margin boxes are not scaled.
    pub scale: f32,
    /// Shrinks a document whose content is laid out wider than the page area, as a
    /// browser's print does, instead of letting it run off the right edge: the document is
    /// laid out on a page area as wide as the content, up to [`MAX_SHRINK_TO_FIT`] times as
    /// wide, and scaled down to the page. Content that is wider still is cut off at the
    /// right of the page area. Never enlarges.
    pub shrink_to_fit: bool,
    /// Files the document can refer to, such as linked stylesheets and images.
    pub assets: Assets,
    /// Bounds on the work a render may do.
    pub limits: Limits,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            stylesheet: None,
            title: None,
            page_size: Size::A4,
            margin: Sides::all(2.0 * options::CM),
            scale: 1.0,
            shrink_to_fit: false,
            assets: Assets::default(),
            limits: Limits::default(),
        }
    }
}

#[cfg(feature = "serde")]
impl RenderOptions {
    /// Options in the JSON format of `schema/render-options.schema.json`. Fields left out
    /// keep their defaults; unknown fields are an error. JSON cannot give assets: those come
    /// from the caller.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        json::from_json(json)
    }
}

/// Bounds on the work a render may do. Exceeding one fails the render with
/// [`Error::LimitExceeded`] or [`Error::Timeout`], except that images over `max_image_pixels`
/// are skipped with a warning.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    /// Size of the HTML in bytes.
    pub max_input_bytes: usize,
    /// Number of DOM nodes, including text and comments, across the document and its `iframe`
    /// documents.
    pub max_nodes: usize,
    /// How deeply elements nest in a document: 1 for the root element alone.
    pub max_depth: usize,
    pub max_pages: usize,
    /// Width times height of an image.
    pub max_image_pixels: u64,
    /// How long a render may take; `None` for no deadline.
    pub timeout: Option<Duration>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 20 * 1024 * 1024,
            max_nodes: 500_000,
            max_depth: 512,
            max_pages: 2_000,
            max_image_pixels: 100_000_000,
            timeout: Some(Duration::from_secs(60)),
        }
    }
}

/// A limit in [`Limits`] that fails the render when exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Limit {
    InputSize,
    Nodes,
    Depth,
    Pages,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InputSize => "input size",
            Self::Nodes => "node count",
            Self::Depth => "nesting depth",
            Self::Pages => "page count",
        })
    }
}

/// Loads the bundled fonts and the UA stylesheet, which the first render otherwise does.
pub fn warm_up() {
    static WARM: Once = Once::new();
    WARM.call_once(|| {
        let _ = render("<p>Warm</p>", &RenderOptions::default());
    });
}

/// A rendered document.
#[derive(Debug, Clone)]
pub struct Output {
    pub pdf: Vec<u8>,
    /// Problems found in the document's CSS, as `source:line: message`, and problems loading
    /// images.
    pub warnings: Vec<String>,
}

/// Errors returned by [`render`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The options are not valid, such as an unknown unit or page size.
    #[error("invalid options: {0}")]
    InvalidOptions(String),
    #[error("{limit} exceeds the limit of {max}")]
    LimitExceeded { limit: Limit, max: u64 },
    /// The render took longer than [`Limits::timeout`].
    #[error("rendering took longer than the deadline")]
    Timeout,
    /// A bug in rustyprint, such as a caught panic.
    #[error("internal error: {0}")]
    Internal(String),
}

/// The smallest [`RenderOptions::scale`], as in Chrome's print.
pub const MIN_SCALE: f32 = 0.1;

/// The largest [`RenderOptions::scale`], as in Chrome's print.
pub const MAX_SCALE: f32 = 2.0;

/// How many times wider than the page area [`RenderOptions::shrink_to_fit`] lays a document
/// out at the most, as in Chrome's print (Blink's `printingMaximumShrinkFactor`), so that a
/// document far wider than the page stays readable.
pub const MAX_SHRINK_TO_FIT: f32 = 1.5;

/// The overflow [`RenderOptions::shrink_to_fit`] leaves alone: Blink's layout unit, the
/// smallest overflow its print can see.
const SHRINK_TO_FIT_TOLERANCE: f32 = 1.0 / 64.0;

/// The stack of the thread each render runs on. Layout recurses for every level of nesting,
/// using tens of kilobytes of stack per level, which would overflow the 1 MB stacks of the
/// Windows main thread and of Ruby and .NET threads. The stack is reserved rather than
/// committed: only what a render uses takes memory.
const STACK_SIZE: usize = 256 * 1024 * 1024;

/// Renders an HTML document to PDF.
///
/// The render runs on a thread of its own, so it neither depends on the caller's stack nor
/// leaves anything behind on the caller's thread. A panic inside it is returned as
/// [`Error::Internal`].
pub fn render(html: &str, options: &RenderOptions) -> Result<Output, Error> {
    let limits = &options.limits;
    if html.len() > limits.max_input_bytes {
        return Err(Error::LimitExceeded {
            limit: Limit::InputSize,
            max: limits.max_input_bytes as u64,
        });
    }
    let budget = Budget {
        deadline: limits.timeout.and_then(|t| Instant::now().checked_add(t)),
        max_nodes: limits.max_nodes,
        max_depth: limits.max_depth,
        max_pages: limits.max_pages,
    };
    let joined = thread::scope(|scope| {
        thread::Builder::new()
            .name("rustyprint".into())
            .stack_size(STACK_SIZE)
            .spawn_scoped(scope, || {
                budget::run(budget, || render_on_this_thread(html, options))
            })
            .map(|thread| thread.join())
    });
    let exceeded = |limit, max: usize| Error::LimitExceeded {
        limit,
        max: max as u64,
    };
    match joined {
        Err(error) => Err(Error::Internal(format!(
            "could not start a render thread: {error}"
        ))),
        Ok(Err(panic)) => Err(Error::Internal(panic_message(&*panic))),
        Ok(Ok(Err(Exceeded::Deadline))) => Err(Error::Timeout),
        Ok(Ok(Err(Exceeded::Nodes))) => Err(exceeded(Limit::Nodes, limits.max_nodes)),
        Ok(Ok(Err(Exceeded::Depth))) => Err(exceeded(Limit::Depth, limits.max_depth)),
        Ok(Ok(Err(Exceeded::Pages))) => Err(exceeded(Limit::Pages, limits.max_pages)),
        Ok(Ok(Ok(result))) => result,
    }
}

fn panic_message(panic: &(dyn Any + Send)) -> String {
    let message = panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown cause");
    format!("panicked: {message}")
}

/// Renders within the budget [`budget::run`] has set for this thread.
fn render_on_this_thread(html: &str, options: &RenderOptions) -> Result<Output, Error> {
    #[cfg(feature = "test-support")]
    if html.contains("<!--rustyprint:panic-->") {
        panic!("rustyprint:panic comment in the document");
    }
    let (laid_out, warnings) = lay_out(html, options)?;
    let pdf = rustyprint_pdf::write(&laid_out).map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Output { pdf, warnings })
}

/// Lays a document out into what its pages paint, with the warnings of doing so.
fn lay_out(
    html: &str,
    options: &RenderOptions,
) -> Result<(rustyprint_layout::display::Document, Vec<String>), Error> {
    let document = Document::parse(html);
    budget::check_document(&document);
    let page = PageOptions {
        size: options.page_size,
        margin: options.margin,
    };
    let area = page.area();
    let loader = assets::Loader::new(&options.assets);
    // Relative URLs resolve against the document's `<base>`.
    let base = document.base_url().map(str::to_owned);
    let get = |url: &str| match &base {
        Some(base) => loader.get(&rustyprint_dom::resolve_url(base, url)),
        None => loader.get(url),
    };
    let load = |url: &str| get(url).and_then(|bytes| String::from_utf8(bytes).ok());
    let load_font = |url: &str| get(url);
    let load_file = |url: &str| get(url);
    let images = Images::new(&load_file).with_max_pixels(options.limits.max_image_pixels);
    let scale = options.scale;
    if !(MIN_SCALE..=MAX_SCALE).contains(&scale) {
        return Err(Error::InvalidOptions(format!(
            "scale: {scale} is not from {MIN_SCALE} to {MAX_SCALE}"
        )));
    }
    // A zoom on the root scales every length of the document, so lines and tables reflow
    // to the page area as the document measures it, rather than being scaled as an image.
    let lay_out_at = |zoom: f32| {
        let styles = rustyprint_layout::style(
            &document,
            &StyleOptions {
                page_size: (area.width, area.height),
                media_size: (area.width / scale, area.height / scale),
                zoom,
                stylesheet: options.stylesheet.as_deref(),
                load: &load,
                font_metrics: Some(&rustyprint_layout::font_metrics),
                load_font: Some(&load_font),
            },
            &page,
            &images,
        );
        let pages = rustyprint_layout::layout_pages(&document, &styles, &page, &images);
        (styles, pages)
    };
    let (mut styles, mut pages) = lay_out_at(scale);
    let overflows = |pages: &[rustyprint_layout::LaidPage]| {
        let overflow = rustyprint_layout::overflow(pages);
        ((overflow - 1.0) * area.width > SHRINK_TO_FIT_TOLERANCE).then_some(overflow)
    };
    let mut clip = false;
    if options.shrink_to_fit
        && let Some(overflow) = overflows(&pages)
    {
        (styles, pages) = lay_out_at(scale / overflow.min(MAX_SHRINK_TO_FIT));
        // What is still too wide once laid out again, whether because it was wider than
        // the most the document shrinks or because it grows with the page area, is cut off.
        clip = overflows(&pages).is_some();
    }
    let mut laid_out = rustyprint_layout::paint_pages(&document, &styles, &pages, &images, clip);
    let mut warnings: Vec<String> = styles
        .warnings
        .iter()
        .map(|w| format!("{}:{}: {}", w.source, w.warning.line, w.warning.message))
        .collect();
    warnings.extend(loader.warnings());
    warnings.extend(images.warnings());
    if options.title.is_some() {
        laid_out.title = options.title.clone();
    }
    Ok((laid_out, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(pdf: &[u8]) -> lopdf::Document {
        lopdf::Document::load_mem(pdf).expect("valid PDF")
    }

    #[test]
    fn renders_text() {
        let output = render(
            "<h1>Title</h1><p>Hello <b>bold</b> world</p>",
            &RenderOptions::default(),
        )
        .unwrap();
        let doc = parse(&output.pdf);
        assert_eq!(doc.get_pages().len(), 1);
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("Title"), "{text:?}");
        assert!(text.contains("bold"), "{text:?}");
        assert!(output.warnings.is_empty());
    }

    /// A piece of text a page paints: its text, left and right edges, baseline and font
    /// size, in CSS pixels on its page.
    #[derive(Debug, Clone, PartialEq)]
    struct Painted {
        text: String,
        left: f32,
        right: f32,
        baseline: f32,
        size: f32,
    }

    /// The text every page of a document paints, in painting order.
    fn painted(html: &str, options: &RenderOptions) -> Vec<Painted> {
        use rustyprint_layout::display::DisplayItem;
        let (document, _) = lay_out(html, options).unwrap();
        let runs = document.pages.iter().flat_map(|page| &page.items);
        runs.filter_map(|item| match item {
            DisplayItem::Text(run) => Some(Painted {
                text: run
                    .glyphs
                    .iter()
                    .map(|g| &run.text[g.text.clone()])
                    .collect(),
                left: run.origin.x,
                right: run.origin.x + run.width(),
                baseline: run.origin.y,
                size: run.size,
            }),
            _ => None,
        })
        .collect()
    }

    /// The painted text that reads `text`.
    fn find(painted: &[Painted], text: &str) -> Painted {
        let found = painted.iter().find(|p| p.text.trim() == text);
        found
            .unwrap_or_else(|| panic!("no {text:?} in {painted:?}"))
            .clone()
    }

    /// A4 with 20px margins: a page area 753.7px wide.
    fn narrow_margins() -> RenderOptions {
        RenderOptions {
            margin: Sides::all(20.0),
            ..RenderOptions::default()
        }
    }

    const AREA: f32 = 793.700_8 - 40.0;

    const PLAIN: &str = "<style>body { margin: 0; font: 16px sans-serif }</style>";

    /// A 700px box with `END` at its right: where `END` ends tells the scale of the page.
    const RULER: &str = "<div style='width: 700px; text-align: right'>END</div>";

    /// The scale a document with [`RULER`] is painted at.
    fn scale_of(html: &str, options: &RenderOptions) -> f32 {
        (find(&painted(html, options), "END").right - 20.0) / 700.0
    }

    fn shrunk() -> RenderOptions {
        RenderOptions {
            shrink_to_fit: true,
            ..narrow_margins()
        }
    }

    #[test]
    fn shrink_to_fit_scales_content_wider_than_the_page_down_to_fit_it() {
        // 1.4 times the printable width of A4 with 2cm margins, and taller than one page:
        // it takes two pages as it is, and fits on one when laid out on a page area 1.4
        // times as wide and as tall.
        let printable = Size::A4.width - 4.0 * options::CM;
        let html = format!(
            "<style>body {{ margin: 0 }}</style><div style='width: {}px; height: 1300px; background: #eee'></div>",
            printable * 1.4
        );
        let pages = |shrink_to_fit| {
            let options = RenderOptions {
                shrink_to_fit,
                ..RenderOptions::default()
            };
            parse(&render(&html, &options).unwrap().pdf)
                .get_pages()
                .len()
        };
        assert_eq!(pages(false), 2);
        assert_eq!(pages(true), 1);
    }

    #[test]
    fn shrink_to_fit_lays_the_document_out_as_wide_as_its_widest_content() {
        // The 1000px box makes the page area 1000px wide to the document, so everything is
        // 753.7/1000 of its size and the box ends at the right of the page area.
        let html =
            format!("{PLAIN}<div style='width: 1000px; text-align: right'>WIDE</div>{RULER}");
        assert_eq!(scale_of(&html, &narrow_margins()), 1.0);
        let scale = scale_of(&html, &shrunk());
        assert!((scale - AREA / 1000.0).abs() < 0.001, "{scale}");
        let wide = find(&painted(&html, &shrunk()), "WIDE");
        assert!((wide.right - (20.0 + AREA)).abs() < 0.01, "{wide:?}");
        assert!((wide.size - 16.0 * AREA / 1000.0).abs() < 0.01, "{wide:?}");
    }

    #[test]
    fn shrink_to_fit_reflows_text_to_the_wider_page_area() {
        // A paragraph takes fewer lines on the page area the 1000px box makes.
        let html = format!(
            "{PLAIN}<div style='width: 1000px; height: 10px'></div><p>{}</p>",
            "word ".repeat(60)
        );
        let lines = |options: &RenderOptions| painted(&html, options).len();
        assert!(lines(&shrunk()) < lines(&narrow_margins()));
    }

    #[test]
    fn shrink_to_fit_shrinks_to_two_thirds_at_the_most() {
        // Text that cannot wrap, a table of cells that cannot, a box, an image and an
        // absolutely positioned box, all more than 1.5 times as wide as the page area.
        let cells = "<td>unbreakable</td>".repeat(40);
        let pixel =
            "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
        for wide in [
            format!("<p style='white-space: nowrap'>{}</p>", "word ".repeat(80)),
            format!("<table><tr>{cells}</tr></table>"),
            "<div style='width: 3000px; height: 10px'></div>".to_owned(),
            format!("<img src='{pixel}' style='width: 3000px; height: 10px'>"),
            "<div style='position: absolute; left: 2000px; width: 100px; height: 10px'></div>"
                .to_owned(),
        ] {
            let scale = scale_of(&format!("{PLAIN}{wide}{RULER}"), &shrunk());
            assert!((scale - 1.0 / 1.5).abs() < 0.001, "{scale} for {wide}");
        }
    }

    #[test]
    fn shrink_to_fit_shrinks_every_page_by_the_widest() {
        let html = format!(
            "{PLAIN}{RULER}<div style='break-before: page; width: 1000px; height: 10px'></div>"
        );
        let scale = scale_of(&html, &shrunk());
        assert!((scale - AREA / 1000.0).abs() < 0.001, "{scale}");
    }

    #[test]
    fn shrink_to_fit_cuts_off_what_is_wider_than_one_and_a_half_pages() {
        // 300 words on one line: the page shows the ones that start in 1.5 times its width,
        // and the rest are not in the document at all.
        let words: String = (0..300).map(|i| format!("w{i:03} ")).collect();
        let html = format!("{PLAIN}<p style='white-space: nowrap; margin: 0'>{words}</p>");
        let line = find_line(&painted(&html, &shrunk()));
        assert!(line.text.starts_with("w000 "), "{line:?}");
        assert!(!line.text.contains("w299"), "{line:?}");
        // The last glyph starts before the right of the page area, and the next would not.
        let edge = 20.0 + AREA;
        assert!(line.right > edge && line.right < edge + 16.0, "{line:?}");
        // Without it every word is painted, most of them off the page.
        let line = find_line(&painted(&html, &narrow_margins()));
        assert!(line.text.contains("w299"), "{line:?}");
    }

    /// The one line a document paints.
    fn find_line(painted: &[Painted]) -> Painted {
        assert_eq!(painted.len(), 1, "{painted:?}");
        painted[0].clone()
    }

    #[test]
    fn shrink_to_fit_cuts_off_boxes_past_the_page_area() {
        use rustyprint_layout::display::DisplayItem;
        let html = format!(
            "{PLAIN}<div style='width: 3000px; height: 10px; background: red'></div>\
             <div style='margin-left: 2000px; width: 50px; height: 10px; background: blue'></div>"
        );
        let (document, _) = lay_out(&html, &shrunk()).unwrap();
        let rects: Vec<_> = document.pages[0]
            .items
            .iter()
            .filter(|item| matches!(item, DisplayItem::Rect { .. }))
            .collect();
        // The red box is clipped; the blue one starts past the edge and is left out.
        assert_eq!(rects.len(), 1, "{rects:?}");
        let clipped = document.pages[0].items.iter().any(|item| {
            matches!(item, DisplayItem::PushClip(clip) if (clip.right() - (20.0 + AREA)).abs() < 0.01)
        });
        assert!(clipped, "{:?}", document.pages[0].items);
    }

    #[test]
    fn shrink_to_fit_does_not_count_what_a_box_clips() {
        for clipping in ["overflow: hidden", "overflow-x: auto", "contain: paint"] {
            let html = format!(
                "{PLAIN}<div style='{clipping}; width: 700px'><div style='width: 1500px'>clipped</div></div>{RULER}"
            );
            assert_eq!(scale_of(&html, &shrunk()), 1.0, "{clipping}");
        }
    }

    #[test]
    fn shrink_to_fit_does_not_count_spaces_at_the_end_of_a_line() {
        // The line is full to the last word: the space after it hangs past the edge.
        let html = format!("{PLAIN}<p>{}</p>{RULER}", "word ".repeat(200));
        assert_eq!(scale_of(&html, &shrunk()), 1.0);
    }

    #[test]
    fn scale_lays_the_document_out_on_a_wider_page_area() {
        // At 0.5 the 753.7px page area is 1507.4px to the document: a 1400px box fits,
        // media queries see 1507.4px, and everything is half the size.
        let html = format!(
            "{PLAIN}<style>.query {{ display: none }}\
             @media (min-width: 1507px) {{ .met {{ display: block }} }}\
             @media (min-width: 1508px) {{ .unmet {{ display: block }} }}</style>\
             <div style='width: 1400px; text-align: right'>WIDE</div>{RULER}\
             <div class='query met'>MET</div><div class='query unmet'>UNMET</div>"
        );
        let options = RenderOptions {
            scale: 0.5,
            ..narrow_margins()
        };
        let painted = painted(&html, &options);
        assert!(painted.iter().any(|p| p.text == "MET"), "{painted:?}");
        assert!(!painted.iter().any(|p| p.text == "UNMET"), "{painted:?}");
        let wide = find(&painted, "WIDE");
        assert!((wide.right - (20.0 + 700.0)).abs() < 0.01, "{wide:?}");
        assert!((wide.size - 8.0).abs() < 0.01, "{wide:?}");
        assert!((scale_of(&html, &options) - 0.5).abs() < 0.001);
    }

    #[test]
    fn scale_leaves_the_page_and_its_margin_boxes_alone() {
        let html = format!(
            "{PLAIN}<style>@page {{ margin: 100px; @bottom-left {{ content: 'FOOT'; font: 20px sans-serif }} }}</style>\
             <div style='height: 100px'></div><div>BODY</div>"
        );
        let at = |scale: f32| {
            let options = RenderOptions {
                scale,
                ..RenderOptions::default()
            };
            let painted = painted(&html, &options);
            (find(&painted, "FOOT"), find(&painted, "BODY"))
        };
        let ((foot, body), (half_foot, half_body)) = (at(1.0), at(0.5));
        assert_eq!(foot, half_foot);
        // The body starts at the 100px margin either way; its second box is 100px or 50px
        // further down, in text half the size.
        assert_eq!(body.left, 100.0);
        assert_eq!(half_body.left, 100.0);
        assert!(
            (body.baseline - half_body.baseline - 50.0 - 0.5 * (body.baseline - 200.0)).abs()
                < 0.01
        );
        assert_eq!(half_body.size, 8.0);
    }

    #[test]
    fn scale_scales_borders_after_they_snap_to_whole_pixels() {
        // A 1px border is 0.9px at 0.9, not the 1px that 0.9px would snap to.
        let html = "<div style='border-top: 1px solid; margin-top: 50px'></div>";
        let options = RenderOptions {
            scale: 0.9,
            ..RenderOptions::default()
        };
        let (document, _) = lay_out(html, &options).unwrap();
        let heights: Vec<f32> = document.pages[0]
            .items
            .iter()
            .filter_map(|item| item.bounds())
            .map(|bounds| bounds.height)
            .collect();
        assert_eq!(heights.len(), 1, "{heights:?}");
        assert!((heights[0] - 0.9).abs() < 0.001, "{heights:?}");
    }

    #[test]
    fn scale_must_be_one_a_print_can_have() {
        for scale in [0.0, 0.05, 2.5, f32::NAN] {
            let options = RenderOptions {
                scale,
                ..RenderOptions::default()
            };
            assert!(
                matches!(render("<p>x</p>", &options), Err(Error::InvalidOptions(_))),
                "{scale}"
            );
        }
    }

    #[test]
    fn shrink_to_fit_shrinks_a_scaled_document_from_its_scale() {
        // At 0.9 the page area is 837px to the document: a 600px box fits, a 1000px box
        // makes it 1000px, and a 2000px box 1.5 times 837px.
        let options = RenderOptions {
            scale: 0.9,
            ..shrunk()
        };
        for (width, expected) in [(600, 0.9), (1000, AREA / 1000.0), (2000, 0.9 / 1.5)] {
            let html = format!("{PLAIN}<div style='width: {width}px; height: 10px'></div>{RULER}");
            let scale = scale_of(&html, &options);
            assert!((scale - expected).abs() < 0.001, "{width}: {scale}");
        }
    }

    #[test]
    fn shrink_to_fit_leaves_content_that_fits_alone() {
        let html = "<p>Fits.</p>".repeat(80);
        let pages = |shrink_to_fit| {
            let options = RenderOptions {
                shrink_to_fit,
                ..RenderOptions::default()
            };
            render(&html, &options).unwrap().pdf
        };
        assert_eq!(pages(false), pages(true));
    }

    fn at_scale(scale: f32) -> RenderOptions {
        RenderOptions {
            scale,
            ..narrow_margins()
        }
    }

    /// The heights of the boxes the first page paints.
    fn painted_heights(html: &str, options: &RenderOptions) -> Vec<f32> {
        let (document, _) = lay_out(html, options).unwrap();
        document.pages[0]
            .items
            .iter()
            .filter_map(|item| item.bounds())
            .map(|bounds| bounds.height)
            .collect()
    }

    #[test]
    fn scale_leaves_root_relative_and_inherited_lengths_of_the_page_alone() {
        // `rem` in `@page` is the root's font size as written, and the margin box inherits
        // the root's letter spacing as written.
        let html = "<style>html { font: 20px sans-serif; letter-spacing: 3px }\
             @page { margin: 2rem; @bottom-left { content: 'FOOT' } }\
             body { margin: 0 }</style><div>BODY</div>";
        let at = |scale: f32| {
            let options = RenderOptions {
                scale,
                ..RenderOptions::default()
            };
            let painted = painted(html, &options);
            (find(&painted, "FOOT"), find(&painted, "BODY"))
        };
        let ((foot, body), (half_foot, half_body)) = (at(1.0), at(0.5));
        assert_eq!(foot, half_foot);
        assert_eq!(body.left, 40.0);
        assert_eq!(half_body.left, 40.0);
    }

    #[test]
    fn scale_scales_initial_lengths() {
        // A border without a width is `medium`, 3px; `font-size: initial` is 16px.
        let html = "<div style='border-top-style: solid; margin-top: 50px'></div>";
        assert_eq!(painted_heights(html, &at_scale(0.5)), [1.5]);
        let html =
            "<div style='border-top: solid; border-top-width: initial; margin-top: 50px'></div>";
        assert_eq!(painted_heights(html, &at_scale(0.5)), [1.5]);
        let html = format!("{PLAIN}<p style='font-size: initial'>INITIAL</p>");
        assert_eq!(find(&painted(&html, &at_scale(0.5)), "INITIAL").size, 8.0);
    }

    #[test]
    fn scale_snaps_borders_that_are_whole_pixels_as_written() {
        let html = "<div style='border-top: 3px solid; margin-top: 50px'></div>";
        let heights = painted_heights(html, &at_scale(0.9));
        assert_eq!(heights.len(), 1, "{heights:?}");
        assert!((heights[0] - 2.7).abs() < 0.001, "{heights:?}");
    }

    #[test]
    fn scale_scales_what_a_frame_shows() {
        // The frame is 400px to the document: its media queries see 400px, and its text is
        // half the size too.
        let frame = "<style>body { margin: 0; font: 16px sans-serif } .w { display: none }\
             @media (min-width: 400px) { .w { display: block } }</style>\
             <div>FRAME</div><div class=w>WIDE</div>";
        let html = format!(
            "{PLAIN}<iframe style='width: 400px; height: 200px; border: 0' srcdoc='{frame}'></iframe>"
        );
        let painted = painted(&html, &at_scale(0.5));
        assert_eq!(find(&painted, "FRAME").size, 8.0, "{painted:?}");
        assert_eq!(find(&painted, "WIDE").size, 8.0, "{painted:?}");
    }

    #[test]
    fn scale_scales_the_default_size_of_videos_and_frames() {
        // A video without a poster and a frame without a size are 300px wide.
        for element in [
            "<video></video>",
            "<iframe style='border: 0' srcdoc=''></iframe>",
        ] {
            let html = format!("{PLAIN}{element}<span>AFTER</span>");
            let after = find(&painted(&html, &at_scale(0.5)), "AFTER");
            assert!(
                (after.left - (20.0 + 150.0)).abs() < 0.01,
                "{element}: {after:?}"
            );
        }
    }

    #[test]
    fn shrink_to_fit_counts_spaces_that_do_not_hang() {
        // Preserved spaces and no-break spaces take room at the end of a line.
        for wide in [
            format!("<p style='white-space: pre'>x{}</p>", " ".repeat(300)),
            format!("<p>x{}</p>", "&nbsp;".repeat(300)),
        ] {
            let scale = scale_of(&format!("{PLAIN}{wide}{RULER}"), &shrunk());
            assert!(scale < 1.0, "{scale} for {wide}");
        }
    }

    #[test]
    fn shrink_to_fit_cuts_off_what_is_still_too_wide_when_laid_out_again() {
        // 100px wider than any page area: shrinking does not make it fit.
        let html = format!(
            "{PLAIN}<div style='width: calc(100% + 100px); height: 10px; background: red'></div>"
        );
        let (document, _) = lay_out(&html, &shrunk()).unwrap();
        let clipped = document.pages[0].items.iter().any(|item| {
            matches!(item, rustyprint_layout::display::DisplayItem::PushClip(clip) if (clip.right() - (20.0 + AREA)).abs() < 0.01)
        });
        assert!(clipped, "{:?}", document.pages[0].items);
    }

    #[test]
    fn shrink_to_fit_counts_boxes_where_their_transforms_take_them() {
        let html = format!(
            "{PLAIN}<div style='width: 100px; height: 10px; transform: translateX(900px)'></div>{RULER}"
        );
        let scale = scale_of(&html, &shrunk());
        assert!((scale - AREA / 1000.0).abs() < 0.001, "{scale}");
    }

    #[test]
    fn shrink_to_fit_cuts_off_transformed_text_on_the_page() {
        let words: String = (0..300).map(|i| format!("w{i:03} ")).collect();
        let html = format!(
            "{PLAIN}<p style='white-space: nowrap; margin: 0; transform: translateX(10px)'>{words}</p>"
        );
        let line = find_line(&painted(&html, &shrunk()));
        assert!(line.text.starts_with("w000 "), "{line:?}");
        assert!(!line.text.contains("w299"), "{line:?}");
        // The run is moved 10px by the transform: it ends past the edge by less than a glyph.
        let edge = 20.0 + AREA - 10.0 * AREA / 1500.0;
        assert!(line.right > edge && line.right < edge + 16.0, "{line:?}");
    }

    #[test]
    fn shrink_to_fit_cuts_off_links_at_the_page_area() {
        let words: String = (0..300).map(|i| format!("w{i:03} ")).collect();
        let html = format!(
            "{PLAIN}<p style='white-space: nowrap; margin: 0'><a href='https://a.example/'>{words}</a></p>\
             <div style='margin-left: 2000px; width: 50px'><a href='https://b.example/'>B</a></div>"
        );
        let (document, _) = lay_out(&html, &shrunk()).unwrap();
        let links = &document.pages[0].links;
        assert_eq!(links.len(), 1, "{links:?}");
        assert!(
            (links[0].rect.right() - (20.0 + AREA)).abs() < 0.01,
            "{links:?}"
        );
    }

    #[test]
    fn shrink_to_fit_counts_the_nodes_of_a_frame_once() {
        // Laying the document out again does not parse the frame again.
        let frame = "<p>x</p>".repeat(50);
        let html = format!(
            "{PLAIN}<iframe srcdoc='{frame}'></iframe><div style='width: 1000px; height: 10px'></div>"
        );
        let nodes = Document::parse(&html).node_count() + Document::parse(&frame).node_count();
        let options = RenderOptions {
            limits: Limits {
                max_nodes: nodes,
                ..Limits::default()
            },
            ..shrunk()
        };
        let rendered = render(&html, &options);
        assert!(rendered.is_ok(), "{:?}", rendered.err());
    }

    /// The pages `html` takes on A4 with 2cm margins, with or without `shrink_to_fit`.
    fn pages_shrunk(html: &str, shrink_to_fit: bool) -> usize {
        let options = RenderOptions {
            shrink_to_fit,
            ..RenderOptions::default()
        };
        parse(&render(html, &options).unwrap().pdf)
            .get_pages()
            .len()
    }

    /// The width and height of A4's page area with 2cm margins.
    fn printable() -> (f32, f32) {
        (
            Size::A4.width - 4.0 * options::CM,
            Size::A4.height - 4.0 * options::CM,
        )
    }

    #[test]
    fn shrink_to_fit_multiplies_the_documents_own_zoom() {
        // 2.8 printable widths at the document's zoom of 0.5 paint 1.4: shrunk by that much
        // more, within the most the document shrinks, its 2400px fit on one page. Neither a
        // rule nor an inline style for the root replaces the shrink.
        let (width, _) = printable();
        for zoom in [
            "<style>html { zoom: 0.5 }</style><html>",
            "<html style='zoom: 0.5'>",
        ] {
            let html = format!(
                "{zoom}<style>body {{ margin: 0 }}</style><div style='width: {}px; height: 2400px; background: #eee'></div>",
                width * 2.8
            );
            assert_eq!(pages_shrunk(&html, false), 2, "{zoom}");
            assert_eq!(pages_shrunk(&html, true), 1, "{zoom}");
        }
    }

    #[test]
    fn shrink_to_fit_measures_the_page_area_page_rules_set() {
        // 500px is wider than the 360px area of a 400px page with 20px margins, though not
        // than A4's: shrunk to fit, the 1000px block fits in that area's 760px height.
        let html = "<style>@page { size: 400px 800px; margin: 20px } body { margin: 0 }</style>\
            <div style='width: 500px; height: 1000px; background: #eee'></div>";
        assert_eq!(pages_shrunk(html, false), 2);
        assert_eq!(pages_shrunk(html, true), 1);
    }

    #[test]
    fn shrink_to_fit_ignores_page_backgrounds_and_margin_boxes() {
        // The content fits across the page, so it is not shrunk onto one page though the
        // page's background and margin boxes reach the sheet's edge.
        let (_, height) = printable();
        let html = format!(
            "<style>@page {{ background: white; @top-right {{ content: 'x'; width: 100% }} }} body {{ margin: 0 }}</style>\
            <div style='width: 100px; height: {}px; background: #eee'></div>",
            height + 20.0
        );
        assert_eq!(pages_shrunk(&html, false), 2);
        assert_eq!(pages_shrunk(&html, true), 2);
    }

    #[test]
    fn shrink_to_fit_ignores_what_overflow_clips() {
        let (_, height) = printable();
        let html = format!(
            "<style>body {{ margin: 0 }}</style><div style='width: 100px; overflow: hidden'>\
            <div style='width: 2000px; height: {}px; background: #eee'></div></div>",
            height + 20.0
        );
        assert_eq!(pages_shrunk(&html, false), 2);
        assert_eq!(pages_shrunk(&html, true), 2);
    }

    #[test]
    fn shrink_to_fit_fits_content_a_few_pixels_too_wide() {
        // 5px too wide shrinks by about 0.8%, which takes the 3px too tall block onto one
        // page.
        let (width, height) = printable();
        let html = format!(
            "<style>body {{ margin: 0 }}</style><div style='width: {}px; height: {}px; background: #eee'></div>",
            width + 5.0,
            height + 3.0
        );
        assert_eq!(pages_shrunk(&html, false), 2);
        assert_eq!(pages_shrunk(&html, true), 1);
    }

    #[test]
    fn long_documents_get_more_pages() {
        let html = "<p>Line of text.</p>".repeat(200);
        let output = render(&html, &RenderOptions::default()).unwrap();
        assert!(parse(&output.pdf).get_pages().len() > 3);
    }

    /// The objects of an array or a reference to one.
    fn resolve<'a>(doc: &'a lopdf::Document, object: &'a lopdf::Object) -> &'a lopdf::Object {
        match object {
            lopdf::Object::Reference(id) => doc.get_object(*id).unwrap(),
            other => other,
        }
    }

    #[test]
    fn writes_links() {
        let html = "<p><a href='https://example.com/'>external</a> <a href='#far'>internal</a> \
                    <a href='#missing'>dead</a> <a>no href</a></p>\
                    <p style='break-before: page' id=far>target</p>";
        let output = render(html, &RenderOptions::default()).unwrap();
        let doc = parse(&output.pdf);
        let pages = doc.get_pages();
        let page = doc.get_dictionary(pages[&1]).unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(annots.len(), 2);
        // The first link sits on the first line, inside the page margin (2 cm) and the body's
        // margin (8px). PDF coordinates are points from the bottom of the A4 page.
        let rect: Vec<f32> = resolve(&doc, &annots[0])
            .as_dict()
            .unwrap()
            .get(b"Rect")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_float().unwrap())
            .collect();
        let margin = 2.0 / 2.54 * 72.0;
        let left = margin + 6.0;
        assert!((rect[0] - left).abs() < 0.5, "{rect:?}");
        assert!(rect[2] > rect[0] + 20.0, "{rect:?}");
        assert!(
            rect[3] < 841.9 - margin && rect[3] > 841.9 - margin - 30.0,
            "{rect:?}"
        );
        assert!(rect[1] < rect[3] - 8.0, "{rect:?}");
        let link = |index: usize| {
            resolve(&doc, &annots[index])
                .as_dict()
                .unwrap()
                .get(b"A")
                .unwrap()
                .as_dict()
                .unwrap()
                .clone()
        };
        let uri = link(0);
        assert_eq!(uri.get(b"S").unwrap().as_name().unwrap(), b"URI");
        assert_eq!(
            uri.get(b"URI").unwrap().as_str().unwrap(),
            b"https://example.com/"
        );
        let goto = link(1);
        assert_eq!(goto.get(b"S").unwrap().as_name().unwrap(), b"GoTo");
        let dest = goto.get(b"D").unwrap().as_array().unwrap();
        assert_eq!(dest[0].as_reference().unwrap(), pages[&2]);
        assert!(
            doc.get_dictionary(pages[&2])
                .unwrap()
                .get(b"Annots")
                .is_err()
        );
    }

    #[test]
    fn writes_outline_from_headings() {
        let html = "<h1>One</h1><h2>One.a</h2><h3>Deep</h3><h2>One.b</h2><h1>Two</h1>\
                    <h2 style='display: none'>Hidden</h2>";
        let output = render(html, &RenderOptions::default()).unwrap();
        let doc = parse(&output.pdf);
        let catalog = doc.catalog().unwrap();
        let outline = resolve(&doc, catalog.get(b"Outlines").unwrap())
            .as_dict()
            .unwrap();
        assert_eq!(outline.get(b"Count").unwrap().as_i64().unwrap(), 5);
        let item = |object: &lopdf::Object| resolve(&doc, object).as_dict().unwrap().clone();
        let title = |dict: &lopdf::Dictionary| {
            String::from_utf8(dict.get(b"Title").unwrap().as_str().unwrap().to_vec()).unwrap()
        };
        let one = item(outline.get(b"First").unwrap());
        assert_eq!(title(&one), "One");
        assert_eq!(one.get(b"Count").unwrap().as_i64().unwrap(), 3);
        let a = item(one.get(b"First").unwrap());
        assert_eq!(title(&a), "One.a");
        assert_eq!(title(&item(a.get(b"First").unwrap())), "Deep");
        assert_eq!(title(&item(one.get(b"Last").unwrap())), "One.b");
        let two = item(one.get(b"Next").unwrap());
        assert_eq!(title(&two), "Two");
        assert!(two.get(b"First").is_err());
        assert_eq!(title(&item(outline.get(b"Last").unwrap())), "Two");
    }

    #[test]
    fn reports_css_warnings() {
        let output = render(
            "<style>p { colour: red }</style><p>x</p>",
            &RenderOptions::default(),
        )
        .unwrap();
        assert_eq!(output.warnings.len(), 1);
        assert!(output.warnings[0].contains("colour"));
    }

    /// A 2 × 1 PNG: one opaque red pixel and one half-transparent blue one.
    fn png() -> Vec<u8> {
        // Encoded with a stored (uncompressed) zlib block, so no encoder is needed.
        let raw: [u8; 9] = [0, 255, 0, 0, 255, 0, 0, 255, 128];
        let mut zlib = vec![
            0x78,
            0x01,
            0x01,
            raw.len() as u8,
            0,
            !(raw.len() as u8),
            0xFF,
        ];
        zlib.extend_from_slice(&raw);
        let adler = {
            let (mut a, mut b) = (1u32, 0u32);
            for &byte in &raw {
                a = (a + u32::from(byte)) % 65521;
                b = (b + a) % 65521;
            }
            (b << 16) | a
        };
        zlib.extend_from_slice(&adler.to_be_bytes());
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut out = (data.len() as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            let mut crc = !0u32;
            for &byte in kind.iter().chain(data) {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            out.extend_from_slice(&(!crc).to_be_bytes());
            out
        };
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(chunk(b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0, 0, 0]));
        png.extend(chunk(b"IDAT", &zlib));
        png.extend(chunk(b"IEND", &[]));
        png
    }

    #[test]
    fn embeds_background_images() {
        let mut options = RenderOptions::default();
        options.assets.files.insert("dot.png".into(), png());
        let output = render(
            "<div style='width: 50px; height: 20px; background: url(dot.png)'></div>",
            &options,
        )
        .unwrap();
        assert!(output.warnings.is_empty(), "{:?}", output.warnings);
        let doc = parse(&output.pdf);
        let images: Vec<_> = doc
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .filter(|stream| {
                stream
                    .dict
                    .get(b"Subtype")
                    .and_then(|s| s.as_name())
                    .is_ok_and(|name| name == b"Image")
            })
            .collect();
        // The image and its soft mask, each written once although the image tiles.
        assert_eq!(images.len(), 2);
    }

    #[test]
    fn missing_images_warn() {
        let output = render(
            "<div style='height: 20px; background-image: url(missing.png)'></div>",
            &RenderOptions::default(),
        )
        .unwrap();
        assert_eq!(output.warnings.len(), 1);
        assert!(output.warnings[0].contains("missing.png"));
    }

    #[test]
    fn linked_stylesheets_load_from_assets() {
        let mut options = RenderOptions::default();
        options
            .assets
            .files
            .insert("style.css".into(), b"p { colour: red }".to_vec());
        let output = render("<link rel=stylesheet href=style.css><p>x</p>", &options).unwrap();
        assert_eq!(output.warnings.len(), 1, "{:?}", output.warnings);
        assert!(output.warnings[0].starts_with("style.css"));
    }

    #[test]
    fn page_rules_set_pdf_page_boxes() {
        let output = render(
            "<style>@page { size: 300px 150px; margin: 0; bleed: 4px; marks: crop; \
             page-orientation: rotate-right }</style><p>x</p>",
            &RenderOptions::default(),
        )
        .unwrap();
        let doc = parse(&output.pdf);
        let (_, &id) = doc.get_pages().iter().next().unwrap();
        let page = doc.get_dictionary(id).unwrap();
        let rect = |name: &[u8]| -> Vec<f32> {
            page.get(name)
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_float().unwrap())
                .collect()
        };
        // 300 × 150 px is 225 × 112.5 pt; the bleed is 3pt.
        assert_eq!(rect(b"TrimBox"), [0.0, 0.0, 225.0, 112.5]);
        assert_eq!(rect(b"BleedBox"), [-3.0, -3.0, 228.0, 115.5]);
        assert!(rect(b"MediaBox")[0] < -3.0, "room for the marks");
        assert_eq!(page.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
    }

    #[test]
    fn web_fonts_are_embedded_per_instance() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/css-at-rules/assets");
        let mut options = RenderOptions::default();
        for name in ["var.ttf", "color.ttf"] {
            let data = std::fs::read(assets.join(name)).unwrap();
            options.assets.files.insert(name.into(), data);
        }
        let html = "<style>@font-face { font-family: V; src: url(var.ttf) } \
                    @font-face { font-family: C; src: url(color.ttf) }</style>\
                    <p style='font-family: V'>A <b>A</b></p><p style='font-family: C'>AB</p>";
        let output = render(html, &options).unwrap();
        assert!(output.warnings.is_empty(), "{:?}", output.warnings);
        let doc = parse(&output.pdf);
        let fonts: Vec<String> = doc
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .filter(|dict| {
                dict.get(b"Type")
                    .is_ok_and(|t| t.as_name().ok() == Some(&b"Font"[..]))
            })
            .filter(|dict| {
                dict.get(b"Subtype")
                    .is_ok_and(|t| t.as_name().ok() == Some(&b"Type0"[..]))
            })
            .map(|dict| {
                String::from_utf8_lossy(dict.get(b"BaseFont").unwrap().as_name().unwrap())
                    .into_owned()
            })
            .collect();
        // Regular and bold are two instances of the variable font, with different names.
        let var: Vec<&String> = fonts.iter().filter(|f| f.contains("TestVar")).collect();
        assert_eq!(var.len(), 2, "{fonts:?}");
        let mut names = var.clone();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), var.len(), "{fonts:?}");
        // The color glyphs are drawn as shapes, but their text is still there.
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("AB"), "{text:?}");
    }

    #[test]
    fn color_profiles_are_embedded() {
        let profile = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/css-at-rules/assets/cmyk.icc"),
        )
        .unwrap();
        let mut options = RenderOptions::default();
        options.assets.files.insert("press.icc".into(), profile);
        let html = "<style>@color-profile --press { src: url(press.icc) }</style>                    <p style='color: color(--press 0 0 0 1)'>Ink</p>                    <p style='color: device-cmyk(0 1 0 0)'>Magenta</p>";
        let output = render(html, &options).unwrap();
        assert!(output.warnings.is_empty(), "{:?}", output.warnings);
        let doc = parse(&output.pdf);
        let (_, &page) = doc.get_pages().iter().next().unwrap();
        let content = String::from_utf8_lossy(&doc.get_page_content(page)).into_owned();
        assert!(content.contains("/Cs0 cs"), "{content}");
        assert!(content.contains("0 0 0 1 scn"), "{content}");
        assert!(content.contains("0 1 0 0 k"), "{content}");
    }

    #[test]
    fn output_is_deterministic() {
        let html = "<p style='border: 2px dashed red; background: rgb(0 0 255 / 20%)'>Same</p>";
        let a = render(html, &RenderOptions::default()).unwrap().pdf;
        let b = render(html, &RenderOptions::default()).unwrap().pdf;
        assert_eq!(a, b);
        let html = "<p style='break-after: page'>Page</p>".repeat(50);
        let a = render(&html, &RenderOptions::default()).unwrap().pdf;
        let b = render(&html, &RenderOptions::default()).unwrap().pdf;
        assert_eq!(a, b);
    }

    #[test]
    fn pages_keep_their_own_content() {
        let html: String = (1..=50)
            .map(|n| format!("<p style='break-after: page'>Page number {n}.</p>"))
            .collect();
        let doc = parse(&render(&html, &RenderOptions::default()).unwrap().pdf);
        assert_eq!(doc.get_pages().len(), 50);
        for n in 1..=50 {
            let text = doc.extract_text(&[n]).unwrap();
            assert!(text.contains(&format!("number {n}.")), "page {n}: {text:?}");
        }
    }
}
