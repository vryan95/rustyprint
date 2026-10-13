//! Value types of anchor positioning (CSS Anchor Positioning 1).

use std::sync::Arc;

use cssparser::{ParseError, Parser};

use super::{Result, fail};
use crate::properties::{ComputeContext, Property};

fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

/// A `<dashed-ident>`, such as `--tooltip`.
pub(crate) fn parse_dashed_ident(input: &mut Parser<'_>) -> Result<Arc<str>> {
    let ident = input.expect_ident()?;
    if ident.starts_with("--") && ident.len() > 2 {
        Ok(Arc::from(&**ident))
    } else {
        fail()
    }
}

fn names_css(names: &[Arc<str>]) -> String {
    names.iter().map(|n| &**n).collect::<Vec<_>>().join(", ")
}

/// `anchor-name`: `none` or the names that make the element an anchor.
pub struct AnchorName;

impl Property for AnchorName {
    type Specified = Arc<[Arc<str>]>;
    type Computed = Arc<[Arc<str>]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            "none".into()
        } else {
            names_css(computed)
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(Arc::from([]));
        }
        Ok(input.parse_comma_separated(parse_dashed_ident)?.into())
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

/// `anchor-scope`: which anchor names of the element and its descendants are only visible to
/// positioned boxes inside it.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum AnchorScope {
    #[default]
    None,
    All,
    Names(Arc<[Arc<str>]>),
}

impl AnchorScope {
    /// Whether the scope covers anchor name `name`.
    pub fn covers(&self, name: &str) -> bool {
        match self {
            AnchorScope::None => false,
            AnchorScope::All => true,
            AnchorScope::Names(names) => names.iter().any(|n| &**n == name),
        }
    }
}

impl Property for AnchorScope {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            AnchorScope::None => "none".into(),
            AnchorScope::All => "all".into(),
            AnchorScope::Names(names) => names_css(names),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(AnchorScope::None);
        }
        if keyword(input, "all") {
            return Ok(AnchorScope::All);
        }
        Ok(AnchorScope::Names(
            input.parse_comma_separated(parse_dashed_ident)?.into(),
        ))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// `position-anchor`: the default anchor of a positioned box, for `position-area` and
/// `anchor()` without a name. `auto` is an implicit anchor, which a static document never
/// has.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PositionAnchor {
    #[default]
    Auto,
    None,
    Name(Arc<str>),
}

impl Property for PositionAnchor {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            PositionAnchor::Auto => "auto".into(),
            PositionAnchor::None => "none".into(),
            PositionAnchor::Name(name) => name.to_string(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "auto") {
            return Ok(PositionAnchor::Auto);
        }
        if keyword(input, "none") {
            return Ok(PositionAnchor::None);
        }
        Ok(PositionAnchor::Name(parse_dashed_ident(input)?))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// Which axis a `position-area` keyword names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AreaAxis {
    /// The horizontal or inline axis.
    X,
    /// The vertical or block axis.
    Y,
    /// Either, by its place: the first of two keywords is the block axis.
    Either,
}

/// A `position-area` keyword: its axis, and the tracks it covers of start, center and end,
/// as a range of 0 to 2. `logical` keywords count from the start of the writing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AreaKeyword {
    axis: AreaAxis,
    from: u8,
    to: u8,
    /// Whether start and end follow the writing direction rather than left and top.
    logical: bool,
}

fn area_keyword(word: &str) -> Option<AreaKeyword> {
    use AreaAxis::*;
    let k = |axis, from, to, logical| {
        Some(AreaKeyword {
            axis,
            from,
            to,
            logical,
        })
    };
    // `self-` keywords use the box's own writing mode, which is the containing block's
    // here.
    let word = word.replace("self-", "");
    match word.as_str() {
        "left" => k(X, 0, 0, false),
        "right" => k(X, 2, 2, false),
        "span-left" => k(X, 0, 1, false),
        "span-right" => k(X, 1, 2, false),
        "top" => k(Y, 0, 0, false),
        "bottom" => k(Y, 2, 2, false),
        "span-top" => k(Y, 0, 1, false),
        "span-bottom" => k(Y, 1, 2, false),
        "x-start" => k(X, 0, 0, true),
        "x-end" => k(X, 2, 2, true),
        "span-x-start" => k(X, 0, 1, true),
        "span-x-end" => k(X, 1, 2, true),
        "y-start" => k(Y, 0, 0, true),
        "y-end" => k(Y, 2, 2, true),
        "span-y-start" => k(Y, 0, 1, true),
        "span-y-end" => k(Y, 1, 2, true),
        "inline-start" => k(X, 0, 0, true),
        "inline-end" => k(X, 2, 2, true),
        "span-inline-start" => k(X, 0, 1, true),
        "span-inline-end" => k(X, 1, 2, true),
        "block-start" => k(Y, 0, 0, true),
        "block-end" => k(Y, 2, 2, true),
        "span-block-start" => k(Y, 0, 1, true),
        "span-block-end" => k(Y, 1, 2, true),
        "start" => k(Either, 0, 0, true),
        "end" => k(Either, 2, 2, true),
        "span-start" => k(Either, 0, 1, true),
        "span-end" => k(Either, 1, 2, true),
        "center" => k(Either, 1, 1, false),
        "span-all" => k(Either, 0, 2, false),
        _ => None,
    }
}

