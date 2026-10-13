//! Opacity (CSS Color 4 § 14), blending and isolation (Compositing and Blending 1), filters
//! (Filter Effects 1 and 2) and box shadows (CSS Backgrounds 3 § 7).

use std::sync::Arc;

use cssparser::{ParseError, Parser};

use super::gradients::parse_angle;
use super::{
    Result, Shadow, SpecifiedLengthPercentage, fail, layers, layers_css, parse_length_percentage,
    parse_number,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;
use crate::values::Color;

fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

/// `<number> | <percentage>`, as a number: `50%` is 0.5.
pub(crate) fn parse_number_percentage(input: &mut Parser<'_>) -> Result<f32> {
    if let Ok(fraction) = input.try_parse(|input| input.expect_percentage()) {
        return Ok(fraction);
    }
    parse_number(input)
}

/// `opacity`: `<opacity-value>`, clamped to `[0, 1]` when computed.
pub struct Opacity;

impl Property for Opacity {
    type Specified = f32;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        serialize::number(f64::from(*computed))
    }

    fn parse(input: &mut Parser<'_>) -> Result<f32> {
        parse_number_percentage(input)
    }

    fn compute(specified: &f32, _ctx: &ComputeContext) -> f32 {
        specified.clamp(0.0, 1.0)
    }
}

keywords!(
    /// `<blend-mode>`, and the `plus-darker` and `plus-lighter` operators of
    /// `mix-blend-mode`.
    BlendMode {
        Normal = "normal",
        Multiply = "multiply",
        Screen = "screen",
        Overlay = "overlay",
        Darken = "darken",
        Lighten = "lighten",
        ColorDodge = "color-dodge",
        ColorBurn = "color-burn",
        HardLight = "hard-light",
        SoftLight = "soft-light",
        Difference = "difference",
        Exclusion = "exclusion",
        Hue = "hue",
        Saturation = "saturation",
        Color = "color",
        Luminosity = "luminosity",
        PlusDarker = "plus-darker",
        PlusLighter = "plus-lighter",
    }
);

impl BlendMode {
    /// Whether the mode is a `<blend-mode>`, which `background-blend-mode` takes.
    fn is_blend_mode(self) -> bool {
        !matches!(self, BlendMode::PlusDarker | BlendMode::PlusLighter)
    }
}

/// `background-blend-mode`: `<blend-mode>#`.
pub struct BackgroundBlendMode;

impl Property for BackgroundBlendMode {
    type Specified = Arc<[BlendMode]>;
    type Computed = Arc<[BlendMode]>;

