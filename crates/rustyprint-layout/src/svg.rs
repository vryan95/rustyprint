//! SVG images. An `<svg>` element in a document is drawn as shapes and text, so it prints
//! sharply and takes no pixels; what shapes cannot express, such as a gradient or a filter,
//! is drawn to pixels at the size the element is shown at. An SVG file is drawn to pixels,
//! with more of them than its size in CSS pixels. Text is in the fonts the engine draws
//! page text in.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use resvg::{tiny_skia, usvg};
use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::{FamilyName, FontStyle, GenericFamily};
use usvg::fontdb;

use crate::Font;
use crate::display::{DisplayItem, Rect};
use crate::images::{Fit, Image};
use crate::raster::Raster;

/// The image pixels drawn for each CSS pixel of the SVG's size: 384 per inch.
const SCALE: f32 = 4.0;
/// A small SVG file, such as an icon, is drawn with at least this many pixels along its
/// longer side, since it may be shown much larger than its own size.
const MIN_LONG_SIDE: f32 = 1024.0;
/// The most pixels along the longer side.
const MAX_LONG_SIDE: f32 = 4096.0;

/// Whether `bytes` is an XML file, which for an image means SVG: markup after an optional
/// byte order mark and white space.
pub(crate) fn is_svg(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|&byte| byte == b'<')
}

/// What an SVG is drawn with besides its own markup.
#[derive(Default)]
pub(crate) struct Resources {
    /// The style whose fonts the text is drawn in: its `@font-face` rules and its
    /// fallbacks. Without one, as for an SVG file, text uses the bundled fonts.
    pub style: Option<Arc<ComputedValues>>,
    /// The files that `<image>` elements link to, already loaded, by the URL as written.
    pub files: HashMap<String, Arc<Vec<u8>>>,
    /// The fonts given to the document's SVGs so far.
    pub fonts: Fonts,
}

/// An SVG read into shapes.
pub(crate) struct Parsed {
    pub tree: usvg::Tree,
    /// Whether the SVG has a shape and no size of its own.
    pub shape_only: bool,
    pub fit: Option<Fit>,
    /// What was left out of it, such as an image over the limit.
    pub warnings: Vec<String>,
}

/// Reads an SVG file, whose images may have at most `max_pixels` pixels each.
///
/// As for an SVG image in a browser, nothing outside the file is loaded: an image the SVG
/// links to by URL is drawn only if `resources` holds it, which it does for an `<svg>`
/// element in a document.
pub(crate) fn parse(bytes: &[u8], max_pixels: u64, resources: Resources) -> Result<Parsed, String> {
    let Resources {
        style,
        files,
        fonts,
    } = resources;
    let warnings: Arc<Mutex<Vec<String>>> = Arc::default();
    let refused = warnings.clone();
    let data = usvg::ImageHrefResolver::default_data_resolver();
    let file = usvg::ImageHrefResolver::default_data_resolver();
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            // An image held in the SVG, or in an SVG it holds, is under the limit on any
            // image. Its size is read from its header, before anything is decoded.
            resolve_data: Box::new(move |mime, bytes, options| {
                if let Some(message) =
                    crate::images::over_limit("an image in an SVG", &bytes, max_pixels)
                {
                    if let Ok(mut refused) = refused.lock() {
                        refused.push(message);
                    }
                    return None;
                }
                data(mime, bytes, options)
            }),
            // The kind of file is read from its bytes.
            resolve_string: Box::new(move |href, options| {
                file("text/plain", files.get(href)?.clone(), options)
            }),
        },
        fontdb: fonts.database(),
        font_resolver: font_resolver(
            style.unwrap_or_else(|| Arc::new(ComputedValues::initial())),
            fonts,
        ),
        // The font of text that names none, as for page text.
        font_family: "serif".into(),
        font_size: MEDIUM_FONT_SIZE,
        ..usvg::Options::default()
    };
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let xml = usvg::roxmltree::Document::parse_with_options(
        text,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let tree = usvg::Tree::from_xmltree(&xml, &options).map_err(|error| error.to_string())?;
    drop(options);
    let warnings = warnings
        .lock()
        .map(|mut warnings| std::mem::take(&mut *warnings))
        .unwrap_or_default();
    Ok(Parsed {
        tree,
        shape_only: shape_only(xml.root_element()),
        fit: fit(xml.root_element()),
        warnings,
    })
}

