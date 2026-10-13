//! Motion paths (CSS Motion Path 1): placing a box along a path with `offset-*`.

use std::sync::Arc;

use cssparser::Parser;

use super::gradients::parse_angle;
use super::{
    BasicShape, CoordBox, LengthPercentage, PositionComponent, Result, SpecifiedLengthPercentage,
    SpecifiedPosition, fail, length_percentage_css, parse_basic_shape, parse_bg_position,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

fn position_css(position: &(PositionComponent, PositionComponent)) -> String {
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
        side(&position.0, "right"),
        side(&position.1, "bottom")
    )
}

fn compute_position(
    position: &(SpecifiedPosition, SpecifiedPosition),
    ctx: &ComputeContext,
) -> (PositionComponent, PositionComponent) {
    (
        position.0.compute(ctx, true),
        position.1.compute(ctx, false),
    )
}

keywords!(
    /// `<ray-size>`: how far a ray reaches.
    RaySize {
        ClosestSide = "closest-side",
        ClosestCorner = "closest-corner",
        FarthestSide = "farthest-side",
        FarthestCorner = "farthest-corner",
        Sides = "sides",
    }
);

/// `ray()`: a straight path from a point at an angle.
#[derive(Debug, Clone, PartialEq)]
pub struct Ray<P> {
    /// Degrees clockwise from up.
    pub angle: f32,
    pub size: RaySize,
    /// Whether the path shortens so the box stays inside its containing block.
    pub contain: bool,
    pub at: Option<(P, P)>,
}

/// A path of `offset-path`, with lengths of type `L` and positions of type `P`.
#[derive(Debug, Clone, PartialEq)]
pub enum OffsetPathKind<L: super::PositionOf, P> {
    Ray(Ray<P>),
    /// A reference to an SVG shape.
    Url(Arc<str>),
    Shape(BasicShape<L>),
}

/// `offset-path`: `none | <offset-path> || <coord-box>`.
#[derive(Debug, Clone, PartialEq)]
pub enum OffsetPath<L: super::PositionOf, P> {
    None,
    Path {
        path: Option<OffsetPathKind<L, P>>,
        reference: Option<CoordBox>,
    },
}

type SpecifiedOffsetPath = OffsetPath<SpecifiedLengthPercentage, SpecifiedPosition>;

fn parse_ray(input: &mut Parser<'_>) -> Result<Ray<SpecifiedPosition>> {
    input.expect_function_matching("ray")?;
    input.parse_nested_block(|input| {
        let (mut angle, mut size, mut contain, mut at) = (None, None, false, None);
        loop {
            if angle.is_none()
                && let Ok(value) = input.try_parse(parse_angle)
            {
                angle = Some(value);
            } else if size.is_none()
                && let Ok(value) = input.try_parse(<RaySize as Property>::parse)
            {
                size = Some(value);
            } else if !contain && keyword(input, "contain") {
                contain = true;
            } else if at.is_none() && keyword(input, "at") {
                at = Some(parse_bg_position(input)?);
            } else {
                break;
            }
        }
        Ok(Ray {
            angle: angle.ok_or(cssparser::ParseError::custom(()))?,
            size: size.unwrap_or(RaySize::ClosestSide),
            contain,
            at,
        })
    })
}

impl OffsetPath<SpecifiedLengthPercentage, SpecifiedPosition> {
    pub(crate) fn parse_value(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(OffsetPath::None);
        }
        let (mut path, mut reference) = (None, None);
        for _ in 0..2 {
            if path.is_none()
                && let Ok(ray) = input.try_parse(parse_ray)
            {
                path = Some(OffsetPathKind::Ray(ray));
            } else if path.is_none()
                && let Ok(url) = input.try_parse(|input| input.expect_url().map(|u| u.to_string()))
            {
                path = Some(OffsetPathKind::Url(Arc::from(url)));
            } else if path.is_none()
                && let Ok(shape) = input.try_parse(parse_basic_shape)
            {
                path = Some(OffsetPathKind::Shape(shape));
            } else if reference.is_none()
                && let Ok(value) = input.try_parse(<CoordBox as Property>::parse)
            {
                reference = Some(value);
            }
        }
        if path.is_none() && reference.is_none() {
            return fail();
        }
        Ok(OffsetPath::Path { path, reference })
    }
}

impl Property for OffsetPath<LengthPercentage, PositionComponent> {
    type Specified = SpecifiedOffsetPath;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let OffsetPath::Path { path, reference } = computed else {
            return "none".into();
        };
        let mut parts = Vec::new();
        match path {
            Some(OffsetPathKind::Ray(ray)) => {
                let mut args = vec![format!("{}deg", serialize::number(f64::from(ray.angle)))];
                if ray.size != RaySize::ClosestSide {
                    args.push(ray.size.as_str().into());
                }
                if ray.contain {
                    args.push("contain".into());
                }
                if let Some(at) = &ray.at {
                    args.push(format!("at {}", position_css(at)));
                }
                parts.push(format!("ray({})", args.join(" ")));
            }
            Some(OffsetPathKind::Url(url)) => parts.push(format!("url(\"{url}\")")),
            Some(OffsetPathKind::Shape(shape)) => parts.push(shape.to_css()),
            None => {}
        }
        if let Some(reference) = reference {
            parts.push(reference.as_str().into());
        }
        parts.join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<SpecifiedOffsetPath> {
        SpecifiedOffsetPath::parse_value(input)
    }

