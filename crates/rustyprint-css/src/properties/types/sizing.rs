//! Value types of box sizing, margin trimming, overflow clipping and zoom.

use cssparser::{ParseError, Parser};

use super::{
    Result, SpecifiedLengthPercentage, VisualBox, fail, parse_length_percentage, parse_number,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

/// `<ratio>`: `<number [0,∞]> [ / <number [0,∞]> ]?`.
fn parse_ratio(input: &mut Parser<'_>) -> Result<(f32, f32)> {
    let width = parse_number(input)?;
    let height = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
        parse_number(input)?
    } else {
        1.0
    };
    if width < 0.0 || height < 0.0 {
        return fail();
    }
    Ok((width, height))
}

/// `aspect-ratio`: `auto || <ratio>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AspectRatio {
    /// Whether `auto` is given: replaced elements use their natural ratio, and the ratio
    /// applies to the content box.
    pub auto: bool,
    pub ratio: Option<(f32, f32)>,
}

impl AspectRatio {
    pub const AUTO: Self = AspectRatio {
        auto: true,
        ratio: None,
    };

    /// Width divided by height, if the ratio is usable: degenerate ratios behave as `auto`.
    pub fn value(&self) -> Option<f32> {
        let (w, h) = self.ratio?;
        let value = w / h;
        (value.is_finite() && value > 0.0).then_some(value)
    }
}