/// Draws an SVG file with at most `max_pixels` pixels, and says what was left out of it.
/// Text is drawn in the fonts the engine draws page text in.
pub(crate) fn rasterize(
    bytes: &[u8],
    max_pixels: u64,
    resources: Resources,
) -> Result<(Image, Vec<String>), String> {
    let parsed = parse(bytes, max_pixels, resources)?;
    let (width, height) = (parsed.tree.size().width(), parsed.tree.size().height());
    let long = width.max(height);
    let scale = SCALE.max(MIN_LONG_SIDE / long).min(MAX_LONG_SIDE / long);
    let (pixel_width, pixel_height) = pixel_size(width, height, scale, max_pixels);
    // At least one pixel on each side may still be more than the limit.
    if u64::from(pixel_width) * u64::from(pixel_height) > max_pixels {
        return Err(format!(
            "an SVG image of {width}×{height} cannot be drawn within {max_pixels} pixels"
        ));
    }
    let transform =
        tiny_skia::Transform::from_scale(pixel_width as f32 / width, pixel_height as f32 / height);
    let mut image = draw(&parsed.tree, pixel_width, pixel_height, transform)
        .ok_or_else(|| format!("an SVG image of {width}×{height} cannot be drawn"))?;
    image.natural = Some((width, height));
    image.shape_only = parsed.shape_only;
    image.fit = parsed.fit;
    Ok((image, parsed.warnings))
}

/// The pixels a drawing `width` by `height` CSS pixels has at `scale` pixels for each, or
/// fewer so as to stay within `max_pixels`.
fn pixel_size(width: f32, height: f32, mut scale: f32, max_pixels: u64) -> (u32, u32) {
    let pixels = f64::from(width * scale) * f64::from(height * scale);
    if pixels > max_pixels as f64 {
        scale *= (max_pixels as f64 / pixels).sqrt() as f32;
    }
    // Rounded down, so that rounding never passes the limit.
    let size = |length: f32| ((length * scale).floor() as u32).max(1);
    (size(width), size(height))
}

/// Draws `tree`, placed by `transform`, to an image of `width` by `height` pixels.
fn draw(
    tree: &usvg::Tree,
    width: u32,
    height: u32,
    transform: tiny_skia::Transform,
) -> Option<Image> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(tree, transform, &mut pixmap.as_mut());
    let raster = Raster {
        pixmap,
        rect: Rect::new(0.0, 0.0, width as f32, height as f32),
        scale: 1.0,
    };
    Some(raster.to_image())
}

/// The pixels the SVGs of one document may be drawn to, all together, where they cannot be
/// drawn as shapes. The pictures of the document share it.
#[derive(Debug)]
pub(crate) struct PixelBudget {
    max: u64,
    used: AtomicU64,
    /// How many drawings were left blank for want of pixels.
    refused: AtomicU64,
}

impl PixelBudget {
    pub fn new(max: u64) -> Self {
        Self {
            max,
            used: AtomicU64::new(0),
            refused: AtomicU64::new(0),
        }
    }

    /// How many pixels have been drawn.
    pub fn used(&self) -> u64 {
        self.used.load(Ordering::Relaxed)
    }

    /// How many drawings were left blank so far.
    pub(crate) fn refused(&self) -> u64 {
        self.refused.load(Ordering::Relaxed)
    }

    /// What to warn of when a drawing was left blank.
    pub(crate) fn warning(&self) -> String {
        format!(
            "an `<svg>` element was left blank: it has to be drawn to pixels, and the \
             limit of {} pixels for such drawings is used up",
            self.max
        )
    }

