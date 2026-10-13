use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use harfrust::font::Blob;
use skrifa::bitmap::{BitmapData, BitmapStrikes, Origin};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::raw::types::{F2Dot14, Tag};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::display::Color;

/// Error returned when font data cannot be parsed.
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("invalid font data")]
    Parse,
    #[error("invalid WOFF data")]
    Woff,
}

/// A font face shared by layout and the PDF backend: a font file, and how it is used.
///
/// Cloning is cheap because the font file is reference-counted. Two fonts are equal when
/// they share the same file and are used the same way, so a file loaded twice counts as two
/// fonts.
#[derive(Clone)]
pub struct Font(Arc<FontInner>);

struct FontInner {
    file: Arc<FontFile>,
    instance: Instance,
    /// The instance's variations as normalized axis coordinates, one per axis, or empty at
    /// the default location.
    coords: Vec<F2Dot14>,
    /// The font at the instance's location, for shaping and glyph metrics.
    shaping: harfrust::Font,
}

/// A parsed font file.
struct FontFile {
    data: Blob,
    index: u32,
    units_per_em: u16,
    postscript_name: String,
    full_name: String,
    metrics: VerticalMetrics,
    /// OpenType layout feature tags of the `GSUB` and `GPOS` tables, sorted.
    features: Vec<[u8; 4]>,
    weight: u16,
    /// The width class as a percentage of normal.
    width: f32,
    italic: bool,
    axes: Vec<Axis>,
    /// Whether the font has color glyphs: `COLR` layers or bitmaps.
    color: bool,
    /// The colors of each `CPAL` palette, and whether it is meant for light (1) or dark (2)
    /// backgrounds.
    palettes: Vec<(Vec<Color>, u32)>,
    /// The font at its default location.
    shaping: harfrust::Font,
}

/// A variation axis of a variable font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub tag: [u8; 4],
    pub min: f32,
    pub default: f32,
    pub max: f32,
}

/// How a font file is used: what CSS asks of it beyond its glyphs.
#[derive(Debug, Clone)]
pub struct Instance {
    /// Variation axis values, clamped to the axes' ranges.
    pub variations: Vec<([u8; 4], f32)>,
    /// `size-adjust`: how much larger than the font size the glyphs are drawn.
    pub size_adjust: f32,
    /// `ascent-override`, `descent-override` and `line-gap-override`, as fractions of the
    /// font size.
    pub ascent: Option<f32>,
    pub descent: Option<f32>,
    pub line_gap: Option<f32>,
    /// The code points the font is used for, inclusive, or empty for all of them.
    pub unicode_range: Arc<[(u32, u32)]>,
    /// Features the `@font-face` rule turns on or off.
    pub features: Arc<[([u8; 4], u32)]>,
    /// The colors of color glyphs, by palette index, or `None` for the font's first palette.
    pub palette: Option<Arc<[Color]>>,
    /// The family name the font was chosen by, which `@font-feature-values` and
    /// `@font-palette-values` rules name.
    pub family: Arc<str>,
    /// The weight and whether it is italic as the `@font-face` rule declares them, which
    /// decide what is synthesized instead of the font file's own.
    pub weight: Option<u16>,
    pub italic: Option<bool>,
}

/// Instances are equal when they draw the same glyphs: the family name they were chosen
/// by does not count.
impl PartialEq for Instance {
    fn eq(&self, other: &Self) -> bool {
        self.variations == other.variations
            && self.size_adjust == other.size_adjust
            && self.ascent == other.ascent
            && self.descent == other.descent
            && self.line_gap == other.line_gap
            && self.unicode_range == other.unicode_range
            && self.features == other.features
            && self.palette == other.palette
            && self.weight == other.weight
            && self.italic == other.italic
    }
}

impl Default for Instance {
    fn default() -> Self {
        Self {
            variations: Vec::new(),
            size_adjust: 1.0,
            ascent: None,
            descent: None,
            line_gap: None,
            unicode_range: Arc::from([]),
            features: Arc::from([]),
            palette: None,
            family: Arc::from(""),
            weight: None,
            italic: None,
        }
    }
}

