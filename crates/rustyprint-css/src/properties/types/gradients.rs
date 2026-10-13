//! Gradients (CSS Images 3 § 3, CSS Images 4 § 3): `linear-gradient()`, `radial-gradient()`,
//! `conic-gradient()` and their repeating forms.

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{
    LengthPercentage, PositionComponent, Result, SpecifiedLengthPercentage, SpecifiedPosition,
    fail, length_percentage_css, parse_bg_position, parse_length_percentage,
};
use crate::properties::ComputeContext;
use crate::serialize;
use crate::values::{
    AbsoluteColor, Base, CalcContext, CalcNode, Color, ColorSpace, HueMethod, NumericType, Unit,
    is_math_function, parse_interpolation,
};

/// A computed gradient.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub repeating: bool,
    /// Color stops and transition hints, in order.
    pub items: Vec<GradientItem>,
    /// The color space colors interpolate in.
    pub space: ColorSpace,
    pub hue: HueMethod,
    /// Whether `in <color-space>` was given, for serialization.
    pub explicit_space: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GradientKind {
    Linear(LinearDirection),
    Radial {
        ellipse: bool,
        size: RadialSize,
        position: (PositionComponent, PositionComponent),
    },
    Conic {
        /// The starting angle in degrees, clockwise from the top.
        from: f32,
        position: (PositionComponent, PositionComponent),
    },
}

/// The direction of a linear gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinearDirection {
    /// Degrees clockwise from `to top`.
    Angle(f32),
    /// `to <corner>`: the gradient points at the corner, perpendicular to the diagonal
    /// through the other two.
    Corner { right: bool, bottom: bool },
}

/// How large a radial gradient's ending shape is.
#[derive(Debug, Clone, PartialEq)]
pub enum RadialSize {
    ClosestSide,
    FarthestSide,
    ClosestCorner,
    FarthestCorner,
    /// A circle's radius in pixels.
    Circle(f32),
    /// An ellipse's horizontal and vertical radii.
    Ellipse(LengthPercentage, LengthPercentage),
}

/// A color stop or a transition hint. Conic positions are fractions of a turn, as
/// percentages.
#[derive(Debug, Clone, PartialEq)]
pub enum GradientItem {
    Stop {
        color: AbsoluteColor,
        position: Option<LengthPercentage>,
    },
    /// Where the midpoint of the transition between the stops around it is.
    Hint(LengthPercentage),
}

/// A specified gradient, before lengths and colors compute.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecifiedGradient {
    kind: SpecifiedKind,
    repeating: bool,
    items: Vec<SpecifiedItem>,
    interpolation: Option<(ColorSpace, HueMethod)>,
}

#[derive(Debug, Clone, PartialEq)]
enum SpecifiedKind {
    Linear(LinearDirection),
    Radial {
        ellipse: bool,
        size: SpecifiedSize,
        position: (SpecifiedPosition, SpecifiedPosition),
    },
    Conic {
        from: f32,
        position: (SpecifiedPosition, SpecifiedPosition),
    },
}

#[derive(Debug, Clone, PartialEq)]
enum SpecifiedSize {
    Keyword(RadialSize),
    Circle(SpecifiedLengthPercentage),
    Ellipse(SpecifiedLengthPercentage, SpecifiedLengthPercentage),
}

#[derive(Debug, Clone, PartialEq)]
enum SpecifiedItem {
    Stop {
        color: Color,
        position: Option<SpecifiedLengthPercentage>,
    },
    Hint(SpecifiedLengthPercentage),
}