    /// Takes `pixels` from what is left, or counts a drawing left blank.
    pub(crate) fn take(&self, pixels: u64) -> bool {
        let used = self.used();
        if pixels > self.max.saturating_sub(used) {
            self.refused.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.used.store(used + pixels, Ordering::Relaxed);
        true
    }
}

/// The picture of an `<svg>` element.
#[derive(Debug)]
pub struct SvgPicture {
    /// The size of the drawing in CSS pixels: of the `viewBox`, or else of the element.
    size: (f32, f32),
    shape_only: bool,
    fit: Option<Fit>,
    content: Content,
}

#[derive(Debug)]
enum Content {
    /// Shapes and text, in the drawing's own CSS pixels.
    Items(Arc<[DisplayItem]>),
    /// A drawing that shapes cannot express, drawn to pixels for each size it is shown at.
    Pixels {
        tree: Box<usvg::Tree>,
        /// The most pixels of one drawing.
        max_pixels: u64,
        budget: Arc<PixelBudget>,
        drawn: Mutex<Vec<Drawn>>,
    },
}

/// A drawing in pixels, or none if there were too few left, by its size in pixels.
type Drawn = ((u32, u32), Option<Arc<Image>>);

impl SvgPicture {
    /// The picture of a parsed `<svg>` element whose style is `style`. Images it holds are
    /// decoded by `decode`; `max_pixels` and `budget` limit what is drawn to pixels, and
    /// `warn` is told what was left out of it.
    pub(crate) fn new(
        parsed: Parsed,
        style: &ComputedValues,
        decode: &dyn Fn(&[u8]) -> Option<Arc<Image>>,
        max_pixels: u64,
        budget: &Arc<PixelBudget>,
        warn: &dyn Fn(String),
    ) -> Self {
        let size = (parsed.tree.size().width(), parsed.tree.size().height());
        let context = crate::svg_shapes::Context { style, decode };
        let content = match crate::svg_shapes::items(&parsed.tree, &context) {
            Ok(drawing) => {
                drawing.warnings.into_iter().for_each(warn);
                Content::Items(drawing.items.into())
            }
            Err(_) => Content::Pixels {
                tree: Box::new(parsed.tree),
                max_pixels,
                budget: budget.clone(),
                drawn: Mutex::new(Vec::new()),
            },
        };
        Self {
            size,
            shape_only: parsed.shape_only,
            fit: parsed.fit,
            content,
        }
    }

    /// The size of the drawing in CSS pixels.
    pub fn size(&self) -> (f32, f32) {
        self.size
    }

    /// Whether the picture has a shape and no size of its own: a `viewBox` alone.
    pub fn shape_only(&self) -> bool {
        self.shape_only
    }

    /// Paints the picture in `rect`, the box of its element, which clips it.
    pub(crate) fn paint(&self, rect: Rect, smooth: bool, items: &mut Vec<DisplayItem>) {
        let (width, height) = self.size;
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        // The drawing keeps its shape in a box of another shape.
        let placed = match self.fit {
            Some(fit) => fit.place(self.size, rect),
            None => rect,
        };
        match &self.content {
            Content::Items(drawing) => {
                items.push(DisplayItem::PushClip(rect));
                items.push(DisplayItem::PushTransform([
                    placed.width / width,
                    0.0,
                    0.0,
                    placed.height / height,
                    placed.x,
                    placed.y,
                ]));
                items.extend(drawing.iter().cloned());
                items.push(DisplayItem::PopClip);
                items.push(DisplayItem::PopClip);
            }
            Content::Pixels { .. } => {
                if let Some(image) = self.pixels(rect, placed) {
                    items.push(DisplayItem::Image {
                        image,
                        rect,
                        smooth,
                        orientation: 1,
                        alpha: 255,
                    });
                }
            }
        }
    }