/// Font metrics in font units. Vertical positions are positive upwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerticalMetrics {
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub x_height: i16,
    pub cap_height: i16,
    /// Top of the underline and its thickness.
    pub underline_position: i16,
    pub underline_thickness: i16,
    /// Top of the strikeout line and its thickness.
    pub strikeout_position: i16,
    pub strikeout_thickness: i16,
    /// How far subscripts drop and superscripts rise, and their size.
    pub subscript_offset: i16,
    pub superscript_offset: i16,
    pub script_size: i16,
    /// Advance of `0`, for `ch` units.
    pub zero_advance: Option<u16>,
    /// Advance of `水`, for `ic` units.
    pub ideograph_advance: Option<u16>,
}

/// What a PDF font descriptor records about a font, in font units, at the font's
/// variations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Descriptor {
    /// The bounding box of all glyphs: left, bottom, right and top.
    pub bbox: (i16, i16, i16, i16),
    /// Degrees counter-clockwise from vertical.
    pub italic_angle: f32,
    pub italic: bool,
    pub monospaced: bool,
    /// The weight class, from 1 to 1000.
    pub weight: u16,
    pub ascender: i16,
    pub descender: i16,
    pub cap_height: i16,
}

/// A glyph image from a `sbix` or `CBDT` table, stored as a PNG file.
#[derive(Debug, Clone, Copy)]
pub struct Bitmap<'a> {
    /// The image's bottom left corner relative to the glyph origin, in image pixels,
    /// positive upwards.
    pub x: f32,
    pub y: f32,
    /// The image's size in pixels.
    pub width: u32,
    pub height: u32,
    /// How many image pixels make an em.
    pub pixels_per_em: f32,
    pub png: &'a [u8],
}

/// Receives a glyph outline, in font units with y pointing up.
pub trait GlyphPen {
    fn move_to(&mut self, x: f32, y: f32);
    fn line_to(&mut self, x: f32, y: f32);
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32);
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32);
    fn close(&mut self);
}

/// Passes a skrifa outline on to a [`GlyphPen`].
struct Pen<'p, P: ?Sized>(&'p mut P);

impl<P: GlyphPen + ?Sized> OutlinePen for Pen<'_, P> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, cy0, x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(cx0, cy0, cx1, cy1, x, y);
    }

    fn close(&mut self) {
        self.0.close();
    }
}

/// Decompresses WOFF and WOFF2 files; other font files are returned as they are.
pub fn decompress(data: &[u8]) -> Result<Option<Vec<u8>>, FontError> {
    match data.get(..4) {
        Some(b"wOFF") => wuff::decompress_woff1(data)
            .map(Some)
            .map_err(|_| FontError::Woff),
        Some(b"wOF2") => wuff::decompress_woff2(data)
            .map(Some)
            .map_err(|_| FontError::Woff),
        _ => Ok(None),
    }
}

/// The first `name` table entry with `id` in a Unicode encoding.
fn name(font: &FontRef, id: u16) -> Option<String> {
    let name = font.name().ok()?;
    let data = name.string_data();
    name.name_record()
        .iter()
        .filter(|record| record.name_id().to_u16() == id)
        .filter(|record| {
            matches!(
                (record.platform_id(), record.encoding_id()),
                (0, _) | (3, 0) | (3, 1)
            )
        })
        .find_map(|record| record.string(data).ok().map(|s| s.chars().collect()))
}

/// The palettes of a font's `CPAL` table.
fn palettes(font: &FontRef) -> Vec<(Vec<Color>, u32)> {
    let Ok(cpal) = font.cpal() else {
        return Vec::new();
    };
    let Some(Ok(records)) = cpal.color_records_array() else {
        return Vec::new();
    };
    let entries = usize::from(cpal.num_palette_entries());
    let types = cpal.palette_types_array().and_then(Result::ok);
    cpal.color_record_indices()
        .iter()
        .enumerate()
        .map(|(palette, first)| {
            let first = usize::from(first.get());
            let colors = (first..first + entries)
                .map(|i| {
                    records.get(i).map_or(Color::BLACK, |c| {
                        Color::rgba(c.red(), c.green(), c.blue(), c.alpha())
                    })
                })
                .collect();
            let kind = types
                .and_then(|types| types.get(palette))
                .map_or(0, |t| t.get().bits());
            (colors, kind)
        })
        .collect()
}

/// The feature tags of the `GSUB` and `GPOS` tables, sorted and without duplicates.
fn layout_features(font: &FontRef) -> Vec<[u8; 4]> {
    let mut features = Vec::new();
    if let Ok(list) = font.gsub().and_then(|gsub| gsub.feature_list()) {
        features.extend(
            list.feature_records()
                .iter()
                .map(|r| r.feature_tag().to_be_bytes()),
        );
    }
    if let Ok(list) = font.gpos().and_then(|gpos| gpos.feature_list()) {
        features.extend(
            list.feature_records()
                .iter()
                .map(|r| r.feature_tag().to_be_bytes()),
        );
    }
    features.sort_unstable();
    features.dedup();
    features
}

