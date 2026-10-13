//! Colors: CSS Color 4 and 5 syntax, color spaces, `color-mix()` and gamut mapping.

// The conversion matrices are copied verbatim from the CSS Color 4 sample code.
#![allow(clippy::excessive_precision)]

use cssparser::color::{parse_hash_color, parse_named_color};
use cssparser::{ParseError, Parser, Token};

use super::{Base, CalcContext, CalcNode, NumericType, is_math_function};

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    Srgb,
    SrgbLinear,
    DisplayP3,
    A98Rgb,
    ProphotoRgb,
    Rec2020,
    XyzD50,
    XyzD65,
    Lab,
    Lch,
    Oklab,
    Oklch,
    Hsl,
    Hwb,
}

impl ColorSpace {
    /// Index of the hue component in cylindrical spaces.
    pub fn hue_index(self) -> Option<usize> {
        match self {
            ColorSpace::Hsl | ColorSpace::Hwb => Some(0),
            ColorSpace::Lch | ColorSpace::Oklch => Some(2),
            _ => None,
        }
    }

    /// Spaces named in `color()` and `color-mix(in ...)`.
    fn from_name(name: &str, allow_cylindrical: bool) -> Option<Self> {
        Some(match &*name.to_ascii_lowercase() {
            "srgb" => ColorSpace::Srgb,
            "srgb-linear" => ColorSpace::SrgbLinear,
            "display-p3" => ColorSpace::DisplayP3,
            "a98-rgb" => ColorSpace::A98Rgb,
            "prophoto-rgb" => ColorSpace::ProphotoRgb,
            "rec2020" => ColorSpace::Rec2020,
            "xyz-d50" => ColorSpace::XyzD50,
            "xyz" | "xyz-d65" => ColorSpace::XyzD65,
            "lab" if allow_cylindrical => ColorSpace::Lab,
            "lch" if allow_cylindrical => ColorSpace::Lch,
            "oklab" if allow_cylindrical => ColorSpace::Oklab,
            "oklch" if allow_cylindrical => ColorSpace::Oklch,
            "hsl" if allow_cylindrical => ColorSpace::Hsl,
            "hwb" if allow_cylindrical => ColorSpace::Hwb,
            _ => return None,
        })
    }
}

/// A color with known components. Missing components (`none`) are NaN.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsoluteColor {
    pub space: ColorSpace,
    pub components: [f32; 3],
    pub alpha: f32,
    /// The color in a device or ICC color space, which `space` and `components`
    /// approximate, for output that can use it as is.
    pub native: Option<NativeColor>,
}

/// A color in the space of a `@color-profile`, or in device CMYK, as specified.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeColor {
    /// [`DEVICE_CMYK`], or the index of the document's color profile.
    pub space: u32,
    /// The components, from 0 to 1; unused ones are 0.
    pub components: [f32; 4],
}

/// The [`NativeColor`] space of `device-cmyk()`.
pub const DEVICE_CMYK: u32 = u32::MAX;

/// A specified color.
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    Absolute(AbsoluteColor),
    /// `device-cmyk()`: kept as CMYK so the PDF backend can write it natively.
    DeviceCmyk {
        cmyk: [f32; 4],
        alpha: f32,
    },
    CurrentColor,
    /// A color that depends on its element: on `currentcolor`, such as
    /// `color-mix(in srgb, currentcolor, red)`, or on the used color scheme, such as
    /// `light-dark()` and system colors. Its source is parsed again once they are known.
    Deferred(String),
}

/// What colors that depend on their element resolve against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorContext {
    /// The value of `currentcolor`.
    pub current: AbsoluteColor,
    /// Whether the used color scheme is dark.
    pub dark: bool,
}

impl AbsoluteColor {
    pub const BLACK: Self = Self::srgb(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Self = Self::srgb(1.0, 1.0, 1.0, 1.0);
    pub const TRANSPARENT: Self = Self::srgb(0.0, 0.0, 0.0, 0.0);

    pub const fn srgb(r: f32, g: f32, b: f32, alpha: f32) -> Self {
        Self {
            space: ColorSpace::Srgb,
            components: [r, g, b],
            alpha,
            native: None,
        }
    }

    fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::srgb(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        )
    }

    /// The alpha, treating `none` as 0.
    pub fn alpha(&self) -> f32 {
        if self.alpha.is_nan() { 0.0 } else { self.alpha }
    }

    /// Converts to another color space. Missing components are treated as 0.
    pub fn to_space(&self, target: ColorSpace) -> AbsoluteColor {
        if self.space == target {
            return *self;
        }
        let components = self
            .components
            .map(|c| if c.is_nan() { 0.0 } else { f64::from(c) });
        let xyz = to_xyz_d65(self.space, components);
        let converted = from_xyz_d65(target, xyz);
        AbsoluteColor {
            space: target,
            components: converted.map(|c| c as f32),
            alpha: self.alpha,
            native: None,
        }
    }

    /// sRGB components and alpha in 0–1, gamut mapped as CSS Color 4 § 13.2 describes.
    pub fn to_srgb(&self) -> [f32; 4] {
        let [r, g, b] = gamut_map_srgb(self);
        [r as f32, g as f32, b as f32, self.alpha().clamp(0.0, 1.0)]
    }

    /// 8-bit sRGB with alpha.
    pub fn to_rgba8(&self) -> [u8; 4] {
        self.to_srgb().map(|c| (c * 255.0).round() as u8)
    }
}

impl Color {
    pub const BLACK: Self = Color::Absolute(AbsoluteColor::BLACK);

    /// Parses a `<color>`.
    pub fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let start = input.position();
        let color = parse_color(input)?;
        // Anything that needed `currentcolor` to resolve is kept as source.
        Ok(match color {
            Parsed::Deferred => Color::Deferred(input.slice_from(start).to_owned()),
            Parsed::Color(color) => color,
        })
    }

    /// Resolves the color in a light color scheme, given the value of `currentcolor`. CMYK
    /// colors are converted to sRGB; use [`Color::cmyk`] to keep them.
    pub fn resolve(&self, current: &AbsoluteColor) -> AbsoluteColor {
        self.resolve_in(&ColorContext {
            current: *current,
            dark: false,
        })
    }

    /// Resolves the color for an element.
    pub fn resolve_in(&self, context: &ColorContext) -> AbsoluteColor {
        match self {
            Color::Absolute(color) => *color,
            Color::DeviceCmyk { cmyk, alpha } => cmyk_to_srgb(*cmyk, *alpha),
            Color::CurrentColor => context.current,
            Color::Deferred(source) => {
                let mut input = Parser::new(source);
                match input.parse_entirely(|input| parse_color_with(input, Some(context))) {
                    Ok(Parsed::Color(color)) => color.resolve_in(context),
                    _ => context.current,
                }
            }
        }
    }

    /// The CMYK components and alpha of a `device-cmyk()` color.
    pub fn cmyk(&self) -> Option<([f32; 4], f32)> {
        match self {
            Color::DeviceCmyk { cmyk, alpha } => Some((*cmyk, *alpha)),
            _ => None,
        }
    }
}

/// A `device-cmyk()` color: naively converted to sRGB, with its components kept for output
/// in CMYK.
fn cmyk_to_srgb([c, m, y, k]: [f32; 4], alpha: f32) -> AbsoluteColor {
    let channel = |v: f32| 1.0 - (v * (1.0 - k) + k).min(1.0);
    let mut color = AbsoluteColor::srgb(channel(c), channel(m), channel(y), alpha);
    color.native = Some(NativeColor {
        space: DEVICE_CMYK,
        components: [c, m, y, k],
    });
    color
}

