//! Font rules (CSS Fonts 4): `@font-face` (§ 4), `@font-feature-values` (§ 6.9) and
//! `@font-palette-values` (§ 9.1).
//!
//! The rules of a document are shared by every computed style through
//! [`ComputedValues::font_rules`](crate::ComputedValues), so that whatever selects fonts
//! for a style sees the document's faces.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use crate::properties::Property;
use crate::properties::types::{FeatureSetting, FontFeatureSettings, FontVariationSettings};
use crate::values::{AbsoluteColor, Color};

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

/// Parses a whole descriptor value, or returns `None` if it is invalid.
fn parse_value<T>(value: &str, parse: impl FnOnce(&mut Parser<'_>) -> Result<T>) -> Option<T> {
    Parser::new(value).parse_entirely(parse).ok()
}

/// The document's font rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontRules {
    /// `@font-face` rules in order, with their files loaded.
    pub faces: Vec<FontFace>,
    pub feature_values: Vec<FeatureValues>,
    pub palettes: Vec<PaletteValues>,
}

impl FontRules {
    /// Whether any `@font-face` rule defines `family`.
    pub fn defines(&self, family: &str) -> bool {
        self.faces
            .iter()
            .any(|face| face.family.eq_ignore_ascii_case(family))
    }
}

/// The font rules of a document, shared by its computed styles. Two are equal when they are
/// the same rules.
#[derive(Clone, Default)]
pub struct FontRulesRef(pub Arc<FontRules>);

impl PartialEq for FontRulesRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for FontRulesRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FontRules({} faces)", self.0.faces.len())
    }
}

impl std::ops::Deref for FontRulesRef {
    type Target = FontRules;

    fn deref(&self) -> &FontRules {
        &self.0
    }
}

// ---------------------------------------------------------------------------------------------
// @font-face

/// A font file of a face: loaded data, or a `local()` font name.
#[derive(Debug, Clone, PartialEq)]
pub enum FontSource {
    Data(Arc<[u8]>),
    Local(Arc<str>),
}

/// `font-style` of a face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FaceStyle {
    Normal,
    Italic,
    /// A range of oblique angles, in degrees.
    Oblique(f32, f32),
}

/// `font-display`: how a face shows while it loads. Fonts are always loaded before layout,
/// so it has no effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontDisplay {
    Auto,
    Block,
    Swap,
    Fallback,
    Optional,
}

impl FontDisplay {
    fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "auto" => FontDisplay::Auto,
            "block" => FontDisplay::Block,
            "swap" => FontDisplay::Swap,
            "fallback" => FontDisplay::Fallback,
            "optional" => FontDisplay::Optional,
            _ => return None,
        })
    }
}

/// An `@font-face` rule.
#[derive(Debug, Clone, PartialEq)]
pub struct FontFace {
    pub family: Arc<str>,
    /// The sources that can be used, in order: files that loaded and `local()` names.
    pub sources: Vec<FontSource>,
    /// Weight range, or `None` for `auto`.
    pub weight: Option<(f32, f32)>,
    /// Width range in percent, or `None` for `auto`.
    pub width: Option<(f32, f32)>,
    /// Style, or `None` for `auto`.
    pub style: Option<FaceStyle>,
    /// Code point ranges the face is used for, inclusive.
    pub unicode_range: Vec<(u32, u32)>,
    pub feature_settings: Arc<[FeatureSetting]>,
    pub variation_settings: Arc<[([u8; 4], f32)]>,
    /// Ascent, descent and line gap as fractions of the font size.
    pub ascent: Option<f32>,
    pub descent: Option<f32>,
    pub line_gap: Option<f32>,
    /// `size-adjust` as a factor.
    pub size_adjust: f32,
    pub display: FontDisplay,
}

/// A source of `src` before it is loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceSpec {
    Url(String),
    Local(String),
}

/// Formats and technologies that can be used.
const FORMATS: &[&str] = &["truetype", "opentype", "woff", "woff2", "collection"];
const TECHNOLOGIES: &[&str] = &[
    "features-opentype",
    "variations",
    "color-colrv0",
    "color-colrv1",
    "color-sbix",
    "color-cbdt",
    "palettes",
];