/// Line metrics and the decoration and script metrics of the `OS/2` and `post` tables, at
/// `coords`, with the fallbacks layout needs.
fn vertical_metrics(
    font: &FontRef,
    coords: &[F2Dot14],
    advance: impl Fn(char) -> Option<u16>,
) -> VerticalMetrics {
    let em = font.head().map_or(1000, |head| head.units_per_em()) as i16;
    let os2 = font.os2().ok();
    let post = font.post().ok();
    // `value` plus the `MVAR` delta for `tag`, truncated to whole units.
    let mvar = font
        .mvar()
        .ok()
        .filter(|_| coords.iter().any(|c| *c != F2Dot14::ZERO));
    let varied = |tag: &[u8; 4], value: i16| {
        let Some(mvar) = &mvar else {
            return value;
        };
        let delta = mvar
            .metric_delta(Tag::new(tag), coords)
            .map_or(0.0, |d| d.to_f32());
        let varied = f32::from(value) + delta;
        if varied.is_finite() && (f32::from(i16::MIN)..=f32::from(i16::MAX)).contains(&varied) {
            varied as i16
        } else {
            value
        }
    };
    let (line, x_height, cap_height) = line_metrics(font, &varied);
    let x_height = x_height.unwrap_or(em / 2);
    let underline = post.as_ref().map(|post| {
        (
            varied(b"undo", post.underline_position().to_i16()),
            varied(b"unds", post.underline_thickness().to_i16()),
        )
    });
    let strikeout = os2.as_ref().map(|os2| {
        (
            varied(b"stro", os2.y_strikeout_position()),
            varied(b"strs", os2.y_strikeout_size()),
        )
    });
    let subscript_offset = os2
        .as_ref()
        .map(|os2| varied(b"sbyo", os2.y_subscript_y_offset()));
    let superscript = os2.as_ref().map(|os2| {
        (
            varied(b"spyo", os2.y_superscript_y_offset()),
            varied(b"spys", os2.y_superscript_y_size()),
        )
    });
    VerticalMetrics {
        ascender: line.0,
        descender: line.1,
        line_gap: line.2,
        x_height,
        cap_height: cap_height.unwrap_or(line.0),
        underline_position: underline.map_or(-em / 10, |u| u.0),
        underline_thickness: underline.map_or(em / 20, |u| u.1).max(1),
        strikeout_position: strikeout.map_or(x_height / 2 + em / 40, |s| s.0),
        strikeout_thickness: strikeout.map_or(em / 20, |s| s.1).max(1),
        subscript_offset: subscript_offset.unwrap_or(em / 5),
        superscript_offset: superscript.map_or(em / 3, |s| s.0),
        script_size: superscript.map_or(em * 2 / 3, |s| s.1),
        zero_advance: advance('0'),
        ideograph_advance: advance('水'),
    }
}

/// The ascender, descender and line gap, from `OS/2` when it says to use its typographic
/// metrics, else from `hhea` with `OS/2` for the values `hhea` leaves at zero; and the x
/// height and cap height of `OS/2`.
fn line_metrics(
    font: &FontRef,
    varied: &impl Fn(&[u8; 4], i16) -> i16,
) -> ((i16, i16, i16), Option<i16>, Option<i16>) {
    let os2 = font.os2().ok();
    let (ascender, descender, line_gap) = font.hhea().map_or((0, 0, 0), |hhea| {
        (
            hhea.ascender().to_i16(),
            hhea.descender().to_i16(),
            hhea.line_gap().to_i16(),
        )
    });
    let Some(os2) = os2 else {
        return ((ascender, descender, line_gap), None, None);
    };
    let typo = (
        os2.s_typo_ascender(),
        os2.s_typo_descender(),
        os2.s_typo_line_gap(),
    );
    let line = if os2.fs_selection().bits() & USE_TYPO_METRICS != 0 {
        (
            varied(b"hasc", typo.0),
            varied(b"hdsc", typo.1),
            varied(b"hlgp", typo.2),
        )
    } else {
        let fallback =
            |value: i16, typo: i16, win: i16, tag: &[u8; 4], win_tag: &[u8; 4]| match (value, typo)
            {
                (0, 0) => varied(win_tag, win),
                (0, typo) => varied(tag, typo),
                (value, _) => value,
            };
        let gap = if ascender == 0 || descender == 0 {
            if typo.0 != 0 || typo.1 != 0 {
                varied(b"hlgp", typo.2)
            } else {
                0
            }
        } else {
            line_gap
        };
        (
            fallback(
                ascender,
                typo.0,
                os2.us_win_ascent() as i16,
                b"hasc",
                b"hcla",
            ),
            fallback(
                descender,
                typo.1,
                os2.us_win_descent() as i16,
                b"hdsc",
                b"hcld",
            ),
            gap,
        )
    };
    let x_height = os2.sx_height().map(|v| varied(b"xhgt", v));
    let cap_height = os2.s_cap_height().map(|v| varied(b"cpht", v));
    (line, x_height, cap_height)
}