/// `color(--profile <component>+ [/ <alpha>]?)`: a color in the space of an
/// `@color-profile`, which is known once the document is styled. A profile the document
/// lacks gives black.
fn parse_profile_color(
    input: &mut Parser<'_>,
    name: &str,
    current: Option<&ColorContext>,
) -> Result<Parsed> {
    let mut components = Vec::new();
    while let Ok(value) =
        input.try_parse(|input| parse_component(input, Component::Number { percent: 1.0 }, &[]))
    {
        components.push(if value.is_nan() { 0.0 } else { value });
    }
    if components.is_empty() || components.len() > 15 {
        return fail();
    }
    let alpha = parse_alpha(input, &[], 1.0)?;
    if current.is_none() {
        return Ok(Parsed::Deferred);
    }
    let color = match crate::color_profile::convert(name, &components) {
        Some(([r, g, b], index)) => {
            let mut color = AbsoluteColor::srgb(r, g, b, alpha);
            if components.len() <= 4 {
                let mut native = [0.0; 4];
                native[..components.len()].copy_from_slice(&components);
                color.native = Some(NativeColor {
                    space: index,
                    components: native.map(|c| c.clamp(0.0, 1.0)),
                });
            }
            color
        }
        None => AbsoluteColor {
            alpha,
            ..AbsoluteColor::BLACK
        },
    };
    Ok(Parsed::Color(Color::Absolute(color)))
}

// ---------------------------------------------------------------------------------------------
// Parsing

enum Parsed {
    Color(Color),
    /// Depends on `currentcolor` or the color scheme, which are not known yet.
    Deferred,
}

fn parse_color(input: &mut Parser<'_>) -> Result<Parsed> {
    parse_color_with(input, None)
}

/// Parses a color. With `current`, `currentcolor`, `light-dark()` and system colors resolve
/// against it instead of deferring.
fn parse_color_with(input: &mut Parser<'_>, current: Option<&ColorContext>) -> Result<Parsed> {
    let absolute = |c: AbsoluteColor| Ok(Parsed::Color(Color::Absolute(c)));
    match input.next()?.clone() {
        Token::Hash(hex) | Token::IDHash(hex) => match parse_hash_color(hex.as_bytes()) {
            Ok((r, g, b, a)) => {
                let mut color = AbsoluteColor::rgb8(r, g, b);
                color.alpha = a;
                absolute(color)
            }
            Err(()) => fail(),
        },
        Token::Ident(name) => {
            let lower = name.to_ascii_lowercase();
            if lower == "currentcolor" {
                return Ok(match current {
                    Some(c) => Parsed::Color(Color::Absolute(c.current)),
                    None => Parsed::Color(Color::CurrentColor),
                });
            }
            if lower == "transparent" {
                return absolute(AbsoluteColor::TRANSPARENT);
            }
            if let Ok((r, g, b)) = parse_named_color(&lower) {
                return absolute(AbsoluteColor::rgb8(r, g, b));
            }
            match (system_color(&lower, false), current) {
                (None, _) => fail(),
                (Some(_), None) => Ok(Parsed::Deferred),
                (Some(light), Some(context)) if !context.dark => absolute(light),
                (Some(light), Some(_)) => absolute(system_color(&lower, true).unwrap_or(light)),
            }
        }
        Token::Function(name) => {
            let name = name.to_ascii_lowercase();
            input.parse_nested_block(|input| parse_color_function(input, &name, current))
        }
        _ => fail(),
    }
}

/// System colors for a light or dark color scheme. Deprecated names map to their CSS Color 4
/// replacements.
fn system_color(name: &str, dark: bool) -> Option<AbsoluteColor> {
    let rgb = |hex: u32| AbsoluteColor::rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    if dark {
        return Some(rgb(match name {
            "canvas" | "activecaption" | "appworkspace" | "background" | "inactivecaption"
            | "infobackground" | "menu" | "scrollbar" | "window" => 0x121212,
            "canvastext" | "captiontext" | "infotext" | "menutext" | "windowtext" => 0xffffff,
            "linktext" => 0x9e9eff,
            "visitedtext" => 0xd0adf0,
            "activetext" => 0xff9e9e,
            "buttonface" | "buttonhighlight" | "buttonshadow" | "threedface" => 0x6b6b6b,
            "buttontext" => 0xffffff,
            "buttonborder" | "activeborder" | "inactiveborder" | "threeddarkshadow"
            | "threedhighlight" | "threedlightshadow" | "threedshadow" | "windowframe" => 0x6b6b6b,
            "field" => 0x3b3b3b,
            "fieldtext" => 0xffffff,
            "graytext" | "inactivecaptiontext" => 0xa8a8a8,
            "highlight" => 0x99c8ff,
            "highlighttext" => 0x000000,
            "selecteditem" => 0x99c8ff,
            "selecteditemtext" => 0x3b3b3b,
            "mark" => 0x66661a,
            "marktext" => 0xffffff,
            "accentcolor" => 0x99c8ff,
            "accentcolortext" => 0x000000,
            _ => return None,
        }));
    }
    Some(rgb(match name {
        "canvas" | "activecaption" | "appworkspace" | "background" | "inactivecaption"
        | "infobackground" | "menu" | "scrollbar" | "window" => 0xffffff,
        "canvastext" | "captiontext" | "infotext" | "menutext" | "windowtext" => 0x000000,
        "linktext" => 0x0000ee,
        "visitedtext" => 0x551a8b,
        "activetext" => 0xff0000,
        "buttonface" | "buttonhighlight" | "buttonshadow" | "threedface" => 0xefefef,
        "buttontext" => 0x000000,
        "buttonborder" | "activeborder" | "inactiveborder" | "threeddarkshadow"
        | "threedhighlight" | "threedlightshadow" | "threedshadow" | "windowframe" => 0x767676,
        "field" => 0xffffff,
        "fieldtext" => 0x000000,
        "graytext" | "inactivecaptiontext" => 0x808080,
        "highlight" => 0xb4d5fe,
        "highlighttext" => 0x000000,
        "selecteditem" => 0x3875d7,
        "selecteditemtext" => 0xffffff,
        "mark" => 0xffff00,
        "marktext" => 0x000000,
        "accentcolor" => 0x0075ff,
        "accentcolortext" => 0xffffff,
        _ => return None,
    }))
}

/// How a component's percentages and numbers map to the stored value.
#[derive(Clone, Copy)]
enum Component {
    /// A number, or a percentage where 100% equals `percent`.
    Number {
        percent: f64,
    },
    Hue,
    Alpha,
}

