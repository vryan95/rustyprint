//! Basic shapes and the properties that wrap content around floats (CSS Shapes 1 and 2).

use std::sync::Arc;

use cssparser::{ParseError, Parser};

use super::gradients::parse_angle;
use super::{
    CornerRadius, ImageValue, LengthPercentage, PositionComponent, Result,
    SpecifiedLengthPercentage, SpecifiedPosition, fail, length_percentage_css, parse_bg_position,
    parse_length_percentage, parse_number,
};
use crate::properties::{ComputeContext, Property};

fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

keywords!(
    /// The fill rule of a polygon or path, which decides which parts of a shape that
    /// overlaps itself are inside.
    FillRule { Nonzero = "nonzero", Evenodd = "evenodd" }
);

/// The radius of `circle()` or one radius of `ellipse()`.
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeRadius<L> {
    Length(L),
    ClosestSide,
    FarthestSide,
}

/// A corner radius of `inset()`, `rect()` or `xywh()`, horizontal and vertical.
type SpecifiedCorner = (SpecifiedLengthPercentage, SpecifiedLengthPercentage);

/// A point of `polygon()` or a coordinate of `shape()`.
pub type ShapePoint<L> = (L, L);

/// A command of `shape()`, with coordinates that are absolute (`to`) or relative to the
/// current point (`by`).
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeCommand<L> {
    Move {
        by: bool,
        to: ShapePoint<L>,
    },
    Line {
        by: bool,
        to: ShapePoint<L>,
    },
    HLine {
        by: bool,
        x: L,
    },
    VLine {
        by: bool,
        y: L,
    },
    /// A cubic Bézier curve with two control points, or a quadratic one with one.
    Curve {
        by: bool,
        to: ShapePoint<L>,
        controls: Vec<ShapePoint<L>>,
    },
    /// A curve whose first control point reflects the previous curve's last one.
    Smooth {
        by: bool,
        to: ShapePoint<L>,
        control: Option<ShapePoint<L>>,
    },
    Arc {
        by: bool,
        to: ShapePoint<L>,
        radii: (L, L),
        clockwise: bool,
        large: bool,
        rotate: f32,
    },
    Close,
}

/// A `<basic-shape>` (CSS Shapes 1 § 3), with lengths of type `L`.
#[derive(Debug, Clone, PartialEq)]
pub enum BasicShape<L: PositionOf> {
    /// `inset()`, and `rect()` and `xywh()` converted to it: the insets from the top, right,
    /// bottom and left edges of the reference box, and corner radii.
    Inset {
        insets: [L; 4],
        radii: Option<[(L, L); 4]>,
        /// The function as written, for serialization.
        function: InsetFunction,
    },
    Circle {
        radius: ShapeRadius<L>,
        position: Option<(ShapePosition<L>, ShapePosition<L>)>,
    },
    Ellipse {
        radii: (ShapeRadius<L>, ShapeRadius<L>),
        position: Option<(ShapePosition<L>, ShapePosition<L>)>,
    },
    Polygon {
        fill: Option<FillRule>,
        points: Vec<ShapePoint<L>>,
    },
    /// `path()`: SVG path data, in CSS pixels from the reference box's top-left corner.
    Path {
        fill: Option<FillRule>,
        data: Arc<str>,
    },
    /// `shape()` (CSS Shapes 2): a path of commands with CSS lengths.
    Shape {
        fill: Option<FillRule>,
        from: ShapePoint<L>,
        commands: Vec<ShapeCommand<L>>,
    },
}

/// How an inset rectangle was written.
#[derive(Debug, Clone, PartialEq)]
pub enum InsetFunction {
    Inset,
    /// `rect(top right bottom left)`, with each `auto` edge on the reference box.
    Rect([bool; 4]),
    /// `xywh(x y width height)`.
    Xywh,
}

/// A position along one axis: specified, or computed.
pub type ShapePosition<L> = <L as PositionOf>::Position;

/// Maps a length type to its position type.
pub trait PositionOf {
    type Position: Clone + std::fmt::Debug + PartialEq;
}