/// `fsSelection` bits of the `OS/2` table.
const ITALIC: u16 = 1;
const USE_TYPO_METRICS: u16 = 1 << 7;

/// An advance in font units, rounded half up, if it fits.
fn round_advance(advance: f64) -> Option<u16> {
    let rounded = (advance + 0.5).floor();
    (0.0..=f64::from(u16::MAX))
        .contains(&rounded)
        .then_some(rounded as u16)
}

impl Font {
    /// Parses face `index` of a font file (TrueType, OpenType or a collection).
    pub fn new(data: impl Into<Cow<'static, [u8]>>, index: u32) -> Result<Self, FontError> {
        let data = match data.into() {
            Cow::Borrowed(data) => Blob::from(data),
            Cow::Owned(data) => Blob::from(data),
        };
        let font = FontRef::from_index(&data, index).map_err(|_| FontError::Parse)?;
        let shaping = harfrust::Font::new(data.clone(), index).ok_or(FontError::Parse)?;
        let units_per_em = font.head().map_err(|_| FontError::Parse)?.units_per_em();
        let charmap = shaping.charmap();
        let glyph_metrics = shaping.glyph_metrics();
        let hmtx = font.hmtx().is_ok();
        let advance = |c: char| {
            let glyph = charmap.map_unicode(c)?;
            hmtx.then(|| round_advance(glyph_metrics.h_advance_exact(glyph).to_f64()))?
        };
        let metrics = vertical_metrics(&font, &[], advance);
        let os2 = font.os2().ok();
        let italic_angle = font.post().map_or(0.0, |post| post.italic_angle().to_f32());
        let weight = os2.as_ref().map_or(400, |os2| os2.us_weight_class());
        let width = match os2.as_ref().map_or(5, |os2| os2.us_width_class()) {
            1 => 50.0,
            2 => 62.5,
            3 => 75.0,
            4 => 87.5,
            6 => 112.5,
            7 => 125.0,
            8 => 150.0,
            9 => 200.0,
            _ => 100.0,
        };
        let italic = os2
            .as_ref()
            .is_some_and(|os2| os2.fs_selection().bits() & ITALIC != 0)
            || italic_angle != 0.0;
        let axes = font
            .axes()
            .iter()
            .map(|axis| Axis {
                tag: axis.tag().to_be_bytes(),
                min: axis.min_value(),
                default: axis.default_value(),
                max: axis.max_value(),
            })
            .collect();
        let color = font.colr().is_ok()
            || font.sbix().is_ok()
            || (font.cbdt().is_ok() && font.cblc().is_ok());
        let file = FontFile {
            index,
            units_per_em,
            postscript_name: name(&font, 6).unwrap_or_else(|| "Font".to_owned()),
            full_name: name(&font, 4).unwrap_or_default(),
            metrics,
            features: layout_features(&font),
            weight,
            width,
            italic,
            axes,
            color,
            palettes: palettes(&font),
            shaping: shaping.clone(),
            data,
        };
        Ok(Self(Arc::new(FontInner {
            file: Arc::new(file),
            instance: Instance::default(),
            coords: Vec::new(),
            shaping,
        })))
    }

    /// Parses face `index` of a font file, which may be compressed as WOFF or WOFF2.
    pub fn from_file(data: &[u8], index: u32) -> Result<Self, FontError> {
        match decompress(data)? {
            Some(decompressed) => Self::new(decompressed, index),
            None => Self::new(data.to_vec(), index),
        }
    }