/// Channel values of a relative color's origin, by keyword.
type Channels<'a> = &'a [(&'a str, f64)];

/// Parses one component: a number, percentage, angle (for hues), `none`, a channel keyword in
/// relative syntax, or a math function.
fn parse_component(input: &mut Parser<'_>, kind: Component, channels: Channels) -> Result<f32> {
    let percent_scale = match kind {
        Component::Number { percent } => percent,
        Component::Alpha => 1.0,
        Component::Hue => return parse_hue(input, channels),
    };
    let value = match input.next()?.clone() {
        Token::Number { value, .. } => f64::from(value),
        Token::Percentage { unit_value, .. } => f64::from(unit_value) * percent_scale,
        Token::Ident(ident) if ident.eq_ignore_ascii_case("none") => return Ok(f32::NAN),
        Token::Ident(ident) => channel(channels, &ident).ok_or_else(|| ParseError::custom(()))?,
        Token::Function(name) if is_math_function(&name) => {
            let vars = |name: &str| channel(channels, name);
            let node = CalcNode::parse_function_with(input, &name, &vars)?;
            let value = node
                .evaluate(&CalcContext::default())
                .ok_or_else(|| ParseError::custom(()))?;
            match node.numeric_type(None) {
                Some(NumericType::NUMBER) => value,
                Some(t) if t == NumericType::of(Base::Percent) => value / 100.0 * percent_scale,
                _ => return fail(),
            }
        }
        _ => return fail(),
    };
    Ok(value as f32)
}

fn channel(channels: Channels, name: &str) -> Option<f64> {
    channels
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
}

/// A hue in degrees, normalised to 0–360.
fn parse_hue(input: &mut Parser<'_>, channels: Channels) -> Result<f32> {
    let degrees = match input.next()?.clone() {
        Token::Number { value, .. } => f64::from(value),
        Token::Dimension { value, unit, .. } => {
            let unit = super::Unit::from_name(&unit).ok_or_else(|| ParseError::custom(()))?;
            CalcNode::Dimension(f64::from(value), unit)
                .numeric_type(None)
                .filter(|t| *t == NumericType::of(Base::Angle))
                .and_then(|_| {
                    CalcNode::Dimension(f64::from(value), unit).evaluate(&CalcContext::default())
                })
                .ok_or_else(|| ParseError::custom(()))?
        }
        Token::Ident(ident) if ident.eq_ignore_ascii_case("none") => return Ok(f32::NAN),
        Token::Ident(ident) => channel(channels, &ident).ok_or_else(|| ParseError::custom(()))?,
        Token::Function(name) if is_math_function(&name) => {
            let vars = |name: &str| channel(channels, name);
            let node = CalcNode::parse_function_with(input, &name, &vars)?;
            match node.numeric_type(None) {
                Some(NumericType::NUMBER) => {}
                Some(t) if t == NumericType::of(Base::Angle) => {}
                _ => return fail(),
            }
            node.evaluate(&CalcContext::default())
                .ok_or_else(|| ParseError::custom(()))?
        }
        _ => return fail(),
    };
    Ok(degrees.rem_euclid(360.0) as f32)
}

/// Parses `/ alpha` if present; otherwise returns `default`.
fn parse_alpha(input: &mut Parser<'_>, channels: Channels, default: f32) -> Result<f32> {
    if input.try_parse(|input| input.expect_delim('/')).is_ok() {
        let alpha = parse_component(input, Component::Alpha, channels)?;
        Ok(if alpha.is_nan() {
            alpha
        } else {
            alpha.clamp(0.0, 1.0)
        })
    } else {
        Ok(default)
    }
}

/// Parses `from <color>` for relative color syntax, returning the origin converted to `space`
/// and its channels; `Err(None)` means the origin needs `currentcolor`.
fn parse_origin(
    input: &mut Parser<'_>,
    space: ColorSpace,
    current: Option<&ColorContext>,
) -> Result<Option<Option<AbsoluteColor>>> {
    if input
        .try_parse(|input| input.expect_ident_matching("from"))
        .is_err()
    {
        return Ok(None);
    }
    Ok(Some(match parse_color_with(input, current)? {
        Parsed::Color(Color::Absolute(origin)) => Some(origin.to_space(space)),
        Parsed::Color(Color::DeviceCmyk { cmyk, alpha }) => {
            Some(cmyk_to_srgb(cmyk, alpha).to_space(space))
        }
        Parsed::Color(Color::CurrentColor)
        | Parsed::Color(Color::Deferred(_))
        | Parsed::Deferred => None,
    }))
}

/// Skips the rest of a function whose result depends on `currentcolor`.
fn defer(input: &mut Parser<'_>) -> Result<Parsed> {
    while input.next().is_ok() {}
    Ok(Parsed::Deferred)
}

fn parse_color_function(
    input: &mut Parser<'_>,
    name: &str,
    current: Option<&ColorContext>,
) -> Result<Parsed> {
    let done = |color: AbsoluteColor| Ok(Parsed::Color(Color::Absolute(color)));
    match name {
        "rgb" | "rgba" => parse_rgb(input, current),
        "hsl" | "hsla" => parse_hsl(input, current),
        "hwb" => {
            let Some((components, alpha)) = parse_modern(
                input,
                ColorSpace::Hwb,
                current,
                &[
                    ("h", Component::Hue),
                    ("w", Component::Number { percent: 100.0 }),
                    ("b", Component::Number { percent: 100.0 }),
                ],
            )?
            else {
                return defer(input);
            };
            done(AbsoluteColor {
                space: ColorSpace::Hwb,
                components,
                alpha,
                native: None,
            })
        }
        "lab" | "oklab" | "lch" | "oklch" => {
            let (space, lightness, chroma) = match name {
                "lab" => (ColorSpace::Lab, 100.0, 125.0),
                "oklab" => (ColorSpace::Oklab, 1.0, 0.4),
                "lch" => (ColorSpace::Lch, 100.0, 150.0),
                _ => (ColorSpace::Oklch, 1.0, 0.4),
            };
            let spec: [(&str, Component); 3] =
                if matches!(space, ColorSpace::Lab | ColorSpace::Oklab) {
                    [
                        ("l", Component::Number { percent: lightness }),
                        ("a", Component::Number { percent: chroma }),
                        ("b", Component::Number { percent: chroma }),
                    ]
                } else {
                    [
                        ("l", Component::Number { percent: lightness }),
                        ("c", Component::Number { percent: chroma }),
                        ("h", Component::Hue),
                    ]
                };
            let Some((mut components, alpha)) = parse_modern(input, space, current, &spec)? else {
                return defer(input);
            };
            components[0] = components[0].clamp(0.0, lightness as f32);
            if matches!(space, ColorSpace::Lch | ColorSpace::Oklch) && components[1] < 0.0 {
                components[1] = 0.0;
            }
            done(AbsoluteColor {
                space,
                components,
                alpha,
                native: None,
            })
        }
        "color" => {
            if let Ok(name) = input.try_parse(|input| {
                let ident = input.expect_ident_cloned()?;
                if ident.starts_with("--") {
                    Ok(ident)
                } else {
                    fail()
                }
            }) {
                return parse_profile_color(input, &name, current);
            }
            let origin = match parse_origin(input, ColorSpace::XyzD65, current)? {
                Some(None) => return defer(input),
                origin => origin.flatten(),
            };
            let space_name = input.expect_ident_cloned()?;
            let space =
                ColorSpace::from_name(&space_name, false).ok_or_else(|| ParseError::custom(()))?;
            let origin = origin.map(|o| o.to_space(space));
            let names: [&str; 3] = if matches!(space, ColorSpace::XyzD50 | ColorSpace::XyzD65) {
                ["x", "y", "z"]
            } else {
                ["r", "g", "b"]
            };
            let channels: Vec<(&str, f64)> = match &origin {
                Some(o) => names
                    .iter()
                    .zip(o.components)
                    .map(|(n, c)| (*n, f64::from(c)))
                    .chain([("alpha", f64::from(o.alpha))])
                    .collect(),
                None => Vec::new(),
            };
            let mut components = [0.0; 3];
            for component in &mut components {
                *component = parse_component(input, Component::Number { percent: 1.0 }, &channels)?;
            }
            let alpha = parse_alpha(input, &channels, origin.map_or(1.0, |o| o.alpha))?;
            done(AbsoluteColor {
                space,
                components,
                alpha,
                native: None,
            })
        }
        "color-mix" => parse_color_mix(input, current),
        "light-dark" => {
            let light = parse_color_with(input, current)?;
            input.expect_comma()?;
            let dark = parse_color_with(input, current)?;
            match current {
                Some(context) if context.dark => Ok(dark),
                Some(_) => Ok(light),
                None => Ok(Parsed::Deferred),
            }
        }
        "contrast-color" => match parse_color_with(input, current)? {
            Parsed::Color(color) => {
                let Some(color) = absolute_of(&color) else {
                    return Ok(Parsed::Deferred);
                };
                done(contrast_color(&color))
            }
            Parsed::Deferred => Ok(Parsed::Deferred),
        },
        "alpha" => {
            let Some(origin) = parse_origin(input, ColorSpace::Srgb, current)? else {
                return fail();
            };
            let Some(origin) = origin else {
                return defer(input);
            };
            let channels = [("alpha", f64::from(origin.alpha))];
            let alpha = parse_alpha(input, &channels, origin.alpha)?;
            done(AbsoluteColor { alpha, ..origin })
        }
        "device-cmyk" => {
            let percent = Component::Number { percent: 1.0 };
            let first = parse_component(input, percent, &[])?;
            let legacy = input.try_parse(|input| input.expect_comma()).is_ok();
            let mut cmyk = [first, 0.0, 0.0, 0.0];
            for (i, value) in cmyk.iter_mut().enumerate().skip(1) {
                if legacy && i > 1 {
                    input.expect_comma()?;
                }
                *value = parse_component(input, percent, &[])?;
            }
            let alpha = if legacy {
                if input.try_parse(|input| input.expect_comma()).is_ok() {
                    parse_component(input, Component::Alpha, &[])?.clamp(0.0, 1.0)
                } else {
                    1.0
                }
            } else {
                parse_alpha(input, &[], 1.0)?
            };
            let cmyk = cmyk.map(|v| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) });
            Ok(Parsed::Color(Color::DeviceCmyk { cmyk, alpha }))
        }
        _ => fail(),
    }
}

