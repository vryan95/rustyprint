//! Transforms (CSS Transforms 1 and 2): `transform`, the individual transform properties,
//! their origins and perspective.

use std::sync::Arc;

use cssparser::Parser;

use super::effects::parse_number_percentage;
use super::gradients::parse_angle;
use super::{
    LengthPercentage, Result, SpecifiedLengthPercentage, fail, length_percentage_css,
    parse_length_percentage, parse_number,
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

fn degrees(value: f32) -> String {
    format!("{}deg", number(value))
}

/// A `<transform-function>`, with lengths of type `L`. Angles are degrees and scales are
/// numbers.
#[derive(Debug, Clone, PartialEq)]
pub enum TransformFunction<L> {
    Matrix([f32; 6]),
    /// Column-major, as written.
    Matrix3d([f32; 16]),
    Translate(L, Option<L>),
    TranslateX(L),
    TranslateY(L),
    TranslateZ(L),
    Translate3d(L, L, L),
    Scale(f32, Option<f32>),
    ScaleX(f32),
    ScaleY(f32),
    ScaleZ(f32),
    Scale3d(f32, f32, f32),
    Rotate(f32),
    RotateX(f32),
    RotateY(f32),
    RotateZ(f32),
    /// An axis and an angle.
    Rotate3d(f32, f32, f32, f32),
    Skew(f32, Option<f32>),
    SkewX(f32),
    SkewY(f32),
    /// `perspective(none)` is `None`.
    Perspective(Option<L>),
}

impl TransformFunction<SpecifiedLengthPercentage> {
    fn compute(&self, ctx: &ComputeContext) -> TransformFunction<LengthPercentage> {
        let c = |l: &SpecifiedLengthPercentage| l.compute(ctx, false);
        use TransformFunction as T;
        match self {
            T::Matrix(m) => T::Matrix(*m),
            T::Matrix3d(m) => T::Matrix3d(*m),
            T::Translate(x, y) => T::Translate(c(x), y.as_ref().map(c)),
            T::TranslateX(x) => T::TranslateX(c(x)),
            T::TranslateY(y) => T::TranslateY(c(y)),
            T::TranslateZ(z) => T::TranslateZ(c(z)),
            T::Translate3d(x, y, z) => T::Translate3d(c(x), c(y), c(z)),
            T::Scale(x, y) => T::Scale(*x, *y),
            T::ScaleX(v) => T::ScaleX(*v),
            T::ScaleY(v) => T::ScaleY(*v),
            T::ScaleZ(v) => T::ScaleZ(*v),
            T::Scale3d(x, y, z) => T::Scale3d(*x, *y, *z),
            T::Rotate(a) => T::Rotate(*a),
            T::RotateX(a) => T::RotateX(*a),
            T::RotateY(a) => T::RotateY(*a),
            T::RotateZ(a) => T::RotateZ(*a),
            T::Rotate3d(x, y, z, a) => T::Rotate3d(*x, *y, *z, *a),
            T::Skew(x, y) => T::Skew(*x, *y),
            T::SkewX(a) => T::SkewX(*a),
            T::SkewY(a) => T::SkewY(*a),
            T::Perspective(d) => T::Perspective(d.as_ref().map(c)),
        }
    }
}

impl TransformFunction<LengthPercentage> {
    pub fn to_css(&self) -> String {
        let l = length_percentage_css;
        use TransformFunction as T;
        let list = |values: &[f32]| values.iter().map(|v| number(*v)).collect::<Vec<_>>();
        match self {
            T::Matrix(m) => format!("matrix({})", list(m).join(", ")),
            T::Matrix3d(m) => format!("matrix3d({})", list(m).join(", ")),
            T::Translate(x, None) => format!("translate({})", l(x)),
            T::Translate(x, Some(y)) => format!("translate({}, {})", l(x), l(y)),
            T::TranslateX(x) => format!("translateX({})", l(x)),
            T::TranslateY(y) => format!("translateY({})", l(y)),
            T::TranslateZ(z) => format!("translateZ({})", l(z)),
            T::Translate3d(x, y, z) => format!("translate3d({}, {}, {})", l(x), l(y), l(z)),
            T::Scale(x, None) => format!("scale({})", number(*x)),
            T::Scale(x, Some(y)) => format!("scale({}, {})", number(*x), number(*y)),
            T::ScaleX(v) => format!("scaleX({})", number(*v)),
            T::ScaleY(v) => format!("scaleY({})", number(*v)),
            T::ScaleZ(v) => format!("scaleZ({})", number(*v)),
            T::Scale3d(x, y, z) => format!("scale3d({})", list(&[*x, *y, *z]).join(", ")),
            T::Rotate(a) => format!("rotate({})", degrees(*a)),
            T::RotateX(a) => format!("rotateX({})", degrees(*a)),
            T::RotateY(a) => format!("rotateY({})", degrees(*a)),
            T::RotateZ(a) => format!("rotateZ({})", degrees(*a)),
            T::Rotate3d(x, y, z, a) => format!(
                "rotate3d({}, {})",
                list(&[*x, *y, *z]).join(", "),
                degrees(*a)
            ),
            T::Skew(x, None) => format!("skew({})", degrees(*x)),
            T::Skew(x, Some(y)) => format!("skew({}, {})", degrees(*x), degrees(*y)),
            T::SkewX(a) => format!("skewX({})", degrees(*a)),
            T::SkewY(a) => format!("skewY({})", degrees(*a)),
            T::Perspective(None) => "perspective(none)".into(),
            T::Perspective(Some(d)) => format!("perspective({})", l(d)),
        }
    }
}

/// A comma between arguments.
fn comma(input: &mut Parser<'_>) -> Result<()> {
    input.expect_comma()?;
    Ok(())
}

fn length(input: &mut Parser<'_>) -> Result<SpecifiedLengthPercentage> {
    parse_length_percentage(input, false, false)
}

fn length_percentage(input: &mut Parser<'_>) -> Result<SpecifiedLengthPercentage> {
    parse_length_percentage(input, true, false)
}

/// An angle, or unitless zero.
fn angle(input: &mut Parser<'_>) -> Result<f32> {
    parse_angle(input)
}

/// Numbers separated by commas.
fn numbers<const N: usize>(input: &mut Parser<'_>) -> Result<[f32; N]> {
    let mut out = [0.0; N];
    for (i, value) in out.iter_mut().enumerate() {
        if i > 0 {
            comma(input)?;
        }
        *value = parse_number(input)?;
    }
    Ok(out)
}

fn parse_function(input: &mut Parser<'_>) -> Result<TransformFunction<SpecifiedLengthPercentage>> {
    let name = input.expect_function()?.to_ascii_lowercase();
    use TransformFunction as T;
    input.parse_nested_block(|input| {
        Ok(match name.as_str() {
            "matrix" => T::Matrix(numbers::<6>(input)?),
            "matrix3d" => T::Matrix3d(numbers::<16>(input)?),
            "translate" => {
                let x = length_percentage(input)?;
                let y = input
                    .try_parse(|input| {
                        comma(input)?;
                        length_percentage(input)
                    })
                    .ok();
                T::Translate(x, y)
            }
            "translatex" => T::TranslateX(length_percentage(input)?),
            "translatey" => T::TranslateY(length_percentage(input)?),
            "translatez" => T::TranslateZ(length(input)?),
            "translate3d" => {
                let x = length_percentage(input)?;
                comma(input)?;
                let y = length_percentage(input)?;
                comma(input)?;
                T::Translate3d(x, y, length(input)?)
            }
            "scale" => {
                let x = parse_number_percentage(input)?;
                let y = input
                    .try_parse(|input| {
                        comma(input)?;
                        parse_number_percentage(input)
                    })
                    .ok();
                T::Scale(x, y)
            }
            "scalex" => T::ScaleX(parse_number_percentage(input)?),
            "scaley" => T::ScaleY(parse_number_percentage(input)?),
            "scalez" => T::ScaleZ(parse_number_percentage(input)?),
            "scale3d" => {
                let x = parse_number_percentage(input)?;
                comma(input)?;
                let y = parse_number_percentage(input)?;
                comma(input)?;
                T::Scale3d(x, y, parse_number_percentage(input)?)
            }
            "rotate" => T::Rotate(angle(input)?),
            "rotatex" => T::RotateX(angle(input)?),
            "rotatey" => T::RotateY(angle(input)?),
            "rotatez" => T::RotateZ(angle(input)?),
            "rotate3d" => {
                let [x, y, z] = numbers::<3>(input)?;
                comma(input)?;
                T::Rotate3d(x, y, z, angle(input)?)
            }
            "skew" => {
                let x = angle(input)?;
                let y = input
                    .try_parse(|input| {
                        comma(input)?;
                        angle(input)
                    })
                    .ok();
                T::Skew(x, y)
            }
            "skewx" => T::SkewX(angle(input)?),
            "skewy" => T::SkewY(angle(input)?),
            "perspective" => {
                if keyword(input, "none") {
                    T::Perspective(None)
                } else {
                    T::Perspective(Some(parse_length_percentage(input, false, true)?))
                }
            }
            _ => return fail(),
        })
    })
}

/// `transform`: `none | <transform-function>+`.
pub struct Transform;

impl Property for Transform {
    type Specified = Arc<[TransformFunction<SpecifiedLengthPercentage>]>;
    type Computed = Arc<[TransformFunction<LengthPercentage>]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        computed
            .iter()
            .map(TransformFunction::to_css)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(Arc::from([]));
        }
        let mut functions = vec![parse_function(input)?];
        while let Ok(function) = input.try_parse(parse_function) {
            functions.push(function);
        }
        Ok(functions.into())
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.iter().map(|f| f.compute(ctx)).collect()
    }
}