    /// The same font file used as `instance` says, with variations clamped to the axes.
    pub fn with_instance(&self, mut instance: Instance) -> Font {
        let file = &self.0.file;
        let axes = &file.axes;
        instance
            .variations
            .retain(|(tag, _)| axes.iter().any(|a| a.tag == *tag));
        for (tag, value) in &mut instance.variations {
            let axis = axes.iter().find(|a| a.tag == *tag).expect("kept above");
            *value = value.clamp(axis.min, axis.max);
        }
        instance.variations.sort_by_key(|(tag, _)| *tag);
        let location = self.font_ref().axes().location(
            instance
                .variations
                .iter()
                .map(|(tag, value)| (Tag::new(tag), *value)),
        );
        let coords: Vec<F2Dot14> = if location.coords().iter().all(|c| *c == F2Dot14::ZERO) {
            Vec::new()
        } else {
            location.coords().to_vec()
        };
        let shaping = if coords.is_empty() {
            file.shaping.clone()
        } else {
            file.shaping
                .instance_builder()
                .normalized_coords(coords.iter().copied())
                .build()
        };
        Font(Arc::new(FontInner {
            file: file.clone(),
            instance,
            coords,
            shaping,
        }))
    }

    /// How the font file is used.
    pub fn instance(&self) -> &Instance {
        &self.0.instance
    }

    /// The bundled default font, Noto Sans Regular.
    pub fn bundled() -> Self {
        crate::fonts::default_sans()
    }

    /// The raw font file.
    pub fn data(&self) -> &[u8] {
        &self.0.file.data
    }

    /// The face index within the font file.
    pub fn index(&self) -> u32 {
        self.0.file.index
    }

    /// Whether two fonts use the same font file.
    pub fn same_file(&self, other: &Font) -> bool {
        Arc::ptr_eq(&self.0.file, &other.0.file)
    }

    /// Font design units per em.
    pub fn units_per_em(&self) -> u16 {
        self.0.file.units_per_em
    }

    /// The PostScript name from the `name` table, or `Font` if there is none.
    pub fn postscript_name(&self) -> &str {
        &self.0.file.postscript_name
    }

    /// The full name from the `name` table, such as `Noto Sans Bold`.
    pub fn full_name(&self) -> &str {
        &self.0.file.full_name
    }

    /// Font metrics in font units, with the instance's overrides.
    pub fn metrics(&self) -> VerticalMetrics {
        let mut metrics = self.0.file.metrics;
        let em = f32::from(self.units_per_em());
        let units = |fraction: f32| (fraction * em).round().clamp(-32768.0, 32767.0) as i16;
        let instance = &self.0.instance;
        if let Some(ascent) = instance.ascent {
            metrics.ascender = units(ascent);
        }
        if let Some(descent) = instance.descent {
            metrics.descender = -units(descent);
        }
        if let Some(line_gap) = instance.line_gap {
            metrics.line_gap = units(line_gap);
        }
        metrics
    }

    /// The weight, from 1 to 1000: the `wght` axis of a variable font, what the
    /// `@font-face` rule declares, or the weight class.
    pub fn weight(&self) -> u16 {
        self.variation(b"wght")
            .map(|w| w.round() as u16)
            .or(self.0.instance.weight)
            .unwrap_or(self.0.file.weight)
    }

    /// The weights the font file has: the range of its `wght` axis, or its weight class.
    pub fn weight_range(&self) -> (f32, f32) {
        self.axis_range(b"wght")
            .unwrap_or((f32::from(self.0.file.weight), f32::from(self.0.file.weight)))
    }

    /// The widths the font file has, in percent: the range of its `wdth` axis, or its width
    /// class.
    pub fn width_range(&self) -> (f32, f32) {
        self.axis_range(b"wdth")
            .unwrap_or((self.0.file.width, self.0.file.width))
    }

    /// Whether the font file is italic, by its style or an `ital` axis.
    pub fn file_italic(&self) -> bool {
        self.0.file.italic || self.axis_range(b"ital").is_some_and(|(_, max)| max >= 1.0)
    }

    fn axis_range(&self, tag: &[u8; 4]) -> Option<(f32, f32)> {
        self.0
            .file
            .axes
            .iter()
            .find(|axis| axis.tag == *tag)
            .map(|axis| (axis.min, axis.max))
    }