/// Parses `src`: `[<url> [format(<font-format>)]? [tech(<font-tech>#)]? | local(<family-name>)]#`.
/// Sources with formats or technologies that cannot be used are left out; the value is
/// invalid if none is left.
pub fn parse_src(value: &str) -> Option<Vec<SourceSpec>> {
    let sources = parse_value(value, |input| {
        let sources = input.parse_comma_separated(|input| {
            // A source that cannot be parsed is skipped, but the others still count.
            Ok(input.try_parse(parse_source).ok().flatten())
        })?;
        Ok(sources)
    })?;
    let sources: Vec<SourceSpec> = sources.into_iter().flatten().collect();
    (!sources.is_empty()).then_some(sources)
}

/// One source, or `None` if it uses a format or technology that is not supported.
fn parse_source(input: &mut Parser<'_>) -> Result<Option<SourceSpec>> {
    if input
        .try_parse(|input| input.expect_function_matching("local"))
        .is_ok()
    {
        let name = input.parse_nested_block(|input| {
            parse_family_name(input).map_err(|_| ParseError::custom(()))
        })?;
        return Ok(Some(SourceSpec::Local(name)));
    }
    let url = input.expect_url()?.to_string();
    let mut supported = true;
    if input
        .try_parse(|input| input.expect_function_matching("format"))
        .is_ok()
    {
        let format = input.parse_nested_block(|input| {
            let format = match input.next()? {
                Token::Ident(s) | Token::QuotedString(s) => s.to_ascii_lowercase(),
                _ => return Err(ParseError::custom(())),
            };
            Ok(format)
        })?;
        // Legacy names such as `woff2-variations` add the technology to the format.
        let base = format.split('-').next().unwrap_or("").to_owned();
        supported &= FORMATS.contains(&format.as_str())
            || (FORMATS.contains(&base.as_str()) && format.ends_with("-variations"));
    }
    if input
        .try_parse(|input| input.expect_function_matching("tech"))
        .is_ok()
    {
        let techs = input.parse_nested_block(|input| {
            input.parse_comma_separated(|input| {
                Ok(input.expect_ident_cloned()?.to_ascii_lowercase())
            })
        })?;
        supported &= techs.iter().all(|t| TECHNOLOGIES.contains(&t.as_str()));
    }
    Ok(supported.then_some(SourceSpec::Url(url)))
}

/// `<family-name>`: a string, or identifiers joined by spaces.
pub fn parse_family_name(input: &mut Parser<'_>) -> Result<String> {
    if let Ok(name) = input.try_parse(|input| input.expect_string_cloned()) {
        return Ok(name.to_string());
    }
    let mut words = vec![input.expect_ident_cloned()?.to_string()];
    while let Ok(word) = input.try_parse(|input| input.expect_ident_cloned()) {
        words.push(word.to_string());
    }
    // Generic family keywords cannot name a face.
    const GENERIC: &[&str] = &[
        "serif",
        "sans-serif",
        "monospace",
        "cursive",
        "fantasy",
        "system-ui",
        "math",
        "emoji",
        "fangsong",
        "initial",
        "inherit",
        "unset",
        "default",
        "revert",
        "revert-layer",
    ];
    if words.len() == 1 && GENERIC.contains(&words[0].to_ascii_lowercase().as_str()) {
        return fail();
    }
    Ok(words.join(" "))
}

/// A font weight: `normal`, `bold` or a number from 1 to 1000.
fn parse_weight(input: &mut Parser<'_>) -> Result<f32> {
    if keyword(input, "normal") {
        return Ok(400.0);
    }
    if keyword(input, "bold") {
        return Ok(700.0);
    }
    let value = crate::properties::types::parse_number(input)?;
    if (1.0..=1000.0).contains(&value) {
        Ok(value)
    } else {
        fail()
    }
}