/// Parses an angle, or unitless zero, in degrees.
pub(super) fn parse_angle(input: &mut Parser<'_>) -> Result<f32> {
    let node = match input.next()?.clone() {
        Token::Number { value: 0.0, .. } => return Ok(0.0),
        Token::Dimension { value, unit, .. } => {
            let unit = Unit::from_name(&unit).ok_or_else(|| ParseError::custom(()))?;
            CalcNode::Dimension(f64::from(value), unit)
        }
        Token::Function(name) if is_math_function(&name) => CalcNode::parse_function(input, &name)?,
        _ => return fail(),
    };
    if node.numeric_type(None) != Some(NumericType::of(Base::Angle)) {
        return fail();
    }
    node.evaluate(&CalcContext::default())
        .map(|v| v as f32)
        .ok_or_else(|| ParseError::custom(()))
}

/// A conic stop position: an angle or a percentage, as a percentage of a turn.
fn parse_angle_percentage(input: &mut Parser<'_>) -> Result<SpecifiedLengthPercentage> {
    if let Ok(fraction) = input.try_parse(|input| input.expect_percentage()) {
        return Ok(SpecifiedLengthPercentage::Percentage(fraction));
    }
    Ok(SpecifiedLengthPercentage::Percentage(
        parse_angle(input)? / 360.0,
    ))
}

fn center() -> (SpecifiedPosition, SpecifiedPosition) {
    let center = || SpecifiedPosition {
        edge: super::PositionEdge::Center,
        offset: None,
    };
    (center(), center())
}

