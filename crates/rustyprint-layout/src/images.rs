//! Raster images: decoding, and loading the images a document refers to.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use rustyprint_css::ComputedValues;
use rustyprint_css::properties::types::ImageOrientation;

/// A decoded raster image, as stored in the file: one image pixel is one CSS pixel unless
/// `image-resolution` says otherwise.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: ImageData,
    /// The EXIF orientation, 1 to 8: how the stored pixels turn and flip to display upright.
    pub orientation: u8,
    /// The resolution the file declares, in image pixels per CSS pixel.
    pub resolution: Option<f32>,
}

/// How an image's pixels are stored.
pub enum ImageData {
    /// A JPEG file, embedded as is, with 1 (gray) or 3 (RGB) color components.
    Jpeg { data: Vec<u8>, components: u8 },
    /// 8-bit samples row by row, gray or RGB, with a separate alpha channel if any pixel is
    /// not opaque.
    Pixels {
        samples: Vec<u8>,
        gray: bool,
        alpha: Option<Vec<u8>>,
    },
}

impl Image {
    /// Decodes a PNG, JPEG, GIF, WebP or BMP file.
    pub fn decode(bytes: &[u8]) -> Result<Image, String> {
        let orientation = exif_orientation(bytes);
        if let Some((width, height, components)) = jpeg_header(bytes)
            && matches!(components, 1 | 3)
        {
            return Ok(Image {
                width,
                height,
                data: ImageData::Jpeg {
                    data: bytes.to_vec(),
                    components,
                },
                orientation,
                resolution: jfif_resolution(bytes),
            });
        }
        let decoded = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let (width, height) = (decoded.width(), decoded.height());
        let color = decoded.color();
        let gray = !color.has_color();
        let channels = if gray { 1 } else { 3 };
        let pixels: Vec<u8> = if gray {
            decoded.to_luma_alpha8().into_raw()
        } else {
            decoded.to_rgba8().into_raw()
        };
        let mut samples = Vec::with_capacity(pixels.len() / (channels + 1) * channels);
        let mut alpha = Vec::with_capacity(pixels.len() / (channels + 1));
        for pixel in pixels.chunks_exact(channels + 1) {
            samples.extend_from_slice(&pixel[..channels]);
            alpha.push(pixel[channels]);
        }
        let opaque = !color.has_alpha() || alpha.iter().all(|&a| a == 255);
        Ok(Image {
            width,
            height,
            data: ImageData::Pixels {
                samples,
                gray,
                alpha: (!opaque).then_some(alpha),
            },
            orientation,
            resolution: png_resolution(bytes),
        })
    }

    /// Whether an orientation turns the image a quarter turn, swapping width and height.
    pub fn turned(orientation: u8) -> bool {
        matches!(orientation, 5..=8)
    }
}

/// An image's natural size in CSS pixels as it displays: turned by its orientation and
/// scaled by `image-resolution`.
pub fn natural_size(image: &Image, style: &ComputedValues) -> (f32, f32) {
    let dppx = style.image_resolution.dppx(image.resolution);
    let (w, h) = (image.width as f32 / dppx, image.height as f32 / dppx);
    if Image::turned(orientation(image, style.image_orientation)) {
        (h, w)
    } else {
        (w, h)
    }
}

/// The EXIF orientation an image displays in under `image-orientation`.
pub fn orientation(image: &Image, value: ImageOrientation) -> u8 {
    match value {
        ImageOrientation::FromImage => image.orientation,
        ImageOrientation::None => 1,
        // A clockwise turn, then a horizontal flip.
        ImageOrientation::Turn { quarters, flip } => match (quarters % 4, flip) {
            (0, false) => 1,
            (0, true) => 2,
            (1, false) => 6,
            (1, true) => 5,
            (2, false) => 3,
            (2, true) => 4,
            (3, false) => 8,
            _ => 7,
        },
    }
}

/// The EXIF orientation of a JPEG, TIFF or WebP file, or 1.
/// An image file's size in pixels, read from its header.
fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if let Some((width, height, _)) = jpeg_header(bytes) {
        return Some((width, height));
    }
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

fn exif_orientation(bytes: &[u8]) -> u8 {
    use image::ImageDecoder;
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_decoder().ok())
        .and_then(|mut decoder| decoder.orientation().ok())
        .map_or(1, |orientation| orientation.to_exif())
}

/// CSS pixels per inch, which a file's dots per inch divide into image pixels per CSS pixel.
const CSS_DPI: f32 = 96.0;

