//! Value types of CSS Backgrounds and Borders 3 and of outlines (CSS UI 4 § 3).

use std::sync::Arc;

use cssparser::{ParseError, Parser};

use super::{
    BorderStyle, ImageValue, LengthPercentage, Result, SpecifiedLengthPercentage, fail, layers,
    layers_css, length_percentage_css, parse_length_percentage,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;
use crate::values::Color;

// ---------------------------------------------------------------------------------------------
// Border radius

/// `border-*-radius`: a horizontal and a vertical radius.
pub struct BorderRadius;

/// Horizontal and vertical radii of one corner.
pub type CornerRadius = (LengthPercentage, LengthPercentage);

impl Property for BorderRadius {
    type Specified = (SpecifiedLengthPercentage, SpecifiedLengthPercentage);
    type Computed = CornerRadius;

    fn to_css(computed: &Self::Computed) -> String {
        let (h, v) = computed;
        if h == v {
            length_percentage_css(h)
        } else {
            format!("{} {}", length_percentage_css(h), length_percentage_css(v))
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let h = parse_length_percentage(input, true, true)?;
        let v = input
            .try_parse(|input| parse_length_percentage(input, true, true))
            .unwrap_or_else(|_| h.clone());
        Ok((h, v))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        (
            specified.0.compute(ctx, true),
            specified.1.compute(ctx, true),
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Outlines

keywords!(
    /// `outline-style`: `auto` or a border style other than `hidden`.
    OutlineStyle {
        Auto = "auto",
        None = "none",
        Dotted = "dotted",
        Dashed = "dashed",
        Solid = "solid",
        Double = "double",
        Groove = "groove",
        Ridge = "ridge",
        Inset = "inset",
        Outset = "outset",
    }
);

impl OutlineStyle {
    /// The border style the outline is drawn with. `auto` draws a solid line.
    pub fn border_style(self) -> BorderStyle {
        match self {
            OutlineStyle::Auto | OutlineStyle::Solid => BorderStyle::Solid,
            OutlineStyle::None => BorderStyle::None,
            OutlineStyle::Dotted => BorderStyle::Dotted,
            OutlineStyle::Dashed => BorderStyle::Dashed,
            OutlineStyle::Double => BorderStyle::Double,
            OutlineStyle::Groove => BorderStyle::Groove,
            OutlineStyle::Ridge => BorderStyle::Ridge,
            OutlineStyle::Inset => BorderStyle::Inset,
            OutlineStyle::Outset => BorderStyle::Outset,
        }
    }
}

/// `outline-color`: `auto | <color>`. `auto` (`None`) draws in `currentcolor`.
pub struct OutlineColor;

impl Property for OutlineColor {
    type Specified = Option<Color>;
    type Computed = Option<Color>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "auto".into(),
            Some(color) => serialize::color(color),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if input
            .try_parse(|input| input.expect_ident_matching("auto"))
            .is_ok()
        {
            return Ok(None);
        }
        Ok(Some(Color::parse(input)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified
            .as_ref()
            .map(|color| <super::ColorValue as Property>::compute(color, ctx))
    }
}

/// A `<length>` computed to pixels, which may be negative (`outline-offset`).
pub struct LengthValue;

impl Property for LengthValue {
    type Specified = SpecifiedLengthPercentage;
    type Computed = f32;

    fn to_css(computed: &Self::Computed) -> String {
        serialize::px(*computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_length_percentage(input, false, false)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.compute(ctx, false).as_length().unwrap_or(0.0)
    }
}

// ---------------------------------------------------------------------------------------------
// Background layers

/// `background-image`: `<bg-image>#`.
pub struct BackgroundImage;

impl Property for BackgroundImage {
    type Specified = Arc<[ImageValue]>;
    type Computed = Arc<[ImageValue]>;

    fn to_css(computed: &Self::Computed) -> String {
        layers_css(computed, ImageValue::to_css)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        layers(input, ImageValue::parse)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.iter().map(|image| image.compute(ctx)).collect()
    }
}

keywords!(
    /// `background-attachment` of one layer.
    Attachment { Scroll = "scroll", Fixed = "fixed", Local = "local" }
);

keywords!(
    /// `<visual-box>`: which box a background layer is positioned in.
    VisualBox { BorderBox = "border-box", PaddingBox = "padding-box", ContentBox = "content-box" }
);

keywords!(
    /// `background-clip` of one layer.
    BackgroundClip {
        BorderBox = "border-box",
        PaddingBox = "padding-box",
        ContentBox = "content-box",
        Text = "text",
        BorderArea = "border-area",
    }
);

keywords!(
    /// How a background image repeats along one axis.
    Repeat { Repeat = "repeat", Space = "space", Round = "round", NoRepeat = "no-repeat" }
);

keyword_layers!(
    /// `background-attachment`: `<attachment>#`.
    BackgroundAttachment: Attachment
);
keyword_layers!(
    /// `background-origin`: `<visual-box>#`.
    BackgroundOrigin: VisualBox
);
keyword_layers!(
    /// `background-clip`: `<bg-clip>#`.
    BackgroundClipList: BackgroundClip
);

/// `background-repeat`: `<repeat-style>#`, as horizontal and vertical repetition.
pub struct BackgroundRepeat;

impl BackgroundRepeat {
    pub(crate) fn parse_one(input: &mut Parser<'_>) -> Result<(Repeat, Repeat)> {
        let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
        match ident.as_str() {
            "repeat-x" => return Ok((Repeat::Repeat, Repeat::NoRepeat)),
            "repeat-y" => return Ok((Repeat::NoRepeat, Repeat::Repeat)),
            _ => {}
        }
        let x = Repeat::from_keyword(&ident).ok_or_else(|| ParseError::custom(()))?;
        let y = input.try_parse(<Repeat as Property>::parse).unwrap_or(x);
        Ok((x, y))
    }
}

impl Property for BackgroundRepeat {
    type Specified = Arc<[(Repeat, Repeat)]>;
    type Computed = Arc<[(Repeat, Repeat)]>;

    fn to_css(computed: &Self::Computed) -> String {
        layers_css(computed, |&(x, y)| match (x, y) {
            (Repeat::Repeat, Repeat::NoRepeat) => "repeat-x".into(),
            (Repeat::NoRepeat, Repeat::Repeat) => "repeat-y".into(),
            (x, y) if x == y => x.as_str().into(),
            (x, y) => format!("{} {}", x.as_str(), y.as_str()),
        })
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        layers(input, Self::parse_one)
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

/// `<bg-size>`: the size of one layer's image.
#[derive(Debug, Clone, PartialEq)]
pub enum BackgroundSize<T> {
    Cover,
    Contain,
    /// Width and height; `None` is `auto`.
    Explicit(Option<T>, Option<T>),
}

impl BackgroundSize<SpecifiedLengthPercentage> {
    pub(crate) fn parse_one(input: &mut Parser<'_>) -> Result<Self> {
        if let Ok(keyword) = input.try_parse(|input| {
            let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
            match ident.as_str() {
                "cover" => Ok(BackgroundSize::Cover),
                "contain" => Ok(BackgroundSize::Contain),
                _ => fail(),
            }
        }) {
            return Ok(keyword);
        }
        let component = |input: &mut Parser<'_>| -> Result<Option<SpecifiedLengthPercentage>> {
            if input
                .try_parse(|input| input.expect_ident_matching("auto"))
                .is_ok()
            {
                return Ok(None);
            }
            Ok(Some(parse_length_percentage(input, true, true)?))
        };
        let width = component(input)?;
        let height = input.try_parse(component).unwrap_or(None);
        Ok(BackgroundSize::Explicit(width, height))
    }
}

/// `background-size`: `<bg-size>#`.
pub struct BackgroundSizeList;

impl Property for BackgroundSizeList {
    type Specified = Arc<[BackgroundSize<SpecifiedLengthPercentage>]>;
    type Computed = Arc<[BackgroundSize<LengthPercentage>]>;

    fn to_css(computed: &Self::Computed) -> String {
        let auto = |value: &Option<LengthPercentage>| match value {
            None => "auto".to_owned(),
            Some(value) => length_percentage_css(value),
        };
        layers_css(computed, |size| match size {
            BackgroundSize::Cover => "cover".into(),
            BackgroundSize::Contain => "contain".into(),
            BackgroundSize::Explicit(width, None) => auto(width),
            BackgroundSize::Explicit(width, height) => format!("{} {}", auto(width), auto(height)),
        })
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        layers(input, BackgroundSize::parse_one)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified
            .iter()
            .map(|size| match size {
                BackgroundSize::Cover => BackgroundSize::Cover,
                BackgroundSize::Contain => BackgroundSize::Contain,
                BackgroundSize::Explicit(width, height) => BackgroundSize::Explicit(
                    width.as_ref().map(|w| w.compute(ctx, true)),
                    height.as_ref().map(|h| h.compute(ctx, true)),
                ),
            })
            .collect()
    }
}

/// Where a position along one axis is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionEdge {
    Center,
    /// Left or top.
    Start,
    /// Right or bottom.
    End,
    /// `x-start` or `y-start`: the start of the axis in the element's direction.
    LogicalStart,
    LogicalEnd,
}

/// A specified position along one axis: an edge keyword with an optional offset from it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecifiedPosition {
    pub edge: PositionEdge,
    pub offset: Option<SpecifiedLengthPercentage>,
}

/// A computed position along one axis: an offset from the start edge, or from the end edge.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionComponent {
    pub from_end: bool,
    pub offset: LengthPercentage,
}

impl PositionComponent {
    pub const START: Self = PositionComponent {
        from_end: false,
        offset: LengthPercentage::Percentage(0.0),
    };

    /// The offset from the start edge, where `free` is the space left over by the image:
    /// percentages align that fraction of the image with that fraction of the area.
    pub fn resolve(&self, free: f32) -> f32 {
        let offset = self.offset.resolve(free);
        if self.from_end { free - offset } else { offset }
    }
}

impl SpecifiedPosition {
    pub(crate) fn compute(&self, ctx: &ComputeContext, horizontal: bool) -> PositionComponent {
        // `x-start` follows the inline direction on the horizontal axis.
        let reversed = horizontal && ctx.rtl;
        let edge = match self.edge {
            PositionEdge::LogicalStart if reversed => PositionEdge::End,
            PositionEdge::LogicalEnd if reversed => PositionEdge::Start,
            PositionEdge::LogicalStart => PositionEdge::Start,
            PositionEdge::LogicalEnd => PositionEdge::End,
            other => other,
        };
        let offset = self.offset.as_ref().map(|o| o.compute(ctx, false));
        let percent = LengthPercentage::Percentage;
        match (edge, offset) {
            (PositionEdge::Center, _) => PositionComponent {
                from_end: false,
                offset: percent(0.5),
            },
            (PositionEdge::End, None) => PositionComponent {
                from_end: false,
                offset: percent(1.0),
            },
            (edge, offset) => PositionComponent {
                from_end: edge == PositionEdge::End,
                offset: offset.unwrap_or(percent(0.0)),
            },
        }
    }
}

/// A position keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PositionKeyword {
    Left,
    Right,
    Top,
    Bottom,
    Center,
    XStart,
    XEnd,
    YStart,
    YEnd,
}

impl PositionKeyword {
    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Ok(match &*input.expect_ident()?.to_ascii_lowercase() {
            "left" => PositionKeyword::Left,
            "right" => PositionKeyword::Right,
            "top" => PositionKeyword::Top,
            "bottom" => PositionKeyword::Bottom,
            "center" => PositionKeyword::Center,
            "x-start" => PositionKeyword::XStart,
            "x-end" => PositionKeyword::XEnd,
            "y-start" => PositionKeyword::YStart,
            "y-end" => PositionKeyword::YEnd,
            _ => return fail(),
        })
    }

    /// `Some(true)` for horizontal keywords, `Some(false)` for vertical ones and `None` for
    /// `center`.
    fn horizontal(self) -> Option<bool> {
        match self {
            PositionKeyword::Left
            | PositionKeyword::Right
            | PositionKeyword::XStart
            | PositionKeyword::XEnd => Some(true),
            PositionKeyword::Top
            | PositionKeyword::Bottom
            | PositionKeyword::YStart
            | PositionKeyword::YEnd => Some(false),
            PositionKeyword::Center => None,
        }
    }

    fn position(self, offset: Option<SpecifiedLengthPercentage>) -> SpecifiedPosition {
        let edge = match self {
            PositionKeyword::Left | PositionKeyword::Top => PositionEdge::Start,
            PositionKeyword::Right | PositionKeyword::Bottom => PositionEdge::End,
            PositionKeyword::XStart | PositionKeyword::YStart => PositionEdge::LogicalStart,
            PositionKeyword::XEnd | PositionKeyword::YEnd => PositionEdge::LogicalEnd,
            PositionKeyword::Center => PositionEdge::Center,
        };
        SpecifiedPosition { edge, offset }
    }
}

enum PositionPart {
    Keyword(PositionKeyword),
    Offset(SpecifiedLengthPercentage),
}

/// `<bg-position>`: one to four values, as horizontal and vertical positions.
pub(crate) fn parse_bg_position(
    input: &mut Parser<'_>,
) -> Result<(SpecifiedPosition, SpecifiedPosition)> {
    use PositionPart::{Keyword, Offset};
    let mut parts = Vec::new();
    while parts.len() < 4 {
        if let Ok(keyword) = input.try_parse(PositionKeyword::parse) {
            parts.push(Keyword(keyword));
        } else if let Ok(offset) =
            input.try_parse(|input| parse_length_percentage(input, true, false))
        {
            parts.push(Offset(offset));
        } else {
            break;
        }
    }
    let center = || PositionKeyword::Center.position(None);
    let offset = |value: SpecifiedLengthPercentage| SpecifiedPosition {
        edge: PositionEdge::Start,
        offset: Some(value),
    };
    match parts.len() {
        0 => fail(),
        1 => Ok(match parts.pop().expect("one part") {
            Offset(value) => (offset(value), center()),
            Keyword(k) if k.horizontal() == Some(false) => (center(), k.position(None)),
            Keyword(k) => (k.position(None), center()),
        }),
        2 => {
            let mut parts = parts.into_iter();
            let (a, b) = (parts.next().expect("two"), parts.next().expect("two"));
            match (a, b) {
                // Two keywords may come in either order.
                (Keyword(a), Keyword(b)) => match (a.horizontal(), b.horizontal()) {
                    (Some(x), Some(y)) if x == y => fail(),
                    (Some(false), _) | (None, Some(true)) => {
                        Ok((b.position(None), a.position(None)))
                    }
                    _ => Ok((a.position(None), b.position(None))),
                },
                (Keyword(a), Offset(b)) if a.horizontal() != Some(false) => {
                    Ok((a.position(None), offset(b)))
                }
                (Offset(a), Keyword(b)) if b.horizontal() != Some(true) => {
                    Ok((offset(a), b.position(None)))
                }
                (Offset(a), Offset(b)) => Ok((offset(a), offset(b))),
                _ => fail(),
            }
        }
        _ => {
            // Three or four values: two keywords, each optionally followed by an offset.
            let mut groups: Vec<(PositionKeyword, Option<SpecifiedLengthPercentage>)> = Vec::new();
            for part in parts {
                match part {
                    Keyword(k) => groups.push((k, None)),
                    Offset(value) => match groups.last_mut() {
                        Some((k, slot @ None)) if *k != PositionKeyword::Center => {
                            *slot = Some(value)
                        }
                        _ => return fail(),
                    },
                }
            }
            let [(a, a_offset), (b, b_offset)]: [_; 2] =
                groups.try_into().map_err(|_| ParseError::custom(()))?;
            let (a_pos, b_pos) = (a.position(a_offset), b.position(b_offset));
            match (a.horizontal(), b.horizontal()) {
                (Some(true), Some(false) | None) | (None, Some(false)) => Ok((a_pos, b_pos)),
                (Some(false), Some(true) | None) | (None, Some(true)) => Ok((b_pos, a_pos)),
                _ => fail(),
            }
        }
    }
}

/// `background-position-x`: `[ center | [ [ left | right | x-start | x-end ]?
/// <length-percentage>? ]! ]#`.
pub struct BackgroundPositionX;

/// `background-position-y`: `[ center | [ [ top | bottom | y-start | y-end ]?
/// <length-percentage>? ]! ]#`.
pub struct BackgroundPositionY;

fn parse_axis_position(input: &mut Parser<'_>, horizontal: bool) -> Result<SpecifiedPosition> {
    let keyword = input
        .try_parse(|input| {
            let keyword = PositionKeyword::parse(input)?;
            if keyword.horizontal().is_some_and(|h| h != horizontal) {
                return fail();
            }
            Ok(keyword)
        })
        .ok();
    let offset = if keyword == Some(PositionKeyword::Center) {
        None
    } else {
        input
            .try_parse(|input| parse_length_percentage(input, true, false))
            .ok()
    };
    match (keyword, offset) {
        (None, None) => fail(),
        (None, Some(value)) => Ok(SpecifiedPosition {
            edge: PositionEdge::Start,
            offset: Some(value),
        }),
        (Some(keyword), offset) => Ok(keyword.position(offset)),
    }
}

fn position_css(list: &[PositionComponent], horizontal: bool) -> String {
    layers_css(list, |position| {
        let offset = length_percentage_css(&position.offset);
        match (position.from_end, horizontal) {
            (false, _) => offset,
            (true, true) => format!("right {offset}"),
            (true, false) => format!("bottom {offset}"),
        }
    })
}

impl Property for BackgroundPositionX {
    type Specified = Arc<[SpecifiedPosition]>;
    type Computed = Arc<[PositionComponent]>;

    fn to_css(computed: &Self::Computed) -> String {
        position_css(computed, true)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        layers(input, |input| parse_axis_position(input, true))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.iter().map(|p| p.compute(ctx, true)).collect()
    }
}

impl Property for BackgroundPositionY {
    type Specified = Arc<[SpecifiedPosition]>;
    type Computed = Arc<[PositionComponent]>;

    fn to_css(computed: &Self::Computed) -> String {
        position_css(computed, false)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        layers(input, |input| parse_axis_position(input, false))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.iter().map(|p| p.compute(ctx, false)).collect()
    }
}