impl Property for AspectRatio {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let ratio = computed.ratio.map(|(w, h)| {
            format!(
                "{} / {}",
                serialize::number(f64::from(w)),
                serialize::number(f64::from(h))
            )
        });
        match (computed.auto, ratio) {
            (true, Some(ratio)) => format!("auto {ratio}"),
            (_, Some(ratio)) => ratio,
            (_, None) => "auto".into(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let auto = |input: &mut Parser<'_>| input.try_parse(|i| i.expect_ident_matching("auto"));
        let mut value = AspectRatio {
            auto: auto(input).is_ok(),
            ratio: input.try_parse(parse_ratio).ok(),
        };
        if !value.auto && value.ratio.is_some() {
            value.auto = auto(input).is_ok();
        }
        if !value.auto && value.ratio.is_none() {
            return fail();
        }
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `contain-intrinsic-width` and the other sizes used under size containment:
/// `auto? [ none | <length [0,∞]> ]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainIntrinsicSize<T> {
    /// Whether a remembered size is used when there is one.
    pub auto: bool,
    /// The size, or `None` for `none`.
    pub length: Option<T>,
}

pub struct ContainIntrinsic;

impl ContainIntrinsic {
    pub(crate) fn parse_one(
        input: &mut Parser<'_>,
    ) -> Result<ContainIntrinsicSize<SpecifiedLengthPercentage>> {
        let auto = input
            .try_parse(|input| input.expect_ident_matching("auto"))
            .is_ok();
        let length = if input
            .try_parse(|input| input.expect_ident_matching("none"))
            .is_ok()
        {
            None
        } else {
            Some(parse_length_percentage(input, false, true)?)
        };
        Ok(ContainIntrinsicSize { auto, length })
    }
}

impl Property for ContainIntrinsic {
    type Specified = ContainIntrinsicSize<SpecifiedLengthPercentage>;
    type Computed = ContainIntrinsicSize<f32>;

    fn to_css(computed: &Self::Computed) -> String {
        let length = computed.length.map_or("none".into(), serialize::px);
        if computed.auto {
            format!("auto {length}")
        } else {
            length
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        Self::parse_one(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        ContainIntrinsicSize {
            auto: specified.auto,
            length: specified
                .length
                .as_ref()
                .map(|l| l.compute(ctx, true).as_length().unwrap_or(0.0)),
        }
    }
}

keywords!(
    /// `frame-sizing`: whether an `iframe` sizes to its document's content.
    FrameSizing {
        Auto = "auto",
        ContentWidth = "content-width",
        ContentHeight = "content-height",
        ContentBlockSize = "content-block-size",
        ContentInlineSize = "content-inline-size",
    }
);

/// `margin-trim`: `none | [ block || inline ] | [ block-start || inline-start || block-end ||
/// inline-end ]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MarginTrim {
    pub block_start: bool,
    pub inline_start: bool,
    pub block_end: bool,
    pub inline_end: bool,
}

impl Property for MarginTrim {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let block = computed.block_start && computed.block_end;
        let inline = computed.inline_start && computed.inline_end;
        let mut words = Vec::new();
        let paired = |a: bool, b: bool| (a && b) || (!a && !b);
        if paired(computed.block_start, computed.block_end)
            && paired(computed.inline_start, computed.inline_end)
        {
            if block {
                words.push("block");
            }
            if inline {
                words.push("inline");
            }
        } else {
            for (on, name) in [
                (computed.block_start, "block-start"),
                (computed.inline_start, "inline-start"),
                (computed.block_end, "block-end"),
                (computed.inline_end, "inline-end"),
            ] {
                if on {
                    words.push(name);
                }
            }
        }
        if words.is_empty() {
            "none".into()
        } else {
            words.join(" ")
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let first = input.expect_ident_cloned()?.to_ascii_lowercase();
        let mut trim = MarginTrim::default();
        match first.as_str() {
            "none" => return Ok(trim),
            "block" | "inline" => {
                let set = |word: &str, trim: &mut MarginTrim| match word {
                    "block" if !trim.block_start => {
                        trim.block_start = true;
                        trim.block_end = true;
                        Ok(())
                    }
                    "inline" if !trim.inline_start => {
                        trim.inline_start = true;
                        trim.inline_end = true;
                        Ok(())
                    }
                    _ => Err(ParseError::custom(())),
                };
                set(&first, &mut trim)?;
                if let Ok(second) = input.try_parse(|input| {
                    let word = input.expect_ident_cloned()?.to_ascii_lowercase();
                    if matches!(word.as_str(), "block" | "inline") {
                        Ok(word)
                    } else {
                        fail()
                    }
                }) {
                    set(&second, &mut trim)?;
                }
            }
            _ => {
                let mut word = Some(first);
                while let Some(current) = word.take() {
                    let slot = match current.as_str() {
                        "block-start" => &mut trim.block_start,
                        "inline-start" => &mut trim.inline_start,
                        "block-end" => &mut trim.block_end,
                        "inline-end" => &mut trim.inline_end,
                        _ => return fail(),
                    };
                    if *slot {
                        return fail();
                    }
                    *slot = true;
                    word = input
                        .try_parse(|input| input.expect_ident_cloned())
                        .ok()
                        .map(|w| w.to_ascii_lowercase());
                }
            }
        }
        Ok(trim)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `overflow-clip-margin`: `<visual-box> || <length [0,∞]>`: how far outside a box
/// `overflow: clip` clips.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverflowClipMargin<T> {
    pub visual_box: VisualBox,
    pub margin: T,
}

pub struct OverflowClipMarginValue;

impl Property for OverflowClipMarginValue {
    type Specified = OverflowClipMargin<SpecifiedLengthPercentage>;
    type Computed = OverflowClipMargin<f32>;

    fn to_css(computed: &Self::Computed) -> String {
        match (computed.visual_box, computed.margin) {
            (VisualBox::PaddingBox, margin) => serialize::px(margin),
            (visual_box, 0.0) => visual_box.as_str().into(),
            (visual_box, margin) => format!("{} {}", visual_box.as_str(), serialize::px(margin)),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let mut visual_box = None;
        let mut margin = None;
        for _ in 0..2 {
            if visual_box.is_none()
                && let Ok(value) = input.try_parse(<VisualBox as Property>::parse)
            {
                visual_box = Some(value);
            } else if margin.is_none()
                && let Ok(value) =
                    input.try_parse(|input| parse_length_percentage(input, false, true))
            {
                margin = Some(value);
            } else {
                break;
            }
        }
        if visual_box.is_none() && margin.is_none() {
            return fail();
        }
        Ok(OverflowClipMargin {
            visual_box: visual_box.unwrap_or(VisualBox::PaddingBox),
            margin: margin.unwrap_or(SpecifiedLengthPercentage::Length(
                crate::values::Length::px(0.0),
            )),
        })
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        OverflowClipMargin {
            visual_box: specified.visual_box,
            margin: specified
                .margin
                .compute(ctx, true)
                .as_length()
                .unwrap_or(0.0),
        }
    }
}

/// `zoom`: `normal | <number [0,∞]> | <percentage [0,∞]>`, computed to a factor. Zero
/// behaves as `normal`.
pub struct Zoom;

impl Property for Zoom {
    type Specified = f32;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        serialize::number(f64::from(*computed))
    }

    fn parse(input: &mut Parser<'_>) -> Result<f32> {
        if input
            .try_parse(|input| input.expect_ident_matching("normal"))
            .is_ok()
        {
            return Ok(1.0);
        }
        let factor = match input.try_parse(|input| input.expect_percentage()) {
            Ok(fraction) => fraction,
            Err(_) => parse_number(input)?,
        };
        if factor < 0.0 {
            return fail();
        }
        Ok(if factor == 0.0 { 1.0 } else { factor })
    }

    fn compute(specified: &f32, _ctx: &ComputeContext) -> f32 {
        *specified
    }
}