/// A font width: a keyword or a percentage.
fn parse_width(input: &mut Parser<'_>) -> Result<f32> {
    if let Ok(fraction) = input.try_parse(|input| input.expect_percentage()) {
        return if fraction >= 0.0 {
            Ok(fraction * 100.0)
        } else {
            fail()
        };
    }
    let keyword = input.expect_ident_cloned()?.to_ascii_lowercase();
    Ok(match keyword.as_str() {
        "ultra-condensed" => 50.0,
        "extra-condensed" => 62.5,
        "condensed" => 75.0,
        "semi-condensed" => 87.5,
        "normal" => 100.0,
        "semi-expanded" => 112.5,
        "expanded" => 125.0,
        "extra-expanded" => 150.0,
        "ultra-expanded" => 200.0,
        _ => return fail(),
    })
}

/// `auto | <x>{1,2}`, as a range with the smaller value first.
fn parse_range(
    value: &str,
    parse: fn(&mut Parser<'_>) -> Result<f32>,
) -> Option<Option<(f32, f32)>> {
    parse_value(value, |input| {
        if keyword(input, "auto") {
            return Ok(None);
        }
        let a = parse(input)?;
        let b = input.try_parse(parse).unwrap_or(a);
        Ok(Some((a.min(b), a.max(b))))
    })
}

/// An angle in degrees, for `oblique`.
fn parse_angle(input: &mut Parser<'_>) -> Result<f32> {
    match input.next()?.clone() {
        Token::Dimension { value, unit, .. } => Ok(match &*unit.to_ascii_lowercase() {
            "deg" => value,
            "rad" => value.to_degrees(),
            "grad" => value * 0.9,
            "turn" => value * 360.0,
            _ => return fail(),
        }),
        Token::Number { value: 0.0, .. } => Ok(0.0),
        _ => fail(),
    }
}

/// `font-style`: `auto | normal | italic | oblique <angle [-90deg,90deg]>{0,2}`.
fn parse_face_style(value: &str) -> Option<Option<FaceStyle>> {
    parse_value(value, |input| {
        if keyword(input, "auto") {
            return Ok(None);
        }
        if keyword(input, "normal") {
            return Ok(Some(FaceStyle::Normal));
        }
        if keyword(input, "italic") {
            return Ok(Some(FaceStyle::Italic));
        }
        input.expect_ident_matching("oblique")?;
        let Ok(a) = input.try_parse(parse_angle) else {
            return Ok(Some(FaceStyle::Oblique(14.0, 14.0)));
        };
        let b = input.try_parse(parse_angle).unwrap_or(a);
        if !(-90.0..=90.0).contains(&a) || !(-90.0..=90.0).contains(&b) {
            return fail();
        }
        Ok(Some(FaceStyle::Oblique(a.min(b), a.max(b))))
    })
}

/// `unicode-range`: `<urange>#`.
fn parse_unicode_range(value: &str) -> Option<Vec<(u32, u32)>> {
    parse_value(value, |input| {
        input.parse_comma_separated(|input| {
            let range = cssparser::UnicodeRange::parse(input)?;
            Ok((range.start, range.end))
        })
    })
}

/// `normal | <percentage [0,∞]>`, as a fraction, for the metric overrides.
fn parse_override(value: &str) -> Option<Option<f32>> {
    parse_value(value, |input| {
        if keyword(input, "normal") {
            return Ok(None);
        }
        let fraction = input.expect_percentage()?;
        if fraction >= 0.0 {
            Ok(Some(fraction))
        } else {
            fail()
        }
    })
}

/// `size-adjust`: `<percentage [0,∞]>`.
fn parse_size_adjust(value: &str) -> Option<f32> {
    parse_value(value, |input| {
        let fraction = input.expect_percentage()?;
        if fraction >= 0.0 {
            Ok(fraction)
        } else {
            fail()
        }
    })
}

/// The descriptors of an `@font-face` rule, before its sources load.
#[derive(Debug, Clone)]
pub struct FaceDescriptors {
    pub family: Option<String>,
    pub sources: Option<Vec<SourceSpec>>,
    pub face: FontFace,
}

impl Default for FaceDescriptors {
    fn default() -> Self {
        Self {
            family: None,
            sources: None,
            face: FontFace {
                family: Arc::from(""),
                sources: Vec::new(),
                weight: None,
                width: None,
                style: None,
                unicode_range: vec![(0, 0x10FFFF)],
                feature_settings: Arc::from([]),
                variation_settings: Arc::from([]),
                ascent: None,
                descent: None,
                line_gap: None,
                size_adjust: 1.0,
                display: FontDisplay::Auto,
            },
        }
    }
}

impl FaceDescriptors {
    /// Sets a descriptor, or returns a warning.
    pub fn set(&mut self, name: &str, value: &str) -> std::result::Result<(), String> {
        let invalid = || format!("invalid value for `{name}`: `{value}`");
        let face = &mut self.face;
        match name {
            "font-family" => {
                self.family = Some(parse_value(value, parse_family_name).ok_or_else(invalid)?);
            }
            "src" => self.sources = Some(parse_src(value).ok_or_else(invalid)?),
            "font-weight" => face.weight = parse_range(value, parse_weight).ok_or_else(invalid)?,
            "font-stretch" | "font-width" => {
                face.width = parse_range(value, parse_width).ok_or_else(invalid)?
            }
            "font-style" => face.style = parse_face_style(value).ok_or_else(invalid)?,
            "unicode-range" => {
                face.unicode_range = parse_unicode_range(value).ok_or_else(invalid)?
            }
            "font-feature-settings" => {
                face.feature_settings =
                    parse_value(value, FontFeatureSettings::parse).ok_or_else(invalid)?
            }
            "font-variation-settings" => {
                face.variation_settings =
                    parse_value(value, FontVariationSettings::parse).ok_or_else(invalid)?
            }
            "ascent-override" => face.ascent = parse_override(value).ok_or_else(invalid)?,
            "descent-override" => face.descent = parse_override(value).ok_or_else(invalid)?,
            "line-gap-override" => face.line_gap = parse_override(value).ok_or_else(invalid)?,
            "size-adjust" => face.size_adjust = parse_size_adjust(value).ok_or_else(invalid)?,
            "font-display" => {
                face.display = parse_value(value, |input| {
                    let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
                    FontDisplay::from_keyword(&ident).map_or_else(fail, Ok)
                })
                .ok_or_else(invalid)?
            }
            _ => return Err(format!("unknown descriptor `{name}` in @font-face")),
        }
        Ok(())
    }
}

/// Whether font data looks like a font file this renderer reads: TrueType, OpenType, a
/// collection, WOFF or WOFF2.
pub fn looks_like_font(data: &[u8]) -> bool {
    matches!(
        data.get(..4),
        Some(b"\x00\x01\x00\x00" | b"OTTO" | b"true" | b"ttcf" | b"wOFF" | b"wOF2")
    )
}

// ---------------------------------------------------------------------------------------------
// @font-feature-values

/// The kinds of feature value blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeatureBlock {
    Stylistic,
    Styleset,
    CharacterVariant,
    Swash,
    Ornaments,
    Annotation,
    HistoricalForms,
}