impl SpecifiedGradient {
    /// Parses a gradient function, whose name is `name`, inside its parentheses.
    pub(crate) fn parse_function(name: &str, input: &mut Parser<'_>) -> Result<Self> {
        let (base, repeating) = match name.strip_prefix("repeating-") {
            Some(base) => (base, true),
            None => (name, false),
        };
        let mut interpolation = None;
        // `in <color-space>`, which can come before or after the shape.
        let mut parse_in = |input: &mut Parser<'_>| {
            if interpolation.is_none()
                && let Ok(value) = input.try_parse(|input| {
                    input.expect_ident_matching("in")?;
                    parse_interpolation(input)
                })
            {
                interpolation = Some(value);
            }
        };
        let (kind, angular) = match base {
            "linear-gradient" => {
                parse_in(input);
                let direction = if let Ok(angle) = input.try_parse(parse_angle) {
                    Some(LinearDirection::Angle(angle))
                } else if input
                    .try_parse(|input| input.expect_ident_matching("to"))
                    .is_ok()
                {
                    Some(parse_side_or_corner(input)?)
                } else {
                    None
                };
                parse_in(input);
                if direction.is_some() || interpolation.is_some() {
                    input.expect_comma()?;
                }
                (
                    SpecifiedKind::Linear(direction.unwrap_or(LinearDirection::Angle(180.0))),
                    false,
                )
            }
            "radial-gradient" => {
                parse_in(input);
                let mut shape = None;
                let mut size = None;
                for _ in 0..2 {
                    if shape.is_none()
                        && let Ok(ident) = input.try_parse(|input| {
                            let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
                            match ident.as_str() {
                                "circle" | "ellipse" => Ok(ident),
                                _ => fail(),
                            }
                        })
                    {
                        shape = Some(ident == "ellipse");
                    } else if size.is_none()
                        && let Ok(value) = input.try_parse(parse_radial_size)
                    {
                        size = Some(value);
                    }
                }
                let position = if input
                    .try_parse(|input| input.expect_ident_matching("at"))
                    .is_ok()
                {
                    Some(parse_bg_position(input)?)
                } else {
                    None
                };
                parse_in(input);
                // A single length makes a circle, two make an ellipse.
                let ellipse = match (&size, shape) {
                    (Some(SpecifiedSize::Circle(_)), Some(true)) => return fail(),
                    (Some(SpecifiedSize::Ellipse(..)), Some(false)) => return fail(),
                    (Some(SpecifiedSize::Circle(_)), _) => false,
                    (_, Some(ellipse)) => ellipse,
                    _ => true,
                };
                if shape.is_some()
                    || size.is_some()
                    || position.is_some()
                    || interpolation.is_some()
                {
                    input.expect_comma()?;
                }
                (
                    SpecifiedKind::Radial {
                        ellipse,
                        size: size.unwrap_or(SpecifiedSize::Keyword(RadialSize::FarthestCorner)),
                        position: position.unwrap_or_else(center),
                    },
                    false,
                )
            }
            "conic-gradient" => {
                parse_in(input);
                let from = if input
                    .try_parse(|input| input.expect_ident_matching("from"))
                    .is_ok()
                {
                    Some(parse_angle(input)?)
                } else {
                    None
                };
                let position = if input
                    .try_parse(|input| input.expect_ident_matching("at"))
                    .is_ok()
                {
                    Some(parse_bg_position(input)?)
                } else {
                    None
                };
                parse_in(input);
                if from.is_some() || position.is_some() || interpolation.is_some() {
                    input.expect_comma()?;
                }
                (
                    SpecifiedKind::Conic {
                        from: from.unwrap_or(0.0),
                        position: position.unwrap_or_else(center),
                    },
                    true,
                )
            }
            _ => return fail(),
        };
        let items = parse_stops(input, angular)?;
        Ok(SpecifiedGradient {
            kind,
            repeating,
            items,
            interpolation,
        })
    }

    pub(crate) fn compute(&self, ctx: &ComputeContext) -> Gradient {
        let colors = ctx.colors();
        let items: Vec<GradientItem> = self
            .items
            .iter()
            .map(|item| match item {
                SpecifiedItem::Stop { color, position } => GradientItem::Stop {
                    color: color.resolve_in(&colors),
                    position: position.as_ref().map(|p| p.compute(ctx, false)),
                },
                SpecifiedItem::Hint(position) => GradientItem::Hint(position.compute(ctx, false)),
            })
            .collect();
        let kind = match &self.kind {
            SpecifiedKind::Linear(direction) => GradientKind::Linear(*direction),
            SpecifiedKind::Radial {
                ellipse,
                size,
                position,
            } => GradientKind::Radial {
                ellipse: *ellipse,
                size: match size {
                    SpecifiedSize::Keyword(keyword) => keyword.clone(),
                    SpecifiedSize::Circle(r) => {
                        RadialSize::Circle(r.compute(ctx, true).resolve(0.0))
                    }
                    SpecifiedSize::Ellipse(x, y) => {
                        RadialSize::Ellipse(x.compute(ctx, true), y.compute(ctx, true))
                    }
                },
                position: (
                    position.0.compute(ctx, true),
                    position.1.compute(ctx, false),
                ),
            },
            SpecifiedKind::Conic { from, position } => GradientKind::Conic {
                from: *from,
                position: (
                    position.0.compute(ctx, true),
                    position.1.compute(ctx, false),
                ),
            },
        };
        // Without `in`, gradients of legacy sRGB colors interpolate in sRGB, others in
        // Oklab (CSS Images 4 § 3.1).
        let legacy = self.items.iter().all(|item| match item {
            SpecifiedItem::Stop { color, .. } => match color {
                Color::Absolute(c) => {
                    matches!(
                        c.space,
                        ColorSpace::Srgb | ColorSpace::Hsl | ColorSpace::Hwb
                    )
                }
                _ => true,
            },
            SpecifiedItem::Hint(_) => true,
        });
        let (space, hue) = self.interpolation.unwrap_or(if legacy {
            (ColorSpace::Srgb, HueMethod::Shorter)
        } else {
            (ColorSpace::Oklab, HueMethod::Shorter)
        });
        Gradient {
            kind,
            repeating: self.repeating,
            items,
            space,
            hue,
            explicit_space: self.interpolation.is_some(),
        }
    }
}

