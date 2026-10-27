//! Font selection from the bundled Noto families (SIL Open Font License 1.1).

use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::{Arc, OnceLock, Weak};

use rustyprint_css::ComputedValues;
use rustyprint_css::font_rules::{BasePalette, FaceStyle, FontFace, FontSource};
use rustyprint_css::properties::types::{
    FamilyName, FontOpticalSizing, FontPalette, FontStyle, FontSynthesisStyle, GenericFamily,
    Synthesis,
};
use rustyprint_css::values::{AbsoluteColor, FontMetrics, mix};

use crate::Font;
use crate::display::Color;
use crate::font::Instance;

/// A bundled family and its faces: regular, bold, italic and bold italic.
struct Family {
    name: &'static str,
    faces: [&'static [u8]; 4],
}

const SANS: usize = 0;
const SERIF: usize = 1;
const MONO: usize = 2;

static FAMILIES: [Family; 3] = [
    Family {
        name: "Noto Sans",
        faces: [
            include_bytes!("../../../fonts/NotoSans-Regular.ttf"),
            include_bytes!("../../../fonts/NotoSans-Bold.ttf"),
            include_bytes!("../../../fonts/NotoSans-Italic.ttf"),
            include_bytes!("../../../fonts/NotoSans-BoldItalic.ttf"),
        ],
    },
    Family {
        name: "Noto Serif",
        faces: [
            include_bytes!("../../../fonts/NotoSerif-Regular.ttf"),
            include_bytes!("../../../fonts/NotoSerif-Bold.ttf"),
            include_bytes!("../../../fonts/NotoSerif-Italic.ttf"),
            include_bytes!("../../../fonts/NotoSerif-BoldItalic.ttf"),
        ],
    },
    // Noto Sans Mono has no italic faces; italics are synthesized from the upright ones.
    Family {
        name: "Noto Sans Mono",
        faces: [
            include_bytes!("../../../fonts/NotoSansMono-Regular.ttf"),
            include_bytes!("../../../fonts/NotoSansMono-Bold.ttf"),
            include_bytes!("../../../fonts/NotoSansMono-Regular.ttf"),
            include_bytes!("../../../fonts/NotoSansMono-Bold.ttf"),
        ],
    },
];

/// A bundled variable font for a script the Latin families lack, tried when no other font
/// covers a character.
struct ScriptFont {
    name: &'static str,
    data: &'static [u8],
    /// The blocks it is for, inclusive; it is only parsed when one of them is needed.
    ranges: &'static [(u32, u32)],
    /// Whether it stands in for sans-serif and monospace text rather than serif.
    sans: bool,
}

const ARABIC: &[(u32, u32)] = &[
    (0x0600, 0x06FF),
    (0x0750, 0x077F),
    (0x0870, 0x08FF),
    (0xFB50, 0xFDFF),
    (0xFE70, 0xFEFF),
];

#[cfg(feature = "cjk")]
const CJK: &[(u32, u32)] = &[
    (0x1100, 0x11FF),
    (0x2E80, 0x2FDF),
    (0x2FF0, 0x9FFF),
    (0xA960, 0xA97F),
    (0xAC00, 0xD7FF),
    (0xF900, 0xFAFF),
    (0xFE10, 0xFE1F),
    (0xFE30, 0xFE4F),
    (0xFF00, 0xFFEF),
    (0x1F200, 0x1F2FF),
    (0x20000, 0x3FFFF),
];

static SCRIPT_FONTS: &[ScriptFont] = &[
    ScriptFont {
        name: "Noto Naskh Arabic",
        data: include_bytes!("../../../fonts/NotoNaskhArabic.ttf"),
        ranges: ARABIC,
        sans: false,
    },
    ScriptFont {
        name: "Noto Sans Arabic",
        data: include_bytes!("../../../fonts/NotoSansArabic.ttf"),
        ranges: ARABIC,
        sans: true,
    },
    // Simplified Chinese glyphs, also used for Han characters in other languages. Stored as
    // WOFF2, less than half the size, and decompressed the first time it is needed.
    #[cfg(feature = "cjk")]
    ScriptFont {
        name: "Noto Sans SC",
        data: include_bytes!("../../../fonts/NotoSansSC.woff2"),
        ranges: CJK,
        sans: true,
    },
];

/// The parsed script font, shared so the PDF embeds each font once.
fn script_face(index: usize) -> Font {
    static FONTS: OnceLock<Vec<OnceLock<Font>>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| SCRIPT_FONTS.iter().map(|_| OnceLock::new()).collect());
    fonts[index]
        .get_or_init(|| {
            let data = SCRIPT_FONTS[index].data;
            match crate::font::decompress(data).expect("bundled fonts are valid") {
                Some(decompressed) => Font::new(decompressed, 0),
                None => Font::new(Cow::Borrowed(data), 0),
            }
            .expect("bundled fonts are valid")
        })
        .clone()
}