/// A computed `position-area`: the columns and rows of the 3×3 grid around the anchor that
/// the box's containing block covers, from left to right and top to bottom, and the
/// keywords as written.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionArea {
    pub columns: (u8, u8),
    pub rows: (u8, u8),
    keywords: Arc<str>,
}

impl PositionArea {
    /// The area flipped top to bottom.
    pub fn flip_block(&self) -> Self {
        Self {
            rows: (2 - self.rows.1, 2 - self.rows.0),
            ..self.clone()
        }
    }

    /// The area flipped left to right.
    pub fn flip_inline(&self) -> Self {
        Self {
            columns: (2 - self.columns.1, 2 - self.columns.0),
            ..self.clone()
        }
    }

    /// The area mirrored across the diagonal from top left to bottom right.
    pub fn flip_start(&self) -> Self {
        Self {
            columns: self.rows,
            rows: self.columns,
            ..self.clone()
        }
    }
}

/// `position-area`: `none`, or one or two keywords.
pub struct PositionAreaValue;

impl Property for PositionAreaValue {
    type Specified = Option<Vec<String>>;
    type Computed = Option<PositionArea>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "none".into(),
            Some(area) => area.keywords.to_string(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(None);
        }
        parse_area(input).map(Some)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.as_ref().map(|words| compute_area(words, ctx.rtl))
    }
}

fn parse_area(input: &mut Parser<'_>) -> Result<Vec<String>> {
    let first = input.expect_ident()?.to_ascii_lowercase();
    let first_keyword = area_keyword(&first).ok_or_else(|| ParseError::custom(()))?;
    let second = input
        .try_parse(|input| {
            let word = input.expect_ident()?.to_ascii_lowercase();
            area_keyword(&word)
                .map(|k| (word, k))
                .ok_or_else(|| ParseError::<()>::custom(()))
        })
        .ok();
    match second {
        None => Ok(vec![first]),
        Some((word, keyword)) => {
            // Two keywords name different axes, and `start`-like ones only go with each other.
            let axes = (first_keyword.axis, keyword.axis);
            let valid = match axes {
                (AreaAxis::X, AreaAxis::X) | (AreaAxis::Y, AreaAxis::Y) => false,
                (AreaAxis::Either, AreaAxis::Either) => true,
                _ => true,
            };
            if !valid {
                return fail();
            }
            // A second keyword the same as the first is left out.
            if word == first {
                Ok(vec![first])
            } else {
                Ok(vec![first, word])
            }
        }
    }
}

fn compute_area(words: &[String], rtl: bool) -> PositionArea {
    let keywords: Vec<AreaKeyword> = words.iter().filter_map(|w| area_keyword(w)).collect();
    let mut columns = None;
    let mut rows = None;
    let place = |k: &AreaKeyword, horizontal: bool| -> (u8, u8) {
        // Logical keywords count from the right in right-to-left text.
        if horizontal && k.logical && rtl {
            (2 - k.to, 2 - k.from)
        } else {
            (k.from, k.to)
        }
    };
    match keywords.as_slice() {
        [k] => match k.axis {
            AreaAxis::X => {
                columns = Some(place(k, true));
                rows = Some((0, 2));
            }
            AreaAxis::Y => {
                rows = Some(place(k, false));
                columns = Some((0, 2));
            }
            AreaAxis::Either => {
                columns = Some(place(k, true));
                rows = Some(place(k, false));
            }
        },
        [a, b] => {
            // An axis-specific keyword decides the other's axis; two generic ones are the
            // block axis, then the inline axis.
            let (vertical, horizontal) = match (a.axis, b.axis) {
                (AreaAxis::X, _) | (_, AreaAxis::Y) => (b, a),
                _ => (a, b),
            };
            rows = Some(place(vertical, false));
            columns = Some(place(horizontal, true));
        }
        _ => {}
    }
    PositionArea {
        columns: columns.unwrap_or((0, 2)),
        rows: rows.unwrap_or((0, 2)),
        keywords: Arc::from(words.join(" ")),
    }
}