    /// The drawing in pixels for the box `rect`, with the drawing itself at `placed`: four
    /// pixels for each CSS pixel of the box, or fewer within the limit on an image. `None`
    /// with a warning if the document has too few pixels left for it.
    fn pixels(&self, rect: Rect, placed: Rect) -> Option<Arc<Image>> {
        let Content::Pixels {
            tree,
            max_pixels,
            budget,
            drawn,
        } = &self.content
        else {
            return None;
        };
        let scale = SCALE.min(MAX_LONG_SIDE / rect.width.max(rect.height));
        let size = pixel_size(rect.width, rect.height, scale, *max_pixels);
        let mut drawn = drawn.lock().ok()?;
        if let Some((_, image)) = drawn.iter().find(|(known, _)| *known == size) {
            return image.clone();
        }
        let image = budget
            .take(u64::from(size.0) * u64::from(size.1))
            .then(|| {
                rustyprint_dom::budget::check_deadline();
                let (sx, sy) = (size.0 as f32 / rect.width, size.1 as f32 / rect.height);
                let transform = tiny_skia::Transform::from_row(
                    sx * placed.width / self.size.0,
                    0.0,
                    0.0,
                    sy * placed.height / self.size.1,
                    sx * (placed.x - rect.x),
                    sy * (placed.y - rect.y),
                );
                draw(tree, size.0, size.1, transform).map(Arc::new)
            })
            .flatten();
        drawn.push((size, image.clone()));
        image
    }
}

/// Whether the SVG has a shape and no size of its own: a `viewBox`, and neither a `width`
/// nor a `height` that is a length. A percentage, as in `width="100%"`, is not a size.
fn shape_only(root: usvg::roxmltree::Node) -> bool {
    let sized = |name: &str| {
        root.attribute(name)
            .is_some_and(|value| !value.trim().ends_with('%'))
    };
    root.has_attribute("viewBox") && !sized("width") && !sized("height")
}

/// How the SVG keeps its shape in a box of another shape. It does so when it has a
/// `viewBox`, as `preserveAspectRatio` says; `none`, or no `viewBox`, stretches it.
///
/// The picture placed is the SVG at its own size. For an SVG whose `width` and `height`
/// are not in the proportion of its `viewBox`, that is the `viewBox` already placed in
/// that size, where a browser places the `viewBox` in the box itself.
fn fit(root: usvg::roxmltree::Node) -> Option<Fit> {
    root.attribute("viewBox")?;
    let value = root.attribute("preserveAspectRatio").unwrap_or("xMidYMid");
    let mut words = value
        .split_ascii_whitespace()
        .filter(|word| *word != "defer");
    let align = words.next().unwrap_or("xMidYMid");
    let slice = words.next() == Some("slice");
    let (x, y) = align.strip_prefix('x')?.split_once('Y')?;
    let along = |word: &str| match word {
        "Min" => Some(0.0),
        "Mid" => Some(0.5),
        "Max" => Some(1.0),
        _ => None,
    };
    Some(Fit {
        align: (along(x)?, along(y)?),
        slice,
    })
}

/// The size of text that sets none, in CSS pixels: `medium`, as in a browser.
const MEDIUM_FONT_SIZE: f32 = 16.0;

/// The fonts given to the SVGs of one document. Each font is given once, without copying
/// its file, and one that cannot be read is not tried again.
#[derive(Clone, Default)]
pub(crate) struct Fonts(Arc<Mutex<FontSet>>);

#[derive(Default)]
struct FontSet {
    /// Every font given so far.
    database: Arc<fontdb::Database>,
    /// The fonts given, each with its place in the database, or none if it cannot be read.
    loaded: Vec<(Font, Option<fontdb::ID>)>,
}

/// A font's file, as the bytes a font database reads.
struct FontBytes(Font);

impl AsRef<[u8]> for FontBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.data()
    }
}

impl Fonts {
    /// The database an SVG starts from: every font given so far.
    fn database(&self) -> Arc<fontdb::Database> {
        self.0
            .lock()
            .map(|set| set.database.clone())
            .unwrap_or_default()
    }

    /// How many fonts have been given.
    #[cfg(test)]
    pub fn given(&self) -> usize {
        self.database().len()
    }