impl PositionOf for SpecifiedLengthPercentage {
    type Position = SpecifiedPosition;
}

impl PositionOf for LengthPercentage {
    type Position = PositionComponent;
}

keywords!(
    /// The box a shape refers to.
    ShapeBox {
        MarginBox = "margin-box",
        BorderBox = "border-box",
        PaddingBox = "padding-box",
        ContentBox = "content-box",
    }
);

/// An optional fill rule and the comma after it.
fn parse_fill(input: &mut Parser<'_>) -> Option<FillRule> {
    input
        .try_parse(|input| {
            let fill = FillRule::parse(input)?;
            input.expect_comma()?;
            Ok::<_, ParseError<()>>(fill)
        })
        .ok()
}

fn parse_radius(input: &mut Parser<'_>) -> Result<ShapeRadius<SpecifiedLengthPercentage>> {
    if keyword(input, "closest-side") {
        return Ok(ShapeRadius::ClosestSide);
    }
    if keyword(input, "farthest-side") {
        return Ok(ShapeRadius::FarthestSide);
    }
    Ok(ShapeRadius::Length(parse_length_percentage(
        input, true, true,
    )?))
}

/// One to four values, expanded to four in top, right, bottom, left order.
fn four<T: Clone>(mut values: Vec<T>) -> [T; 4] {
    while values.len() < 4 {
        let copy = match values.len() {
            1 => values[0].clone(),
            2 => values[0].clone(),
            _ => values[1].clone(),
        };
        values.push(copy);
    }
    [
        values[0].clone(),
        values[1].clone(),
        values[2].clone(),
        values[3].clone(),
    ]
}

fn parse_lengths(
    input: &mut Parser<'_>,
    non_negative: bool,
) -> Result<Vec<SpecifiedLengthPercentage>> {
    let mut values = vec![parse_length_percentage(input, true, non_negative)?];
    while values.len() < 4 {
        match input.try_parse(|input| parse_length_percentage(input, true, non_negative)) {
            Ok(value) => values.push(value),
            Err(_) => break,
        }
    }
    Ok(values)
}

/// `round <'border-radius'>`.
fn parse_round(input: &mut Parser<'_>) -> Result<Option<[SpecifiedCorner; 4]>> {
    if !keyword(input, "round") {
        return Ok(None);
    }
    let horizontal = four(parse_lengths(input, true)?);
    let vertical = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
        four(parse_lengths(input, true)?)
    } else {
        horizontal.clone()
    };
    let [h0, h1, h2, h3] = horizontal;
    let [v0, v1, v2, v3] = vertical;
    Ok(Some([(h0, v0), (h1, v1), (h2, v2), (h3, v3)]))
}

fn parse_at_position(
    input: &mut Parser<'_>,
) -> Result<Option<(SpecifiedPosition, SpecifiedPosition)>> {
    if keyword(input, "at") {
        Ok(Some(parse_bg_position(input)?))
    } else {
        Ok(None)
    }
}

fn parse_point(input: &mut Parser<'_>) -> Result<ShapePoint<SpecifiedLengthPercentage>> {
    Ok((
        parse_length_percentage(input, true, false)?,
        parse_length_percentage(input, true, false)?,
    ))
}

fn parse_to_by(input: &mut Parser<'_>) -> Result<bool> {
    if keyword(input, "to") {
        Ok(false)
    } else if keyword(input, "by") {
        Ok(true)
    } else {
        fail()
    }
}

