//! Animations at time zero: where each animation is when the PDF is taken (Web Animations 1
//! § 4), and the values its keyframes give there (CSS Animations 1 § 3).
//!
//! Keyframe values are interpolated as computed values serialized to text: numbers in the
//! same places of both values are interpolated, colors in Oklab, and anything else is
//! discrete. Lengths and percentages mix through `calc()`, and `none` transforms and
//! filters interpolate from their identity.

use std::sync::Arc;

use cssparser::{Parser, ToCss, Token};

use crate::properties::PropertyDeclaration;
use crate::properties::types::{
    AnimationComposition, AnimationDirection, AnimationFillMode, Easing,
};
use crate::serialize;
use crate::values::{Color, ColorSpace, HueMethod, mix};

/// One keyframe of an `@keyframes` rule.
#[derive(Debug, Clone)]
pub(crate) struct Keyframe {
    pub offset: f32,
    pub declarations: Vec<PropertyDeclaration>,
    /// The keyframe's own `animation-timing-function`, for the interval it starts.
    pub easing: Option<Easing>,
    pub composition: Option<AnimationComposition>,
}

/// The keyframes of an `@keyframes` rule, in offset order.
#[derive(Debug, Clone, Default)]
pub(crate) struct Keyframes {
    pub frames: Vec<Keyframe>,
}

/// An animation's timing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    /// The iteration duration in seconds.
    pub duration: f32,
    pub delay: f32,
    pub iterations: f32,
    pub direction: AnimationDirection,
    pub fill: AnimationFillMode,
}

/// The directed progress of an animation at time zero, from 0 to 1, or `None` when it has no
/// effect then: before it starts without filling backwards, or after it ends without
/// filling forwards.
pub(crate) fn progress_at_zero(timing: Timing) -> Option<f32> {
    let Timing {
        duration,
        delay,
        iterations,
        direction,
        fill,
    } = timing;
    let duration = duration.max(0.0);
    let iterations = iterations.max(0.0);
    let active = if duration == 0.0 || iterations == 0.0 {
        0.0
    } else {
        duration * iterations
    };
    let end = (delay + active).max(0.0);
    let before_boundary = delay.min(end).max(0.0);
    let after_boundary = (delay + active).min(end).max(0.0);
    let backwards = matches!(fill, AnimationFillMode::Backwards | AnimationFillMode::Both);
    let forwards = matches!(fill, AnimationFillMode::Forwards | AnimationFillMode::Both);
    // The local time is zero.
    enum Phase {
        Before,
        Active,
        After,
    }
    let phase = if 0.0 < before_boundary {
        Phase::Before
    } else if 0.0 >= after_boundary {
        Phase::After
    } else {
        Phase::Active
    };
    let active_time = match phase {
        Phase::Before if backwards => 0.0,
        Phase::Active => -delay,
        Phase::After if forwards => (-delay).min(active).max(0.0),
        _ => return None,
    };
    let overall = if duration == 0.0 {
        match phase {
            Phase::Before => 0.0,
            _ => iterations,
        }
    } else {
        active_time / duration
    };
    let mut simple = if overall.is_infinite() {
        0.0
    } else {
        overall.rem_euclid(1.0)
    };
    let at_end = !matches!(phase, Phase::Before) && active_time == active && iterations != 0.0;
    if simple == 0.0 && at_end {
        simple = 1.0;
    }
    let mut iteration = overall.floor();
    if simple == 1.0 && at_end {
        iteration -= 1.0;
    }
    let even = iteration.is_infinite() || (iteration as i64) % 2 == 0;
    Some(match direction {
        AnimationDirection::Normal => simple,
        AnimationDirection::Reverse => 1.0 - simple,
        AnimationDirection::Alternate if even => simple,
        AnimationDirection::Alternate => 1.0 - simple,
        AnimationDirection::AlternateReverse if even => 1.0 - simple,
        AnimationDirection::AlternateReverse => simple,
    })
}

/// A property's keyframe: offset, value as text, easing of the interval it starts, and
/// whether its value adds to the underlying one.
pub(crate) struct PropertyFrame {
    pub offset: f32,
    pub value: String,
    pub easing: Easing,
    pub composition: AnimationComposition,
}

