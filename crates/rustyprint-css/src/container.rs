//! Container queries (CSS Conditional 5 § 2): conditions on the size and style of an
//! element's nearest query container, which `@container` rules apply on.
//!
//! Sizes come from layout: styles are computed again once the containers are laid out.

use std::collections::HashMap;
use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};
use rustyprint_dom::NodeId;

use crate::media::{Comparison, MediaCondition, MediaFeature, MediaValue, RangePrefix};
use crate::properties::ComputedValues;
use crate::properties::types::ContainerSize;
use crate::values::{FontMetrics, LengthContext};

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

/// The content-box sizes of the size containers of a document, in CSS pixels, as laid out.
pub type ContainerSizes = HashMap<NodeId, (f32, f32)>;

/// A `<container-condition>`: an optional container name and an optional query.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerCondition {
    pub name: Option<Arc<str>>,
    pub query: Option<ContainerQuery>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContainerQuery {
    Not(Box<ContainerQuery>),
    And(Vec<ContainerQuery>),
    Or(Vec<ContainerQuery>),
    /// A size feature, such as `(width > 400px)`.
    Size(MediaFeature),
    /// `style()`, with its contents.
    Style(String),
    /// `scroll-state()`, with its contents.
    ScrollState(String),
    /// Syntax this engine does not recognise; evaluates to unknown.
    Unknown,
}

/// What kinds of query a condition makes, which decides the containers it can query.
#[derive(Debug, Clone, Copy, Default)]
struct Needs {
    size: bool,
    scroll_state: bool,
}

impl ContainerQuery {
    fn needs(&self, needs: &mut Needs) {
        match self {
            ContainerQuery::Not(inner) => inner.needs(needs),
            ContainerQuery::And(all) | ContainerQuery::Or(all) => {
                all.iter().for_each(|q| q.needs(needs))
            }
            ContainerQuery::Size(_) => needs.size = true,
            ContainerQuery::ScrollState(_) => needs.scroll_state = true,
            ContainerQuery::Style(_) | ContainerQuery::Unknown => {}
        }
    }
}

/// Parses an `@container` prelude: `<container-condition>#`.
pub fn parse_conditions(input: &mut Parser<'_>) -> Result<Vec<ContainerCondition>> {
    input.parse_comma_separated(|input| {
        let name = input
            .try_parse(|input| -> Result<Arc<str>> {
                let ident = input.expect_ident_cloned()?;
                let lower = ident.to_ascii_lowercase();
                if matches!(
                    &*lower,
                    "none"
                        | "and"
                        | "or"
                        | "not"
                        | "initial"
                        | "inherit"
                        | "unset"
                        | "revert"
                        | "revert-layer"
                        | "default"
                ) {
                    return fail();
                }
                Ok(Arc::from(&*ident))
            })
            .ok();
        let query = if input.is_exhausted() {
            None
        } else {
            Some(parse_query(input)?)
        };
        if name.is_none() && query.is_none() {
            return fail();
        }
        Ok(ContainerCondition { name, query })
    })
}

/// `<container-query>`: `not` a query in parentheses, or an `and` or `or` chain of them.
fn parse_query(input: &mut Parser<'_>) -> Result<ContainerQuery> {
    if input
        .try_parse(|input| input.expect_ident_matching("not"))
        .is_ok()
    {
        return Ok(ContainerQuery::Not(Box::new(parse_in_parens(input)?)));
    }
    let first = parse_in_parens(input)?;
    let operator = match input.try_parse(|input| input.expect_ident_cloned()) {
        Ok(op) if op.eq_ignore_ascii_case("and") => "and",
        Ok(op) if op.eq_ignore_ascii_case("or") => "or",
        Ok(_) => return fail(),
        Err(_) => return Ok(first),
    };
    let mut queries = vec![first, parse_in_parens(input)?];
    while input
        .try_parse(|input| input.expect_ident_matching(operator))
        .is_ok()
    {
        queries.push(parse_in_parens(input)?);
    }
    Ok(if operator == "and" {
        ContainerQuery::And(queries)
    } else {
        ContainerQuery::Or(queries)
    })
}

fn rest(input: &mut Parser<'_>) -> String {
    let start = input.position();
    while input.next().is_ok() {}
    input.slice_from(start).trim().to_owned()
}

