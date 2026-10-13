//! Border images (CSS Backgrounds 3 § 6), corner shapes and border shapes (CSS Borders 4).
//! The `mask-border-*` properties share the border image value types.

use std::sync::Arc;

use cssparser::{Parser, Token};

use super::{
    BasicShape, GeometryBox, LengthPercentage, Result, SpecifiedLengthPercentage, fail,
    length_percentage_css, parse_basic_shape, parse_length_percentage, parse_number,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

fn number(value: f32) -> String {
    serialize::number(f64::from(value))
}

/// One to four values for the top, right, bottom and left sides.
fn four<T: Clone>(
    input: &mut Parser<'_>,
    parse: impl Fn(&mut Parser<'_>) -> Result<T>,
) -> Result<[T; 4]> {
    let mut values = vec![parse(input)?];
    while values.len() < 4 {
        match input.try_parse(&parse) {
            Ok(value) => values.push(value),
            Err(_) => break,
        }
    }
    Ok(sides(values))
}

/// Expands one to four values to the four sides.
fn sides<T: Clone>(values: Vec<T>) -> [T; 4] {
    let get = |i: usize| values[i].clone();
    match values.len() {
        1 => [get(0), get(0), get(0), get(0)],
        2 => [get(0), get(1), get(0), get(1)],
        3 => [get(0), get(1), get(2), get(1)],
        _ => [get(0), get(1), get(2), get(3)],
    }
}

/// The shortest one to four values that expand to `values`.
pub(crate) fn four_css<T: PartialEq>(values: &[T; 4], css: impl Fn(&T) -> String) -> String {
    let [top, right, bottom, left] = values;
    let count = if left != right {
        4
    } else if bottom != top {
        3
    } else if right != top {
        2
    } else {
        1
    };
    values[..count]
        .iter()
        .map(css)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A number or a percentage, as `border-image-slice` takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberOrPercentage {
    Number(f32),
    /// A fraction: `50%` is 0.5.
    Percentage(f32),
}

impl NumberOrPercentage {
    /// The value in pixels, with percentages of `basis`.
    pub fn resolve(self, basis: f32) -> f32 {
        match self {
            NumberOrPercentage::Number(n) => n,
            NumberOrPercentage::Percentage(f) => f * basis,
        }
    }

    fn css(&self) -> String {
        match self {
            NumberOrPercentage::Number(n) => number(*n),
            NumberOrPercentage::Percentage(f) => serialize::percentage(*f),
        }
    }
}

/// `border-image-slice` and `mask-border-slice`: the four inward offsets and whether the
/// middle is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageSlice {
    pub offsets: [NumberOrPercentage; 4],
    pub fill: bool,
}

/// `[<number [0,∞]> | <percentage [0,∞]>]{1,4} && fill?`.
pub struct BorderImageSlice;

impl BorderImageSlice {
    pub(crate) fn parse_slice(input: &mut Parser<'_>) -> Result<ImageSlice> {
        let fill = keyword(input, "fill");
        let offsets = four(input, |input| {
            if let Ok(fraction) = input.try_parse(|input| input.expect_percentage()) {
                return if fraction >= 0.0 {
                    Ok(NumberOrPercentage::Percentage(fraction))
                } else {
                    fail()
                };
            }
            let value = parse_number(input)?;
            if value >= 0.0 {
                Ok(NumberOrPercentage::Number(value))
            } else {
                fail()
            }
        })?;
        let fill = fill || keyword(input, "fill");
        Ok(ImageSlice { offsets, fill })
    }
}

impl Property for BorderImageSlice {
    type Specified = ImageSlice;
    type Computed = ImageSlice;

    fn to_css(computed: &ImageSlice) -> String {
        let offsets = four_css(&computed.offsets, NumberOrPercentage::css);
        if computed.fill {
            format!("{offsets} fill")
        } else {
            offsets
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<ImageSlice> {
        Self::parse_slice(input)
    }

    fn compute(specified: &ImageSlice, _ctx: &ComputeContext) -> ImageSlice {
        *specified
    }
}

/// One side of `border-image-width` or `border-image-outset`: a multiple of the border
/// width, a length (or for widths, a percentage of the border image area), or `auto`.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageSideWidth<L> {
    Number(f32),
    Length(L),
    Auto,
}

impl<L> ImageSideWidth<L> {
    /// The same multiple of the border width on every side.
    pub fn numbers(value: f32) -> [Self; 4] {
        [
            ImageSideWidth::Number(value),
            ImageSideWidth::Number(value),
            ImageSideWidth::Number(value),
            ImageSideWidth::Number(value),
        ]
    }

    /// `auto` on every side.
    pub fn auto() -> [Self; 4] {
        [
            ImageSideWidth::Auto,
            ImageSideWidth::Auto,
            ImageSideWidth::Auto,
            ImageSideWidth::Auto,
        ]
    }
}

impl ImageSideWidth<LengthPercentage> {
    fn css(&self) -> String {
        match self {
            ImageSideWidth::Number(n) => number(*n),
            ImageSideWidth::Length(l) => length_percentage_css(l),
            ImageSideWidth::Auto => "auto".into(),
        }
    }
}

fn compute_sides(
    sides: &[ImageSideWidth<SpecifiedLengthPercentage>; 4],
    ctx: &ComputeContext,
) -> [ImageSideWidth<LengthPercentage>; 4] {
    sides.clone().map(|side| match side {
        ImageSideWidth::Number(n) => ImageSideWidth::Number(n),
        ImageSideWidth::Length(l) => ImageSideWidth::Length(l.compute(ctx, true)),
        ImageSideWidth::Auto => ImageSideWidth::Auto,
    })
}

fn parse_side(
    input: &mut Parser<'_>,
    widths: bool,
) -> Result<ImageSideWidth<SpecifiedLengthPercentage>> {
    if widths && keyword(input, "auto") {
        return Ok(ImageSideWidth::Auto);
    }
    if let Ok(value) = input.try_parse(|input| match input.next()? {
        Token::Number { value, .. } => Ok(*value),
        _ => fail(),
    }) {
        return if value >= 0.0 {
            Ok(ImageSideWidth::Number(value))
        } else {
            fail()
        };
    }
    Ok(ImageSideWidth::Length(parse_length_percentage(
        input, widths, true,
    )?))
}

/// `border-image-width` and `mask-border-width`:
/// `[<length-percentage [0,∞]> | <number [0,∞]> | auto]{1,4}`.
pub struct BorderImageWidth;

impl BorderImageWidth {
    pub(crate) fn parse_widths(
        input: &mut Parser<'_>,
    ) -> Result<[ImageSideWidth<SpecifiedLengthPercentage>; 4]> {
        four(input, |input| parse_side(input, true))
    }
}

impl Property for BorderImageWidth {
    type Specified = [ImageSideWidth<SpecifiedLengthPercentage>; 4];
    type Computed = [ImageSideWidth<LengthPercentage>; 4];

    fn to_css(computed: &Self::Computed) -> String {
        four_css(computed, ImageSideWidth::css)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        Self::parse_widths(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        compute_sides(specified, ctx)
    }
}

/// `border-image-outset` and `mask-border-outset`: `[<length [0,∞]> | <number [0,∞]>]{1,4}`.
pub struct BorderImageOutset;

impl BorderImageOutset {
    pub(crate) fn parse_outsets(
        input: &mut Parser<'_>,
    ) -> Result<[ImageSideWidth<SpecifiedLengthPercentage>; 4]> {
        four(input, |input| parse_side(input, false))
    }
}

impl Property for BorderImageOutset {
    type Specified = [ImageSideWidth<SpecifiedLengthPercentage>; 4];
    type Computed = [ImageSideWidth<LengthPercentage>; 4];

    fn to_css(computed: &Self::Computed) -> String {
        four_css(computed, ImageSideWidth::css)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        Self::parse_outsets(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        compute_sides(specified, ctx)
    }
}

keywords!(
    /// How a border image's edges fill their sides along one axis.
    ImageRepeat { Stretch = "stretch", Repeat = "repeat", Round = "round", Space = "space" }
);

/// `border-image-repeat` and `mask-border-repeat`: `[stretch | repeat | round | space]{1,2}`,
/// horizontal then vertical.
pub struct BorderImageRepeat;

impl BorderImageRepeat {
    pub(crate) fn parse_repeat(input: &mut Parser<'_>) -> Result<(ImageRepeat, ImageRepeat)> {
        let x = <ImageRepeat as Property>::parse(input)?;
        let y = input
            .try_parse(<ImageRepeat as Property>::parse)
            .unwrap_or(x);
        Ok((x, y))
    }
}

impl Property for BorderImageRepeat {
    type Specified = (ImageRepeat, ImageRepeat);
    type Computed = (ImageRepeat, ImageRepeat);

    fn to_css(computed: &Self::Computed) -> String {
        if computed.0 == computed.1 {
            computed.0.as_str().into()
        } else {
            format!("{} {}", computed.0.as_str(), computed.1.as_str())
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        Self::parse_repeat(input)
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        *specified
    }
}

// ---------------------------------------------------------------------------------------------
// Corner shapes

/// `<corner-shape-value>`: a keyword, or `superellipse()` with its parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CornerShape {
    Round,
    Scoop,
    Bevel,
    Notch,
    Square,
    Squircle,
    Superellipse(f32),
}

impl CornerShape {
    /// The superellipse parameter: the corner follows `|x|^(2^k) + |y|^(2^k) = 1`.
    pub fn parameter(self) -> f32 {
        match self {
            CornerShape::Round => 1.0,
            CornerShape::Scoop => -1.0,
            CornerShape::Bevel => 0.0,
            CornerShape::Notch => f32::NEG_INFINITY,
            CornerShape::Square => f32::INFINITY,
            CornerShape::Squircle => 2.0,
            CornerShape::Superellipse(k) => k,
        }
    }

    pub(crate) fn parse_value(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_function_matching("superellipse"))
            .is_ok()
        {
            return input.parse_nested_block(|input| {
                if let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) {
                    return match &*ident.to_ascii_lowercase() {
                        "infinity" => Ok(CornerShape::Superellipse(f32::INFINITY)),
                        "-infinity" => Ok(CornerShape::Superellipse(f32::NEG_INFINITY)),
                        _ => fail(),
                    };
                }
                Ok(CornerShape::Superellipse(parse_number(input)?))
            });
        }
        Ok(match &*input.expect_ident()?.to_ascii_lowercase() {
            "round" => CornerShape::Round,
            "scoop" => CornerShape::Scoop,
            "bevel" => CornerShape::Bevel,
            "notch" => CornerShape::Notch,
            "square" => CornerShape::Square,
            "squircle" => CornerShape::Squircle,
            _ => return fail(),
        })
    }

    pub fn to_css(self) -> String {
        match self {
            CornerShape::Round => "round".into(),
            CornerShape::Scoop => "scoop".into(),
            CornerShape::Bevel => "bevel".into(),
            CornerShape::Notch => "notch".into(),
            CornerShape::Square => "square".into(),
            CornerShape::Squircle => "squircle".into(),
            CornerShape::Superellipse(k) => {
                format!("superellipse({})", serialize::number(f64::from(k)))
            }
        }
    }
}

impl Property for CornerShape {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        computed.to_css()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Self::parse_value(input)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

// ---------------------------------------------------------------------------------------------
// Border shapes

/// A shape of `border-shape`, and the box it is drawn in.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderShapeValue<L: super::PositionOf> {
    pub shape: BasicShape<L>,
    pub reference: Option<GeometryBox>,
}

/// `border-shape`: `none | [<basic-shape> <geometry-box>?]{1,2}`. With one shape the border
/// is drawn along it; with two, between them.
pub struct BorderShape;

impl Property for BorderShape {
    type Specified = Arc<[BorderShapeValue<SpecifiedLengthPercentage>]>;
    type Computed = Arc<[BorderShapeValue<LengthPercentage>]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        computed
            .iter()
            .map(|value| match value.reference {
                Some(reference) => format!("{} {}", value.shape.to_css(), reference.as_str()),
                None => value.shape.to_css(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(Arc::from([]));
        }
        let mut shapes = Vec::new();
        while shapes.len() < 2 {
            let Ok(shape) = input.try_parse(parse_basic_shape) else {
                break;
            };
            let reference = input.try_parse(<GeometryBox as Property>::parse).ok();
            shapes.push(BorderShapeValue { shape, reference });
        }
        if shapes.is_empty() {
            return fail();
        }
        Ok(shapes.into())
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified
            .iter()
            .map(|value| BorderShapeValue {
                shape: value.shape.compute(ctx),
                reference: value.reference,
            })
            .collect()
    }
}