fn parse_command(input: &mut Parser<'_>) -> Result<ShapeCommand<SpecifiedLengthPercentage>> {
    let name = input.expect_ident()?.to_ascii_lowercase();
    Ok(match name.as_str() {
        "close" => ShapeCommand::Close,
        "move" => ShapeCommand::Move {
            by: parse_to_by(input)?,
            to: parse_point(input)?,
        },
        "line" => ShapeCommand::Line {
            by: parse_to_by(input)?,
            to: parse_point(input)?,
        },
        "hline" => ShapeCommand::HLine {
            by: parse_to_by(input)?,
            x: parse_length_percentage(input, true, false)?,
        },
        "vline" => ShapeCommand::VLine {
            by: parse_to_by(input)?,
            y: parse_length_percentage(input, true, false)?,
        },
        "curve" => {
            let by = parse_to_by(input)?;
            let to = parse_point(input)?;
            if !keyword(input, "with") {
                return fail();
            }
            let mut controls = vec![parse_point(input)?];
            if input.try_parse(|input| input.expect_delim('/')).is_ok() {
                controls.push(parse_point(input)?);
            }
            ShapeCommand::Curve { by, to, controls }
        }
        "smooth" => {
            let by = parse_to_by(input)?;
            let to = parse_point(input)?;
            let control = if keyword(input, "with") {
                Some(parse_point(input)?)
            } else {
                None
            };
            ShapeCommand::Smooth { by, to, control }
        }
        "arc" => {
            let by = parse_to_by(input)?;
            let to = parse_point(input)?;
            if !keyword(input, "of") {
                return fail();
            }
            let rx = parse_length_percentage(input, true, true)?;
            let ry = input
                .try_parse(|input| parse_length_percentage(input, true, true))
                .unwrap_or_else(|_| rx.clone());
            let (mut clockwise, mut large, mut rotate) = (false, false, 0.0);
            loop {
                if keyword(input, "cw") {
                    clockwise = true;
                } else if keyword(input, "ccw") {
                    clockwise = false;
                } else if keyword(input, "large") {
                    large = true;
                } else if keyword(input, "small") {
                    large = false;
                } else if keyword(input, "rotate") {
                    rotate = parse_angle(input)?;
                } else {
                    break;
                }
            }
            ShapeCommand::Arc {
                by,
                to,
                radii: (rx, ry),
                clockwise,
                large,
                rotate,
            }
        }
        _ => return fail(),
    })
}

/// Parses a `<basic-shape>` function.
pub fn parse_basic_shape(input: &mut Parser<'_>) -> Result<BasicShape<SpecifiedLengthPercentage>> {
    let name = input.expect_function()?.to_ascii_lowercase();
    input.parse_nested_block(|input| {
        Ok(match name.as_str() {
            "inset" => {
                let insets = four(parse_lengths(input, false)?);
                BasicShape::Inset {
                    insets,
                    radii: parse_round(input)?,
                    function: InsetFunction::Inset,
                }
            }
            "rect" => {
                let mut edges = Vec::new();
                let mut auto = [false; 4];
                for (index, slot) in auto.iter_mut().enumerate() {
                    if keyword(input, "auto") {
                        *slot = true;
                        // An auto edge is on the reference box: 0% from the top and left,
                        // 100% for the right and bottom.
                        let at = if index == 1 || index == 2 { 1.0 } else { 0.0 };
                        edges.push(SpecifiedLengthPercentage::Percentage(at));
                    } else {
                        edges.push(parse_length_percentage(input, true, false)?);
                    }
                }
                BasicShape::Inset {
                    insets: [
                        edges[0].clone(),
                        edges[1].clone(),
                        edges[2].clone(),
                        edges[3].clone(),
                    ],
                    radii: parse_round(input)?,
                    function: InsetFunction::Rect(auto),
                }
            }
            "xywh" => {
                let x = parse_length_percentage(input, true, false)?;
                let y = parse_length_percentage(input, true, false)?;
                let w = parse_length_percentage(input, true, true)?;
                let h = parse_length_percentage(input, true, true)?;
                BasicShape::Inset {
                    insets: [y, w, h, x],
                    radii: parse_round(input)?,
                    function: InsetFunction::Xywh,
                }
            }
            "circle" => {
                let radius = input
                    .try_parse(parse_radius)
                    .unwrap_or(ShapeRadius::ClosestSide);
                BasicShape::Circle {
                    radius,
                    position: parse_at_position(input)?,
                }
            }
            "ellipse" => {
                let radii = input
                    .try_parse(|input| {
                        Ok::<_, ParseError<()>>((parse_radius(input)?, parse_radius(input)?))
                    })
                    .unwrap_or((ShapeRadius::ClosestSide, ShapeRadius::ClosestSide));
                BasicShape::Ellipse {
                    radii,
                    position: parse_at_position(input)?,
                }
            }
            "polygon" => {
                let fill = parse_fill(input);
                let points = input.parse_comma_separated(parse_point)?;
                BasicShape::Polygon { fill, points }
            }
            "path" => {
                let fill = parse_fill(input);
                let data = input.expect_string()?.as_ref().into();
                BasicShape::Path { fill, data }
            }
            "shape" => {
                let fill = input.try_parse(FillRule::parse).ok();
                if !keyword(input, "from") {
                    return fail();
                }
                let from = parse_point(input)?;
                input.expect_comma()?;
                let commands = input.parse_comma_separated(parse_command)?;
                BasicShape::Shape {
                    fill,
                    from,
                    commands,
                }
            }
            _ => fail()?,
        })
    })
}