fn parse_side_or_corner(input: &mut Parser<'_>) -> Result<LinearDirection> {
    let mut horizontal = None;
    let mut vertical = None;
    for _ in 0..2 {
        let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) else {
            break;
        };
        match &*ident.to_ascii_lowercase() {
            "left" if horizontal.is_none() => horizontal = Some(false),
            "right" if horizontal.is_none() => horizontal = Some(true),
            "top" if vertical.is_none() => vertical = Some(false),
            "bottom" if vertical.is_none() => vertical = Some(true),
            _ => return fail(),
        }
    }
    Ok(match (horizontal, vertical) {
        (Some(right), Some(bottom)) => LinearDirection::Corner { right, bottom },
        (Some(true), None) => LinearDirection::Angle(90.0),
        (Some(false), None) => LinearDirection::Angle(270.0),
        (None, Some(false)) => LinearDirection::Angle(0.0),
        (None, Some(true)) => LinearDirection::Angle(180.0),
        (None, None) => return fail(),
    })
}

fn parse_radial_size(input: &mut Parser<'_>) -> Result<SpecifiedSize> {
    if let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) {
        return Ok(SpecifiedSize::Keyword(match &*ident.to_ascii_lowercase() {
            "closest-side" => RadialSize::ClosestSide,
            "farthest-side" => RadialSize::FarthestSide,
            "closest-corner" => RadialSize::ClosestCorner,
            "farthest-corner" => RadialSize::FarthestCorner,
            _ => return fail(),
        }));
    }
    let first = parse_length_percentage(input, true, true)?;
    match input.try_parse(|input| parse_length_percentage(input, true, true)) {
        Ok(second) => Ok(SpecifiedSize::Ellipse(first, second)),
        // A circle's radius cannot be a percentage.
        Err(_) if matches!(first, SpecifiedLengthPercentage::Percentage(_)) => fail(),
        Err(_) => Ok(SpecifiedSize::Circle(first)),
    }
}

/// `<color-stop-list>`: stops with up to two positions each, and hints between stops.
fn parse_stops(input: &mut Parser<'_>, angular: bool) -> Result<Vec<SpecifiedItem>> {
    let position = |input: &mut Parser<'_>| {
        if angular {
            parse_angle_percentage(input)
        } else {
            parse_length_percentage(input, true, false)
        }
    };
    let mut items = Vec::new();
    let entries = input.parse_comma_separated(|input| {
        // A hint is a position alone.
        if let Ok(hint) = input.try_parse(|input| {
            let hint = position(input)?;
            input.expect_exhausted()?;
            Ok::<_, ParseError<()>>(hint)
        }) {
            return Ok(vec![SpecifiedItem::Hint(hint)]);
        }
        let color = Color::parse(input)?;
        let first = input.try_parse(position).ok();
        let second = first.as_ref().and_then(|_| input.try_parse(position).ok());
        let mut stops = vec![SpecifiedItem::Stop {
            color: color.clone(),
            position: first,
        }];
        if let Some(second) = second {
            stops.push(SpecifiedItem::Stop {
                color,
                position: Some(second),
            });
        }
        Ok(stops)
    })?;
    for entry in entries {
        items.extend(entry);
    }
    // At least two stops, and hints only between stops.
    let stops = items
        .iter()
        .filter(|i| matches!(i, SpecifiedItem::Stop { .. }))
        .count();
    let hint_at_edge = matches!(items.first(), Some(SpecifiedItem::Hint(_)))
        || matches!(items.last(), Some(SpecifiedItem::Hint(_)));
    let double_hint = items
        .windows(2)
        .any(|w| matches!(w, [SpecifiedItem::Hint(_), SpecifiedItem::Hint(_)]));
    if stops < 2 || hint_at_edge || double_hint {
        return fail();
    }
    Ok(items)
}