/// `translate`: `none | <length-percentage> [<length-percentage> <length>?]?`.
pub struct Translate;

impl Property for Translate {
    type Specified = Option<Vec<SpecifiedLengthPercentage>>;
    type Computed = Option<Arc<[LengthPercentage]>>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "none".into(),
            Some(values) => values
                .iter()
                .map(length_percentage_css)
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(None);
        }
        let mut values = vec![length_percentage(input)?];
        if let Ok(y) = input.try_parse(length_percentage) {
            values.push(y);
            if let Ok(z) = input.try_parse(length) {
                values.push(z);
            }
        }
        Ok(Some(values))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified
            .as_ref()
            .map(|values| values.iter().map(|v| v.compute(ctx, false)).collect())
    }
}

/// The axis of the `rotate` property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RotateAxis {
    X,
    Y,
    Z,
    Vector(f32, f32, f32),
}

/// `rotate`: `none | <angle> | [x | y | z | <number>{3}] && <angle>`.
pub struct Rotate;

impl Property for Rotate {
    type Specified = Option<(Option<RotateAxis>, f32)>;
    type Computed = Option<(Option<RotateAxis>, f32)>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "none".into(),
            Some((None, angle)) => degrees(*angle),
            Some((Some(axis), angle)) => {
                let axis = match axis {
                    RotateAxis::X => "x".into(),
                    RotateAxis::Y => "y".into(),
                    RotateAxis::Z => "z".into(),
                    RotateAxis::Vector(x, y, z) => {
                        format!("{} {} {}", number(*x), number(*y), number(*z))
                    }
                };
                format!("{axis} {}", degrees(*angle))
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(None);
        }
        let axis = |input: &mut Parser<'_>| -> Result<RotateAxis> {
            if let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) {
                return match &*ident.to_ascii_lowercase() {
                    "x" => Ok(RotateAxis::X),
                    "y" => Ok(RotateAxis::Y),
                    "z" => Ok(RotateAxis::Z),
                    _ => fail(),
                };
            }
            let x = parse_number(input)?;
            let y = parse_number(input)?;
            Ok(RotateAxis::Vector(x, y, parse_number(input)?))
        };
        if let Ok(angle) = input.try_parse(angle) {
            let axis = input.try_parse(axis).ok();
            return Ok(Some((axis, angle)));
        }
        let axis = axis(input)?;
        Ok(Some((Some(axis), angle(input)?)))
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        *specified
    }
}