fn absolute_of(color: &Color) -> Option<AbsoluteColor> {
    match color {
        Color::Absolute(c) => Some(*c),
        Color::DeviceCmyk { cmyk, alpha } => Some(cmyk_to_srgb(*cmyk, *alpha)),
        Color::CurrentColor | Color::Deferred(_) => None,
    }
}

/// Parses the modern space-separated syntax with optional relative origin. Returns `None` if
/// the origin depends on `currentcolor`.
fn parse_modern(
    input: &mut Parser<'_>,
    space: ColorSpace,
    current: Option<&ColorContext>,
    spec: &[(&str, Component); 3],
) -> Result<Option<([f32; 3], f32)>> {
    let origin = match parse_origin(input, space, current)? {
        Some(None) => {
            while input.next().is_ok() {}
            return Ok(None);
        }
        Some(Some(origin)) => Some(origin),
        None => None,
    };
    let channels: Vec<(&str, f64)> = match &origin {
        Some(o) => spec
            .iter()
            .zip(o.components)
            .map(|((n, _), c)| (*n, f64::from(if c.is_nan() { 0.0 } else { c })))
            .chain([("alpha", f64::from(o.alpha()))])
            .collect(),
        None => Vec::new(),
    };
    let mut components = [0.0; 3];
    for (component, (_, kind)) in components.iter_mut().zip(spec) {
        *component = parse_component(input, *kind, &channels)?;
    }
    let alpha = parse_alpha(input, &channels, origin.map_or(1.0, |o| o.alpha))?;
    Ok(Some((components, alpha)))
}

fn parse_rgb(input: &mut Parser<'_>, current: Option<&ColorContext>) -> Result<Parsed> {
    // Legacy comma syntax: all numbers or all percentages, no `none`.
    if let Ok(color) = input.try_parse(|input| -> Result<AbsoluteColor> {
        let first = input.next()?.clone();
        let percentages = matches!(first, Token::Percentage { .. });
        let read = |token: Token| -> Result<f32> {
            match token {
                Token::Number { value, .. } if !percentages => Ok(value / 255.0),
                Token::Percentage { unit_value, .. } if percentages => Ok(unit_value),
                _ => fail(),
            }
        };
        let r = read(first)?;
        input.expect_comma()?;
        let g = read(input.next()?.clone())?;
        input.expect_comma()?;
        let b = read(input.next()?.clone())?;
        let alpha = if input.try_parse(|input| input.expect_comma()).is_ok() {
            parse_component(input, Component::Alpha, &[])?.clamp(0.0, 1.0)
        } else {
            1.0
        };
        input.expect_exhausted()?;
        Ok(AbsoluteColor::srgb(
            r.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            b.clamp(0.0, 1.0),
            alpha,
        ))
    }) {
        return Ok(Parsed::Color(Color::Absolute(color)));
    }
    let spec = [
        ("r", Component::Number { percent: 255.0 }),
        ("g", Component::Number { percent: 255.0 }),
        ("b", Component::Number { percent: 255.0 }),
    ];
    // Relative syntax works in 0–255 channel values.
    let origin_space = ColorSpace::Srgb;
    let origin = match parse_origin(input, origin_space, current)? {
        Some(None) => return defer(input),
        other => other.flatten(),
    };
    let channels: Vec<(&str, f64)> = match &origin {
        Some(o) => ["r", "g", "b"]
            .iter()
            .zip(o.components)
            .map(|(n, c)| (*n, f64::from(c) * 255.0))
            .chain([("alpha", f64::from(o.alpha))])
            .collect(),
        None => Vec::new(),
    };
    let mut components = [0.0f32; 3];
    for (component, (_, kind)) in components.iter_mut().zip(&spec) {
        let value = parse_component(input, *kind, &channels)?;
        *component = if value.is_nan() {
            value
        } else {
            (value / 255.0).clamp(0.0, 1.0)
        };
    }
    let alpha = parse_alpha(input, &channels, origin.map_or(1.0, |o| o.alpha))?;
    Ok(Parsed::Color(Color::Absolute(AbsoluteColor {
        space: ColorSpace::Srgb,
        components,
        alpha,
        native: None,
    })))
}

fn parse_hsl(input: &mut Parser<'_>, current: Option<&ColorContext>) -> Result<Parsed> {
    if let Ok(color) = input.try_parse(|input| -> Result<AbsoluteColor> {
        let h = parse_hue(input, &[])?;
        input.expect_comma()?;
        let s = input.expect_percentage()? * 100.0;
        input.expect_comma()?;
        let l = input.expect_percentage()? * 100.0;
        let alpha = if input.try_parse(|input| input.expect_comma()).is_ok() {
            parse_component(input, Component::Alpha, &[])?.clamp(0.0, 1.0)
        } else {
            1.0
        };
        input.expect_exhausted()?;
        if h.is_nan() {
            return fail();
        }
        Ok(AbsoluteColor {
            space: ColorSpace::Hsl,
            components: [h, s.clamp(0.0, 100.0), l.clamp(0.0, 100.0)],
            alpha,
            native: None,
        })
    }) {
        return Ok(Parsed::Color(Color::Absolute(color)));
    }
    let Some((mut components, alpha)) = parse_modern(
        input,
        ColorSpace::Hsl,
        current,
        &[
            ("h", Component::Hue),
            ("s", Component::Number { percent: 100.0 }),
            ("l", Component::Number { percent: 100.0 }),
        ],
    )?
    else {
        return defer(input);
    };
    if components[1] < 0.0 {
        components[1] = 0.0;
    }
    Ok(Parsed::Color(Color::Absolute(AbsoluteColor {
        space: ColorSpace::Hsl,
        components,
        alpha,
        native: None,
    })))
}

/// How hues interpolate in cylindrical color spaces (CSS Color 4 § 12.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HueMethod {
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

/// Parses `<color-space> [<hue-interpolation-method> hue]?`, after `in`.
pub fn parse_interpolation(input: &mut Parser<'_>) -> Result<(ColorSpace, HueMethod)> {
    let space = ColorSpace::from_name(&input.expect_ident_cloned()?, true)
        .ok_or_else(|| ParseError::custom(()))?;
    let method = if space.hue_index().is_some() {
        input
            .try_parse(|input| -> Result<HueMethod> {
                let method = match &*input.expect_ident_cloned()?.to_ascii_lowercase() {
                    "shorter" => HueMethod::Shorter,
                    "longer" => HueMethod::Longer,
                    "increasing" => HueMethod::Increasing,
                    "decreasing" => HueMethod::Decreasing,
                    _ => return fail(),
                };
                input.expect_ident_matching("hue")?;
                Ok(method)
            })
            .unwrap_or(HueMethod::Shorter)
    } else {
        HueMethod::Shorter
    };
    Ok((space, method))
}