fn parse_in_parens(input: &mut Parser<'_>) -> Result<ContainerQuery> {
    match input.next()?.clone() {
        Token::ParenthesisBlock => input.parse_nested_block(|input| {
            if let Ok(query) = input.try_parse(|input| input.parse_entirely(parse_query)) {
                return Ok(query);
            }
            if let Ok(MediaCondition::Feature(feature)) =
                input.try_parse(|input| input.parse_entirely(crate::media::parse_feature))
            {
                return Ok(ContainerQuery::Size(feature));
            }
            rest(input);
            Ok(ContainerQuery::Unknown)
        }),
        Token::Function(name) => {
            let name = name.to_ascii_lowercase();
            input.parse_nested_block(|input| {
                let text = rest(input);
                Ok(match &*name {
                    "style" => ContainerQuery::Style(text),
                    "scroll-state" => ContainerQuery::ScrollState(text),
                    _ => ContainerQuery::Unknown,
                })
            })
        }
        _ => fail(),
    }
}

/// A candidate query container: an element, its style and, for size containers, its laid
/// out content-box size.
pub struct Candidate<'a> {
    pub style: &'a ComputedValues,
    pub size: Option<(f32, f32)>,
}

/// Whether any of `conditions` holds for an element whose candidate containers are
/// `candidates`, nearest first. `style_query` tests a `style()` query against a container's
/// style.
pub fn matches(
    conditions: &[ContainerCondition],
    candidates: &[Candidate],
    style_query: &dyn Fn(&str, &ComputedValues) -> Option<bool>,
) -> bool {
    conditions
        .iter()
        .any(|condition| evaluate(condition, candidates, style_query) == Some(true))
}

fn evaluate(
    condition: &ContainerCondition,
    candidates: &[Candidate],
    style_query: &dyn Fn(&str, &ComputedValues) -> Option<bool>,
) -> Option<bool> {
    let mut needs = Needs::default();
    if let Some(query) = &condition.query {
        query.needs(&mut needs);
    }
    // The nearest container with the name that can answer every kind of query made.
    let container = candidates.iter().find(|candidate| {
        let style = candidate.style;
        let named = condition
            .name
            .as_ref()
            .is_none_or(|name| style.container_name.iter().any(|n| n == name));
        named
            && (!needs.size || style.container_type.size.is_some())
            && (!needs.scroll_state || style.container_type.scroll_state)
    })?;
    match &condition.query {
        None => Some(true),
        Some(query) => query_holds(query, container, style_query),
    }
}

fn query_holds(
    query: &ContainerQuery,
    container: &Candidate,
    style_query: &dyn Fn(&str, &ComputedValues) -> Option<bool>,
) -> Option<bool> {
    match query {
        ContainerQuery::Not(inner) => query_holds(inner, container, style_query).map(|v| !v),
        ContainerQuery::And(all) => {
            let mut result = Some(true);
            for query in all {
                match query_holds(query, container, style_query) {
                    Some(false) => return Some(false),
                    None => result = None,
                    Some(true) => {}
                }
            }
            result
        }
        ContainerQuery::Or(any) => {
            let mut result = Some(false);
            for query in any {
                match query_holds(query, container, style_query) {
                    Some(true) => return Some(true),
                    None => result = None,
                    Some(false) => {}
                }
            }
            result
        }
        ContainerQuery::Size(feature) => size_feature(feature, container),
        ContainerQuery::Style(text) => style_query(text, container.style),
        // Nothing scrolls, sticks or snaps on paper.
        ContainerQuery::ScrollState(_) => Some(false),
        ContainerQuery::Unknown => None,
    }
}