impl BasicShape<SpecifiedLengthPercentage> {
    pub fn compute(&self, ctx: &ComputeContext) -> BasicShape<LengthPercentage> {
        let lp = |value: &SpecifiedLengthPercentage| value.compute(ctx, false);
        let point = |(x, y): &ShapePoint<SpecifiedLengthPercentage>| (lp(x), lp(y));
        let radius = |radius: &ShapeRadius<SpecifiedLengthPercentage>| match radius {
            ShapeRadius::Length(length) => ShapeRadius::Length(length.compute(ctx, true)),
            ShapeRadius::ClosestSide => ShapeRadius::ClosestSide,
            ShapeRadius::FarthestSide => ShapeRadius::FarthestSide,
        };
        let position = |position: &Option<(SpecifiedPosition, SpecifiedPosition)>| {
            position
                .as_ref()
                .map(|(x, y)| (x.compute(ctx, true), y.compute(ctx, false)))
        };
        match self {
            BasicShape::Inset {
                insets,
                radii,
                function,
            } => BasicShape::Inset {
                insets: insets.clone().map(|inset| lp(&inset)),
                radii: radii.as_ref().map(|radii| {
                    radii
                        .clone()
                        .map(|(h, v)| (h.compute(ctx, true), v.compute(ctx, true)))
                }),
                function: function.clone(),
            },
            BasicShape::Circle {
                radius: r,
                position: p,
            } => BasicShape::Circle {
                radius: radius(r),
                position: position(p),
            },
            BasicShape::Ellipse { radii, position: p } => BasicShape::Ellipse {
                radii: (radius(&radii.0), radius(&radii.1)),
                position: position(p),
            },
            BasicShape::Polygon { fill, points } => BasicShape::Polygon {
                fill: *fill,
                points: points.iter().map(point).collect(),
            },
            BasicShape::Path { fill, data } => BasicShape::Path {
                fill: *fill,
                data: data.clone(),
            },
            BasicShape::Shape {
                fill,
                from,
                commands,
            } => BasicShape::Shape {
                fill: *fill,
                from: point(from),
                commands: commands
                    .iter()
                    .map(|command| match command {
                        ShapeCommand::Move { by, to } => ShapeCommand::Move {
                            by: *by,
                            to: point(to),
                        },
                        ShapeCommand::Line { by, to } => ShapeCommand::Line {
                            by: *by,
                            to: point(to),
                        },
                        ShapeCommand::HLine { by, x } => ShapeCommand::HLine { by: *by, x: lp(x) },
                        ShapeCommand::VLine { by, y } => ShapeCommand::VLine { by: *by, y: lp(y) },
                        ShapeCommand::Curve { by, to, controls } => ShapeCommand::Curve {
                            by: *by,
                            to: point(to),
                            controls: controls.iter().map(point).collect(),
                        },
                        ShapeCommand::Smooth { by, to, control } => ShapeCommand::Smooth {
                            by: *by,
                            to: point(to),
                            control: control.as_ref().map(point),
                        },
                        ShapeCommand::Arc {
                            by,
                            to,
                            radii,
                            clockwise,
                            large,
                            rotate,
                        } => ShapeCommand::Arc {
                            by: *by,
                            to: point(to),
                            radii: (radii.0.compute(ctx, true), radii.1.compute(ctx, true)),
                            clockwise: *clockwise,
                            large: *large,
                            rotate: *rotate,
                        },
                        ShapeCommand::Close => ShapeCommand::Close,
                    })
                    .collect(),
            },
        }
    }
}

