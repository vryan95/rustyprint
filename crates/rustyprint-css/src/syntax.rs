//! Syntax definitions of registered custom properties and custom function parameters (CSS
//! Properties and Values API 1 § 5): what values they accept and how those compute.

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token, serialize_string};

use crate::properties::types::{ImageValue, Transform, parse_length_percentage, parse_number};
use crate::properties::{ComputeContext, Property};
use crate::serialize;
use crate::values::{Base, CalcContext, CalcNode, Color, NumericType, Unit, is_math_function};

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

/// A data type a syntax component names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Length,
    Number,
    Percentage,
    LengthPercentage,
    Color,
    Image,
    Url,
    Integer,
    Angle,
    Time,
    Resolution,
    TransformFunction,
    CustomIdent,
    TransformList,
    String,
}

impl DataType {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "length" => DataType::Length,
            "number" => DataType::Number,
            "percentage" => DataType::Percentage,
            "length-percentage" => DataType::LengthPercentage,
            "color" => DataType::Color,
            "image" => DataType::Image,
            "url" => DataType::Url,
            "integer" => DataType::Integer,
            "angle" => DataType::Angle,
            "time" => DataType::Time,
            "resolution" => DataType::Resolution,
            "transform-function" => DataType::TransformFunction,
            "custom-ident" => DataType::CustomIdent,
            "transform-list" => DataType::TransformList,
            "string" => DataType::String,
            _ => return None,
        })
    }

    /// Whether values of the type interpolate, rather than animating discretely.
    pub fn interpolates(self) -> bool {
        !matches!(
            self,
            DataType::Image | DataType::Url | DataType::CustomIdent | DataType::String
        )
    }
}

/// What one component of a syntax matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matcher {
    Type(DataType),
    /// An identifier, matched exactly.
    Ident(Arc<str>),
}

/// How many values a component takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multiplier {
    One,
    /// `+`: one or more, separated by spaces.
    Spaces,
    /// `#`: one or more, separated by commas.
    Commas,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub matcher: Matcher,
    pub multiplier: Multiplier,
}

/// A syntax definition: `*`, or components separated by `|`, the first that matches winning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Syntax {
    Universal,
    Components(Vec<Component>),
}