    fn to_css(computed: &Self::Computed) -> String {
        layers_css(computed, |mode| mode.as_str().into())
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        layers(input, |input| {
            let mode = <BlendMode as Property>::parse(input)?;
            if mode.is_blend_mode() {
                Ok(mode)
            } else {
                fail()
            }
        })
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

keywords!(
    /// `isolation`.
    Isolation { Auto = "auto", Isolate = "isolate" }
);

// ---------------------------------------------------------------------------------------------
// Filters

/// A `<filter-function>` or a `url()` reference to an SVG filter, with lengths of type `L`
/// and colors of type `C`. Amounts are numbers: `50%` is 0.5.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterFunction<L, C> {
    Blur(L),
    Brightness(f32),
    Contrast(f32),
    DropShadow {
        color: C,
        x: L,
        y: L,
        blur: L,
    },
    Grayscale(f32),
    /// Degrees.
    HueRotate(f32),
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
    Url(Arc<str>),
}

/// A computed filter function: lengths in pixels, colors absolute or `currentcolor`.
pub type Filter = FilterFunction<f32, Color>;

type SpecifiedFilter = FilterFunction<SpecifiedLengthPercentage, Option<Color>>;

fn parse_filter(input: &mut Parser<'_>) -> Result<SpecifiedFilter> {
    if let Ok(url) = input.try_parse(|input| input.expect_url().map(|u| u.to_string())) {
        return Ok(FilterFunction::Url(Arc::from(url)));
    }
    let name = input.expect_function()?.to_ascii_lowercase();
    input.parse_nested_block(|input| {
        // An amount that defaults to 1 and may not be negative.
        let amount = |input: &mut Parser<'_>| -> Result<f32> {
            match input.try_parse(parse_number_percentage) {
                Ok(value) if value >= 0.0 => Ok(value),
                Ok(_) => fail(),
                Err(_) => Ok(1.0),
            }
        };
        Ok(match name.as_str() {
            "blur" => FilterFunction::Blur(
                input
                    .try_parse(|input| parse_length_percentage(input, false, true))
                    .unwrap_or(SpecifiedLengthPercentage::Length(
                        crate::values::Length::px(0.0),
                    )),
            ),
            "brightness" => FilterFunction::Brightness(amount(input)?),
            "contrast" => FilterFunction::Contrast(amount(input)?),
            "grayscale" => FilterFunction::Grayscale(amount(input)?),
            "invert" => FilterFunction::Invert(amount(input)?),
            "opacity" => FilterFunction::Opacity(amount(input)?),
            "saturate" => FilterFunction::Saturate(amount(input)?),
            "sepia" => FilterFunction::Sepia(amount(input)?),
            "hue-rotate" => FilterFunction::HueRotate(input.try_parse(parse_angle).unwrap_or(0.0)),
            "drop-shadow" => {
                let mut color = input.try_parse(Color::parse).ok();
                let mut lengths = Vec::new();
                while lengths.len() < 3 {
                    match input.try_parse(|input| parse_length_percentage(input, false, false)) {
                        Ok(length) => lengths.push(length),
                        Err(_) => break,
                    }
                }
                if lengths.len() < 2 {
                    return fail();
                }
                if let Some(SpecifiedLengthPercentage::Length(blur)) = lengths.get(2)
                    && blur.value < 0.0
                {
                    return fail();
                }
                if color.is_none() {
                    color = input.try_parse(Color::parse).ok();
                }
                let zero = SpecifiedLengthPercentage::Length(crate::values::Length::px(0.0));
                let mut lengths = lengths.into_iter();
                FilterFunction::DropShadow {
                    color,
                    x: lengths.next().expect("two lengths"),
                    y: lengths.next().expect("two lengths"),
                    blur: lengths.next().unwrap_or(zero),
                }
            }
            _ => return fail(),
        })
    })
}

fn filter_css(filter: &Filter) -> String {
    let number = |v: f32| serialize::number(f64::from(v));
    match filter {
        FilterFunction::Blur(radius) => format!("blur({})", serialize::px(*radius)),
        FilterFunction::Brightness(v) => format!("brightness({})", number(*v)),
        FilterFunction::Contrast(v) => format!("contrast({})", number(*v)),
        FilterFunction::DropShadow { color, x, y, blur } => format!(
            "drop-shadow({} {} {} {})",
            serialize::color(color),
            serialize::px(*x),
            serialize::px(*y),
            serialize::px(*blur)
        ),
        FilterFunction::Grayscale(v) => format!("grayscale({})", number(*v)),
        FilterFunction::HueRotate(v) => format!("hue-rotate({}deg)", number(*v)),
        FilterFunction::Invert(v) => format!("invert({})", number(*v)),
        FilterFunction::Opacity(v) => format!("opacity({})", number(*v)),
        FilterFunction::Saturate(v) => format!("saturate({})", number(*v)),
        FilterFunction::Sepia(v) => format!("sepia({})", number(*v)),
        FilterFunction::Url(url) => format!("url(\"{url}\")"),
    }
}

/// `filter` and `backdrop-filter`: `none | <filter-value-list>`.
pub struct FilterList;

