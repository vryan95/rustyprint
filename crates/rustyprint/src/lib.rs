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
use rustyprint_layout::{Images, PageOptions, layout};

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
    let styles = rustyprint_layout::style(
        &document,
        &StyleOptions {
            page_size: (area.width, area.height),
            stylesheet: options.stylesheet.as_deref(),
            load: &load,
            font_metrics: Some(&rustyprint_layout::font_metrics),
            load_font: Some(&load_font),
        },
        &page,
        &images,
    );
    let mut warnings: Vec<String> = styles
        .warnings
        .iter()
        .map(|w| format!("{}:{}: {}", w.source, w.warning.line, w.warning.message))
        .collect();
    let mut laid_out = layout(&document, &styles, &page, &images);
    warnings.extend(loader.warnings());
    warnings.extend(images.warnings());
    if options.title.is_some() {
        laid_out.title = options.title.clone();
    }
    let pdf = rustyprint_pdf::write(&laid_out).map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Output { pdf, warnings })
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