    /// Whether the face is italic or oblique, or slanted by its variation axes.
    pub fn is_italic(&self) -> bool {
        self.0.instance.italic.unwrap_or(self.0.file.italic)
            || self.variation(b"ital").is_some_and(|v| v >= 0.5)
            || self.variation(b"slnt").is_some_and(|v| v != 0.0)
    }

    /// The value of a variation axis, if the font varies along it.
    pub fn variation(&self, tag: &[u8; 4]) -> Option<f32> {
        self.0
            .instance
            .variations
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, v)| *v)
    }

    /// The variation axes of a variable font.
    pub fn axes(&self) -> &[Axis] {
        &self.0.file.axes
    }

    /// The palettes of a color font: their colors and their type, 1 for light backgrounds
    /// and 2 for dark ones.
    pub fn palettes(&self) -> &[(Vec<Color>, u32)] {
        &self.0.file.palettes
    }

    /// The colors color glyphs are drawn in, by palette index.
    pub fn palette(&self) -> Cow<'_, [Color]> {
        match &self.0.instance.palette {
            Some(palette) => Cow::Borrowed(palette),
            None => self
                .0
                .file
                .palettes
                .first()
                .map_or(Cow::Borrowed(&[][..]), |(colors, _)| Cow::Borrowed(colors)),
        }
    }

    /// Whether the font's layout tables have the OpenType feature `tag`.
    pub fn has_feature(&self, tag: &[u8; 4]) -> bool {
        self.0.file.features.binary_search(tag).is_ok()
    }

    /// `size-adjust`: how much larger than the font size the glyphs are drawn.
    pub fn size_adjust(&self) -> f32 {
        self.0.instance.size_adjust
    }

    /// CSS pixels per font unit at `size` pixels per em, with `size-adjust`.
    pub fn em_scale(&self, size: f32) -> f32 {
        size * self.size_adjust() / f32::from(self.units_per_em())
    }

    /// Converts font units to CSS pixels at `size` pixels per em.
    pub fn scale(&self, units: i16, size: f32) -> f32 {
        f32::from(units) * self.em_scale(size)
    }

    /// The font file for direct table access.
    pub(crate) fn font_ref(&self) -> FontRef<'_> {
        FontRef::from_index(self.data(), self.index()).expect("validated in Font::new")
    }

    /// The instance's variations as normalized coordinates, for skrifa.
    pub(crate) fn location(&self) -> LocationRef<'_> {
        LocationRef::new(&self.0.coords)
    }

    /// The font at the instance's location, for shaping.
    pub(crate) fn shaping(&self) -> &harfrust::Font {
        &self.0.shaping
    }

    /// The glyph the font maps `c` to.
    pub fn glyph(&self, c: char) -> Option<u16> {
        self.0
            .shaping
            .charmap()
            .map_unicode(c)
            .and_then(|glyph| u16::try_from(glyph.to_u32()).ok())
    }

    /// The horizontal advance of glyph `id` in font units, at the instance's variations.
    pub fn advance(&self, id: u16) -> Option<u16> {
        if u32::from(id) >= self.0.shaping.num_glyphs() || self.font_ref().hmtx().is_err() {
            return None;
        }
        let advance = self.0.shaping.glyph_metrics().h_advance_exact(id.into());
        round_advance(advance.to_f64())
    }

    /// The vertical advance of glyph `id` in font units, if the font has vertical metrics.
    pub fn vertical_advance(&self, id: u16) -> Option<u16> {
        if u32::from(id) >= self.0.shaping.num_glyphs() || self.font_ref().vmtx().is_err() {
            return None;
        }
        let advance = self.0.shaping.glyph_metrics().v_advance_exact(id.into());
        round_advance(advance.to_f64())
    }

    /// Draws glyph `id`'s outline at the instance's variations into `pen`, or returns `None`
    /// if the font has no outline for it.
    pub fn outline(&self, id: u16, pen: &mut (impl GlyphPen + ?Sized)) -> Option<()> {
        let font = self.font_ref();
        let outline = font.outline_glyphs().get(GlyphId::from(id))?;
        let settings = DrawSettings::unhinted(Size::unscaled(), self.location());
        outline.draw(settings, &mut Pen(pen)).ok()?;
        Some(())
    }

    /// Glyph `id`'s image, in any format, in the first of the font's bitmap strikes with the
    /// most pixels per em.
    fn bitmap_glyph(&self, id: u16) -> Option<skrifa::bitmap::BitmapGlyph<'_>> {
        let strikes = BitmapStrikes::new(&FontRef::from_index(self.data(), self.index()).ok()?);
        let strike = strikes.iter().reduce(|best, strike| {
            if strike.ppem() > best.ppem() {
                strike
            } else {
                best
            }
        })?;
        strike.get(GlyphId::from(id))
    }

    /// Glyph `id`'s PNG image in the font's largest bitmap strike.
    pub fn bitmap(&self, id: u16) -> Option<Bitmap<'_>> {
        let glyph = self.bitmap_glyph(id)?;
        let BitmapData::Png(png) = glyph.data else {
            return None;
        };
        let height = glyph.height as f32;
        Some(Bitmap {
            x: glyph.inner_bearing_x,
            y: match glyph.placement_origin {
                Origin::TopLeft => glyph.inner_bearing_y - height,
                Origin::BottomLeft => glyph.inner_bearing_y,
            },
            width: glyph.width,
            height: glyph.height,
            pixels_per_em: glyph.ppem_y,
            png,
        })
    }

    /// What a PDF font descriptor records about the font.
    pub fn descriptor(&self) -> Descriptor {
        let font = self.font_ref();
        let metrics = vertical_metrics(&font, &self.0.coords, |_| None);
        let bbox = font.head().map_or((0, 0, 0, 0), |head| {
            (head.x_min(), head.y_min(), head.x_max(), head.y_max())
        });
        let post = font.post().ok();
        Descriptor {
            bbox,
            italic_angle: post
                .as_ref()
                .map_or(0.0, |post| post.italic_angle().to_f32()),
            italic: self.0.file.italic,
            monospaced: post.is_some_and(|post| post.is_fixed_pitch() != 0),
            weight: self.0.file.weight,
            ascender: metrics.ascender,
            descender: metrics.descender,
            cap_height: metrics.cap_height,
        }
    }

    /// Whether the font is used for `c`, by its `unicode-range`.
    pub fn in_range(&self, c: char) -> bool {
        let ranges = &self.0.instance.unicode_range;
        let c = u32::from(c);
        ranges.is_empty() || ranges.iter().any(|&(a, b)| (a..=b).contains(&c))
    }

    /// Whether the font has a glyph for each character of `text` and is used for it.
    pub fn coverage(&self, text: &str) -> Vec<bool> {
        let charmap = self.0.shaping.charmap();
        text.chars()
            .map(|c| self.in_range(c) && charmap.map_unicode(c).is_some())
            .collect()
    }

    /// Whether the font has color glyphs.
    pub fn has_color(&self) -> bool {
        self.0.file.color
    }

    /// Whether glyph `id` is drawn in color, from `COLR` layers or a bitmap.
    pub fn is_color_glyph(&self, id: u16) -> bool {
        self.font_ref()
            .color_glyphs()
            .get(GlyphId::from(id))
            .is_some()
            || self.bitmap_glyph(id).is_some()
    }

    /// The horizontal extent, in font units, of glyph `id`'s ink inside the band between
    /// `bottom` and `top` (font units, positive upwards), or `None` if none of it is there.
    pub fn ink_in_band(&self, id: u16, bottom: f32, top: f32) -> Option<(f32, f32)> {
        let mut band = Band {
            bottom,
            top,
            last: (0.0, 0.0),
            start: (0.0, 0.0),
            ranges: Vec::new(),
        };
        self.outline(id, &mut band)?;
        // The outline is closed, so the ink spans from its leftmost to its rightmost crossing.
        band.ranges
            .into_iter()
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    }
}