    fn compute(specified: &SpecifiedOffsetPath, ctx: &ComputeContext) -> Self {
        match specified {
            OffsetPath::None => OffsetPath::None,
            OffsetPath::Path { path, reference } => OffsetPath::Path {
                path: path.as_ref().map(|path| match path {
                    OffsetPathKind::Ray(ray) => OffsetPathKind::Ray(Ray {
                        angle: ray.angle,
                        size: ray.size,
                        contain: ray.contain,
                        at: ray.at.as_ref().map(|at| compute_position(at, ctx)),
                    }),
                    OffsetPathKind::Url(url) => OffsetPathKind::Url(url.clone()),
                    OffsetPathKind::Shape(shape) => OffsetPathKind::Shape(shape.compute(ctx)),
                }),
                reference: *reference,
            },
        }
    }
}

/// `offset-position`: `normal | auto | <position>`.
#[derive(Debug, Clone, PartialEq)]
pub enum OffsetPosition<P> {
    Normal,
    Auto,
    Position(P, P),
}

impl OffsetPosition<SpecifiedPosition> {
    pub(crate) fn parse_value(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "normal") {
            return Ok(OffsetPosition::Normal);
        }
        if keyword(input, "auto") {
            return Ok(OffsetPosition::Auto);
        }
        let (x, y) = parse_bg_position(input)?;
        Ok(OffsetPosition::Position(x, y))
    }

    fn compute_value(&self, ctx: &ComputeContext) -> OffsetPosition<PositionComponent> {
        match self {
            OffsetPosition::Normal => OffsetPosition::Normal,
            OffsetPosition::Auto => OffsetPosition::Auto,
            OffsetPosition::Position(x, y) => {
                OffsetPosition::Position(x.compute(ctx, true), y.compute(ctx, false))
            }
        }
    }
}

impl OffsetPosition<PositionComponent> {
    fn css(&self) -> String {
        match self {
            OffsetPosition::Normal => "normal".into(),
            OffsetPosition::Auto => "auto".into(),
            OffsetPosition::Position(x, y) => position_css(&(x.clone(), y.clone())),
        }
    }
}

impl Property for OffsetPosition<PositionComponent> {
    type Specified = OffsetPosition<SpecifiedPosition>;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        computed.css()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        OffsetPosition::parse_value(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self {
        specified.compute_value(ctx)
    }
}

/// `offset-anchor`: `auto | <position>`.
pub struct OffsetAnchor;

impl OffsetAnchor {
    pub(crate) fn parse_value(
        input: &mut Parser<'_>,
    ) -> Result<Option<(SpecifiedPosition, SpecifiedPosition)>> {
        if keyword(input, "auto") {
            return Ok(None);
        }
        Ok(Some(parse_bg_position(input)?))
    }
}

impl Property for OffsetAnchor {
    type Specified = Option<(SpecifiedPosition, SpecifiedPosition)>;
    type Computed = Option<(PositionComponent, PositionComponent)>;

    fn to_css(computed: &Self::Computed) -> String {
        computed
            .as_ref()
            .map_or_else(|| "auto".into(), position_css)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        Self::parse_value(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.as_ref().map(|p| compute_position(p, ctx))
    }
}

/// `offset-rotate`: `[auto | reverse] || <angle>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetRotate {
    /// Whether the box turns with the path's direction (`auto`), or against it (`reverse`).
    pub auto: bool,
    pub reverse: bool,
    /// Degrees added to the path's direction, or the fixed rotation without `auto`.
    pub angle: f32,
}

impl OffsetRotate {
    pub const AUTO: Self = OffsetRotate {
        auto: true,
        reverse: false,
        angle: 0.0,
    };

    pub(crate) fn parse_value(input: &mut Parser<'_>) -> Result<Self> {
        let (mut auto, mut reverse, mut angle) = (false, false, None);
        for _ in 0..2 {
            if !auto && !reverse && keyword(input, "auto") {
                auto = true;
            } else if !auto && !reverse && keyword(input, "reverse") {
                reverse = true;
            } else if angle.is_none()
                && let Ok(value) = input.try_parse(parse_angle)
            {
                angle = Some(value);
            }
        }
        if !auto && !reverse && angle.is_none() {
            return fail();
        }
        Ok(OffsetRotate {
            auto: auto || reverse,
            reverse,
            angle: angle.unwrap_or(0.0),
        })
    }
}

impl Property for OffsetRotate {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        let angle = format!("{}deg", serialize::number(f64::from(c.angle)));
        match (c.auto, c.reverse) {
            (true, true) if c.angle == 0.0 => "reverse".into(),
            (true, true) => format!("reverse {angle}"),
            (true, false) if c.angle == 0.0 => "auto".into(),
            (true, false) => format!("auto {angle}"),
            _ => angle,
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Self::parse_value(input)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}
