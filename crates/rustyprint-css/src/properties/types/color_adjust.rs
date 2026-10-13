//! Value types of CSS Color Adjustment 1 and of `dynamic-range-limit` (CSS Color HDR 1).

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{Result, fail};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

/// `color-scheme`: `normal | [ light | dark | <custom-ident> ]+ && only?`.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorScheme {
    /// The schemes the element supports, in order; empty for `normal`.
    pub schemes: Arc<[Arc<str>]>,
    pub only: bool,
}

impl ColorScheme {
    pub fn normal() -> Self {
        Self {
            schemes: Arc::from([]),
            only: false,
        }
    }

    /// Whether the used color scheme is dark. The PDF prefers a light scheme, so it is dark
    /// only when the element supports dark but not light.
    pub fn dark(&self) -> bool {
        let supports = |name: &str| self.schemes.iter().any(|s| &**s == name);
        supports("dark") && !supports("light")
    }
}

impl Property for ColorScheme {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        if computed.schemes.is_empty() {
            return "normal".into();
        }
        let mut out: Vec<String> = computed
            .schemes
            .iter()
            .map(|scheme| serialize_ident(scheme))
            .collect();
        if computed.only {
            out.push("only".into());
        }
        out.join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_ident_matching("normal"))
            .is_ok()
        {
            return Ok(Self::normal());
        }
        let mut schemes: Vec<Arc<str>> = Vec::new();
        let mut only = false;
        while let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) {
            let lower = ident.to_ascii_lowercase();
            match lower.as_str() {
                // `only` comes first or last, once.
                "only" if !only && (schemes.is_empty() || input.is_exhausted()) => only = true,
                "only" | "normal" | "none" | "initial" | "inherit" | "unset" | "revert"
                | "revert-layer" | "default" => return fail(),
                "light" | "dark" => schemes.push(Arc::from(lower.as_str())),
                _ => schemes.push(Arc::from(&*ident)),
            }
        }
        if schemes.is_empty() {
            return fail();
        }
        Ok(Self {
            schemes: schemes.into(),
            only,
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

fn serialize_ident(ident: &str) -> String {
    let mut out = String::new();
    let _ = cssparser::serialize_identifier(ident, &mut out);
    out
}

keywords!(
    /// `print-color-adjust`. The PDF always keeps backgrounds and colors, as with `exact`.
    PrintColorAdjust { Economy = "economy", Exact = "exact" }
);

keywords!(
    /// `forced-color-adjust`. Forced colors are never active in a PDF.
    ForcedColorAdjust {
        Auto = "auto",
        None = "none",
        PreserveParentImage = "preserve-parent-image",
    }
);

/// `dynamic-range-limit`: `standard | no-limit | constrained |
/// <dynamic-range-limit-mix()>`. The PDF is standard dynamic range, so it has no effect.
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicRangeLimit {
    Standard,
    NoLimit,
    Constrained,
    /// `dynamic-range-limit-mix()`: limits with their percentages.
    Mix(Arc<[(DynamicRangeLimit, f32)]>),
}

impl DynamicRangeLimit {
    fn parse_value(input: &mut Parser<'_>) -> Result<Self> {
        let token = input.next()?.clone();
        match token {
            Token::Ident(ident) => Ok(match &*ident.to_ascii_lowercase() {
                "standard" => DynamicRangeLimit::Standard,
                "no-limit" => DynamicRangeLimit::NoLimit,
                "constrained" => DynamicRangeLimit::Constrained,
                _ => return fail(),
            }),
            Token::Function(name) if name.eq_ignore_ascii_case("dynamic-range-limit-mix") => {
                input.parse_nested_block(|input| {
                    let items = input.parse_comma_separated(|input| {
                        // `<'dynamic-range-limit'> && <percentage [0,100]>`
                        let percentage = |input: &mut Parser<'_>| -> Result<f32> {
                            let value = input.expect_percentage()?;
                            if !(0.0..=1.0).contains(&value) {
                                return fail();
                            }
                            Ok(value)
                        };
                        if let Ok(p) = input.try_parse(percentage) {
                            Ok((Self::parse_value(input)?, p))
                        } else {
                            let limit = Self::parse_value(input)?;
                            Ok((limit, percentage(input)?))
                        }
                    })?;
                    if items.len() < 2 {
                        return fail();
                    }
                    Ok(DynamicRangeLimit::Mix(items.into()))
                })
            }
            _ => Err(ParseError::custom(())),
        }
    }

    fn css(&self) -> String {
        match self {
            DynamicRangeLimit::Standard => "standard".into(),
            DynamicRangeLimit::NoLimit => "no-limit".into(),
            DynamicRangeLimit::Constrained => "constrained".into(),
            DynamicRangeLimit::Mix(items) => format!(
                "dynamic-range-limit-mix({})",
                items
                    .iter()
                    .map(|(limit, p)| format!("{} {}", limit.css(), serialize::percentage(*p)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl Property for DynamicRangeLimit {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        computed.css()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Self::parse_value(input)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}