/// `scale`: `none | [<number> | <percentage>]{1,3}`.
pub struct Scale;

impl Property for Scale {
    type Specified = Option<Vec<f32>>;
    type Computed = Option<Arc<[f32]>>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "none".into(),
            Some(values) => values
                .iter()
                .map(|v| number(*v))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(None);
        }
        let mut values = vec![parse_number_percentage(input)?];
        while values.len() < 3 {
            match input.try_parse(parse_number_percentage) {
                Ok(value) => values.push(value),
                Err(_) => break,
            }
        }
        Ok(Some(values))
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified
            .as_ref()
            .map(|values| Arc::from(values.as_slice()))
    }
}

/// One part of `transform-origin` before it is known to be horizontal or vertical.
enum OriginPart {
    /// A keyword, with whether it is horizontal (`Some(true)`), vertical (`Some(false)`) or
    /// `center` (`None`), and the offset it stands for.
    Keyword(Option<bool>, f32),
    Offset(SpecifiedLengthPercentage),
}

fn origin_part(input: &mut Parser<'_>) -> Result<OriginPart> {
    if let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) {
        return Ok(match &*ident.to_ascii_lowercase() {
            "left" => OriginPart::Keyword(Some(true), 0.0),
            "right" => OriginPart::Keyword(Some(true), 1.0),
            "top" => OriginPart::Keyword(Some(false), 0.0),
            "bottom" => OriginPart::Keyword(Some(false), 1.0),
            "center" => OriginPart::Keyword(None, 0.5),
            _ => return fail(),
        });
    }
    Ok(OriginPart::Offset(length_percentage(input)?))
}