impl FeatureBlock {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "stylistic" => FeatureBlock::Stylistic,
            "styleset" => FeatureBlock::Styleset,
            "character-variant" => FeatureBlock::CharacterVariant,
            "swash" => FeatureBlock::Swash,
            "ornaments" => FeatureBlock::Ornaments,
            "annotation" => FeatureBlock::Annotation,
            "historical-forms" => FeatureBlock::HistoricalForms,
            _ => return None,
        })
    }

    /// Whether `values` is a valid number of values for this kind of block.
    pub fn accepts(self, values: &[u32]) -> bool {
        match self {
            FeatureBlock::Styleset => !values.is_empty() && values.iter().all(|&v| v <= 99),
            FeatureBlock::CharacterVariant => {
                (1..=2).contains(&values.len()) && (1..=99).contains(&values[0])
            }
            _ => values.len() == 1,
        }
    }
}

/// An `@font-feature-values` rule: names for feature values, by family.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FeatureValues {
    pub families: Vec<Arc<str>>,
    pub values: BTreeMap<(FeatureBlock, Arc<str>), Vec<u32>>,
}

impl FeatureValues {
    /// The values `name` stands for in blocks of `kind`, for a face of `family`. Later
    /// rules win.
    pub fn lookup<'a>(
        rules: &'a [FeatureValues],
        family: &str,
        kind: FeatureBlock,
        name: &str,
    ) -> Option<&'a [u32]> {
        rules
            .iter()
            .rev()
            .filter(|rule| rule.families.iter().any(|f| f.eq_ignore_ascii_case(family)))
            .find_map(|rule| rule.values.get(&(kind, Arc::from(name))))
            .map(Vec::as_slice)
    }
}

