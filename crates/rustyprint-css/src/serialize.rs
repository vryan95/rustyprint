//! Serializing computed values as CSS text, like `getComputedStyle()`.

use std::fmt::Write;

use crate::values::{
    AbsoluteColor, CalcNode, Color, ColorSpace, MathFunction, RoundStrategy, Unit,
};

/// A number with at most four decimals and no trailing zeros.
pub fn number(value: f64) -> String {
    if value.is_infinite() {
        return if value > 0.0 {
            "infinity".into()
        } else {
            "-infinity".into()
        };
    }
    let rounded = (value * 10_000.0).round() / 10_000.0;
    let mut text = format!("{rounded:.4}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" { "0".into() } else { text }
}

pub fn px(value: f32) -> String {
    format!("{}px", number(f64::from(value)))
}

pub fn percentage(fraction: f32) -> String {
    format!("{}%", number(f64::from(fraction) * 100.0))
}

/// A math expression, wrapped in `calc()` unless it is already a function.
pub fn calc(node: &CalcNode) -> String {
    match node {
        CalcNode::Function(..) | CalcNode::Anchor(_) => calc_node(node),
        _ => format!("calc({})", calc_node(node)),
    }
}

/// A math expression without a `calc()` around it, as inside another function.
pub fn calc_expression(node: &CalcNode) -> String {
    calc_node(node)
}

fn calc_node(node: &CalcNode) -> String {
    match node {
        CalcNode::Number(n) => number(*n),
        CalcNode::Percentage(p) => format!("{}%", number(*p)),
        CalcNode::Dimension(value, unit) => format!("{}{}", number(*value), unit_name(*unit)),
        CalcNode::SiblingIndex => "sibling-index()".into(),
        CalcNode::SizeRef => "size".into(),
        CalcNode::SiblingCount => "sibling-count()".into(),
        CalcNode::Anchor(function) => function.to_css(),
        CalcNode::Random(random) => {
            let mut options = Vec::new();
            if let Some(fixed) = random.fixed {
                options.push(format!("fixed {}", number(fixed)));
            }
            if let Some(name) = &random.name {
                options.push(name.to_string());
            }
            if random.element_shared {
                options.push("element-shared".into());
            }
            let mut args = Vec::new();
            if !options.is_empty() {
                args.push(options.join(" "));
            }
            args.push(calc_node(&random.min));
            args.push(calc_node(&random.max));
            if let Some(step) = &random.step {
                args.push(format!("by {}", calc_node(step)));
            }
            format!("random({})", args.join(", "))
        }
        CalcNode::Sum(terms) => {
            let mut out = calc_node(&terms[0]);
            for term in &terms[1..] {
                match term {
                    CalcNode::Negate(inner) => {
                        let _ = write!(out, " - {}", calc_node(inner));
                    }
                    other => {
                        let _ = write!(out, " + {}", calc_node(other));
                    }
                }
            }
            out
        }
        CalcNode::Product(factors) => {
            let mut out = calc_node(&factors[0]);
            for factor in &factors[1..] {
                match factor {
                    CalcNode::Invert(inner) => {
                        let _ = write!(out, " / {}", calc_node(inner));
                    }
                    other => {
                        let _ = write!(out, " * {}", calc_node(other));
                    }
                }
            }
            out
        }
        CalcNode::Negate(inner) => format!("-1 * {}", calc_node(inner)),
        CalcNode::Invert(inner) => format!("1 / {}", calc_node(inner)),
        CalcNode::Function(function, args) => {
            let name = match function {
                MathFunction::Min => "min",
                MathFunction::Max => "max",
                MathFunction::Clamp => "clamp",
                MathFunction::Round(_) => "round",
                MathFunction::Mod => "mod",
                MathFunction::Rem => "rem",
                MathFunction::Sin => "sin",
                MathFunction::Cos => "cos",
                MathFunction::Tan => "tan",
                MathFunction::Asin => "asin",
                MathFunction::Acos => "acos",
                MathFunction::Atan => "atan",
                MathFunction::Atan2 => "atan2",
                MathFunction::Pow => "pow",
                MathFunction::Sqrt => "sqrt",
                MathFunction::Hypot => "hypot",
                MathFunction::Log => "log",
                MathFunction::Exp => "exp",
                MathFunction::Abs => "abs",
                MathFunction::Sign => "sign",
                MathFunction::Progress => "progress",
            };
            let mut parts: Vec<String> = Vec::new();
            if let MathFunction::Round(strategy) = function
                && *strategy != RoundStrategy::Nearest
            {
                parts.push(
                    match strategy {
                        RoundStrategy::Up => "up",
                        RoundStrategy::Down => "down",
                        RoundStrategy::ToZero => "to-zero",
                        RoundStrategy::Nearest => unreachable!(),
                    }
                    .into(),
                );
            }
            parts.extend(args.iter().map(calc_node));
            format!("{name}({})", parts.join(", "))
        }
    }
}

fn unit_name(unit: Unit) -> &'static str {
    match unit {
        // Computed lengths are absolutized to pixels.
        Unit::Length(_) => "px",
        Unit::Deg => "deg",
        Unit::Grad => "grad",
        Unit::Rad => "rad",
        Unit::Turn => "turn",
        Unit::S => "s",
        Unit::Ms => "ms",
        Unit::Hz => "hz",
        Unit::KHz => "khz",
        Unit::Dppx => "dppx",
        Unit::Dpi => "dpi",
        Unit::Dpcm => "dpcm",
        Unit::Fr => "fr",
    }
}