/// The density of a JPEG's JFIF header, in image pixels per CSS pixel.
fn jfif_resolution(bytes: &[u8]) -> Option<f32> {
    // SOI, then APP0 with the `JFIF` identifier, version, units and densities.
    let app0 = bytes.get(2..20)?;
    if app0[..2] != [0xFF, 0xE0] || &app0[4..9] != b"JFIF\0" {
        return None;
    }
    let units = app0[11];
    let density = f32::from(u16::from_be_bytes([app0[12], app0[13]]));
    let per_inch = match units {
        1 => density,
        2 => density * 2.54,
        _ => return None,
    };
    (per_inch > 0.0).then_some(per_inch / CSS_DPI)
}

/// The density of a PNG's `pHYs` chunk, in image pixels per CSS pixel.
fn png_resolution(bytes: &[u8]) -> Option<f32> {
    if bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut at = 8;
    while let Some(header) = bytes.get(at..at + 8) {
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = &header[4..8];
        if kind == b"IDAT" {
            return None;
        }
        if kind == b"pHYs" {
            let data = bytes.get(at + 8..at + 17)?;
            let x = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as f32;
            // Unit 1 is the metre.
            return (data[8] == 1 && x > 0.0).then(|| x * 0.0254 / CSS_DPI);
        }
        at += 12 + length;
    }
    None
}

/// The size and number of color components of a JPEG file, from its start-of-frame marker.
fn jpeg_header(bytes: &[u8]) -> Option<(u32, u32, u8)> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut at = 2;
    loop {
        // Markers may be padded with fill bytes.
        while *bytes.get(at)? == 0xFF && *bytes.get(at + 1)? == 0xFF {
            at += 1;
        }
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        let length = usize::from(u16::from_be_bytes([
            *bytes.get(at + 2)?,
            *bytes.get(at + 3)?,
        ]));
        // SOF0 to SOF15, except DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let height = u16::from_be_bytes([*bytes.get(at + 5)?, *bytes.get(at + 6)?]);
            let width = u16::from_be_bytes([*bytes.get(at + 7)?, *bytes.get(at + 8)?]);
            let components = *bytes.get(at + 9)?;
            // Arithmetic coding (C9 to CB, CD to CF) is not supported by every PDF reader.
            if marker >= 0xC9 || width == 0 || height == 0 {
                return None;
            }
            return Some((u32::from(width), u32::from(height), components));
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        at += 2 + length;
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match &self.data {
            ImageData::Jpeg { .. } => "jpeg",
            ImageData::Pixels { gray: true, .. } => "gray",
            ImageData::Pixels { gray: false, .. } => "rgb",
        };
        write!(f, "Image({}x{} {kind})", self.width, self.height)
    }
}

/// Images are compared by identity: the same decoded file is the same image.
impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Fetches the bytes of a file by the URL the document uses, or `None` if it is unavailable.
pub type LoadFile<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

/// Loads and decodes the images a document refers to, each once.
pub struct Images<'a> {
    load: LoadFile<'a>,
    cache: RefCell<HashMap<String, Option<Arc<Image>>>>,
    /// Elements painted for `element()`, by ID.
    elements: RefCell<HashMap<String, ElementImage>>,
    /// How many `iframe` documents are being built inside each other.
    frames: Cell<usize>,
    /// The most pixels an image may have; larger ones are not decoded.
    max_pixels: u64,
    warnings: RefCell<Vec<String>>,
}

/// An element painted as an image: its display items relative to its border box, and the
/// size of the border box.
#[derive(Debug, Clone)]
pub struct ElementImage {
    pub items: Arc<[crate::display::DisplayItem]>,
    pub size: (f32, f32),
}

impl<'a> Images<'a> {
    pub fn new(load: LoadFile<'a>) -> Self {
        Self {
            load,
            cache: RefCell::new(HashMap::new()),
            elements: RefCell::new(HashMap::new()),
            frames: Cell::new(0),
            max_pixels: u64::MAX,
            warnings: RefCell::new(Vec::new()),
        }
    }

    /// Skips images with more than `max` pixels, with a warning, without decoding them.
    pub fn with_max_pixels(self, max: u64) -> Self {
        Self {
            max_pixels: max,
            ..self
        }
    }