/// The value of one property at `progress` along its keyframes, which must include offsets
/// 0 and 1, composed onto `underlying`.
pub(crate) fn sample(
    property: &str,
    frames: &[PropertyFrame],
    underlying: &str,
    progress: f32,
) -> String {
    let composed = |frame: &PropertyFrame| match frame.composition {
        AnimationComposition::Replace => frame.value.clone(),
        AnimationComposition::Add | AnimationComposition::Accumulate => {
            add(property, underlying, &frame.value)
        }
    };
    // The interval: from the last keyframe at or before the progress, but never starting at
    // the last keyframe.
    let last = frames.len() - 1;
    let start = if progress >= 1.0 {
        frames
            .iter()
            .position(|f| f.offset >= 1.0)
            .unwrap_or(last)
            .saturating_sub(1)
    } else {
        frames
            .iter()
            .rposition(|f| f.offset <= progress)
            .unwrap_or(0)
            .min(last.saturating_sub(1))
    };
    let (a, b) = (&frames[start], &frames[(start + 1).min(last)]);
    if a.offset == b.offset {
        return composed(if progress < a.offset { a } else { b });
    }
    let local = (progress - a.offset) / (b.offset - a.offset);
    let eased = a.easing.apply(local);
    interpolate(property, &composed(a), &composed(b), eased)
}

/// A value part: a number with its unit (empty for numbers, `%` for percentages), or other
/// text.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    Number { value: f32, unit: String, int: bool },
    Text(String),
}

/// Splits a value into numbers and the text around them, normalized so `rgb()` with three
/// arguments becomes `rgba()` with four.
fn parts(text: &str) -> Option<Vec<Part>> {
    fn walk(input: &mut Parser<'_>, out: &mut Vec<Part>) -> Result<(), cssparser::ParseError<()>> {
        loop {
            let token = match input.next_including_whitespace() {
                Ok(token) => token.clone(),
                Err(_) => return Ok(()),
            };
            match &token {
                Token::Number {
                    value, int_value, ..
                } => out.push(Part::Number {
                    value: *value,
                    unit: String::new(),
                    int: int_value.is_some(),
                }),
                Token::Percentage { unit_value, .. } => out.push(Part::Number {
                    value: unit_value * 100.0,
                    unit: "%".into(),
                    int: false,
                }),
                Token::Dimension { value, unit, .. } => out.push(Part::Number {
                    value: *value,
                    unit: unit.to_ascii_lowercase(),
                    int: false,
                }),
                Token::WhiteSpace(_) => out.push(Part::Text(" ".into())),
                Token::Function(name) => {
                    let name = name.to_ascii_lowercase();
                    let start = out.len();
                    out.push(Part::Text(format!("{name}(")));
                    input.parse_nested_block(|input| walk(input, out))?;
                    if name == "rgb" {
                        let commas = out[start..]
                            .iter()
                            .filter(|p| **p == Part::Text(",".into()))
                            .count();
                        if commas == 2 {
                            out[start] = Part::Text("rgba(".into());
                            out.push(Part::Text(",".into()));
                            out.push(Part::Text(" ".into()));
                            out.push(Part::Number {
                                value: 1.0,
                                unit: String::new(),
                                int: false,
                            });
                        }
                    }
                    out.push(Part::Text(")".into()));
                }
                Token::ParenthesisBlock | Token::SquareBracketBlock | Token::CurlyBracketBlock => {
                    let (open, close) = match token {
                        Token::ParenthesisBlock => ("(", ")"),
                        Token::SquareBracketBlock => ("[", "]"),
                        _ => ("{", "}"),
                    };
                    out.push(Part::Text(open.into()));
                    input.parse_nested_block(|input| walk(input, out))?;
                    out.push(Part::Text(close.into()));
                }
                other => out.push(Part::Text(other.to_css_string())),
            }
        }
    }
    let mut out = Vec::new();
    walk(&mut Parser::new(text), &mut out).ok()?;
    Some(out)
}

fn number_text(value: f32, unit: &str, int: bool) -> String {
    let value = if int { value.round() } else { value };
    format!("{}{unit}", serialize::number(f64::from(value)))
}