/// The bundled script font with a family name, or the full or PostScript name of its regular
/// face, without loading any of them.
fn script_font_named(name: &str) -> Option<usize> {
    SCRIPT_FONTS.iter().position(|font| {
        let postscript = font.name.replace(' ', "");
        name.eq_ignore_ascii_case(font.name)
            || name.eq_ignore_ascii_case(&format!("{} Regular", font.name))
            || name.eq_ignore_ascii_case(&format!("{postscript}-Regular"))
    })
}

/// The bundled script font for a character none of a style's fonts cover, used as the style
/// asks: the one matching the style's generic family, serif or sans-serif, comes first.
pub fn script_fallback(style: &ComputedValues, c: char) -> Option<Font> {
    let code = u32::from(c);
    let sans = style
        .font_family
        .iter()
        .find_map(family_index)
        .is_some_and(|family| family != SERIF);
    let mut candidates: Vec<usize> = (0..SCRIPT_FONTS.len())
        .filter(|&i| {
            SCRIPT_FONTS[i]
                .ranges
                .iter()
                .any(|&(lo, hi)| (lo..=hi).contains(&code))
        })
        .collect();
    candidates.sort_by_key(|&i| SCRIPT_FONTS[i].sans != sans);
    candidates
        .into_iter()
        .map(|i| (i, script_face(i)))
        .find(|(_, font)| font.covers(c))
        .map(|(i, font)| instance(style, &font, None, SCRIPT_FONTS[i].name))
}

/// The parsed face, shared so the PDF embeds each font once.
fn face(family: usize, face: usize) -> Font {
    static FONTS: OnceLock<Vec<Vec<Font>>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        FAMILIES
            .iter()
            .map(|f| {
                let mut faces: Vec<Font> = Vec::new();
                for (i, data) in f.faces.iter().enumerate() {
                    // A face listed twice is the same font.
                    let font = match f.faces[..i].iter().position(|d| std::ptr::eq(*d, *data)) {
                        Some(same) => faces[same].clone(),
                        None => {
                            Font::new(Cow::Borrowed(*data), 0).expect("bundled fonts are valid")
                        }
                    };
                    faces.push(font);
                }
                faces
            })
            .collect()
    });
    fonts[family][face].clone()
}

/// Noto Sans Regular.
pub(crate) fn default_sans() -> Font {
    face(SANS, 0)
}

/// The bundled family for a family name, if any.
fn family_index(name: &FamilyName) -> Option<usize> {
    match name {
        FamilyName::Named(name) => FAMILIES
            .iter()
            .position(|f| f.name.eq_ignore_ascii_case(name)),
        FamilyName::Generic(generic) => Some(match generic {
            GenericFamily::Serif | GenericFamily::UiSerif => SERIF,
            GenericFamily::Monospace | GenericFamily::UiMonospace => MONO,
            _ => SANS,
        }),
    }
}

/// Whether a style asks for an italic or oblique face.
fn wants_italic(style: FontStyle) -> bool {
    match style {
        FontStyle::Normal => false,
        FontStyle::Italic => true,
        FontStyle::Oblique(angle) => angle > 0.0,
    }
}

/// Picks the bundled face for a `font-family` list, weight and style. Families that are not
/// bundled are skipped; if none match, the default serif family is used.
pub fn select_font(families: &[FamilyName], weight: f32, style: FontStyle) -> Font {
    let family = families.iter().find_map(family_index).unwrap_or(SERIF);
    bundled_face(family, weight, style)
}

fn bundled_face(family: usize, weight: f32, style: FontStyle) -> Font {
    let bold = weight >= 600.0;
    face(
        family,
        usize::from(bold) + 2 * usize::from(wants_italic(style)),
    )
}