/// CSS Color 4 § 15: legacy sRGB forms serialize as `rgb()`/`rgba()`; other spaces keep
/// their own function.
pub fn absolute_color(color: &AbsoluteColor) -> String {
    let component = |c: f32| {
        if c.is_nan() {
            "none".to_owned()
        } else {
            number(f64::from(c))
        }
    };
    let alpha = if color.alpha.is_nan() {
        " / none".to_owned()
    } else if color.alpha >= 1.0 {
        String::new()
    } else {
        format!(" / {}", number(f64::from(color.alpha)))
    };
    let [a, b, c] = color.components;
    match color.space {
        ColorSpace::Srgb | ColorSpace::Hsl | ColorSpace::Hwb => {
            let [r, g, b, alpha] = color.to_srgb();
            let channel = |v: f32| (v * 255.0).round() as u8;
            if alpha >= 1.0 {
                format!("rgb({}, {}, {})", channel(r), channel(g), channel(b))
            } else {
                format!(
                    "rgba({}, {}, {}, {})",
                    channel(r),
                    channel(g),
                    channel(b),
                    number(f64::from(alpha))
                )
            }
        }
        ColorSpace::Lab => format!(
            "lab({} {} {}{alpha})",
            component(a),
            component(b),
            component(c)
        ),
        ColorSpace::Lch => format!(
            "lch({} {} {}{alpha})",
            component(a),
            component(b),
            component(c)
        ),
        ColorSpace::Oklab => format!(
            "oklab({} {} {}{alpha})",
            component(a),
            component(b),
            component(c)
        ),
        ColorSpace::Oklch => format!(
            "oklch({} {} {}{alpha})",
            component(a),
            component(b),
            component(c)
        ),
        space => {
            let name = match space {
                ColorSpace::SrgbLinear => "srgb-linear",
                ColorSpace::DisplayP3 => "display-p3",
                ColorSpace::A98Rgb => "a98-rgb",
                ColorSpace::ProphotoRgb => "prophoto-rgb",
                ColorSpace::Rec2020 => "rec2020",
                ColorSpace::XyzD50 => "xyz-d50",
                _ => "xyz-d65",
            };
            format!(
                "color({name} {} {} {}{alpha})",
                component(a),
                component(b),
                component(c)
            )
        }
    }
}

pub fn color(color: &Color) -> String {
    match color {
        Color::Absolute(absolute) => absolute_color(absolute),
        Color::DeviceCmyk {
            cmyk: [c, m, y, k],
            alpha,
        } => {
            let n = |v: f32| number(f64::from(v));
            let alpha = if *alpha >= 1.0 {
                String::new()
            } else {
                format!(" / {}", n(*alpha))
            };
            format!(
                "device-cmyk({} {} {} {}{alpha})",
                n(*c),
                n(*m),
                n(*y),
                n(*k)
            )
        }
        Color::CurrentColor => "currentcolor".into(),
        Color::Deferred(source) => source.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(number(10.0), "10");
        assert_eq!(number(21.440001), "21.44");
        assert_eq!(number(-0.00001), "0");
        assert_eq!(number(0.5), "0.5");
    }
}