/// Combines two values with the same structure number by number. `mixed` builds the text
/// for numbers whose units differ, or `None` if they cannot combine.
fn combine(
    a: &[Part],
    b: &[Part],
    op: impl Fn(f32, f32) -> f32,
    mixed: impl Fn(&str, &str) -> Option<String>,
) -> Option<String> {
    if a.len() != b.len() {
        return None;
    }
    let mut out = String::new();
    for (x, y) in a.iter().zip(b) {
        match (x, y) {
            (Part::Text(x), Part::Text(y)) if x.eq_ignore_ascii_case(y) => out.push_str(x),
            (
                Part::Number {
                    value: vx,
                    unit: ux,
                    int: ix,
                },
                Part::Number {
                    value: vy,
                    unit: uy,
                    int: iy,
                },
            ) => {
                if ux == uy {
                    out.push_str(&number_text(op(*vx, *vy), ux, *ix && *iy));
                } else {
                    let x = number_text(*vx, ux, false);
                    let y = number_text(*vy, uy, false);
                    out.push_str(&mixed(&x, &y)?);
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The identity of a transform, filter or individual transform value, which `none`
/// interpolates as.
fn identity(property: &str, value: &[Part]) -> Option<Vec<Part>> {
    let mut out = value.to_vec();
    let mut function: Vec<String> = Vec::new();
    let mut matrix_index = 0;
    for part in &mut out {
        match part {
            Part::Text(text) if text.ends_with('(') => {
                function.push(text.trim_end_matches('(').to_owned());
                matrix_index = 0;
            }
            Part::Text(text) if text == ")" => {
                function.pop();
            }
            Part::Number { value, .. } => {
                let name = function.last().map(String::as_str).unwrap_or("");
                *value = match (property, name) {
                    ("scale", "") => 1.0,
                    (_, "scale" | "scalex" | "scaley" | "scalez" | "scale3d") => 1.0,
                    (_, "brightness" | "contrast" | "opacity" | "saturate") => 1.0,
                    (_, "matrix") => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0][matrix_index.min(5)],
                    // The diagonal of a 4 × 4 matrix, every fifth value.
                    (_, "matrix3d") if matrix_index % 5 == 0 => 1.0,
                    _ => 0.0,
                };
                matrix_index += 1;
            }
            _ => {}
        }
    }
    matches!(
        property,
        "transform" | "translate" | "rotate" | "scale" | "filter" | "backdrop-filter"
    )
    .then_some(out)
}

/// The value `t` of the way from `from` to `to` (outside 0..1 for easings that overshoot).
pub(crate) fn interpolate(property: &str, from: &str, to: &str, t: f32) -> String {
    let discrete = || {
        if t < 0.5 {
            from.to_owned()
        } else {
            to.to_owned()
        }
    };
    if from == to {
        return from.to_owned();
    }
    // `visible` wins whenever one end is visible (CSS Display 4 § 4).
    if property == "visibility" {
        if t > 0.0 && t < 1.0 && (from == "visible" || to == "visible") {
            return "visible".into();
        }
        return discrete();
    }
    if let (Some(a), Some(b)) = (whole_color(from), whole_color(to)) {
        let mut mixed = mix(ColorSpace::Oklab, HueMethod::Shorter, &a, &b, t);
        // Legacy colors stay in sRGB, as they serialize.
        if a.space == ColorSpace::Srgb && b.space == ColorSpace::Srgb {
            mixed = mixed.to_space(ColorSpace::Srgb);
        }
        return serialize::absolute_color(&mixed);
    }
    // Unregistered custom properties hold tokens, which animate discretely.
    if property.starts_with("--") {
        return discrete();
    }
    let (Some(mut a), Some(mut b)) = (parts(from), parts(to)) else {
        return discrete();
    };
    if from == "none" {
        if let Some(identity) = identity(property, &b) {
            a = identity;
        }
    } else if to == "none"
        && let Some(identity) = identity(property, &a)
    {
        b = identity;
    }
    combine(
        &a,
        &b,
        |x, y| x + (y - x) * t,
        |x, y| {
            Some(format!(
                "calc({x} * {} + {y} * {})",
                serialize::number(f64::from(1.0 - t)),
                serialize::number(f64::from(t))
            ))
        },
    )
    .unwrap_or_else(discrete)
}

/// `value` added to `underlying`, for `animation-composition: add` and `accumulate`:
/// numbers add, transform lists concatenate, and other values replace.
pub(crate) fn add(property: &str, underlying: &str, value: &str) -> String {
    if property == "transform" {
        return match (underlying, value) {
            ("none", value) => value.to_owned(),
            (underlying, "none") => underlying.to_owned(),
            (underlying, value) => format!("{underlying} {value}"),
        };
    }
    let (Some(a), Some(b)) = (parts(underlying), parts(value)) else {
        return value.to_owned();
    };
    combine(
        &a,
        &b,
        |x, y| x + y,
        |x, y| Some(format!("calc({x} + {y})")),
    )
    .unwrap_or_else(|| value.to_owned())
}

/// The color a whole value is, if it is one.
fn whole_color(text: &str) -> Option<crate::values::AbsoluteColor> {
    let mut input = Parser::new(text);
    match input.parse_entirely(Color::parse).ok()? {
        Color::Absolute(color) => Some(color),
        _ => None,
    }
}

/// The animations named in `names` that exist, with their index in the list.
pub(crate) fn named<'k>(
    names: &[crate::properties::types::AnimationName],
    keyframes: &'k std::collections::HashMap<Arc<str>, Keyframes>,
) -> Vec<(usize, &'k Keyframes)> {
    names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| Some((index, keyframes.get(name.name()?)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(duration: f32, delay: f32, fill: AnimationFillMode) -> Timing {
        Timing {
            duration,
            delay,
            iterations: 1.0,
            direction: AnimationDirection::Normal,
            fill,
        }
    }

    #[test]
    fn phases_at_time_zero() {
        use AnimationFillMode::*;
        assert_eq!(progress_at_zero(timing(1.0, 0.0, None)), Some(0.0));
        assert_eq!(progress_at_zero(timing(1.0, 1.0, None)), Option::None);
        assert_eq!(progress_at_zero(timing(1.0, 1.0, Backwards)), Some(0.0));
        assert_eq!(progress_at_zero(timing(4.0, -1.0, None)), Some(0.25));
        assert_eq!(progress_at_zero(timing(1.0, -2.0, None)), Option::None);
        assert_eq!(progress_at_zero(timing(1.0, -2.0, Forwards)), Some(1.0));
        assert_eq!(progress_at_zero(timing(0.0, 0.0, Both)), Some(1.0));
        let mut alternate = timing(1.0, -1.5, None);
        alternate.iterations = 3.0;
        alternate.direction = AnimationDirection::Alternate;
        assert_eq!(progress_at_zero(alternate), Some(0.5));
        alternate.delay = -1.25;
        assert_eq!(progress_at_zero(alternate), Some(0.75));
        let mut reverse = timing(2.0, -0.5, None);
        reverse.direction = AnimationDirection::Reverse;
        assert_eq!(progress_at_zero(reverse), Some(0.75));
    }

    #[test]
    fn interpolates_values() {
        assert_eq!(interpolate("width", "10px", "20px", 0.25), "12.5px");
        assert_eq!(interpolate("z-index", "1", "4", 0.5), "3");
        assert_eq!(
            interpolate("width", "10px", "50%", 0.5),
            "calc(10px * 0.5 + 50% * 0.5)"
        );
        assert_eq!(
            interpolate("transform", "none", "translate(10px, 20px)", 0.5),
            "translate(5px, 10px)"
        );
        assert_eq!(
            interpolate("transform", "scale(3)", "none", 0.5),
            "scale(2)"
        );
        assert_eq!(interpolate("display", "block", "none", 0.4), "block");
        assert_eq!(interpolate("display", "block", "none", 0.6), "none");
        assert_eq!(
            interpolate("visibility", "hidden", "visible", 0.1),
            "visible"
        );
        assert_eq!(
            interpolate(
                "box-shadow",
                "rgb(0, 0, 0) 0px 0px 0px 0px",
                "rgba(0, 0, 0, 0) 2px 2px 0px 0px",
                0.5
            ),
            "rgba(0, 0, 0, 0.5) 1px 1px 0px 0px"
        );
        assert_eq!(
            interpolate("color", "rgb(255, 0, 0)", "rgb(255, 0, 0)", 0.5),
            "rgb(255, 0, 0)"
        );
    }

    #[test]
    fn adds_values() {
        assert_eq!(add("width", "10px", "5px"), "15px");
        assert_eq!(
            add("transform", "rotate(10deg)", "scale(2)"),
            "rotate(10deg) scale(2)"
        );
        assert_eq!(add("display", "block", "none"), "none");
    }

    #[test]
    fn samples_keyframes() {
        let frame = |offset: f32, value: &str| PropertyFrame {
            offset,
            value: value.into(),
            easing: Easing::Linear,
            composition: AnimationComposition::Replace,
        };
        let frames = [frame(0.0, "0px"), frame(0.5, "10px"), frame(1.0, "30px")];
        assert_eq!(sample("width", &frames, "0px", 0.25), "5px");
        assert_eq!(sample("width", &frames, "0px", 0.75), "20px");
        assert_eq!(sample("width", &frames, "0px", 1.0), "30px");
        assert_eq!(sample("width", &frames, "0px", 0.0), "0px");
    }
}