/// A face for a style and what has to be synthesized because the face lacks it.
#[derive(Debug, Clone, PartialEq)]
pub struct FontChoice {
    pub font: Font,
    /// Draw the glyphs bolder.
    pub synthetic_bold: bool,
    /// Slant the glyphs by this many degrees.
    pub synthetic_oblique: f32,
    /// Fonts to try, in order, for characters the first available font lacks.
    pub fallbacks: Vec<Font>,
}

/// The first available font of a style, with synthesis, and the fonts that follow it
/// (CSS Fonts 4 § 5 and § 7).
pub fn select(style: &ComputedValues) -> FontChoice {
    let mut fonts: Vec<Font> = Vec::new();
    for family in style.font_family.iter() {
        match family {
            FamilyName::Named(name) if style.font_rules.defines(name) => {
                fonts.extend(web_fonts(style, name));
            }
            FamilyName::Named(name) if script_font_named(name).is_some() => {
                let index = script_font_named(name).expect("matched above");
                let font = script_face(index);
                fonts.push(instance(style, &font, None, SCRIPT_FONTS[index].name));
            }
            other => {
                if let Some(index) = family_index(other) {
                    let font = bundled_face(index, style.font_weight, style.font_style);
                    fonts.push(instance(style, &font, None, family_name(other)));
                }
            }
        }
    }
    if fonts.is_empty() {
        let font = bundled_face(SERIF, style.font_weight, style.font_style);
        fonts.push(instance(style, &font, None, "Noto Serif"));
    }
    let font = fonts.remove(0);
    // After the families, the same face of the other bundled families.
    for fallback in fallbacks(&font) {
        if !fonts.iter().any(|f| f.same_file(&fallback)) {
            fonts.push(fallback);
        }
    }
    let synthetic_bold = style.font_weight >= 600.0
        && font.weight() < 600
        && style.font_synthesis_weight == Synthesis::Auto;
    let angle = match style.font_style {
        FontStyle::Normal => 0.0,
        FontStyle::Italic => 14.0,
        FontStyle::Oblique(angle) => angle,
    };
    let allowed = match style.font_synthesis_style {
        FontSynthesisStyle::Auto => true,
        FontSynthesisStyle::None => false,
        FontSynthesisStyle::ObliqueOnly => matches!(style.font_style, FontStyle::Oblique(_)),
    };
    let synthetic_oblique = if wants_italic(style.font_style) && !font.is_italic() && allowed {
        angle
    } else {
        0.0
    };
    FontChoice {
        font,
        synthetic_bold,
        synthetic_oblique,
        fallbacks: fonts,
    }
}

/// The name of a family, as `@font-feature-values` and `@font-palette-values` rules name it.
fn family_name(family: &FamilyName) -> &str {
    match family {
        FamilyName::Named(name) => name,
        FamilyName::Generic(_) => match family_index(family) {
            Some(SANS) => "Noto Sans",
            Some(MONO) => "Noto Sans Mono",
            _ => "Noto Serif",
        },
    }
}

/// Fonts parsed from `@font-face` files, by the file's data.
type LoadedFaces = Vec<(Weak<[u8]>, Option<Font>)>;

/// The font of an `@font-face` rule: its first source that loads.
fn face_font(face: &FontFace) -> Option<Font> {
    thread_local! {
        static LOADED: RefCell<LoadedFaces> = const { RefCell::new(Vec::new()) };
    }
    face.sources.iter().find_map(|source| match source {
        FontSource::Local(name) => local(name),
        FontSource::Data(data) => LOADED.with(|loaded| {
            let mut loaded = loaded.borrow_mut();
            loaded.retain(|(weak, _)| weak.strong_count() > 0);
            if let Some((_, font)) = loaded
                .iter()
                .find(|(weak, _)| std::ptr::eq(weak.as_ptr() as *const u8, data.as_ptr()))
            {
                return font.clone();
            }
            let font = Font::from_file(data, 0).ok();
            loaded.push((Arc::downgrade(data), font.clone()));
            font
        }),
    })
}

/// A bundled face by its full or PostScript name, for `local()`.
fn local(name: &str) -> Option<Font> {
    if let Some(index) = script_font_named(name) {
        return Some(script_face(index));
    }
    (0..FAMILIES.len())
        .flat_map(|family| (0..4).map(move |index| face(family, index)))
        .find(|font| {
            font.full_name().eq_ignore_ascii_case(name)
                || font.postscript_name().eq_ignore_ascii_case(name)
        })
}