/// `transform-origin`: `[<x> | <y>] | <x> <y> <length>?` (horizontal and vertical in either
/// order when both are keywords).
pub struct TransformOrigin;

impl Property for TransformOrigin {
    type Specified = (
        SpecifiedLengthPercentage,
        SpecifiedLengthPercentage,
        SpecifiedLengthPercentage,
    );
    type Computed = (LengthPercentage, LengthPercentage, f32);

    fn to_css(computed: &Self::Computed) -> String {
        let (x, y, z) = computed;
        let xy = format!("{} {}", length_percentage_css(x), length_percentage_css(y));
        if *z == 0.0 {
            xy
        } else {
            format!("{xy} {}", serialize::px(*z))
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let fraction = SpecifiedLengthPercentage::Percentage;
        let value = |part: OriginPart| match part {
            OriginPart::Keyword(_, f) => fraction(f),
            OriginPart::Offset(offset) => offset,
        };
        let first = origin_part(input)?;
        let Ok(second) = input.try_parse(origin_part) else {
            // One value: a vertical keyword sets the vertical position.
            return Ok(match first {
                OriginPart::Keyword(Some(false), f) => (fraction(0.5), fraction(f), zero()),
                other => (value(other), fraction(0.5), zero()),
            });
        };
        // A vertical keyword first, or a horizontal one second, swaps the pair.
        let (x, y) = match (&first, &second) {
            (OriginPart::Keyword(Some(false), _), OriginPart::Offset(_))
            | (OriginPart::Offset(_), OriginPart::Keyword(Some(true), _)) => return fail(),
            (OriginPart::Keyword(Some(false), _), _) | (_, OriginPart::Keyword(Some(true), _)) => {
                (second, first)
            }
            _ => (first, second),
        };
        if matches!(x, OriginPart::Keyword(Some(false), _))
            || matches!(y, OriginPart::Keyword(Some(true), _))
        {
            return fail();
        }
        let z = input.try_parse(length).unwrap_or_else(|_| zero());
        Ok((value(x), value(y), z))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        (
            specified.0.compute(ctx, false),
            specified.1.compute(ctx, false),
            specified.2.compute(ctx, false).as_length().unwrap_or(0.0),
        )
    }
}

fn zero() -> SpecifiedLengthPercentage {
    SpecifiedLengthPercentage::Length(crate::values::Length::px(0.0))
}

keywords!(
    /// `transform-box`: the reference box of `transform` and `transform-origin`.
    TransformBox {
        ContentBox = "content-box",
        BorderBox = "border-box",
        FillBox = "fill-box",
        StrokeBox = "stroke-box",
        ViewBox = "view-box",
    }
);

keywords!(
    /// `transform-style`.
    TransformStyle { Flat = "flat", Preserve3d = "preserve-3d" }
);

keywords!(
    /// `backface-visibility`.
    BackfaceVisibility { Visible = "visible", Hidden = "hidden" }
);

/// `perspective`: `none | <length [0,∞]>`.
pub struct Perspective;

impl Property for Perspective {
    type Specified = Option<SpecifiedLengthPercentage>;
    type Computed = Option<f32>;

    fn to_css(computed: &Self::Computed) -> String {
        computed.map_or_else(|| "none".into(), serialize::px)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(None);
        }
        Ok(Some(parse_length_percentage(input, false, true)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified
            .as_ref()
            .map(|d| d.compute(ctx, true).as_length().unwrap_or(0.0))
    }
}