fn string_css(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn position_css((x, y): &(PositionComponent, PositionComponent)) -> String {
    let side = |p: &PositionComponent, end: &str| {
        let offset = length_percentage_css(&p.offset);
        if p.from_end {
            format!("{end} {offset}")
        } else {
            offset
        }
    };
    format!("{} {}", side(x, "right"), side(y, "bottom"))
}

fn radius_css(radius: &ShapeRadius<LengthPercentage>) -> String {
    match radius {
        ShapeRadius::Length(length) => length_percentage_css(length),
        ShapeRadius::ClosestSide => "closest-side".into(),
        ShapeRadius::FarthestSide => "farthest-side".into(),
    }
}

fn fill_css(fill: &Option<FillRule>, comma: bool) -> String {
    match fill {
        Some(fill) if comma => format!("{}, ", fill.as_str()),
        Some(fill) => format!("{} ", fill.as_str()),
        None => String::new(),
    }
}

impl BasicShape<LengthPercentage> {
    pub fn to_css(&self) -> String {
        let lp = length_percentage_css;
        let point = |(x, y): &ShapePoint<LengthPercentage>| format!("{} {}", lp(x), lp(y));
        let round = |radii: &Option<[CornerRadius; 4]>| match radii {
            None => String::new(),
            Some(radii) => {
                let h: Vec<String> = radii.iter().map(|(h, _)| lp(h)).collect();
                let v: Vec<String> = radii.iter().map(|(_, v)| lp(v)).collect();
                if h == v {
                    format!(" round {}", h.join(" "))
                } else {
                    format!(" round {} / {}", h.join(" "), v.join(" "))
                }
            }
        };
        match self {
            BasicShape::Inset {
                insets,
                radii,
                function,
            } => {
                let values: Vec<String> = insets.iter().map(lp).collect();
                match function {
                    InsetFunction::Inset => {
                        format!("inset({}{})", values.join(" "), round(radii))
                    }
                    InsetFunction::Rect(auto) => {
                        let edges: Vec<String> = values
                            .iter()
                            .zip(auto)
                            .map(|(v, auto)| if *auto { "auto".into() } else { v.clone() })
                            .collect();
                        format!("rect({}{})", edges.join(" "), round(radii))
                    }
                    InsetFunction::Xywh => format!(
                        "xywh({} {} {} {}{})",
                        values[3],
                        values[0],
                        values[1],
                        values[2],
                        round(radii)
                    ),
                }
            }
            BasicShape::Circle { radius, position } => {
                let mut out = format!("circle({}", radius_css(radius));
                if let Some(position) = position {
                    out.push_str(&format!(" at {}", position_css(position)));
                }
                out + ")"
            }
            BasicShape::Ellipse { radii, position } => {
                let mut out = format!("ellipse({} {}", radius_css(&radii.0), radius_css(&radii.1));
                if let Some(position) = position {
                    out.push_str(&format!(" at {}", position_css(position)));
                }
                out + ")"
            }
            BasicShape::Polygon { fill, points } => format!(
                "polygon({}{})",
                fill_css(fill, true),
                points.iter().map(point).collect::<Vec<_>>().join(", ")
            ),
            BasicShape::Path { fill, data } => {
                format!("path({}{})", fill_css(fill, true), string_css(data))
            }
            BasicShape::Shape {
                fill,
                from,
                commands,
            } => {
                let to_by = |by: &bool| if *by { "by" } else { "to" };
                let mut parts = vec![format!("{}from {}", fill_css(fill, false), point(from))];
                for command in commands {
                    parts.push(match command {
                        ShapeCommand::Move { by, to } => {
                            format!("move {} {}", to_by(by), point(to))
                        }
                        ShapeCommand::Line { by, to } => {
                            format!("line {} {}", to_by(by), point(to))
                        }
                        ShapeCommand::HLine { by, x } => format!("hline {} {}", to_by(by), lp(x)),
                        ShapeCommand::VLine { by, y } => format!("vline {} {}", to_by(by), lp(y)),
                        ShapeCommand::Curve { by, to, controls } => format!(
                            "curve {} {} with {}",
                            to_by(by),
                            point(to),
                            controls.iter().map(point).collect::<Vec<_>>().join(" / ")
                        ),
                        ShapeCommand::Smooth { by, to, control } => match control {
                            Some(control) => format!(
                                "smooth {} {} with {}",
                                to_by(by),
                                point(to),
                                point(control)
                            ),
                            None => format!("smooth {} {}", to_by(by), point(to)),
                        },
                        ShapeCommand::Arc {
                            by,
                            to,
                            radii,
                            clockwise,
                            large,
                            rotate,
                        } => {
                            let mut out = format!(
                                "arc {} {} of {}",
                                to_by(by),
                                point(to),
                                if radii.0 == radii.1 {
                                    lp(&radii.0)
                                } else {
                                    format!("{} {}", lp(&radii.0), lp(&radii.1))
                                }
                            );
                            if *clockwise {
                                out.push_str(" cw");
                            }
                            if *large {
                                out.push_str(" large");
                            }
                            if *rotate != 0.0 {
                                out.push_str(&format!(" rotate {rotate}deg"));
                            }
                            out
                        }
                        ShapeCommand::Close => "close".into(),
                    });
                }
                format!("shape({})", parts.join(", "))
            }
        }
    }
}

/// `shape-outside`: `none`, a basic shape and its reference box, a box alone, or an image
/// whose alpha channel is the shape.
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeOutside<L: PositionOf> {
    None,
    Shape {
        shape: Option<BasicShape<L>>,
        reference: Option<ShapeBox>,
    },
    Image(ImageValue),
}

impl Property for ShapeOutside<LengthPercentage> {
    type Specified = ShapeOutside<SpecifiedLengthPercentage>;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ShapeOutside::None => "none".into(),
            ShapeOutside::Shape { shape, reference } => {
                let parts: Vec<String> = shape
                    .iter()
                    .map(BasicShape::to_css)
                    .chain(reference.map(|r| r.as_str().to_owned()))
                    .collect();
                parts.join(" ")
            }
            ShapeOutside::Image(image) => image.to_css(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(ShapeOutside::None);
        }
        let (mut shape, mut reference) = (None, None);
        for _ in 0..2 {
            if shape.is_none()
                && let Ok(value) = input.try_parse(parse_basic_shape)
            {
                shape = Some(value);
            } else if reference.is_none()
                && let Ok(value) = input.try_parse(ShapeBox::parse)
            {
                reference = Some(value);
            }
        }
        if shape.is_some() || reference.is_some() {
            return Ok(ShapeOutside::Shape { shape, reference });
        }
        Ok(ShapeOutside::Image(ImageValue::parse(input)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self {
        match specified {
            ShapeOutside::None => ShapeOutside::None,
            ShapeOutside::Shape { shape, reference } => ShapeOutside::Shape {
                shape: shape.as_ref().map(|shape| shape.compute(ctx)),
                reference: *reference,
            },
            ShapeOutside::Image(image) => ShapeOutside::Image(image.compute(ctx)),
        }
    }
}

/// `shape-image-threshold`: the alpha above which an image's pixels are inside its shape,
/// clamped to 0 to 1.
pub struct ShapeImageThreshold;

impl Property for ShapeImageThreshold {
    type Specified = f32;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        crate::serialize::number(f64::from(*computed))
    }

    fn parse(input: &mut Parser<'_>) -> Result<f32> {
        parse_number(input)
    }

    fn compute(specified: &f32, _ctx: &ComputeContext) -> f32 {
        specified.clamp(0.0, 1.0)
    }
}