/// A way `position-try-fallbacks` changes a box's position to find room for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryTactic {
    /// Swaps the block-axis insets, margins and `position-area`.
    FlipBlock,
    /// Swaps the inline-axis ones.
    FlipInline,
    /// Swaps the block and inline axes.
    FlipStart,
}

impl TryTactic {
    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Ok(match &*input.expect_ident()?.to_ascii_lowercase() {
            "flip-block" | "flip-y" => TryTactic::FlipBlock,
            "flip-inline" | "flip-x" => TryTactic::FlipInline,
            "flip-start" => TryTactic::FlipStart,
            _ => return fail(),
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            TryTactic::FlipBlock => "flip-block",
            TryTactic::FlipInline => "flip-inline",
            TryTactic::FlipStart => "flip-start",
        }
    }
}

/// One option of `position-try-fallbacks`: a `@position-try` rule and tactics applied after
/// it, or a `position-area`.
#[derive(Debug, Clone, PartialEq)]
pub enum TryFallback<A> {
    Rule {
        name: Option<Arc<str>>,
        tactics: Vec<TryTactic>,
    },
    Area(A),
}

/// `position-try-fallbacks`: `none` or the options to try when the box overflows.
pub struct PositionTryFallbacks;

impl Property for PositionTryFallbacks {
    type Specified = Arc<[TryFallback<Vec<String>>]>;
    type Computed = Arc<[TryFallback<PositionArea>]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        computed
            .iter()
            .map(|fallback| match fallback {
                TryFallback::Rule { name, tactics } => name
                    .iter()
                    .map(|n| n.to_string())
                    .chain(tactics.iter().map(|t| t.as_str().to_owned()))
                    .collect::<Vec<_>>()
                    .join(" "),
                TryFallback::Area(area) => area.keywords.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(Arc::from([]));
        }
        let fallbacks = input.parse_comma_separated(|input| {
            if let Ok(area) = input.try_parse(parse_area) {
                return Ok(TryFallback::Area(area));
            }
            let mut name = input.try_parse(parse_dashed_ident).ok();
            let mut tactics = Vec::new();
            while let Ok(tactic) = input.try_parse(TryTactic::parse) {
                if tactics.contains(&tactic) {
                    return fail();
                }
                tactics.push(tactic);
            }
            if name.is_none() {
                name = input.try_parse(parse_dashed_ident).ok();
            }
            if name.is_none() && tactics.is_empty() {
                return fail();
            }
            Ok(TryFallback::Rule { name, tactics })
        })?;
        Ok(fallbacks.into())
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified
            .iter()
            .map(|fallback| match fallback {
                TryFallback::Rule { name, tactics } => TryFallback::Rule {
                    name: name.clone(),
                    tactics: tactics.clone(),
                },
                TryFallback::Area(words) => TryFallback::Area(compute_area(words, ctx.rtl)),
            })
            .collect()
    }
}

keywords!(
    /// `position-try-order`: which fallback to prefer, by the room it has.
    PositionTryOrder {
        Normal = "normal",
        MostWidth = "most-width",
        MostHeight = "most-height",
        MostBlockSize = "most-block-size",
        MostInlineSize = "most-inline-size",
    }
);

/// `position-visibility`: when a positioned box is hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionVisibility {
    /// Hidden when an anchor it refers to does not exist.
    pub anchors_valid: bool,
    /// Hidden when its default anchor is not visible.
    pub anchors_visible: bool,
    /// Hidden when it overflows its containing block.
    pub no_overflow: bool,
}

impl PositionVisibility {
    pub const INITIAL: Self = Self {
        anchors_valid: false,
        anchors_visible: true,
        no_overflow: false,
    };
}

impl Property for PositionVisibility {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let mut parts = Vec::new();
        if computed.anchors_valid {
            parts.push("anchors-valid");
        }
        if computed.anchors_visible {
            parts.push("anchors-visible");
        }
        if computed.no_overflow {
            parts.push("no-overflow");
        }
        if parts.is_empty() {
            "always".into()
        } else {
            parts.join(" ")
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "always") {
            return Ok(Self {
                anchors_valid: false,
                anchors_visible: false,
                no_overflow: false,
            });
        }
        let mut value = Self {
            anchors_valid: false,
            anchors_visible: false,
            no_overflow: false,
        };
        let mut any = false;
        loop {
            if !value.anchors_valid && keyword(input, "anchors-valid") {
                value.anchors_valid = true;
            } else if !value.anchors_visible && keyword(input, "anchors-visible") {
                value.anchors_visible = true;
            } else if !value.no_overflow && keyword(input, "no-overflow") {
                value.no_overflow = true;
            } else {
                break;
            }
            any = true;
        }
        if any { Ok(value) } else { fail() }
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}