/// The faces of an `@font-face` family that best match a style, as fonts: the first one
/// covers the most characters, the others the rest of a font split by `unicode-range`
/// (CSS Fonts 4 § 5.2).
fn web_fonts(style: &ComputedValues, family: &str) -> Vec<Font> {
    let faces: Vec<(&FontFace, Font)> = style
        .font_rules
        .faces
        .iter()
        .filter(|face| face.family.eq_ignore_ascii_case(family))
        .filter_map(|face| Some((face, face_font(face)?)))
        .collect();
    let Some(best) = faces
        .iter()
        .map(|(face, font)| match_key(style, face, font))
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    else {
        return Vec::new();
    };
    // Faces that match equally well; later rules come first.
    faces
        .iter()
        .rev()
        .filter(|(face, font)| match_key(style, face, font) == best)
        .map(|(face, font)| instance(style, font, Some(face), family))
        .collect()
}

/// How badly a face matches a style, by width, then style, then weight: lower is better.
/// Descriptors that are `auto` take the font file's own values.
fn match_key(style: &ComputedValues, face: &FontFace, font: &Font) -> [f32; 3] {
    let face_width = face.width.or(Some(font.width_range()));
    let face_weight = face.weight.or(Some(font.weight_range()));
    let face_style = face.style.or(Some(if font.file_italic() {
        FaceStyle::Italic
    } else {
        FaceStyle::Normal
    }));
    // A distance along an axis: zero inside the range; otherwise values on the preferred
    // side come before the others.
    let distance = |range: Option<(f32, f32)>, wanted: f32, prefer_lower: bool| -> f32 {
        let Some((lo, hi)) = range else { return 0.0 };
        if (lo..=hi).contains(&wanted) {
            0.0
        } else if wanted < lo {
            let d = lo - wanted;
            if prefer_lower { 10_000.0 + d } else { d }
        } else {
            let d = wanted - hi;
            if prefer_lower { d } else { 10_000.0 + d }
        }
    };
    let width = distance(face_width, style.font_width, style.font_width <= 100.0);
    let style_rank = match (style.font_style, face_style) {
        (_, None) => 0.0,
        (FontStyle::Normal, Some(FaceStyle::Normal)) => 0.0,
        (FontStyle::Normal, Some(FaceStyle::Oblique(..))) => 1.0,
        (FontStyle::Normal, Some(FaceStyle::Italic)) => 2.0,
        (FontStyle::Italic, Some(FaceStyle::Italic)) => 0.0,
        (FontStyle::Italic, Some(FaceStyle::Oblique(..))) => 1.0,
        (FontStyle::Italic, Some(FaceStyle::Normal)) => 2.0,
        (FontStyle::Oblique(angle), Some(FaceStyle::Oblique(lo, hi))) => {
            if (lo..=hi).contains(&angle) { 0.0 } else { 0.5 }
        }
        (FontStyle::Oblique(_), Some(FaceStyle::Italic)) => 1.0,
        (FontStyle::Oblique(_), Some(FaceStyle::Normal)) => 2.0,
    };
    // Weights from 400 to 500 look up to 500 first, lighter ones down and heavier ones up.
    let w = style.font_weight;
    let weight = match face_weight {
        None => 0.0,
        Some((lo, hi)) if (lo..=hi).contains(&w) => 0.0,
        Some((lo, hi)) if (400.0..=500.0).contains(&w) => {
            if lo > w && lo <= 500.0 {
                lo - w
            } else if hi < w {
                1_000.0 + (w - hi)
            } else {
                2_000.0 + (lo - w)
            }
        }
        Some(range) => distance(Some(range), w, w < 400.0),
    };
    [width, style_rank, weight]
}