    /// The place of `font` in `database`, which becomes the document's database with the
    /// font in it.
    fn load(&self, font: &Font, database: &mut Arc<fontdb::Database>) -> Option<fontdb::ID> {
        let mut set = self.0.lock().ok()?;
        // The document's database has every font given so far, which one handed down from
        // the start of an SVG may not.
        if !Arc::ptr_eq(database, &set.database) {
            *database = set.database.clone();
        }
        let known = |(known, _): &&(Font, Option<fontdb::ID>)| {
            known.same_file(font) && known.index() == font.index()
        };
        if let Some((_, id)) = set.loaded.iter().find(known) {
            return *id;
        }
        let source = fontdb::Source::Binary(Arc::new(FontBytes(font.clone())));
        let ids = Arc::make_mut(database).load_font_source(source);
        let id = ids.get(font.index() as usize).copied();
        set.loaded.push((font.clone(), id));
        set.database = database.clone();
        id
    }
}

/// The style of page text in the font an SVG asks for, where `style` is the style of the
/// SVG: the engine chooses the font from it as it does for page text.
pub(crate) fn font_style(style: &ComputedValues, font: &usvg::Font) -> ComputedValues {
    let mut wanted = style.clone();
    wanted.font_family = font.families().iter().map(family_name).collect();
    wanted.font_weight = f32::from(font.weight());
    wanted.font_style = match font.style() {
        usvg::FontStyle::Normal => FontStyle::Normal,
        usvg::FontStyle::Italic => FontStyle::Italic,
        usvg::FontStyle::Oblique => FontStyle::Oblique(14.0),
    };
    wanted
}

/// Chooses the fonts of SVG text as the engine chooses them for page text with `style`:
/// from its `@font-face` rules and the bundled families, with the same fallbacks.
fn font_resolver(style: Arc<ComputedValues>, fonts: Fonts) -> usvg::FontResolver<'static> {
    let (select_style, select_fonts) = (style.clone(), fonts.clone());
    usvg::FontResolver {
        select_font: Box::new(move |font, database| {
            let wanted = font_style(&select_style, font);
            select_fonts.load(&crate::fonts::select(&wanted).font, database)
        }),
        select_fallback: Box::new(move |c, used, database| {
            let choice = crate::fonts::select(&style);
            choice
                .fallbacks
                .into_iter()
                .chain(crate::fonts::script_fallback(&style, c))
                .filter(|font| font.covers(c))
                .filter_map(|font| fonts.load(&font, database))
                .find(|id| !used.contains(id))
        }),
    }
}

/// A family of SVG text as a family of page text. A generic family written as a name, as
/// the default family is, is the generic family.
fn family_name(family: &usvg::FontFamily) -> FamilyName {
    FamilyName::Generic(match family {
        usvg::FontFamily::Serif => GenericFamily::Serif,
        usvg::FontFamily::SansSerif => GenericFamily::SansSerif,
        usvg::FontFamily::Cursive => GenericFamily::Cursive,
        usvg::FontFamily::Fantasy => GenericFamily::Fantasy,
        usvg::FontFamily::Monospace => GenericFamily::Monospace,
        usvg::FontFamily::Named(name) => match name.to_ascii_lowercase().as_str() {
            "serif" => GenericFamily::Serif,
            "sans-serif" => GenericFamily::SansSerif,
            "monospace" => GenericFamily::Monospace,
            _ => return FamilyName::Named(name.clone()),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_is_given_once_and_to_any_database() {
        let fonts = Fonts::default();
        let (sans, serif) = (
            crate::fonts::default_sans(),
            crate::fonts::select(&ComputedValues::initial()).font,
        );
        let mut first = fonts.database();
        let id = fonts.load(&sans, &mut first);
        assert!(id.is_some());
        // The next SVG starts with the font, and asking again gives the same one.
        let mut second = fonts.database();
        assert_eq!(second.len(), 1);
        assert_eq!(fonts.load(&sans, &mut second), id);
        assert_ne!(fonts.load(&serif, &mut second), id);
        assert_eq!(fonts.given(), 2);
        // A database from before the fonts were given is brought up to date.
        let mut stale = Arc::new(fontdb::Database::new());
        assert_eq!(fonts.load(&sans, &mut stale), id);
        assert_eq!(stale.len(), 2);
    }
}