/// Collects the horizontal extents of an outline's segments inside a horizontal band.
struct Band {
    bottom: f32,
    top: f32,
    last: (f32, f32),
    start: (f32, f32),
    ranges: Vec<(f32, f32)>,
}

impl Band {
    fn segment(&mut self, to: (f32, f32)) {
        let (from, to_point) = (self.last, to);
        self.last = to;
        let (y0, y1) = (from.1.min(to_point.1), from.1.max(to_point.1));
        if y1 < self.bottom || y0 > self.top {
            return;
        }
        // Clip the segment to the band and keep its horizontal extent.
        let at = |y: f32| {
            if (to_point.1 - from.1).abs() < f32::EPSILON {
                from.0
            } else {
                from.0 + (to_point.0 - from.0) * (y - from.1) / (to_point.1 - from.1)
            }
        };
        let lo = y0.max(self.bottom);
        let hi = y1.min(self.top);
        let (xa, xb) = (at(lo), at(hi));
        let (xa, xb) = if (to_point.1 - from.1).abs() < f32::EPSILON {
            (from.0, to_point.0)
        } else {
            (xa, xb)
        };
        self.ranges.push((xa.min(xb), xa.max(xb)));
    }

    /// Flattens a curve into line segments.
    fn curve(&mut self, points: &[(f32, f32)]) {
        const STEPS: usize = 8;
        let p0 = self.last;
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            let point = match points {
                [c, end] => {
                    let mt = 1.0 - t;
                    (
                        mt * mt * p0.0 + 2.0 * mt * t * c.0 + t * t * end.0,
                        mt * mt * p0.1 + 2.0 * mt * t * c.1 + t * t * end.1,
                    )
                }
                [c1, c2, end] => {
                    let mt = 1.0 - t;
                    (
                        mt * mt * mt * p0.0
                            + 3.0 * mt * mt * t * c1.0
                            + 3.0 * mt * t * t * c2.0
                            + t * t * t * end.0,
                        mt * mt * mt * p0.1
                            + 3.0 * mt * mt * t * c1.1
                            + 3.0 * mt * t * t * c2.1
                            + t * t * t * end.1,
                    )
                }
                _ => return,
            };
            self.segment(point);
        }
    }
}

