//! Media queries, evaluated for paged print output (see `css-at-rules.md`).

use std::collections::HashMap;

use cssparser::{ParseError, Parser, Token};

use crate::values::{Length, LengthContext, LengthUnit};

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

/// A comma-separated list of media queries. An empty list matches everything.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaQueryList(pub Vec<MediaQuery>);

#[derive(Debug, Clone, PartialEq)]
pub enum MediaQuery {
    /// `[not | only]? <type> [and <condition>]?`
    Typed {
        not: bool,
        media_type: MediaType,
        condition: Option<MediaCondition>,
    },
    /// A bare condition, such as `(min-width: 20em)`.
    Condition(MediaCondition),
    /// A query that failed to parse; it never matches.
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaType {
    All,
    Print,
    /// Any other type, including `screen`; none of them match.
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaCondition {
    Not(Box<MediaCondition>),
    And(Vec<MediaCondition>),
    Or(Vec<MediaCondition>),
    Feature(MediaFeature),
    /// `(--name)`, defined by `@custom-media`.
    Custom(String),
    /// Syntax this engine does not recognise (`<general-enclosed>`); evaluates to unknown.
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaFeature {
    /// `(name)`
    Boolean(String),
    /// `(name: value)`, `(min-name: value)` and `(max-name: value)`.
    Plain {
        name: String,
        prefix: Option<RangePrefix>,
        value: MediaValue,
    },
    /// `(name op value)`, `(value op name)` or `(value op name op value)`, normalised so the
    /// feature is always on the left: `name op value`.
    Range {
        name: String,
        comparisons: Vec<(Comparison, MediaValue)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangePrefix {
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

impl Comparison {
    /// The comparison with its operands swapped: `a < b` is `b > a`.
    fn flip(self) -> Self {
        match self {
            Comparison::Lt => Comparison::Gt,
            Comparison::Le => Comparison::Ge,
            Comparison::Gt => Comparison::Lt,
            Comparison::Ge => Comparison::Le,
            Comparison::Eq => Comparison::Eq,
        }
    }

    pub(crate) fn holds(self, a: f64, b: f64) -> bool {
        match self {
            Comparison::Lt => a < b,
            Comparison::Le => a <= b,
            Comparison::Gt => a > b,
            Comparison::Ge => a >= b,
            Comparison::Eq => a == b,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaValue {
    Number(f32),
    Length(Length),
    /// Dots per CSS pixel.
    Resolution(f32),
    Ratio(f32, f32),
    Ident(String),
}

/// A `@custom-media` definition.
#[derive(Debug, Clone, PartialEq)]
pub enum CustomMedia {
    Queries(MediaQueryList),
    True,
    False,
}

/// What media queries are evaluated against.
#[derive(Debug, Clone, Copy)]
pub struct MediaEnvironment<'a> {
    /// The page area, in CSS pixels.
    pub width: f32,
    pub height: f32,
    pub custom_media: &'a HashMap<String, CustomMedia>,
}

// ---------------------------------------------------------------------------------------------
// Parsing

impl MediaQueryList {
    /// Parses a media query list. Invalid queries become [`MediaQuery::Invalid`] instead of
    /// invalidating the whole list.
    pub fn parse(input: &mut Parser<'_>) -> Self {
        if input.is_exhausted() {
            return Self::default();
        }
        let mut queries = Vec::new();
        loop {
            let query = input
                .parse_until_before(cssparser::Delimiter::Comma, |input| {
                    input.parse_entirely(parse_media_query)
                })
                .unwrap_or(MediaQuery::Invalid);
            queries.push(query);
            match input.next() {
                Ok(Token::Comma) => continue,
                _ => break,
            }
        }
        Self(queries)
    }
}

fn parse_media_query(input: &mut Parser<'_>) -> Result<MediaQuery> {
    if let Ok(condition) = input.try_parse(parse_condition) {
        return Ok(MediaQuery::Condition(condition));
    }
    let not = input
        .try_parse(|input| input.expect_ident_matching("not"))
        .is_ok();
    if !not {
        let _ = input.try_parse(|input| input.expect_ident_matching("only"));
    }
    let name = input.expect_ident()?.to_ascii_lowercase();
    let media_type = match name.as_str() {
        "all" => MediaType::All,
        "print" => MediaType::Print,
        "only" | "not" | "and" | "or" | "layer" => return fail(),
        _ => MediaType::Other(name),
    };
    let condition = if input
        .try_parse(|input| input.expect_ident_matching("and"))
        .is_ok()
    {
        Some(parse_condition_without_or(input)?)
    } else {
        None
    };
    Ok(MediaQuery::Typed {
        not,
        media_type,
        condition,
    })
}

/// `<media-condition>`: `not` or an `and`/`or` chain.
fn parse_condition(input: &mut Parser<'_>) -> Result<MediaCondition> {
    parse_condition_inner(input, true)
}

fn parse_condition_without_or(input: &mut Parser<'_>) -> Result<MediaCondition> {
    parse_condition_inner(input, false)
}

fn parse_condition_inner(input: &mut Parser<'_>, allow_or: bool) -> Result<MediaCondition> {
    if input
        .try_parse(|input| input.expect_ident_matching("not"))
        .is_ok()
    {
        return Ok(MediaCondition::Not(Box::new(parse_in_parens(input)?)));
    }
    let first = parse_in_parens(input)?;
    let operator = match input.try_parse(|input| input.expect_ident_cloned()) {
        Ok(op) if op.eq_ignore_ascii_case("and") => "and",
        Ok(op) if op.eq_ignore_ascii_case("or") && allow_or => "or",
        Ok(_) => return fail(),
        Err(_) => return Ok(first),
    };
    let mut conditions = vec![first, parse_in_parens(input)?];
    while input
        .try_parse(|input| input.expect_ident_matching(operator))
        .is_ok()
    {
        conditions.push(parse_in_parens(input)?);
    }
    Ok(if operator == "and" {
        MediaCondition::And(conditions)
    } else {
        MediaCondition::Or(conditions)
    })
}

/// `( <media-condition> )`, `( <media-feature> )` or `<general-enclosed>`.
fn parse_in_parens(input: &mut Parser<'_>) -> Result<MediaCondition> {
    match input.next()?.clone() {
        Token::ParenthesisBlock => input.parse_nested_block(|input| {
            if let Ok(condition) = input.try_parse(|input| input.parse_entirely(parse_condition)) {
                return Ok(condition);
            }
            if let Ok(feature) = input.try_parse(|input| input.parse_entirely(parse_feature)) {
                return Ok(feature);
            }
            while input.next().is_ok() {}
            Ok(MediaCondition::Unknown)
        }),
        Token::Function(_) => input.parse_nested_block(|input| {
            while input.next().is_ok() {}
            Ok(MediaCondition::Unknown)
        }),
        _ => fail(),
    }
}

pub(crate) fn parse_feature(input: &mut Parser<'_>) -> Result<MediaCondition> {
    // `(name)`, `(name: value)` or `(name op value)`.
    if let Ok(name) = input.try_parse(|input| input.expect_ident_cloned()) {
        let name = name.to_string();
        if input.is_exhausted() {
            if name.starts_with("--") {
                return Ok(MediaCondition::Custom(name));
            }
            return Ok(MediaCondition::Feature(MediaFeature::Boolean(
                name.to_ascii_lowercase(),
            )));
        }
        let name_lower = name.to_ascii_lowercase();
        if input.try_parse(|input| input.expect_colon()).is_ok() {
            let value = parse_value(input)?;
            let (prefix, name) = if let Some(rest) = name_lower.strip_prefix("min-") {
                (Some(RangePrefix::Min), rest.to_owned())
            } else if let Some(rest) = name_lower.strip_prefix("max-") {
                (Some(RangePrefix::Max), rest.to_owned())
            } else {
                (None, name_lower)
            };
            return Ok(MediaCondition::Feature(MediaFeature::Plain {
                name,
                prefix,
                value,
            }));
        }
        let comparison = parse_comparison(input)?;
        let value = parse_value(input)?;
        return Ok(MediaCondition::Feature(MediaFeature::Range {
            name: name_lower,
            comparisons: vec![(comparison, value)],
        }));
    }
    // `(value op name)` or `(value op name op value)`.
    let low = parse_value(input)?;
    let first = parse_comparison(input)?;
    let name = input.expect_ident()?.to_ascii_lowercase();
    let mut comparisons = vec![(first.flip(), low)];
    if !input.is_exhausted() {
        let second = parse_comparison(input)?;
        // Both comparisons must point the same way: `a < x < b` or `a > x > b`.
        let ascending = matches!(first, Comparison::Lt | Comparison::Le);
        if matches!(second, Comparison::Eq)
            || ascending != matches!(second, Comparison::Lt | Comparison::Le)
        {
            return fail();
        }
        comparisons.push((second, parse_value(input)?));
    }
    Ok(MediaCondition::Feature(MediaFeature::Range {
        name,
        comparisons,
    }))
}

fn parse_comparison(input: &mut Parser<'_>) -> Result<Comparison> {
    let first = match input.next()?.clone() {
        Token::Delim('<') => '<',
        Token::Delim('>') => '>',
        Token::Delim('=') => return Ok(Comparison::Eq),
        _ => return fail(),
    };
    // `<=` and `>=` must not contain whitespace.
    let equals = input
        .try_parse(|input| match input.next_including_whitespace()? {
            Token::Delim('=') => Ok(()),
            _ => fail(),
        })
        .is_ok();
    Ok(match (first, equals) {
        ('<', false) => Comparison::Lt,
        ('<', true) => Comparison::Le,
        ('>', false) => Comparison::Gt,
        _ => Comparison::Ge,
    })
}

fn parse_value(input: &mut Parser<'_>) -> Result<MediaValue> {
    let token = input.next()?.clone();
    let value = match token {
        Token::Number { value, .. } => {
            // `<ratio>`: `a / b`.
            if input.try_parse(|input| input.expect_delim('/')).is_ok() {
                let denominator = input.expect_number()?;
                if value < 0.0 || denominator < 0.0 {
                    return fail();
                }
                return Ok(MediaValue::Ratio(value, denominator));
            }
            MediaValue::Number(value)
        }
        Token::Dimension {
            value, ref unit, ..
        } => {
            if let Some(unit) = LengthUnit::from_name(unit) {
                MediaValue::Length(Length { value, unit })
            } else {
                let dppx = match &*unit.to_ascii_lowercase() {
                    "dppx" | "x" => 1.0,
                    "dpi" => 1.0 / 96.0,
                    "dpcm" => 2.54 / 96.0,
                    _ => return fail(),
                };
                MediaValue::Resolution(value * dppx)
            }
        }
        Token::Ident(ref ident) => MediaValue::Ident(ident.to_ascii_lowercase()),
        _ => return fail(),
    };
    Ok(value)
}

// ---------------------------------------------------------------------------------------------
// Evaluation

impl MediaQueryList {
    /// Whether the list matches: an empty list matches, otherwise any query must match.
    pub fn matches(&self, env: &MediaEnvironment) -> bool {
        self.0.is_empty() || self.0.iter().any(|query| query.matches(env))
    }
}

impl MediaQuery {
    pub fn matches(&self, env: &MediaEnvironment) -> bool {
        match self {
            MediaQuery::Typed {
                not,
                media_type,
                condition,
            } => {
                let type_matches = matches!(media_type, MediaType::All | MediaType::Print);
                let result = type_matches
                    && condition
                        .as_ref()
                        .is_none_or(|c| c.evaluate(env, 0) == Some(true));
                // `not` negates the whole query, but an unknown condition stays false.
                if *not {
                    let unknown = condition
                        .as_ref()
                        .is_some_and(|c| c.evaluate(env, 0).is_none());
                    !result && !unknown
                } else {
                    result
                }
            }
            MediaQuery::Condition(condition) => condition.evaluate(env, 0) == Some(true),
            MediaQuery::Invalid => false,
        }
    }
}

/// `@custom-media` references deeper than this are treated as cycles.
const MAX_CUSTOM_MEDIA_DEPTH: u32 = 16;

impl MediaCondition {
    /// Three-valued evaluation: `None` is unknown.
    fn evaluate(&self, env: &MediaEnvironment, depth: u32) -> Option<bool> {
        match self {
            MediaCondition::Not(inner) => inner.evaluate(env, depth).map(|v| !v),
            MediaCondition::And(conditions) => {
                let mut result = Some(true);
                for condition in conditions {
                    match condition.evaluate(env, depth) {
                        Some(false) => return Some(false),
                        None => result = None,
                        Some(true) => {}
                    }
                }
                result
            }
            MediaCondition::Or(conditions) => {
                let mut result = Some(false);
                for condition in conditions {
                    match condition.evaluate(env, depth) {
                        Some(true) => return Some(true),
                        None => result = None,
                        Some(false) => {}
                    }
                }
                result
            }
            MediaCondition::Feature(feature) => feature.evaluate(env),
            MediaCondition::Custom(name) => {
                if depth >= MAX_CUSTOM_MEDIA_DEPTH {
                    return Some(false);
                }
                match env.custom_media.get(name)? {
                    CustomMedia::True => Some(true),
                    CustomMedia::False => Some(false),
                    CustomMedia::Queries(list) => Some(
                        list.0.is_empty()
                            || list.0.iter().any(|q| match q {
                                MediaQuery::Condition(c) => {
                                    c.evaluate(env, depth + 1) == Some(true)
                                }
                                other => other.matches(env),
                            }),
                    ),
                }
            }
            MediaCondition::Unknown => None,
        }
    }
}

/// The value of a discrete media feature in print output, or `None` if the feature is
/// unknown. `scan` has no value: it applies only to TV displays.
fn discrete_value(name: &str) -> Option<Option<&'static str>> {
    Some(Some(match name {
        "any-hover" | "hover" | "any-pointer" | "pointer" | "forced-colors" | "inverted-colors"
        | "overflow-inline" | "scripting" | "update" => "none",
        "color-gamut" => "srgb",
        "device-posture" => "continuous",
        "display-mode" => "browser",
        "dynamic-range" | "video-dynamic-range" => "standard",
        "overflow-block" => "paged",
        "prefers-color-scheme" => "light",
        "prefers-contrast"
        | "prefers-reduced-data"
        | "prefers-reduced-motion"
        | "prefers-reduced-transparency" => "no-preference",
        "scan" => return Some(None),
        _ => return None,
    }))
}

/// Range feature values, in the units they compare in (pixels, dppx or plain numbers).
enum RangeValue {
    Length(f32),
    Ratio(f32, f32),
    Number(f32),
    Resolution(f32),
}

impl MediaFeature {
    fn evaluate(&self, env: &MediaEnvironment) -> Option<bool> {
        match self {
            MediaFeature::Boolean(name) => {
                if let Some(value) = discrete_value(name) {
                    return Some(value.is_some_and(|v| v != "none" && v != "no-preference"));
                }
                if name == "orientation" {
                    return Some(true);
                }
                Some(match range_value(name, env)? {
                    RangeValue::Length(v) | RangeValue::Number(v) | RangeValue::Resolution(v) => {
                        v != 0.0
                    }
                    RangeValue::Ratio(a, _) => a != 0.0,
                })
            }
            MediaFeature::Plain {
                name,
                prefix,
                value,
            } => {
                if prefix.is_none() {
                    if let Some(actual) = discrete_value(name) {
                        let MediaValue::Ident(wanted) = value else {
                            return None;
                        };
                        return Some(actual == Some(wanted.as_str()));
                    }
                    if name == "orientation" {
                        let MediaValue::Ident(wanted) = value else {
                            return None;
                        };
                        let actual = if env.height >= env.width {
                            "portrait"
                        } else {
                            "landscape"
                        };
                        return Some(actual == wanted);
                    }
                }
                let comparison = match prefix {
                    Some(RangePrefix::Min) => Comparison::Ge,
                    Some(RangePrefix::Max) => Comparison::Le,
                    None => Comparison::Eq,
                };
                compare(name, comparison, value, env)
            }
            MediaFeature::Range { name, comparisons } => {
                let mut result = true;
                for (comparison, value) in comparisons {
                    result &= compare(name, *comparison, value, env)?;
                }
                Some(result)
            }
        }
    }
}

fn range_value(name: &str, env: &MediaEnvironment) -> Option<RangeValue> {
    Some(match name {
        "width" | "device-width" => RangeValue::Length(env.width),
        "height" | "device-height" => RangeValue::Length(env.height),
        "aspect-ratio" | "device-aspect-ratio" => RangeValue::Ratio(env.width, env.height),
        "resolution" => RangeValue::Resolution(1.0),
        "color" => RangeValue::Number(8.0),
        "color-index" | "monochrome" | "grid" => RangeValue::Number(0.0),
        _ => return None,
    })
}

/// Evaluates `feature comparison value`.
fn compare(
    name: &str,
    comparison: Comparison,
    value: &MediaValue,
    env: &MediaEnvironment,
) -> Option<bool> {
    let actual = range_value(name, env)?;
    let (a, b) = match (actual, value) {
        (RangeValue::Length(actual), MediaValue::Length(length)) => (
            f64::from(actual),
            f64::from(length.to_px(&LengthContext::initial((env.width, env.height)))),
        ),
        // `0` is a valid length.
        (RangeValue::Length(actual), MediaValue::Number(n)) if *n == 0.0 => {
            (f64::from(actual), 0.0)
        }
        (RangeValue::Ratio(w, h), MediaValue::Ratio(a, b)) => {
            (f64::from(w) / f64::from(h), f64::from(*a) / f64::from(*b))
        }
        (RangeValue::Ratio(w, h), MediaValue::Number(n)) => {
            (f64::from(w) / f64::from(h), f64::from(*n))
        }
        (RangeValue::Number(actual), MediaValue::Number(n)) => (f64::from(actual), f64::from(*n)),
        (RangeValue::Resolution(actual), MediaValue::Resolution(r)) => {
            (f64::from(actual), f64::from(*r))
        }
        _ => return None,
    };
    Some(comparison.holds(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(query: &str) -> bool {
        let custom = HashMap::from([
            (
                "--narrow".to_owned(),
                CustomMedia::Queries(parse("(max-width: 600px)")),
            ),
            ("--on".to_owned(), CustomMedia::True),
        ]);
        // A4 portrait page area with 2cm margins, in CSS pixels.
        let env = MediaEnvironment {
            width: 643.0,
            height: 971.0,
            custom_media: &custom,
        };
        parse(query).matches(&env)
    }

    fn parse(query: &str) -> MediaQueryList {
        let mut input = Parser::new(query);
        MediaQueryList::parse(&mut input)
    }

    #[test]
    fn media_types() {
        assert!(matches(""));
        assert!(matches("print"));
        assert!(matches("all"));
        assert!(matches("only print"));
        assert!(!matches("screen"));
        assert!(matches("not screen"));
        assert!(!matches("not print"));
        assert!(matches("screen, print"));
        assert!(!matches("only"));
        assert!(!matches("layer"));
    }

    #[test]
    fn ranges() {
        assert!(matches("(min-width: 600px)"));
        assert!(!matches("(min-width: 700px)"));
        assert!(matches("(max-width: 50em)"));
        assert!(matches("(width > 600px)"));
        assert!(matches("(600px < width <= 700px)"));
        assert!(!matches("(700px < width < 900px)"));
        assert!(matches("(400px >= width) or (width >= 400px)"));
        assert!(matches("(orientation: portrait)"));
        assert!(matches("(aspect-ratio < 1/1)"));
        assert!(matches("(resolution: 1dppx) and (resolution: 96dpi)"));
        assert!(matches("(color) and (min-color: 8)"));
        assert!(!matches("(grid)"));
        assert!(!matches("(width >= 600px >= height)"));
    }

    #[test]
    fn print_environment() {
        assert!(matches("(hover: none) and (pointer: none)"));
        assert!(!matches("(hover)"));
        assert!(matches("(prefers-color-scheme: light)"));
        assert!(!matches("(prefers-reduced-motion)"));
        assert!(matches("(overflow-block: paged)"));
        assert!(!matches("(scan: progressive)"));
        assert!(!matches("(scripting)"));
    }

    #[test]
    fn unknown_is_never_true() {
        assert!(!matches("(unknown-feature)"));
        assert!(!matches("not (unknown-feature)"));
        assert!(!matches("print and (unknown-feature)"));
        assert!(!matches("not print and (unknown-feature)"));
        assert!(matches("(unknown) or (color)"));
        assert!(!matches("(min-width: 10s)"));
        assert!(!matches("whatever(1)"));
    }

    #[test]
    fn invalid_queries_do_not_spoil_the_list() {
        assert!(matches("(width: ), print"));
        assert!(!matches("print and"));
    }

    #[test]
    fn custom_media() {
        assert!(!matches("(--narrow)"));
        assert!(matches("not (--narrow)"));
        assert!(matches("(--on)"));
        assert!(!matches("(--undefined)"));
    }
}