/// Evaluates a size feature against a container. An inline-size container only knows its
/// inline size; features of its block size are unknown.
fn size_feature(feature: &MediaFeature, container: &Candidate) -> Option<bool> {
    let (width, height) = container.size?;
    let style = container.style;
    let vertical = style.writing_mode.is_vertical();
    let both = style.container_type.size == Some(ContainerSize::Both);
    let known_width = both || !vertical;
    let known_height = both || vertical;
    let value = |name: &str| -> Option<Feature> {
        Some(match name {
            "width" if known_width => Feature::Length(width),
            "height" if known_height => Feature::Length(height),
            "inline-size" => Feature::Length(if vertical { height } else { width }),
            "block-size" if both => Feature::Length(if vertical { width } else { height }),
            "aspect-ratio" if both => Feature::Ratio(width, height),
            "orientation" if both => Feature::Orientation(height >= width),
            _ => return None,
        })
    };
    // Relative lengths in queries resolve against the container's font.
    let font = FontMetrics::fallback(style.font_size);
    let ctx = LengthContext {
        font,
        root_font: FontMetrics::fallback(16.0),
        ..LengthContext::initial((width, height))
    };
    let compare = |actual: &Feature, comparison: Comparison, wanted: &MediaValue| {
        let (a, b) = match (actual, wanted) {
            (Feature::Length(actual), MediaValue::Length(length)) => {
                (f64::from(*actual), f64::from(length.to_px(&ctx)))
            }
            (Feature::Length(actual), MediaValue::Number(n)) if *n == 0.0 => {
                (f64::from(*actual), 0.0)
            }
            (Feature::Ratio(w, h), MediaValue::Ratio(a, b)) => {
                (f64::from(*w) / f64::from(*h), f64::from(*a) / f64::from(*b))
            }
            (Feature::Ratio(w, h), MediaValue::Number(n)) => {
                (f64::from(*w) / f64::from(*h), f64::from(*n))
            }
            _ => return None,
        };
        Some(comparison.holds(a, b))
    };
    match feature {
        MediaFeature::Boolean(name) => Some(match value(name)? {
            Feature::Length(v) => v != 0.0,
            Feature::Ratio(w, _) => w != 0.0,
            Feature::Orientation(_) => true,
        }),
        MediaFeature::Plain {
            name,
            prefix,
            value: wanted,
        } => {
            let actual = value(name)?;
            if let Feature::Orientation(portrait) = actual {
                let MediaValue::Ident(wanted) = wanted else {
                    return None;
                };
                return Some(match wanted.as_str() {
                    "portrait" => portrait,
                    "landscape" => !portrait,
                    _ => return None,
                });
            }
            let comparison = match prefix {
                Some(RangePrefix::Min) => Comparison::Ge,
                Some(RangePrefix::Max) => Comparison::Le,
                None => Comparison::Eq,
            };
            compare(&actual, comparison, wanted)
        }
        MediaFeature::Range { name, comparisons } => {
            let actual = value(name)?;
            let mut result = true;
            for (comparison, wanted) in comparisons {
                result &= compare(&actual, *comparison, wanted)?;
            }
            Some(result)
        }
    }
}

enum Feature {
    Length(f32),
    Ratio(f32, f32),
    /// Whether the container is portrait.
    Orientation(bool),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::types::ContainerType;

    fn conditions(text: &str) -> Vec<ContainerCondition> {
        Parser::new(text)
            .parse_entirely(parse_conditions)
            .expect("valid conditions")
    }

    fn container(size: Option<ContainerSize>, name: &str) -> ComputedValues {
        let mut style = ComputedValues::initial();
        style.container_type = ContainerType {
            size,
            scroll_state: false,
        };
        if !name.is_empty() {
            style.container_name = Arc::from([Arc::from(name)]);
        }
        style
    }

    #[test]
    fn parses_conditions() {
        assert_eq!(conditions("card").len(), 1);
        assert_eq!(conditions("(width > 1px), sidebar (height < 2px)").len(), 2);
        assert!(matches!(
            conditions("not (width > 1px)")[0].query,
            Some(ContainerQuery::Not(_))
        ));
        assert!(matches!(
            conditions("style(--x: 1) and scroll-state(stuck: top)")[0].query,
            Some(ContainerQuery::And(_))
        ));
        assert!(
            Parser::new("none (width > 1px)")
                .parse_entirely(parse_conditions)
                .is_err()
        );
    }

    #[test]
    fn evaluates_against_the_nearest_eligible_container() {
        let inline = container(Some(ContainerSize::Inline), "card");
        let both = container(Some(ContainerSize::Both), "page");
        let plain = container(None, "");
        let candidates = [
            Candidate {
                style: &plain,
                size: None,
            },
            Candidate {
                style: &inline,
                size: Some((300.0, 100.0)),
            },
            Candidate {
                style: &both,
                size: Some((800.0, 400.0)),
            },
        ];
        let never = |_: &str, _: &ComputedValues| None;
        let holds = |text: &str| matches(&conditions(text), &candidates, &never);
        assert!(holds("(width > 200px)"));
        assert!(!holds("(width > 400px)"));
        assert!(holds("page (width > 400px)"));
        // The inline-size container does not know its height.
        assert!(!holds("(height > 50px)"));
        assert!(!holds("not (height > 50px)"));
        assert!(holds("page (orientation: landscape)"));
        assert!(holds("(inline-size >= 15em)"));
        assert!(!holds("(inline-size >= 30em)"));
        assert!(holds("card"));
        assert!(!holds("missing"));
        assert!(holds("(width < 10px), (width > 10px)"));
    }
}