/// A font used as a style asks: its variations, `@font-face` overrides and palette.
fn instance(style: &ComputedValues, font: &Font, face: Option<&FontFace>, family: &str) -> Font {
    let has = |tag: &[u8; 4]| font.axes().iter().any(|axis| axis.tag == *tag);
    let mut variations: Vec<([u8; 4], f32)> = Vec::new();
    let mut set = |tag: [u8; 4], value: f32| {
        variations.retain(|(t, _)| *t != tag);
        variations.push((tag, value));
    };
    // Variations from the font properties, within the ranges the face claims (§ 7.3).
    let clamp = |value: f32, range: Option<(f32, f32)>| match range {
        Some((lo, hi)) => value.clamp(lo, hi),
        None => value,
    };
    if has(b"wght") {
        set(
            *b"wght",
            clamp(style.font_weight, face.and_then(|f| f.weight)),
        );
    }
    if has(b"wdth") {
        set(
            *b"wdth",
            clamp(style.font_width, face.and_then(|f| f.width)),
        );
    }
    match style.font_style {
        FontStyle::Italic if has(b"ital") => set(*b"ital", 1.0),
        FontStyle::Italic if has(b"slnt") => set(*b"slnt", -14.0),
        FontStyle::Oblique(angle) if has(b"slnt") => set(*b"slnt", -angle),
        FontStyle::Oblique(angle) if has(b"ital") && angle > 0.0 => set(*b"ital", 1.0),
        _ => {}
    }
    if has(b"opsz") && style.font_optical_sizing == FontOpticalSizing::Auto {
        set(*b"opsz", style.font_size);
    }
    if let Some(face) = face {
        for &(tag, value) in face.variation_settings.iter() {
            set(tag, value);
        }
    }
    for &(tag, value) in style.font_variation_settings.iter() {
        set(tag, value);
    }
    let mut instance = Instance {
        variations,
        family: Arc::from(family),
        palette: palette(style, font, family),
        ..Instance::default()
    };
    if let Some(face) = face {
        instance.size_adjust = face.size_adjust;
        instance.ascent = face.ascent;
        instance.descent = face.descent;
        instance.line_gap = face.line_gap;
        if face.unicode_range != [(0, 0x10FFFF)] {
            instance.unicode_range = face.unicode_range.clone().into();
        }
        instance.features = face
            .feature_settings
            .iter()
            .map(|s| (s.tag, s.value))
            .collect();
        instance.weight = face.weight.map(|(_, heaviest)| heaviest.round() as u16);
        instance.italic = face.style.map(|style| style != FaceStyle::Normal);
    }
    font.with_instance(instance)
}

/// The colors a style draws a color font's glyphs in, or `None` for its first palette
/// (CSS Fonts 4 § 9).
fn palette(style: &ComputedValues, font: &Font, family: &str) -> Option<Arc<[Color]>> {
    if font.palettes().is_empty() || style.font_palette == FontPalette::Normal {
        return None;
    }
    Some(palette_colors(style, font, family, &style.font_palette).into())
}

fn palette_colors(
    style: &ComputedValues,
    font: &Font,
    family: &str,
    palette: &FontPalette,
) -> Vec<Color> {
    let palettes = font.palettes();
    let of_type = |bit: u32| {
        palettes
            .iter()
            .find(|(_, kind)| kind & bit != 0)
            .or(palettes.first())
            .map(|(colors, _)| colors.clone())
            .unwrap_or_default()
    };
    match palette {
        FontPalette::Normal => of_type(0),
        FontPalette::Light => of_type(1),
        FontPalette::Dark => of_type(2),
        FontPalette::Named(name) => {
            let Some(rule) = style.font_rules.palettes.iter().rev().find(|rule| {
                *rule.name == **name && rule.families.iter().any(|f| f.eq_ignore_ascii_case(family))
            }) else {
                return of_type(0);
            };
            let mut colors = match rule.base {
                None => of_type(0),
                Some(BasePalette::Light) => of_type(1),
                Some(BasePalette::Dark) => of_type(2),
                Some(BasePalette::Index(i)) => palettes
                    .get(usize::from(i))
                    .map_or_else(|| of_type(0), |(colors, _)| colors.clone()),
            };
            for (index, color) in &rule.overrides {
                if let Some(slot) = colors.get_mut(usize::from(*index)) {
                    let [r, g, b, a] = color.to_rgba8();
                    *slot = Color::rgba(r, g, b, a);
                }
            }
            colors
        }
        FontPalette::Mix {
            space,
            hue,
            palettes: parts,
        } => {
            let [(first, p1), (second, p2)] = &**parts;
            let a = palette_colors(style, font, family, first);
            let b = palette_colors(style, font, family, second);
            // Percentages as in `color-mix()`: a missing one is what the other leaves.
            let (p1, p2) = match (p1, p2) {
                (None, None) => (0.5, 0.5),
                (Some(p1), None) => (*p1, 1.0 - p1),
                (None, Some(p2)) => (1.0 - p2, *p2),
                (Some(p1), Some(p2)) => (*p1, *p2),
            };
            let sum = p1 + p2;
            let absolute = |c: &Color| {
                AbsoluteColor::srgb(
                    f32::from(c.r) / 255.0,
                    f32::from(c.g) / 255.0,
                    f32::from(c.b) / 255.0,
                    f32::from(c.a) / 255.0,
                )
            };
            a.iter()
                .zip(&b)
                .map(|(x, y)| {
                    let mut mixed = mix(*space, *hue, &absolute(x), &absolute(y), p2 / sum);
                    if sum < 1.0 {
                        mixed.alpha *= sum;
                    }
                    let [r, g, b, a] = mixed.to_rgba8();
                    Color::rgba(r, g, b, a)
                })
                .collect()
        }
    }
}