impl ColorSpace {
    /// The name `color-mix()` and gradients use for the space.
    pub fn name(self) -> &'static str {
        match self {
            ColorSpace::Srgb => "srgb",
            ColorSpace::SrgbLinear => "srgb-linear",
            ColorSpace::DisplayP3 => "display-p3",
            ColorSpace::A98Rgb => "a98-rgb",
            ColorSpace::ProphotoRgb => "prophoto-rgb",
            ColorSpace::Rec2020 => "rec2020",
            ColorSpace::XyzD50 => "xyz-d50",
            ColorSpace::XyzD65 => "xyz-d65",
            ColorSpace::Lab => "lab",
            ColorSpace::Lch => "lch",
            ColorSpace::Oklab => "oklab",
            ColorSpace::Oklch => "oklch",
            ColorSpace::Hsl => "hsl",
            ColorSpace::Hwb => "hwb",
        }
    }

    /// Whether the space is cylindrical, with a hue.
    pub fn is_polar(self) -> bool {
        self.hue_index().is_some()
    }
}

impl HueMethod {
    pub fn name(self) -> &'static str {
        match self {
            HueMethod::Shorter => "shorter",
            HueMethod::Longer => "longer",
            HueMethod::Increasing => "increasing",
            HueMethod::Decreasing => "decreasing",
        }
    }
}

/// Interpolates from `a` to `b` by `t` in `space`, with premultiplied alpha.
pub fn interpolate(
    space: ColorSpace,
    method: HueMethod,
    a: &AbsoluteColor,
    b: &AbsoluteColor,
    t: f32,
) -> AbsoluteColor {
    mix(space, method, a, b, t)
}

fn parse_color_mix(input: &mut Parser<'_>, current: Option<&ColorContext>) -> Result<Parsed> {
    input.expect_ident_matching("in")?;
    let (space, method) = parse_interpolation(input)?;
    input.expect_comma()?;
    let mut items = input.parse_comma_separated(|input| -> Result<(Parsed, Option<f32>)> {
        let percentage = input.try_parse(|input| input.expect_percentage()).ok();
        let color = parse_color_with(input, current)?;
        let percentage =
            percentage.or_else(|| input.try_parse(|input| input.expect_percentage()).ok());
        if percentage.is_some_and(|p| !(0.0..=1.0).contains(&p)) {
            return fail();
        }
        Ok((color, percentage))
    })?;
    if items.len() != 2 {
        return fail();
    }
    let (second, p2) = items.pop().expect("two items");
    let (first, p1) = items.pop().expect("two items");
    let (Parsed::Color(first), Parsed::Color(second)) = (first, second) else {
        return Ok(Parsed::Deferred);
    };
    let (Some(first), Some(second)) = (absolute_of(&first), absolute_of(&second)) else {
        return Ok(Parsed::Deferred);
    };
    let (p1, p2) = match (p1, p2) {
        (None, None) => (0.5, 0.5),
        (Some(p1), None) => (p1, 1.0 - p1),
        (None, Some(p2)) => (1.0 - p2, p2),
        (Some(p1), Some(p2)) => (p1, p2),
    };
    let sum = p1 + p2;
    if sum == 0.0 {
        return fail();
    }
    let mut mixed = mix(space, method, &first, &second, p2 / sum);
    if sum < 1.0 {
        mixed.alpha *= sum;
    }
    Ok(Parsed::Color(Color::Absolute(mixed)))
}

/// Interpolates from `a` to `b` by `t` in `space`, with premultiplied alpha (CSS Color 4 § 12).
pub fn mix(
    space: ColorSpace,
    method: HueMethod,
    a: &AbsoluteColor,
    b: &AbsoluteColor,
    t: f32,
) -> AbsoluteColor {
    let mut a = carry_missing(a, space);
    let mut b = carry_missing(b, space);
    // A missing component takes the other color's value.
    for i in 0..3 {
        match (a.components[i].is_nan(), b.components[i].is_nan()) {
            (true, false) => a.components[i] = b.components[i],
            (false, true) => b.components[i] = a.components[i],
            _ => {}
        }
    }
    match (a.alpha.is_nan(), b.alpha.is_nan()) {
        (true, false) => a.alpha = b.alpha,
        (false, true) => b.alpha = a.alpha,
        (true, true) => {
            a.alpha = 1.0;
            b.alpha = 1.0;
        }
        _ => {}
    }
    let hue = space.hue_index();
    if let Some(h) = hue {
        let (h1, h2) = (a.components[h], b.components[h]);
        if !h1.is_nan() && !h2.is_nan() {
            let diff = h2 - h1;
            let adjusted = match method {
                HueMethod::Shorter if diff > 180.0 => h2 - 360.0,
                HueMethod::Shorter if diff < -180.0 => h2 + 360.0,
                HueMethod::Longer if (0.0..180.0).contains(&diff) => h2 - 360.0,
                HueMethod::Longer if (-180.0..=0.0).contains(&diff) => h2 + 360.0,
                HueMethod::Increasing if diff < 0.0 => h2 + 360.0,
                HueMethod::Decreasing if diff > 0.0 => h2 - 360.0,
                _ => h2,
            };
            b.components[h] = adjusted;
        }
    }
    let alpha = a.alpha + (b.alpha - a.alpha) * t;
    let components = std::array::from_fn(|i| {
        let (x, y) = (a.components[i], b.components[i]);
        if x.is_nan() {
            f32::NAN
        } else if Some(i) == hue {
            (x + (y - x) * t).rem_euclid(360.0)
        } else {
            let premultiplied = x * a.alpha + (y * b.alpha - x * a.alpha) * t;
            if alpha == 0.0 {
                premultiplied
            } else {
                premultiplied / alpha
            }
        }
    });
    AbsoluteColor {
        space,
        components,
        alpha,
        native: None,
    }
}

/// Converts to `space`, keeping components that are missing in the original when the two
/// spaces share them (such as hue between HSL and HWB).
fn carry_missing(color: &AbsoluteColor, space: ColorSpace) -> AbsoluteColor {
    let mut converted = color.to_space(space);
    if let (Some(from), Some(to)) = (color.space.hue_index(), space.hue_index())
        && color.components[from].is_nan()
    {
        converted.components[to] = f32::NAN;
    }
    // Achromatic colors have no meaningful hue.
    if let Some(h) = space.hue_index()
        && color.space.hue_index().is_none()
    {
        let chroma = match space {
            ColorSpace::Lch | ColorSpace::Oklch => converted.components[1],
            ColorSpace::Hsl => converted.components[1],
            _ => 1.0,
        };
        if chroma.abs() < 1e-4 {
            converted.components[h] = f32::NAN;
        }
    }
    converted.alpha = color.alpha;
    converted
}

/// White or black, whichever contrasts more with `color` (WCAG 2.1 contrast ratio).
fn contrast_color(color: &AbsoluteColor) -> AbsoluteColor {
    let [r, g, b, _] = color.to_srgb();
    let linear = |c: f32| f64::from(srgb_to_linear(f64::from(c)) as f32);
    let luminance = 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
    let white = 1.05 / (luminance + 0.05);
    let black = (luminance + 0.05) / 0.05;
    if white >= black {
        AbsoluteColor::WHITE
    } else {
        AbsoluteColor::BLACK
    }
}