impl Gradient {
    pub fn to_css(&self) -> String {
        let mut prelude = Vec::new();
        let position_css = |p: &(PositionComponent, PositionComponent)| {
            let side = |c: &PositionComponent, end: &str| {
                let offset = length_percentage_css(&c.offset);
                if c.from_end {
                    format!("{end} {offset}")
                } else {
                    offset
                }
            };
            format!("{} {}", side(&p.0, "right"), side(&p.1, "bottom"))
        };
        let is_center = |p: &(PositionComponent, PositionComponent)| {
            let half = |c: &PositionComponent| {
                !c.from_end && c.offset == LengthPercentage::Percentage(0.5)
            };
            half(&p.0) && half(&p.1)
        };
        let (name, angular) = match &self.kind {
            GradientKind::Linear(direction) => {
                match direction {
                    LinearDirection::Angle(angle) if *angle == 180.0 => {}
                    LinearDirection::Angle(angle) => {
                        prelude.push(format!("{}deg", serialize::number(f64::from(*angle))));
                    }
                    LinearDirection::Corner { right, bottom } => prelude.push(format!(
                        "to {} {}",
                        if *bottom { "bottom" } else { "top" },
                        if *right { "right" } else { "left" }
                    )),
                }
                ("linear-gradient", false)
            }
            GradientKind::Radial {
                ellipse,
                size,
                position,
            } => {
                let mut shape = Vec::new();
                match size {
                    RadialSize::Circle(r) => {
                        shape.push(format!("{}px", serialize::number(f64::from(*r))));
                    }
                    RadialSize::Ellipse(x, y) => shape.push(format!(
                        "{} {}",
                        length_percentage_css(x),
                        length_percentage_css(y)
                    )),
                    keyword => {
                        if !*ellipse {
                            shape.push("circle".into());
                        }
                        let name = match keyword {
                            RadialSize::ClosestSide => "closest-side",
                            RadialSize::FarthestSide => "farthest-side",
                            RadialSize::ClosestCorner => "closest-corner",
                            _ => "",
                        };
                        if !name.is_empty() {
                            shape.push(name.into());
                        }
                    }
                }
                if !is_center(position) {
                    shape.push(format!("at {}", position_css(position)));
                }
                if !shape.is_empty() {
                    prelude.push(shape.join(" "));
                }
                ("radial-gradient", false)
            }
            GradientKind::Conic { from, position } => {
                let mut parts = Vec::new();
                if *from != 0.0 {
                    parts.push(format!("from {}deg", serialize::number(f64::from(*from))));
                }
                if !is_center(position) {
                    parts.push(format!("at {}", position_css(position)));
                }
                if !parts.is_empty() {
                    prelude.push(parts.join(" "));
                }
                ("conic-gradient", true)
            }
        };
        if self.explicit_space {
            let mut space = format!("in {}", self.space.name());
            if self.space.is_polar() && self.hue != HueMethod::Shorter {
                space = format!("{space} {} hue", self.hue.name());
            }
            match prelude.last_mut() {
                Some(last) => *last = format!("{last} {space}"),
                None => prelude.push(space),
            }
        }
        let position_css = |p: &LengthPercentage| match (angular, p) {
            (true, LengthPercentage::Percentage(f)) => {
                format!("{}deg", serialize::number(f64::from(*f) * 360.0))
            }
            _ => length_percentage_css(p),
        };
        let items = self.items.iter().map(|item| match item {
            GradientItem::Stop { color, position } => match position {
                Some(p) => format!("{} {}", serialize::absolute_color(color), position_css(p)),
                None => serialize::absolute_color(color),
            },
            GradientItem::Hint(p) => position_css(p),
        });
        let name = if self.repeating {
            format!("repeating-{name}")
        } else {
            name.to_owned()
        };
        let all: Vec<String> = prelude.into_iter().chain(items).collect();
        format!("{name}({})", all.join(", "))
    }
}

/// Whether `name` is a gradient function.
pub(crate) fn is_gradient(name: &str) -> bool {
    let base = name.strip_prefix("repeating-").unwrap_or(name);
    matches!(
        base,
        "linear-gradient" | "radial-gradient" | "conic-gradient"
    )
}

/// Shared ownership for the image value.
pub(crate) fn parse_gradient(name: &str, input: &mut Parser<'_>) -> Result<Arc<SpecifiedGradient>> {
    input
        .parse_nested_block(|input| SpecifiedGradient::parse_function(name, input))
        .map(Arc::new)
}