/// Fonts to try, in order, for characters the first available font lacks: the same face of
/// the other bundled families.
pub fn fallbacks(font: &Font) -> Vec<Font> {
    let mut found = None;
    for family in 0..FAMILIES.len() {
        for index in 0..4 {
            if face(family, index).same_file(font) {
                found = Some(index);
            }
        }
    }
    let index = found.unwrap_or(0);
    [SANS, SERIF, MONO]
        .into_iter()
        .map(|family| face(family, index))
        .filter(|f| !f.same_file(font))
        .collect()
}

/// Metrics of a style's first available font, for font-relative units (CSS Values 4 § 6.1.1).
pub fn font_metrics(style: &ComputedValues) -> FontMetrics {
    let font = select(style).font;
    let size = style.font_size;
    let m = font.metrics();
    let scale = |units: i16| font.scale(units, size);
    let advance =
        |units: Option<u16>| units.map(|u| font.scale(u.min(i16::MAX as u16) as i16, size));
    FontMetrics {
        size,
        x_height: scale(m.x_height),
        zero_advance: advance(m.zero_advance).unwrap_or(size * 0.5),
        cap_height: scale(m.cap_height),
        ideograph_advance: advance(m.ideograph_advance).unwrap_or(size),
        line_height: scale(m.ascender) - scale(m.descender) + scale(m.line_gap),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_bundled_faces() {
        let pick = |families: &[FamilyName], weight, style| {
            select_font(families, weight, style)
                .postscript_name()
                .to_owned()
        };
        let generic = |g| FamilyName::Generic(g);
        assert_eq!(
            pick(&[generic(GenericFamily::Serif)], 400.0, FontStyle::Normal),
            "NotoSerif-Regular"
        );
        assert_eq!(
            pick(
                &[generic(GenericFamily::SansSerif)],
                700.0,
                FontStyle::Normal
            ),
            "NotoSans-Bold"
        );
        assert_eq!(
            pick(
                &[generic(GenericFamily::SansSerif)],
                400.0,
                FontStyle::Italic
            ),
            "NotoSans-Italic"
        );
        assert_eq!(
            pick(
                &[generic(GenericFamily::Monospace)],
                700.0,
                FontStyle::Italic
            ),
            "NotoSansMono-Bold"
        );
        let named = [
            FamilyName::Named("Helvetica".into()),
            FamilyName::Named("noto sans".into()),
        ];
        assert_eq!(pick(&named, 400.0, FontStyle::Normal), "NotoSans-Regular");
        assert_eq!(
            pick(
                &[FamilyName::Named("Unknown".into())],
                400.0,
                FontStyle::Normal
            ),
            "NotoSerif-Regular"
        );
    }

    #[test]
    fn synthesizes_missing_italics() {
        let mut style = ComputedValues::initial();
        style.font_family = std::sync::Arc::from([FamilyName::Generic(GenericFamily::Monospace)]);
        style.font_style = FontStyle::Italic;
        assert_eq!(select(&style).synthetic_oblique, 14.0);
        style.font_synthesis_style = FontSynthesisStyle::None;
        assert_eq!(select(&style).synthetic_oblique, 0.0);
        style.font_family = std::sync::Arc::from([FamilyName::Generic(GenericFamily::Serif)]);
        style.font_synthesis_style = FontSynthesisStyle::Auto;
        assert_eq!(select(&style).synthetic_oblique, 0.0);
        assert!(!select(&style).synthetic_bold);
    }

    #[test]
    fn metrics_come_from_the_font() {
        let style = ComputedValues::initial();
        let metrics = font_metrics(&style);
        assert!(metrics.x_height > 6.0 && metrics.x_height < 10.0);
        assert!(metrics.zero_advance > 7.0 && metrics.zero_advance < 11.0);
        assert!(metrics.line_height > 16.0);
    }
}