/// Parses a feature value declaration's value: `<integer [0,∞]>+`.
pub fn parse_feature_values(value: &str) -> Option<Vec<u32>> {
    parse_value(value, |input| {
        let mut values = Vec::new();
        while let Ok(value) = input.try_parse(|input| match input.next()? {
            Token::Number {
                int_value: Some(v), ..
            } if *v >= 0 => Ok(*v as u32),
            _ => Err(ParseError::<()>::custom(())),
        }) {
            values.push(value);
        }
        if values.is_empty() {
            fail()
        } else {
            Ok(values)
        }
    })
}

/// Parses the family names of an `@font-feature-values` or `font-family` descriptor list.
pub fn parse_family_list(input: &mut Parser<'_>) -> Result<Vec<String>> {
    input
        .parse_comma_separated(|input| parse_family_name(input).map_err(|_| ParseError::custom(())))
}

// ---------------------------------------------------------------------------------------------
// @font-palette-values

/// `base-palette`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasePalette {
    Light,
    Dark,
    Index(u16),
}

/// An `@font-palette-values` rule.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteValues {
    pub name: Arc<str>,
    pub families: Vec<Arc<str>>,
    pub base: Option<BasePalette>,
    pub overrides: Vec<(u16, AbsoluteColor)>,
}

impl PaletteValues {
    pub fn new(name: &str) -> Self {
        Self {
            name: Arc::from(name),
            families: Vec::new(),
            base: None,
            overrides: Vec::new(),
        }
    }

    /// Sets a descriptor, or returns a warning.
    pub fn set(&mut self, name: &str, value: &str) -> std::result::Result<(), String> {
        let invalid = || format!("invalid value for `{name}`: `{value}`");
        match name {
            "font-family" => {
                self.families = parse_value(value, parse_family_list)
                    .ok_or_else(invalid)?
                    .into_iter()
                    .map(Arc::from)
                    .collect();
            }
            "base-palette" => {
                self.base = Some(
                    parse_value(value, |input| {
                        if keyword(input, "light") {
                            return Ok(BasePalette::Light);
                        }
                        if keyword(input, "dark") {
                            return Ok(BasePalette::Dark);
                        }
                        match input.next()? {
                            Token::Number {
                                int_value: Some(v), ..
                            } if (0..=0xFFFF).contains(v) => Ok(BasePalette::Index(*v as u16)),
                            _ => Err(ParseError::custom(())),
                        }
                    })
                    .ok_or_else(invalid)?,
                );
            }
            "override-colors" => {
                self.overrides = parse_value(value, |input| {
                    input.parse_comma_separated(|input| {
                        let index = match input.next()? {
                            Token::Number {
                                int_value: Some(v), ..
                            } if (0..=0xFFFF).contains(v) => *v as u16,
                            _ => return Err(ParseError::custom(())),
                        };
                        // `currentcolor` and colors that depend on it are not allowed.
                        let color = Color::parse(input).map_err(|_| ParseError::custom(()))?;
                        match color {
                            Color::Absolute(color) => Ok((index, color)),
                            _ => Err(ParseError::custom(())),
                        }
                    })
                })
                .ok_or_else(invalid)?;
            }
            _ => {
                return Err(format!(
                    "unknown descriptor `{name}` in @font-palette-values"
                ));
            }
        }
        Ok(())
    }
}