// ---------------------------------------------------------------------------------------------
// Conversion (CSS Color 4 § 18 sample code)

type Matrix = [[f64; 3]; 3];

fn multiply(m: &Matrix, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

const SRGB_TO_XYZ: Matrix = [
    [0.41239079926595934, 0.357584339383878, 0.1804807884018343],
    [0.21263900587151027, 0.715168678767756, 0.07219231536073371],
    [0.01933081871559182, 0.11919477979462598, 0.9505321522496607],
];
const XYZ_TO_SRGB: Matrix = [
    [3.2409699419045226, -1.537383177570094, -0.4986107602930034],
    [-0.9692436362808796, 1.8759675015077202, 0.04155505740717559],
    [
        0.05563007969699366,
        -0.20397695888897652,
        1.0569715142428786,
    ],
];
const P3_TO_XYZ: Matrix = [
    [0.4865709486482162, 0.26566769316909306, 0.1982172852343625],
    [0.2289745640697488, 0.6917385218365064, 0.079286914093745],
    [0.0, 0.04511338185890264, 1.043944368900976],
];
const XYZ_TO_P3: Matrix = [
    [2.493496911941425, -0.9313836179191239, -0.40271078445071684],
    [
        -0.8294889695615747,
        1.7626640603183463,
        0.023624685841943577,
    ],
    [
        0.03584583024378447,
        -0.07617238926804182,
        0.9568845240076872,
    ],
];
const A98_TO_XYZ: Matrix = [
    [0.5766690429101305, 0.1855582379065463, 0.1882286462349947],
    [0.29734497525053605, 0.6273635662554661, 0.07529145849399788],
    [0.02703136138641234, 0.07068885253582723, 0.9913375368376388],
];
const XYZ_TO_A98: Matrix = [
    [
        2.0415879038107465,
        -0.5650069742788596,
        -0.34473135077832956,
    ],
    [-0.9692436362808795, 1.8759675015077202, 0.04155505740717557],
    [
        0.013444280632031142,
        -0.11836239223101838,
        1.0151749943912054,
    ],
];
const PROPHOTO_TO_XYZ_D50: Matrix = [
    [0.7977666449006423, 0.13518129740053308, 0.0313477341283922],
    [0.2880748288194013, 0.711835234241873, 0.00008993693872564],
    [0.0, 0.0, 0.8251046025104602],
];
const XYZ_D50_TO_PROPHOTO: Matrix = [
    [
        1.3457868816471583,
        -0.25557208737979464,
        -0.05110186497554526,
    ],
    [-0.5446307051249019, 1.5082477428451468, 0.02052744743642139],
    [0.0, 0.0, 1.2119675456389452],
];
const REC2020_TO_XYZ: Matrix = [
    [0.6369580483012914, 0.14461690358620832, 0.1688809751641721],
    [0.2627002120112671, 0.6779980715188708, 0.05930171646986196],
    [0.0, 0.028072693049087428, 1.060985057710791],
];
const XYZ_TO_REC2020: Matrix = [
    [1.716651187971268, -0.355670783776392, -0.25336628137366],
    [-0.666684351832489, 1.616481236634939, 0.0157685458139111],
    [0.017639857445311, -0.042770613257809, 0.942103121235474],
];
const D65_TO_D50: Matrix = [
    [
        1.0479297925449969,
        0.022946870601609652,
        -0.05019226628920524,
    ],
    [
        0.02962780877005599,
        0.9904344267538799,
        -0.017073799063418826,
    ],
    [
        -0.009243040646204504,
        0.015055191490298152,
        0.7518742814281371,
    ],
];
const D50_TO_D65: Matrix = [
    [0.955473421488075, -0.02309845494876471, 0.06325924320057072],
    [
        -0.0283697093338637,
        1.0099953980813041,
        0.021041441191917323,
    ],
    [
        0.012314014864481998,
        -0.020507649298898964,
        1.330365926242124,
    ],
];
const XYZ_TO_LMS: Matrix = [
    [0.8190224379967030, 0.3619062600528904, -0.1288737815209879],
    [0.0329836539323885, 0.9292868615863434, 0.0361446663506424],
    [0.0481771893596242, 0.2642395317527308, 0.6335478284694309],
];
const LMS_TO_OKLAB: Matrix = [
    [0.2104542683093140, 0.7936177747023054, -0.0040720430116193],
    [1.9779985324311684, -2.4285922420485799, 0.4505937096174110],
    [0.0259040424655478, 0.7827717124575296, -0.8086757549230774],
];
const OKLAB_TO_LMS: Matrix = [
    [1.0, 0.3963377773761749, 0.2158037573099136],
    [1.0, -0.1055613458156586, -0.0638541728258133],
    [1.0, -0.0894841775298119, -1.2914855480194092],
];
const LMS_TO_XYZ: Matrix = [
    [1.2268798758459243, -0.5578149944602171, 0.2813910456659647],
    [-0.0405757452148008, 1.1122868032803170, -0.0717110580655164],
    [-0.0763729366746601, -0.4214933324022432, 1.5869240198367816],
];
const D50_WHITE: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];