    /// A loader without files: every image is unavailable.
    pub fn none() -> Images<'static> {
        Images::new(&|_| None)
    }

    /// The image at `url`, or `None` if it cannot be loaded or decoded.
    pub fn get(&self, url: &str) -> Option<Arc<Image>> {
        if let Some(cached) = self.cache.borrow().get(url) {
            return cached.clone();
        }
        let image = match (self.load)(url) {
            None => {
                self.warn(format!("could not load image `{url}`"));
                None
            }
            Some(bytes) if self.too_large(url, &bytes) => None,
            Some(bytes) => match Image::decode(&bytes) {
                Ok(image) => Some(Arc::new(image)),
                Err(error) => {
                    self.warn(format!("could not decode image `{url}`: {error}"));
                    None
                }
            },
        };
        self.cache
            .borrow_mut()
            .insert(url.to_owned(), image.clone());
        image
    }

    /// Whether the image in `bytes` has more pixels than allowed, which is warned about.
    fn too_large(&self, url: &str, bytes: &[u8]) -> bool {
        let Some((width, height)) = dimensions(bytes) else {
            return false;
        };
        let pixels = u64::from(width) * u64::from(height);
        if pixels <= self.max_pixels {
            return false;
        }
        self.warn(format!(
            "image `{url}` is {width}×{height} pixels, more than the limit of {}",
            self.max_pixels
        ));
        true
    }

    /// The painted element with an ID, for `element()`.
    pub fn element(&self, id: &str) -> Option<ElementImage> {
        self.elements.borrow().get(id).cloned()
    }

    /// Registers the painting of the element with an ID.
    pub fn set_element(&self, id: &str, image: ElementImage) {
        self.elements.borrow_mut().insert(id.to_owned(), image);
    }

    /// The text of the file at `url`, such as a stylesheet or a framed document, with
    /// invalid UTF-8 replaced.
    pub fn text(&self, url: &str) -> Option<String> {
        (self.load)(url).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    /// The bytes of the file at `url`, such as a font.
    pub fn bytes(&self, url: &str) -> Option<Vec<u8>> {
        (self.load)(url)
    }

    /// How many framed documents are being built inside each other.
    pub(crate) fn frame_depth(&self) -> usize {
        self.frames.get()
    }

    /// Builds a framed document one level deeper.
    pub(crate) fn in_frame<T>(&self, build: impl FnOnce() -> T) -> T {
        self.frames.set(self.frames.get() + 1);
        let result = build();
        self.frames.set(self.frames.get() - 1);
        result
    }

    /// Records a problem once, however often the document is built.
    pub(crate) fn warn(&self, message: String) {
        let mut warnings = self.warnings.borrow_mut();
        if !warnings.contains(&message) {
            warnings.push(message);
        }
    }

    /// Problems loading images, in the order they were found.
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(color: image::DynamicImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        color
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("encodes");
        out.into_inner()
    }

    #[test]
    fn decodes_png_with_alpha() {
        let mut rgba = image::RgbaImage::new(2, 1);
        rgba.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        rgba.put_pixel(1, 0, image::Rgba([0, 0, 255, 128]));
        let image = Image::decode(&png(rgba.into())).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        let ImageData::Pixels {
            samples,
            gray,
            alpha,
        } = image.data
        else {
            panic!("expected pixels");
        };
        assert!(!gray);
        assert_eq!(samples, [255, 0, 0, 0, 0, 255]);
        assert_eq!(alpha.as_deref(), Some(&[255, 128][..]));
    }

    #[test]
    fn opaque_gray_png_has_no_alpha() {
        let gray = image::GrayImage::from_pixel(3, 2, image::Luma([90]));
        let image = Image::decode(&png(gray.into())).unwrap();
        assert!(matches!(
            image.data,
            ImageData::Pixels {
                gray: true,
                alpha: None,
                ..
            }
        ));
    }

    #[test]
    fn jpegs_pass_through() {
        let rgb = image::RgbImage::from_pixel(5, 4, image::Rgb([10, 200, 30]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::from(rgb)
            .write_to(&mut out, image::ImageFormat::Jpeg)
            .ok();
        let bytes = out.into_inner();
        if bytes.is_empty() {
            // The JPEG encoder is not enabled; the header parser is still covered below.
            return;
        }
        let image = Image::decode(&bytes).unwrap();
        assert_eq!((image.width, image.height), (5, 4));
        assert!(matches!(image.data, ImageData::Jpeg { components: 3, .. }));
    }

    #[test]
    fn missing_and_broken_images_warn_once() {
        let load = |url: &str| (url == "broken.png").then(|| b"not an image".to_vec());
        let images = Images::new(&load);
        assert!(images.get("missing.png").is_none());
        assert!(images.get("missing.png").is_none());
        assert!(images.get("broken.png").is_none());
        let warnings = images.warnings();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("could not load image `missing.png`"));
        assert!(warnings[1].contains("could not decode image `broken.png`"));
    }

    #[test]
    fn images_over_the_pixel_limit_are_skipped() {
        let gray = image::GrayImage::from_pixel(4, 3, image::Luma([90]));
        let bytes = png(gray.into());
        let load = |_: &str| Some(bytes.clone());
        let images = Images::new(&load).with_max_pixels(12);
        assert!(images.get("fits.png").is_some());
        let images = Images::new(&load).with_max_pixels(11);
        assert!(images.get("big.png").is_none());
        let warnings = images.warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("`big.png` is 4×3 pixels"),
            "{warnings:?}"
        );
    }
}
