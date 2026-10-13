//! Value types of images and replaced elements (CSS Images 3 and 4): `object-fit`,
//! `object-position`, `object-view-box` and the `image-*` properties.

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{
    LengthPercentage, PositionComponent, Result, SpecifiedLengthPercentage, SpecifiedPosition,
    fail, length_percentage_css, parse_bg_position, parse_length_percentage,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;
use crate::values::{Base, CalcContext, CalcNode, NumericType, Unit, is_math_function};

keywords!(
    /// `object-fit`: how replaced content fills its box.
    ObjectFit {
        Fill = "fill",
        Contain = "contain",
        Cover = "cover",
        None = "none",
        ScaleDown = "scale-down",
    }
);

keywords!(
    /// `image-rendering`: how images scale.
    ImageRendering {
        Auto = "auto",
        Smooth = "smooth",
        HighQuality = "high-quality",
        Pixelated = "pixelated",
        CrispEdges = "crisp-edges",
    }
);

impl ImageRendering {
    /// Whether scaled images are smoothed, rather than showing their pixels.
    pub fn smooth(self) -> bool {
        !matches!(self, ImageRendering::Pixelated | ImageRendering::CrispEdges)
    }
}

/// Parses a dimension of one type, such as an angle or a resolution, including math
/// functions, in the canonical unit of the type.
pub(crate) fn parse_typed(input: &mut Parser<'_>, base: Base) -> Result<f32> {
    let node = match input.next()?.clone() {
        Token::Dimension { value, unit, .. } => {
            let unit = Unit::from_name(&unit).ok_or_else(|| ParseError::custom(()))?;
            CalcNode::Dimension(f64::from(value), unit)
        }
        Token::Function(name) if is_math_function(&name) => CalcNode::parse_function(input, &name)?,
        _ => return fail(),
    };
    if node.numeric_type(None) != Some(NumericType::of(base)) {
        return fail();
    }
    node.evaluate(&CalcContext::default())
        .map(|v| v as f32)
        .ok_or_else(|| ParseError::custom(()))
}

/// `object-position`.
pub struct ObjectPosition;

impl Property for ObjectPosition {
    type Specified = (SpecifiedPosition, SpecifiedPosition);
    type Computed = (PositionComponent, PositionComponent);

    fn to_css(computed: &Self::Computed) -> String {
        let side = |p: &PositionComponent, end: &str| {
            let offset = length_percentage_css(&p.offset);
            if p.from_end {
                format!("{end} {offset}")
            } else {
                offset
            }
        };
        format!(
            "{} {}",
            side(&computed.0, "right"),
            side(&computed.1, "bottom")
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_bg_position(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        (
            specified.0.compute(ctx, true),
            specified.1.compute(ctx, false),
        )
    }
}

/// `image-orientation`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageOrientation {
    /// The EXIF orientation of the image.
    FromImage,
    None,
    /// Clockwise quarter turns, then a horizontal flip if set.
    Turn {
        quarters: u8,
        flip: bool,
    },
}

impl Property for ImageOrientation {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ImageOrientation::FromImage => "from-image".into(),
            ImageOrientation::None => "none".into(),
            ImageOrientation::Turn { quarters, flip } => {
                let angle = format!("{}deg", u32::from(*quarters) * 90);
                if *flip {
                    format!("{angle} flip")
                } else {
                    angle
                }
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) {
            return match &*ident.to_ascii_lowercase() {
                "from-image" => Ok(ImageOrientation::FromImage),
                "none" => Ok(ImageOrientation::None),
                "flip" => Ok(ImageOrientation::Turn {
                    quarters: 0,
                    flip: true,
                }),
                _ => fail(),
            };
        }
        let degrees = parse_typed(input, Base::Angle)?;
        let flip = input
            .try_parse(|input| input.expect_ident_matching("flip"))
            .is_ok();
        // Angles round to the nearest quarter turn.
        let quarters = ((degrees / 90.0).round() as i64).rem_euclid(4) as u8;
        Ok(ImageOrientation::Turn { quarters, flip })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `image-resolution`: `[from-image || <resolution>] && snap?`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageResolution {
    pub from_image: bool,
    /// Image pixels per CSS pixel.
    pub resolution: Option<f32>,
    pub snap: bool,
}

impl ImageResolution {
    /// The image pixels in one CSS pixel, given the resolution the file declares, if any.
    pub fn dppx(&self, declared: Option<f32>) -> f32 {
        let dppx = match (self.from_image, declared) {
            (true, Some(declared)) => declared,
            _ => self.resolution.unwrap_or(1.0),
        };
        // `snap` rounds to whole device pixels, and PDF output has none: it changes nothing.
        if dppx > 0.0 { dppx } else { 1.0 }
    }
}

impl Property for ImageResolution {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let mut parts = Vec::new();
        if computed.from_image {
            parts.push("from-image".to_owned());
        }
        if let Some(dppx) = computed.resolution {
            parts.push(format!("{}dppx", serialize::number(f64::from(dppx))));
        }
        if computed.snap {
            parts.push("snap".into());
        }
        parts.join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let mut value = ImageResolution {
            from_image: false,
            resolution: None,
            snap: false,
        };
        loop {
            if !value.from_image
                && input
                    .try_parse(|input| input.expect_ident_matching("from-image"))
                    .is_ok()
            {
                value.from_image = true;
            } else if !value.snap
                && input
                    .try_parse(|input| input.expect_ident_matching("snap"))
                    .is_ok()
            {
                value.snap = true;
            } else if value.resolution.is_none()
                && let Ok(dppx) = input.try_parse(|input| parse_typed(input, Base::Resolution))
            {
                if dppx <= 0.0 {
                    return fail();
                }
                value.resolution = Some(dppx);
            } else {
                break;
            }
        }
        if !value.from_image && value.resolution.is_none() {
            return fail();
        }
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `object-view-box`: the part of the image shown, as insets from its edges.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectViewBox {
    None,
    /// Insets from the top, right, bottom and left edges.
    Inset([LengthPercentage; 4]),
    /// The left and top offsets, width and height.
    Xywh([LengthPercentage; 4]),
    /// The top, right, bottom and left edges as offsets from the top and left edges, or
    /// `None` for `auto`, the image's own edge.
    Rect([Option<LengthPercentage>; 4]),
}

impl ObjectViewBox {
    /// The part of an image of `width` × `height` to show, as `(x, y, width, height)`.
    pub fn resolve(&self, width: f32, height: f32) -> Option<(f32, f32, f32, f32)> {
        match self {
            ObjectViewBox::None => None,
            ObjectViewBox::Inset([top, right, bottom, left]) => {
                let (x, y) = (left.resolve(width), top.resolve(height));
                Some((
                    x,
                    y,
                    (width - x - right.resolve(width)).max(0.0),
                    (height - y - bottom.resolve(height)).max(0.0),
                ))
            }
            ObjectViewBox::Xywh([x, y, w, h]) => Some((
                x.resolve(width),
                y.resolve(height),
                w.resolve(width).max(0.0),
                h.resolve(height).max(0.0),
            )),
            ObjectViewBox::Rect([top, right, bottom, left]) => {
                let edge = |value: &Option<LengthPercentage>, basis: f32, auto: f32| {
                    value.as_ref().map_or(auto, |v| v.resolve(basis))
                };
                let (x, y) = (edge(left, width, 0.0), edge(top, height, 0.0));
                let right = edge(right, width, width);
                let bottom = edge(bottom, height, height);
                Some((x, y, (right - x).max(0.0), (bottom - y).max(0.0)))
            }
        }
    }
}

/// The specified `object-view-box`.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecifiedViewBox {
    None,
    Inset([SpecifiedLengthPercentage; 4]),
    /// `xywh(x y width height)`.
    Xywh([SpecifiedLengthPercentage; 4]),
    /// `rect(top right bottom left)`, with `None` for `auto`.
    Rect([Option<SpecifiedLengthPercentage>; 4]),
}

impl Property for ObjectViewBox {
    type Specified = SpecifiedViewBox;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ObjectViewBox::None => "none".into(),
            ObjectViewBox::Rect(edges) => format!(
                "rect({})",
                edges
                    .iter()
                    .map(|edge| edge.as_ref().map_or("auto".into(), length_percentage_css))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            ObjectViewBox::Inset(values) | ObjectViewBox::Xywh(values) => format!(
                "{}({})",
                if matches!(computed, ObjectViewBox::Inset(_)) {
                    "inset"
                } else {
                    "xywh"
                },
                values
                    .iter()
                    .map(length_percentage_css)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<SpecifiedViewBox> {
        if input
            .try_parse(|input| input.expect_ident_matching("none"))
            .is_ok()
        {
            return Ok(SpecifiedViewBox::None);
        }
        let name = input.expect_function()?.to_ascii_lowercase();
        input.parse_nested_block(|input| {
            let value = parse_view_box(&name, input)?;
            // Rounded corners do not change which part of the image shows.
            if input
                .try_parse(|input| input.expect_ident_matching("round"))
                .is_ok()
            {
                let mut radii = 0;
                while input
                    .try_parse(|input| {
                        if input.try_parse(|input| input.expect_delim('/')).is_ok() {
                            return Ok(());
                        }
                        parse_length_percentage(input, true, true).map(drop)
                    })
                    .is_ok()
                {
                    radii += 1;
                }
                if radii == 0 {
                    return fail();
                }
            }
            Ok(value)
        })
    }

    fn compute(specified: &SpecifiedViewBox, ctx: &ComputeContext) -> Self {
        match specified {
            SpecifiedViewBox::None => ObjectViewBox::None,
            SpecifiedViewBox::Inset(sides) => {
                ObjectViewBox::Inset(sides.clone().map(|side| side.compute(ctx, false)))
            }
            SpecifiedViewBox::Xywh(values) => {
                ObjectViewBox::Xywh(values.clone().map(|value| value.compute(ctx, false)))
            }
            SpecifiedViewBox::Rect(edges) => ObjectViewBox::Rect(
                edges
                    .clone()
                    .map(|edge| edge.map(|value| value.compute(ctx, false))),
            ),
        }
    }
}

/// The function of an `object-view-box`, up to any `round`.
fn parse_view_box(name: &str, input: &mut Parser<'_>) -> Result<SpecifiedViewBox> {
    match name {
        "inset" => {
            let mut values = vec![parse_length_percentage(input, true, false)?];
            while values.len() < 4 {
                match input.try_parse(|input| parse_length_percentage(input, true, false)) {
                    Ok(value) => values.push(value),
                    Err(_) => break,
                }
            }
            // One to four values, like `margin`.
            let [top, right, bottom, left] = match values.len() {
                1 => [0, 0, 0, 0],
                2 => [0, 1, 0, 1],
                3 => [0, 1, 2, 1],
                _ => [0, 1, 2, 3],
            };
            Ok(SpecifiedViewBox::Inset([
                values[top].clone(),
                values[right].clone(),
                values[bottom].clone(),
                values[left].clone(),
            ]))
        }
        "xywh" => {
            let x = parse_length_percentage(input, true, false)?;
            let y = parse_length_percentage(input, true, false)?;
            let w = parse_length_percentage(input, true, true)?;
            let h = parse_length_percentage(input, true, true)?;
            Ok(SpecifiedViewBox::Xywh([x, y, w, h]))
        }
        "rect" => {
            let mut edge = || -> Result<Option<SpecifiedLengthPercentage>> {
                if input
                    .try_parse(|input| input.expect_ident_matching("auto"))
                    .is_ok()
                {
                    return Ok(None);
                }
                parse_length_percentage(input, true, false).map(Some)
            };
            Ok(SpecifiedViewBox::Rect([edge()?, edge()?, edge()?, edge()?]))
        }
        _ => fail(),
    }
}

/// `link-parameters`: `none | <param()>#`, the parameters passed to the SVG images an
/// element links to (CSS Linked Parameters 1 § 2), as names and values as written. Only
/// raster images are supported, so they have no effect.
pub struct LinkParameters;

impl Property for LinkParameters {
    type Specified = Arc<[(Arc<str>, Arc<str>)]>;
    type Computed = Arc<[(Arc<str>, Arc<str>)]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        computed
            .iter()
            .map(|(name, value)| {
                let mut out = String::from("param(");
                let _ = cssparser::serialize_identifier(name, &mut out);
                if !value.is_empty() {
                    out.push_str(", ");
                    out.push_str(value);
                }
                out.push(')');
                out
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if input
            .try_parse(|input| input.expect_ident_matching("none"))
            .is_ok()
        {
            return Ok(Arc::from([]));
        }
        let params = input.parse_comma_separated(|input| {
            input.expect_function_matching("param")?;
            input.parse_nested_block(|input| {
                let name = input.expect_ident_cloned()?;
                if !name.starts_with("--") {
                    return fail();
                }
                let value = if input.try_parse(|input| input.expect_comma()).is_ok() {
                    let start = input.position();
                    while input.next().is_ok() {}
                    input.slice_from(start).trim().to_owned()
                } else {
                    String::new()
                };
                Ok((Arc::from(&*name), Arc::from(value)))
            })
        })?;
        Ok(params.into())
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}