impl Property for FilterList {
    type Specified = Arc<[SpecifiedFilter]>;
    type Computed = Arc<[Filter]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        computed
            .iter()
            .map(filter_css)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(Arc::from([]));
        }
        let mut filters = vec![parse_filter(input)?];
        while let Ok(filter) = input.try_parse(parse_filter) {
            filters.push(filter);
        }
        Ok(filters.into())
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        let px = |v: &SpecifiedLengthPercentage| v.compute(ctx, false).as_length().unwrap_or(0.0);
        // Amounts above 100% have the effect of 100% for these.
        let at_most_one = |v: f32| v.min(1.0);
        specified
            .iter()
            .map(|filter| match filter {
                FilterFunction::Blur(radius) => FilterFunction::Blur(px(radius).max(0.0)),
                FilterFunction::Brightness(v) => FilterFunction::Brightness(*v),
                FilterFunction::Contrast(v) => FilterFunction::Contrast(*v),
                FilterFunction::DropShadow { color, x, y, blur } => FilterFunction::DropShadow {
                    color: shadow_color(color, ctx),
                    x: px(x),
                    y: px(y),
                    blur: px(blur).max(0.0),
                },
                FilterFunction::Grayscale(v) => FilterFunction::Grayscale(at_most_one(*v)),
                FilterFunction::HueRotate(v) => FilterFunction::HueRotate(*v),
                FilterFunction::Invert(v) => FilterFunction::Invert(at_most_one(*v)),
                FilterFunction::Opacity(v) => FilterFunction::Opacity(at_most_one(*v)),
                FilterFunction::Saturate(v) => FilterFunction::Saturate(*v),
                FilterFunction::Sepia(v) => FilterFunction::Sepia(at_most_one(*v)),
                FilterFunction::Url(url) => FilterFunction::Url(url.clone()),
            })
            .collect()
    }
}

/// A shadow's color: `currentcolor` when it has none.
fn shadow_color(color: &Option<Color>, ctx: &ComputeContext) -> Color {
    match color {
        Some(Color::Deferred(_)) | None => Color::Absolute(
            color
                .as_ref()
                .map_or(ctx.color, |c| c.resolve_in(&ctx.colors())),
        ),
        Some(other) => other.clone(),
    }
}

// ---------------------------------------------------------------------------------------------
// Box shadows

/// One shadow of `box-shadow`.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadowValue {
    pub shadow: Shadow,
    pub spread: f32,
    pub inset: bool,
}

/// `box-shadow`: `none | <shadow>#`.
pub struct BoxShadow;

type SpecifiedBoxShadow = (Option<Color>, Vec<SpecifiedLengthPercentage>, bool);

impl Property for BoxShadow {
    type Specified = Arc<[SpecifiedBoxShadow]>;
    type Computed = Arc<[BoxShadowValue]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        layers_css(computed, |s| {
            format!(
                "{} {} {} {} {}{}",
                serialize::color(&s.shadow.color),
                serialize::px(s.shadow.x),
                serialize::px(s.shadow.y),
                serialize::px(s.shadow.blur),
                serialize::px(s.spread),
                if s.inset { " inset" } else { "" }
            )
        })
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(Arc::from([]));
        }
        layers(input, |input| {
            let (mut color, mut lengths, mut inset) = (None, Vec::new(), false);
            loop {
                if !inset && keyword(input, "inset") {
                    inset = true;
                } else if color.is_none()
                    && let Ok(value) = input.try_parse(Color::parse)
                {
                    color = Some(value);
                } else if lengths.is_empty() {
                    while lengths.len() < 4 {
                        match input.try_parse(|input| parse_length_percentage(input, false, false))
                        {
                            Ok(length) => lengths.push(length),
                            Err(_) => break,
                        }
                    }
                    if lengths.len() < 2 {
                        return fail();
                    }
                } else {
                    break;
                }
            }
            if lengths.is_empty() {
                return fail();
            }
            if let Some(SpecifiedLengthPercentage::Length(blur)) = lengths.get(2)
                && blur.value < 0.0
            {
                return Err(ParseError::custom(()));
            }
            Ok((color, lengths, inset))
        })
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        let px = |v: &SpecifiedLengthPercentage| v.compute(ctx, false).as_length().unwrap_or(0.0);
        specified
            .iter()
            .map(|(color, lengths, inset)| BoxShadowValue {
                shadow: Shadow {
                    color: shadow_color(color, ctx),
                    x: px(&lengths[0]),
                    y: px(&lengths[1]),
                    blur: lengths.get(2).map_or(0.0, |b| px(b).max(0.0)),
                },
                spread: lengths.get(3).map_or(0.0, px),
                inset: *inset,
            })
            .collect()
    }
}
