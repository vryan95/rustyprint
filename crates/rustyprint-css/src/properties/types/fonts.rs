//! The font properties of CSS Fonts 4 that select variations, alternates, emoji
//! presentation and color palettes.

use std::sync::Arc;

use cssparser::{ParseError, Parser};

use super::{Result, fail, parse_number};
use crate::properties::{ComputeContext, Property};
use crate::serialize;
use crate::values::{ColorSpace, HueMethod, parse_interpolation};

fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

fn string_css(value: &str) -> String {
    let mut out = String::new();
    let _ = cssparser::serialize_string(value, &mut out);
    out
}

/// A four-letter OpenType tag written as a string.
fn parse_tag(input: &mut Parser<'_>) -> Result<[u8; 4]> {
    let tag = input.expect_string_cloned()?;
    let bytes = tag.as_bytes();
    if bytes.len() != 4 || !bytes.iter().all(|b| (0x20..=0x7e).contains(b)) {
        return fail();
    }
    Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// `font-variation-settings`: `normal | [<string> <number>]#`. Later settings of an axis
/// win.
pub struct FontVariationSettings;

impl Property for FontVariationSettings {
    type Specified = Arc<[([u8; 4], f32)]>;
    type Computed = Arc<[([u8; 4], f32)]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "normal".into();
        }
        computed
            .iter()
            .map(|(tag, value)| {
                format!(
                    "{} {}",
                    string_css(&String::from_utf8_lossy(tag)),
                    serialize::number(f64::from(*value))
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "normal") {
            return Ok(Arc::from([]));
        }
        let settings = input.parse_comma_separated(|input| {
            let tag = parse_tag(input)?;
            Ok((tag, parse_number(input)?))
        })?;
        // One setting per axis: the last.
        let mut out: Vec<([u8; 4], f32)> = Vec::new();
        for (tag, value) in settings {
            out.retain(|(t, _)| *t != tag);
            out.push((tag, value));
        }
        Ok(out.into())
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

keywords!(
    /// `font-optical-sizing`.
    FontOpticalSizing { Auto = "auto", None = "none" }
);

keywords!(
    /// `font-variant-emoji`.
    FontVariantEmoji { Normal = "normal", Text = "text", Emoji = "emoji", Unicode = "unicode" }
);

/// `font-variant-alternates`: names of feature values from `@font-feature-values`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontVariantAlternates {
    pub stylistic: Option<Arc<str>>,
    pub historical_forms: bool,
    pub styleset: Vec<Arc<str>>,
    pub character_variant: Vec<Arc<str>>,
    pub swash: Option<Arc<str>>,
    pub ornaments: Option<Arc<str>>,
    pub annotation: Option<Arc<str>>,
}

impl FontVariantAlternates {
    pub fn is_normal(&self) -> bool {
        *self == Self::default()
    }

    /// Parses `normal | [stylistic() || historical-forms || styleset() || character-variant()
    /// || swash() || ornaments() || annotation()]`.
    pub(crate) fn parse_value(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "normal") {
            return Ok(Self::default());
        }
        let mut out = Self::default();
        let mut any = false;
        while input.try_parse(|input| out.parse_item(input)).is_ok() {
            any = true;
        }
        if any { Ok(out) } else { fail() }
    }

    /// Parses one part of the value that is not set yet: `historical-forms` or one of the
    /// functions.
    pub(crate) fn parse_item(&mut self, input: &mut Parser<'_>) -> Result<()> {
        if !self.historical_forms && keyword(input, "historical-forms") {
            self.historical_forms = true;
            return Ok(());
        }
        let name = input.expect_function()?.to_ascii_lowercase();
        let names = input.parse_nested_block(|input| {
            input.parse_comma_separated(|input| {
                Ok::<_, ParseError<()>>(Arc::from(&*input.expect_ident_cloned()?))
            })
        })?;
        let single = |names: Vec<Arc<str>>| -> Result<Arc<str>> {
            match <[Arc<str>; 1]>::try_from(names) {
                Ok([name]) => Ok(name),
                Err(_) => fail(),
            }
        };
        match name.as_str() {
            "stylistic" if self.stylistic.is_none() => self.stylistic = Some(single(names)?),
            "styleset" if self.styleset.is_empty() => self.styleset = names,
            "character-variant" if self.character_variant.is_empty() => {
                self.character_variant = names
            }
            "swash" if self.swash.is_none() => self.swash = Some(single(names)?),
            "ornaments" if self.ornaments.is_none() => self.ornaments = Some(single(names)?),
            "annotation" if self.annotation.is_none() => self.annotation = Some(single(names)?),
            _ => return fail(),
        }
        Ok(())
    }

    fn css(&self) -> String {
        if self.is_normal() {
            return "normal".into();
        }
        let list = |names: &[Arc<str>]| {
            names
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut parts = Vec::new();
        if let Some(name) = &self.stylistic {
            parts.push(format!("stylistic({name})"));
        }
        if self.historical_forms {
            parts.push("historical-forms".into());
        }
        if !self.styleset.is_empty() {
            parts.push(format!("styleset({})", list(&self.styleset)));
        }
        if !self.character_variant.is_empty() {
            parts.push(format!(
                "character-variant({})",
                list(&self.character_variant)
            ));
        }
        if let Some(name) = &self.swash {
            parts.push(format!("swash({name})"));
        }
        if let Some(name) = &self.ornaments {
            parts.push(format!("ornaments({name})"));
        }
        if let Some(name) = &self.annotation {
            parts.push(format!("annotation({name})"));
        }
        parts.join(" ")
    }
}

impl Property for FontVariantAlternates {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        computed.css()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Self::parse_value(input)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// `font-palette`: `normal | light | dark | <palette-identifier> | <palette-mix()>`.
#[derive(Debug, Clone, PartialEq)]
pub enum FontPalette {
    Normal,
    Light,
    Dark,
    /// The name of an `@font-palette-values` rule.
    Named(Arc<str>),
    /// `palette-mix()`: two palettes mixed color by color, each with its percentage.
    Mix {
        space: ColorSpace,
        hue: HueMethod,
        palettes: Arc<[(FontPalette, Option<f32>); 2]>,
    },
}

impl FontPalette {
    pub(crate) fn parse_value(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_function_matching("palette-mix"))
            .is_ok()
        {
            return input.parse_nested_block(|input| {
                input.expect_ident_matching("in")?;
                let (space, hue) = parse_interpolation(input)?;
                input.expect_comma()?;
                let part = |input: &mut Parser<'_>| -> Result<(FontPalette, Option<f32>)> {
                    let percentage = |input: &mut Parser<'_>| -> Result<f32> {
                        let p = input.expect_percentage()?;
                        if (0.0..=1.0).contains(&p) {
                            Ok(p)
                        } else {
                            fail()
                        }
                    };
                    let before = input.try_parse(percentage).ok();
                    let palette = FontPalette::parse_value(input)?;
                    let after = if before.is_none() {
                        input.try_parse(percentage).ok()
                    } else {
                        None
                    };
                    Ok((palette, before.or(after)))
                };
                let first = part(input)?;
                input.expect_comma()?;
                let second = part(input)?;
                if first.1.zip(second.1).is_some_and(|(a, b)| a + b == 0.0) {
                    return fail();
                }
                Ok(FontPalette::Mix {
                    space,
                    hue,
                    palettes: Arc::new([first, second]),
                })
            });
        }
        let ident = input.expect_ident_cloned()?;
        Ok(match &*ident.to_ascii_lowercase() {
            "normal" => FontPalette::Normal,
            "light" => FontPalette::Light,
            "dark" => FontPalette::Dark,
            _ if ident.starts_with("--") && ident.len() > 2 => {
                FontPalette::Named(Arc::from(&*ident))
            }
            _ => return fail(),
        })
    }

    fn css(&self) -> String {
        match self {
            FontPalette::Normal => "normal".into(),
            FontPalette::Light => "light".into(),
            FontPalette::Dark => "dark".into(),
            FontPalette::Named(name) => name.to_string(),
            FontPalette::Mix {
                space,
                hue,
                palettes,
            } => {
                let method = match hue {
                    HueMethod::Shorter => space.name().to_owned(),
                    other if space.hue_index().is_some() => format!(
                        "{} {} hue",
                        space.name(),
                        format!("{other:?}").to_ascii_lowercase()
                    ),
                    _ => space.name().to_owned(),
                };
                let part = |(palette, p): &(FontPalette, Option<f32>)| match p {
                    Some(p) => format!("{} {}", palette.css(), serialize::percentage(*p)),
                    None => palette.css(),
                };
                format!(
                    "palette-mix(in {method}, {}, {})",
                    part(&palettes[0]),
                    part(&palettes[1])
                )
            }
        }
    }
}

impl Property for FontPalette {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        computed.css()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Self::parse_value(input)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}