impl Syntax {
    /// Parses a syntax string, such as `<length> | auto` or `<color>#`.
    pub fn parse(text: &str) -> Option<Syntax> {
        let text = text.trim();
        if text == "*" {
            return Some(Syntax::Universal);
        }
        let mut components = Vec::new();
        for part in text.split('|') {
            let part = part.trim();
            let (body, multiplier) = if let Some(body) = part.strip_suffix('+') {
                (body, Multiplier::Spaces)
            } else if let Some(body) = part.strip_suffix('#') {
                (body, Multiplier::Commas)
            } else {
                (part, Multiplier::One)
            };
            let matcher = if let Some(name) = body.strip_prefix('<') {
                let name = name.strip_suffix('>')?;
                Matcher::Type(DataType::from_name(name)?)
            } else {
                let valid = !body.is_empty()
                    && body
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii())
                    && !body.starts_with(|c: char| c.is_ascii_digit());
                let reserved = matches!(
                    &*body.to_ascii_lowercase(),
                    "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default"
                );
                if !valid || reserved {
                    return None;
                }
                Matcher::Ident(Arc::from(body))
            };
            // A transform list is already a list.
            if matcher == Matcher::Type(DataType::TransformList) && multiplier != Multiplier::One {
                return None;
            }
            components.push(Component {
                matcher,
                multiplier,
            });
        }
        Some(Syntax::Components(components))
    }

    pub fn is_universal(&self) -> bool {
        *self == Syntax::Universal
    }

    /// Whether values interpolate when animated, as numbers, colors and transforms do.
    pub fn interpolates(&self) -> bool {
        match self {
            Syntax::Universal => false,
            Syntax::Components(components) => components.iter().all(|c| match c.matcher {
                Matcher::Type(ty) => ty.interpolates(),
                Matcher::Ident(_) => false,
            }),
        }
    }

    /// The computed value of `text` as text, or `None` if it does not match. The universal
    /// syntax keeps the tokens as they are.
    pub fn compute(&self, text: &str, ctx: &ComputeContext) -> Option<String> {
        let components = match self {
            Syntax::Universal => return Some(text.trim().to_owned()),
            Syntax::Components(components) => components,
        };
        components.iter().find_map(|component| {
            let mut input = Parser::new(text);
            input
                .parse_entirely(|input| compute_component(component, input, ctx))
                .ok()
        })
    }

    /// Whether `text` matches, computed against initial values.
    pub fn matches(&self, text: &str) -> bool {
        static INITIAL: std::sync::OnceLock<crate::properties::ComputedValues> =
            std::sync::OnceLock::new();
        let parent = INITIAL.get_or_init(crate::properties::ComputedValues::initial);
        let ctx = ComputeContext::initial(parent, (0.0, 0.0));
        self.compute(text, &ctx).is_some()
    }

    /// Whether `text` matches without depending on an element: no relative lengths or
    /// references, as an `initial-value` must.
    pub fn is_computationally_independent(&self, text: &str) -> bool {
        fn independent(input: &mut Parser<'_>) -> bool {
            while let Ok(token) = input.next_including_whitespace() {
                match token.clone() {
                    Token::Dimension { unit, .. } => {
                        if let Some(Unit::Length(length)) = Unit::from_name(&unit)
                            && length.absolute_px().is_none()
                        {
                            return false;
                        }
                    }
                    Token::Function(name) => {
                        let lower = name.to_ascii_lowercase();
                        if matches!(&*lower, "var" | "env" | "attr") {
                            return false;
                        }
                        let nested = input
                            .parse_nested_block(|input| Ok::<_, ParseError<()>>(independent(input)))
                            .unwrap_or(false);
                        if !nested {
                            return false;
                        }
                    }
                    Token::ParenthesisBlock
                    | Token::SquareBracketBlock
                    | Token::CurlyBracketBlock => {
                        let nested = input
                            .parse_nested_block(|input| Ok::<_, ParseError<()>>(independent(input)))
                            .unwrap_or(false);
                        if !nested {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
            true
        }
        independent(&mut Parser::new(text))
    }
}

fn compute_component(
    component: &Component,
    input: &mut Parser<'_>,
    ctx: &ComputeContext,
) -> Result<String> {
    let one = |input: &mut Parser<'_>| compute_one(&component.matcher, input, ctx);
    match component.multiplier {
        Multiplier::One => one(input),
        Multiplier::Spaces => {
            let mut values = vec![one(input)?];
            while !input.is_exhausted() {
                values.push(one(input)?);
            }
            Ok(values.join(" "))
        }
        Multiplier::Commas => Ok(input.parse_comma_separated(one)?.join(", ")),
    }
}

/// Parses one value of `matcher` and returns it computed, as text.
fn compute_one(matcher: &Matcher, input: &mut Parser<'_>, ctx: &ComputeContext) -> Result<String> {
    let ty = match matcher {
        Matcher::Ident(ident) => {
            input.expect_ident_matching(ident)?;
            return Ok(ident.to_string());
        }
        Matcher::Type(ty) => *ty,
    };
    Ok(match ty {
        DataType::Length => {
            let value = parse_length_percentage(input, false, false)?;
            serialize::px(value.compute(ctx, false).as_length().unwrap_or(0.0))
        }
        DataType::LengthPercentage => {
            let value = parse_length_percentage(input, true, false)?;
            crate::properties::types::length_percentage_css(&value.compute(ctx, false))
        }
        DataType::Percentage => match input.next()?.clone() {
            Token::Percentage { unit_value, .. } => serialize::percentage(unit_value),
            Token::Function(name) if is_math_function(&name) => {
                let node = CalcNode::parse_function(input, &name)?;
                if node.numeric_type(None) != Some(NumericType::of(Base::Percent)) {
                    return fail();
                }
                let value = node
                    .evaluate(&CalcContext::default())
                    .ok_or_else(|| ParseError::custom(()))?;
                format!("{}%", serialize::number(value))
            }
            _ => return fail(),
        },
        DataType::Number => serialize::number(f64::from(parse_number(input)?)),
        DataType::Integer => {
            let value = parse_number(input)?;
            // Math functions round to the nearest integer; literals must be integers.
            serialize::number(f64::from(value.round()))
        }
        DataType::Color => {
            let color = Color::parse(input)?;
            serialize::absolute_color(&color.resolve_in(&ctx.colors()))
        }
        DataType::Angle => format!("{}deg", serialize::number(dimension(input, Base::Angle)?)),
        DataType::Time => format!("{}s", serialize::number(dimension(input, Base::Time)?)),
        DataType::Resolution => format!(
            "{}dppx",
            serialize::number(dimension(input, Base::Resolution)?)
        ),
        DataType::Image => ImageValue::parse(input)?.compute(ctx).to_css(),
        DataType::Url => {
            let url = input.expect_url()?;
            let mut out = String::from("url(");
            let _ = serialize_string(&url, &mut out);
            out.push(')');
            out
        }
        DataType::CustomIdent => {
            let ident = input.expect_ident_cloned()?;
            let lower = ident.to_ascii_lowercase();
            if matches!(
                &*lower,
                "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default"
            ) {
                return fail();
            }
            ident.to_string()
        }
        DataType::String => {
            let string = input.expect_string_cloned()?;
            let mut out = String::new();
            let _ = serialize_string(&string, &mut out);
            out
        }
        DataType::TransformFunction => {
            // One function: its source, parsed as a transform list of one.
            let start = input.position();
            input.expect_function()?;
            input.parse_nested_block(|input| {
                while input.next().is_ok() {}
                Ok::<_, ParseError<()>>(())
            })?;
            let text = input.slice_from(start).to_owned();
            let functions = Parser::new(&text).parse_entirely(Transform::parse)?;
            Transform::to_css(&Transform::compute(&functions, ctx))
        }
        DataType::TransformList => {
            Transform::to_css(&Transform::compute(&Transform::parse(input)?, ctx))
        }
    })
}

/// A dimension of base type `base` in its canonical unit, from a literal or a math
/// function.
fn dimension(input: &mut Parser<'_>, base: Base) -> Result<f64> {
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
        .ok_or_else(|| ParseError::custom(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::ComputedValues;

    fn compute(syntax: &str, value: &str) -> Option<String> {
        let parent = ComputedValues::initial();
        let ctx = ComputeContext::initial(&parent, (800.0, 600.0));
        Syntax::parse(syntax)?.compute(value, &ctx)
    }

    #[test]
    fn parses_syntaxes() {
        assert_eq!(Syntax::parse(" * "), Some(Syntax::Universal));
        assert!(Syntax::parse("<length> | auto").is_some());
        assert!(Syntax::parse("<color>#").is_some());
        assert!(Syntax::parse("<transform-list>+").is_none());
        assert!(Syntax::parse("<made-up>").is_none());
        assert!(Syntax::parse("inherit").is_none());
    }

    #[test]
    fn computes_values() {
        assert_eq!(compute("<length>", "2em").as_deref(), Some("32px"));
        assert_eq!(compute("<length> | auto", "auto").as_deref(), Some("auto"));
        assert_eq!(compute("<length>", "red"), None);
        assert_eq!(compute("<color>", "red").as_deref(), Some("rgb(255, 0, 0)"));
        assert_eq!(compute("<angle>", "0.5turn").as_deref(), Some("180deg"));
        assert_eq!(compute("<time>", "250ms").as_deref(), Some("0.25s"));
        assert_eq!(compute("<integer>", "calc(2.6)").as_deref(), Some("3"));
        assert_eq!(compute("<number>+", "1 2  3").as_deref(), Some("1 2 3"));
        assert_eq!(compute("<length>#", "1px,2px").as_deref(), Some("1px, 2px"));
        assert_eq!(
            compute("<transform-function>", "translateX(1em)").as_deref(),
            Some("translateX(16px)")
        );
        assert_eq!(compute("*", " a  b ").as_deref(), Some("a  b"));
    }

    #[test]
    fn independence() {
        let syntax = Syntax::parse("<length>").unwrap();
        assert!(syntax.is_computationally_independent("10px"));
        assert!(!syntax.is_computationally_independent("1em"));
        assert!(!syntax.is_computationally_independent("calc(1px + var(--x))"));
    }
}