fn srgb_to_linear(c: f64) -> f64 {
    let abs = c.abs();
    if abs <= 0.04045 {
        c / 12.92
    } else {
        c.signum() * ((abs + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f64) -> f64 {
    let abs = c.abs();
    if abs > 0.0031308 {
        c.signum() * (1.055 * abs.powf(1.0 / 2.4) - 0.055)
    } else {
        12.92 * c
    }
}

fn a98_to_linear(c: f64) -> f64 {
    c.signum() * c.abs().powf(563.0 / 256.0)
}

fn linear_to_a98(c: f64) -> f64 {
    c.signum() * c.abs().powf(256.0 / 563.0)
}

fn prophoto_to_linear(c: f64) -> f64 {
    let abs = c.abs();
    if abs <= 16.0 / 512.0 {
        c / 16.0
    } else {
        c.signum() * abs.powf(1.8)
    }
}

fn linear_to_prophoto(c: f64) -> f64 {
    let abs = c.abs();
    if abs >= 1.0 / 512.0 {
        c.signum() * abs.powf(1.0 / 1.8)
    } else {
        16.0 * c
    }
}

const REC2020_ALPHA: f64 = 1.09929682680944;
const REC2020_BETA: f64 = 0.018053968510807;

fn rec2020_to_linear(c: f64) -> f64 {
    let abs = c.abs();
    if abs < REC2020_BETA * 4.5 {
        c / 4.5
    } else {
        c.signum() * ((abs + REC2020_ALPHA - 1.0) / REC2020_ALPHA).powf(1.0 / 0.45)
    }
}

fn linear_to_rec2020(c: f64) -> f64 {
    let abs = c.abs();
    if abs > REC2020_BETA {
        c.signum() * (REC2020_ALPHA * abs.powf(0.45) - (REC2020_ALPHA - 1.0))
    } else {
        4.5 * c
    }
}

fn lab_to_xyz_d50([l, a, b]: [f64; 3]) -> [f64; 3] {
    const KAPPA: f64 = 24389.0 / 27.0;
    const EPSILON: f64 = 216.0 / 24389.0;
    let f1 = (l + 16.0) / 116.0;
    let f0 = a / 500.0 + f1;
    let f2 = f1 - b / 200.0;
    let x = if f0.powi(3) > EPSILON {
        f0.powi(3)
    } else {
        (116.0 * f0 - 16.0) / KAPPA
    };
    let y = if l > KAPPA * EPSILON {
        ((l + 16.0) / 116.0).powi(3)
    } else {
        l / KAPPA
    };
    let z = if f2.powi(3) > EPSILON {
        f2.powi(3)
    } else {
        (116.0 * f2 - 16.0) / KAPPA
    };
    [x * D50_WHITE[0], y * D50_WHITE[1], z * D50_WHITE[2]]
}

fn xyz_d50_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    const KAPPA: f64 = 24389.0 / 27.0;
    const EPSILON: f64 = 216.0 / 24389.0;
    let f = |i: usize| {
        let v = xyz[i] / D50_WHITE[i];
        if v > EPSILON {
            v.cbrt()
        } else {
            (KAPPA * v + 16.0) / 116.0
        }
    };
    let (f0, f1, f2) = (f(0), f(1), f(2));
    [116.0 * f1 - 16.0, 500.0 * (f0 - f1), 200.0 * (f1 - f2)]
}

fn oklab_to_xyz(lab: [f64; 3]) -> [f64; 3] {
    multiply(&LMS_TO_XYZ, multiply(&OKLAB_TO_LMS, lab).map(|v| v.powi(3)))
}

fn xyz_to_oklab(xyz: [f64; 3]) -> [f64; 3] {
    multiply(&LMS_TO_OKLAB, multiply(&XYZ_TO_LMS, xyz).map(f64::cbrt))
}

fn polar_to_rect([l, c, h]: [f64; 3]) -> [f64; 3] {
    let h = h.to_radians();
    [l, c * h.cos(), c * h.sin()]
}

fn rect_to_polar([l, a, b]: [f64; 3]) -> [f64; 3] {
    let c = (a * a + b * b).sqrt();
    let h = b.atan2(a).to_degrees().rem_euclid(360.0);
    [l, c, h]
}

fn hsl_to_srgb([h, s, l]: [f64; 3]) -> [f64; 3] {
    let (s, l) = (s / 100.0, l / 100.0);
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [f(0.0), f(8.0), f(4.0)]
}

fn srgb_to_hsl([r, g, b]: [f64; 3]) -> [f64; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (min + max) / 2.0;
    let d = max - min;
    let (mut h, mut s) = (f64::NAN, 0.0);
    if d != 0.0 {
        s = if l == 0.0 || l == 1.0 {
            0.0
        } else {
            (max - l) / l.min(1.0 - l)
        };
        h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;
    }
    if s < 0.0 {
        h += 180.0;
        s = s.abs();
    }
    [
        if h.is_nan() { 0.0 } else { h.rem_euclid(360.0) },
        s * 100.0,
        l * 100.0,
    ]
}

fn hwb_to_srgb([h, w, b]: [f64; 3]) -> [f64; 3] {
    let (w, b) = (w / 100.0, b / 100.0);
    if w + b >= 1.0 {
        let gray = w / (w + b);
        return [gray; 3];
    }
    hsl_to_srgb([h, 100.0, 50.0]).map(|c| c * (1.0 - w - b) + w)
}

fn srgb_to_hwb(rgb: [f64; 3]) -> [f64; 3] {
    let [h, _, _] = srgb_to_hsl(rgb);
    let white = rgb[0].min(rgb[1]).min(rgb[2]);
    let black = 1.0 - rgb[0].max(rgb[1]).max(rgb[2]);
    [h, white * 100.0, black * 100.0]
}

fn to_xyz_d65(space: ColorSpace, c: [f64; 3]) -> [f64; 3] {
    match space {
        ColorSpace::Srgb => multiply(&SRGB_TO_XYZ, c.map(srgb_to_linear)),
        ColorSpace::SrgbLinear => multiply(&SRGB_TO_XYZ, c),
        ColorSpace::DisplayP3 => multiply(&P3_TO_XYZ, c.map(srgb_to_linear)),
        ColorSpace::A98Rgb => multiply(&A98_TO_XYZ, c.map(a98_to_linear)),
        ColorSpace::ProphotoRgb => multiply(
            &D50_TO_D65,
            multiply(&PROPHOTO_TO_XYZ_D50, c.map(prophoto_to_linear)),
        ),
        ColorSpace::Rec2020 => multiply(&REC2020_TO_XYZ, c.map(rec2020_to_linear)),
        ColorSpace::XyzD50 => multiply(&D50_TO_D65, c),
        ColorSpace::XyzD65 => c,
        ColorSpace::Lab => multiply(&D50_TO_D65, lab_to_xyz_d50(c)),
        ColorSpace::Lch => multiply(&D50_TO_D65, lab_to_xyz_d50(polar_to_rect(c))),
        ColorSpace::Oklab => oklab_to_xyz(c),
        ColorSpace::Oklch => oklab_to_xyz(polar_to_rect(c)),
        ColorSpace::Hsl => to_xyz_d65(ColorSpace::Srgb, hsl_to_srgb(c)),
        ColorSpace::Hwb => to_xyz_d65(ColorSpace::Srgb, hwb_to_srgb(c)),
    }
}

fn from_xyz_d65(space: ColorSpace, xyz: [f64; 3]) -> [f64; 3] {
    match space {
        ColorSpace::Srgb => multiply(&XYZ_TO_SRGB, xyz).map(linear_to_srgb),
        ColorSpace::SrgbLinear => multiply(&XYZ_TO_SRGB, xyz),
        ColorSpace::DisplayP3 => multiply(&XYZ_TO_P3, xyz).map(linear_to_srgb),
        ColorSpace::A98Rgb => multiply(&XYZ_TO_A98, xyz).map(linear_to_a98),
        ColorSpace::ProphotoRgb => {
            multiply(&XYZ_D50_TO_PROPHOTO, multiply(&D65_TO_D50, xyz)).map(linear_to_prophoto)
        }
        ColorSpace::Rec2020 => multiply(&XYZ_TO_REC2020, xyz).map(linear_to_rec2020),
        ColorSpace::XyzD50 => multiply(&D65_TO_D50, xyz),
        ColorSpace::XyzD65 => xyz,
        ColorSpace::Lab => xyz_d50_to_lab(multiply(&D65_TO_D50, xyz)),
        ColorSpace::Lch => rect_to_polar(xyz_d50_to_lab(multiply(&D65_TO_D50, xyz))),
        ColorSpace::Oklab => xyz_to_oklab(xyz),
        ColorSpace::Oklch => rect_to_polar(xyz_to_oklab(xyz)),
        ColorSpace::Hsl => srgb_to_hsl(from_xyz_d65(ColorSpace::Srgb, xyz)),
        ColorSpace::Hwb => srgb_to_hwb(from_xyz_d65(ColorSpace::Srgb, xyz)),
    }
}

/// Maps a color into the sRGB gamut by reducing OKLCH chroma (CSS Color 4 § 13.2).
fn gamut_map_srgb(color: &AbsoluteColor) -> [f64; 3] {
    const JND: f64 = 0.02;
    const EPSILON: f64 = 0.0001;
    let srgb = |c: &AbsoluteColor| c.to_space(ColorSpace::Srgb).components.map(f64::from);
    let in_gamut = |rgb: [f64; 3]| rgb.iter().all(|&c| (-EPSILON..=1.0 + EPSILON).contains(&c));
    let clip = |rgb: [f64; 3]| rgb.map(|c| c.clamp(0.0, 1.0));

    let direct = srgb(color);
    if in_gamut(direct) {
        return clip(direct);
    }
    let origin = color.to_space(ColorSpace::Oklch);
    let lightness = f64::from(origin.components[0]);
    if lightness >= 1.0 {
        return [1.0; 3];
    }
    if lightness <= 0.0 {
        return [0.0; 3];
    }
    let with_chroma = |chroma: f64| {
        let mut c = origin;
        c.components[1] = chroma as f32;
        c
    };
    let delta_eok = |a: [f64; 3], b: [f64; 3]| {
        let to_oklab = |rgb: [f64; 3]| xyz_to_oklab(to_xyz_d65(ColorSpace::Srgb, rgb));
        let (a, b) = (to_oklab(a), to_oklab(b));
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let mut clipped = clip(direct);
    if delta_eok(clipped, direct) < JND {
        return clipped;
    }
    let (mut min, mut max) = (0.0, f64::from(origin.components[1]));
    let mut min_in_gamut = true;
    while max - min > EPSILON {
        let chroma = (min + max) / 2.0;
        let current = srgb(&with_chroma(chroma));
        if min_in_gamut && in_gamut(current) {
            min = chroma;
            continue;
        }
        clipped = clip(current);
        let e = delta_eok(clipped, current);
        if e < JND {
            if JND - e < EPSILON {
                return clipped;
            }
            min_in_gamut = false;
            min = chroma;
        } else {
            max = chroma;
        }
    }
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Color {
        let mut input = Parser::new(text);
        input
            .parse_entirely(Color::parse)
            .unwrap_or_else(|_| panic!("{text:?} should parse"))
    }

    fn rgba(text: &str) -> [u8; 4] {
        parse(text)
            .resolve(&AbsoluteColor::rgb8(10, 20, 30))
            .to_rgba8()
    }

    fn invalid(text: &str) -> bool {
        let mut input = Parser::new(text);
        input.parse_entirely(Color::parse).is_err()
    }

    #[test]
    fn keywords_and_hex() {
        assert_eq!(rgba("red"), [255, 0, 0, 255]);
        assert_eq!(rgba("RebeccaPurple"), [102, 51, 153, 255]);
        assert_eq!(rgba("transparent"), [0, 0, 0, 0]);
        assert_eq!(rgba("#0f08"), [0, 255, 0, 136]);
        assert_eq!(rgba("#123456"), [0x12, 0x34, 0x56, 255]);
        assert_eq!(rgba("currentcolor"), [10, 20, 30, 255]);
        assert_eq!(rgba("Canvas"), [255, 255, 255, 255]);
        assert_eq!(rgba("LinkText"), [0, 0, 238, 255]);
        assert!(invalid("notacolor"));
        assert!(invalid("#12345"));
    }

    #[test]
    fn rgb_and_hsl() {
        assert_eq!(rgba("rgb(255, 0, 0)"), [255, 0, 0, 255]);
        assert_eq!(rgba("rgba(100%, 50%, 0%, 0.5)"), [255, 128, 0, 128]);
        assert_eq!(rgba("rgb(255 128 0 / 50%)"), [255, 128, 0, 128]);
        assert_eq!(rgba("rgb(300 -5 calc(100 + 28))"), [255, 0, 128, 255]);
        assert_eq!(rgba("rgb(none 0 0)"), [0, 0, 0, 255]);
        assert!(invalid("rgb(255, 0 0)"));
        assert!(invalid("rgb(255, 50%, 0)"));
        assert_eq!(rgba("hsl(120, 100%, 50%)"), [0, 255, 0, 255]);
        assert_eq!(rgba("hsl(0.5turn 100% 25%)"), [0, 128, 128, 255]);
        assert_eq!(rgba("hsla(240deg 100 50 / 0.25)"), [0, 0, 255, 64]);
        assert_eq!(rgba("hwb(0 0% 0%)"), [255, 0, 0, 255]);
        assert_eq!(rgba("hwb(0 60% 60%)"), [128, 128, 128, 255]);
    }

    #[test]
    fn lab_family() {
        assert_eq!(rgba("lab(100 0 0)"), [255, 255, 255, 255]);
        assert_eq!(rgba("lab(0% 0 0)"), [0, 0, 0, 255]);
        assert_eq!(rgba("oklch(62.8% 0.2577 29.23)"), [255, 0, 0, 255]);
        assert_eq!(rgba("oklab(0.628 0.2249 0.1258)"), [255, 0, 0, 255]);
        assert_eq!(rgba("lch(54.29 106.84 40.85)"), [255, 0, 0, 255]);
    }

    #[test]
    fn color_function() {
        assert_eq!(rgba("color(srgb 1 0.5 0)"), [255, 128, 0, 255]);
        assert_eq!(
            rgba("color(srgb-linear 0.21586 0.21586 0.21586)"),
            [128, 128, 128, 255]
        );
        assert_eq!(rgba("color(xyz 0.9505 1 1.089)"), [255, 255, 255, 255]);
        // Display P3 red is outside sRGB and gets gamut mapped, not clipped to pure red.
        let [r, g, b, _] = rgba("color(display-p3 1 0 0)");
        assert_eq!(r, 255);
        assert!(g < 20 && b < 20);
    }

    #[test]
    fn mixing() {
        assert_eq!(rgba("color-mix(in srgb, red, blue)"), [128, 0, 128, 255]);
        assert_eq!(rgba("color-mix(in srgb, red 25%, blue)"), [64, 0, 191, 255]);
        assert_eq!(rgba("color-mix(in srgb, red 20%, blue 20%)")[3], 102);
        assert_eq!(
            rgba("color-mix(in srgb, currentcolor, white)"),
            [133, 138, 143, 255]
        );
        assert!(matches!(
            parse("color-mix(in srgb, currentcolor, white)"),
            Color::Deferred(_)
        ));
        let [r, g, b, _] = rgba("color-mix(in oklch, red, blue)");
        assert!(r > 150 && g < 60 && b > 150, "{r} {g} {b}");
        assert!(invalid("color-mix(in srgb, red 0%, blue 0%)"));
    }

    #[test]
    fn relative_colors() {
        assert_eq!(rgba("rgb(from red r g b / 50%)"), [255, 0, 0, 128]);
        assert_eq!(rgba("rgb(from #336699 b g r)"), [0x99, 0x66, 0x33, 255]);
        assert_eq!(rgba("rgb(from red calc(r / 2) g b)"), [128, 0, 0, 255]);
        assert_eq!(rgba("hsl(from red calc(h + 120) s l)"), [0, 255, 0, 255]);
        assert_eq!(rgba("alpha(from blue / 0.5)"), [0, 0, 255, 128]);
        assert_eq!(
            rgba("rgb(from currentcolor r g b / 0.5)"),
            [10, 20, 30, 128]
        );
    }

    #[test]
    fn other_functions() {
        assert_eq!(rgba("light-dark(white, black)"), [255, 255, 255, 255]);
        assert_eq!(rgba("contrast-color(navy)"), [255, 255, 255, 255]);
        assert_eq!(rgba("contrast-color(yellow)"), [0, 0, 0, 255]);
        let cmyk = parse("device-cmyk(0 1 1 0)");
        assert_eq!(cmyk.cmyk(), Some(([0.0, 1.0, 1.0, 0.0], 1.0)));
        assert_eq!(rgba("device-cmyk(0 1 1 0)"), [255, 0, 0, 255]);
        assert_eq!(rgba("device-cmyk(0%, 0%, 0%, 100%)"), [0, 0, 0, 255]);
    }

    #[test]
    fn round_trips_between_spaces() {
        let color = AbsoluteColor::rgb8(12, 200, 99);
        for space in [
            ColorSpace::SrgbLinear,
            ColorSpace::DisplayP3,
            ColorSpace::A98Rgb,
            ColorSpace::ProphotoRgb,
            ColorSpace::Rec2020,
            ColorSpace::XyzD50,
            ColorSpace::XyzD65,
            ColorSpace::Lab,
            ColorSpace::Lch,
            ColorSpace::Oklab,
            ColorSpace::Oklch,
            ColorSpace::Hsl,
            ColorSpace::Hwb,
        ] {
            let back = color.to_space(space).to_space(ColorSpace::Srgb);
            assert_eq!(back.to_rgba8(), [12, 200, 99, 255], "{space:?}");
        }
    }
}