impl GlyphPen for Band {
    fn move_to(&mut self, x: f32, y: f32) {
        self.last = (x, y);
        self.start = (x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.segment((x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.curve(&[(x1, y1), (x, y)]);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.curve(&[(x1, y1), (x2, y2), (x, y)]);
    }

    fn close(&mut self) {
        let start = self.start;
        self.segment(start);
    }
}

/// The variations of a font as numbers, for caches keyed by what a glyph looks like.
pub(crate) fn variation_key(font: &Font) -> Vec<u32> {
    font.instance()
        .variations
        .iter()
        .flat_map(|(tag, value)| [u32::from_be_bytes(*tag), value.to_bits()])
        .collect()
}

impl PartialEq for Font {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || (Arc::ptr_eq(&self.0.file, &other.0.file) && self.0.instance == other.0.instance)
    }
}

impl Eq for Font {}

impl fmt::Debug for Font {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Font")
            .field(&self.postscript_name())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_loads() {
        let font = Font::bundled();
        assert_eq!(font.postscript_name(), "NotoSans-Regular");
        assert_eq!(font.full_name(), "Noto Sans Regular");
        assert_eq!(font.units_per_em(), 1000);
        assert_eq!(font, Font::bundled());
        assert!(font.has_feature(b"liga"));
        assert!(font.has_feature(b"kern"));
        assert!(!font.has_feature(b"zzzz"));
        let metrics = font.metrics();
        assert!(metrics.underline_position < 0 && metrics.underline_thickness > 0);
        assert!(metrics.strikeout_position > 0);
        assert!(metrics.zero_advance.is_some());
        assert!(font.axes().is_empty() && font.palettes().is_empty());
    }

    #[test]
    fn invalid_data_is_rejected() {
        assert!(Font::new(&b"not a font"[..], 0).is_err());
        assert!(Font::from_file(b"wOF2 not really", 0).is_err());
    }

    #[test]
    fn instances_share_the_file() {
        let font = Font::bundled();
        let adjusted = font.with_instance(Instance {
            size_adjust: 2.0,
            ascent: Some(0.5),
            // Noto Sans does not vary, so this is dropped.
            variations: vec![(*b"wght", 700.0)],
            ..Instance::default()
        });
        assert_ne!(font, adjusted);
        assert!(font.same_file(&adjusted));
        assert_eq!(adjusted.scale(1000, 10.0), 20.0);
        assert_eq!(adjusted.metrics().ascender, 500);
        assert!(adjusted.instance().variations.is_empty());
        assert_eq!(adjusted, font.with_instance(adjusted.instance().clone()));
    }

    #[test]
    fn ink_crosses_bands() {
        let font = Font::bundled();
        let g = font.glyph('g').unwrap();
        let o = font.glyph('o').unwrap();
        // The descender of `g` crosses an underline band; `o` does not.
        assert!(font.ink_in_band(g, -150.0, -100.0).is_some());
        assert!(font.ink_in_band(o, -150.0, -100.0).is_none());
        let (left, right) = font.ink_in_band(o, 200.0, 250.0).unwrap();
        assert!(left < right);
    }
}
