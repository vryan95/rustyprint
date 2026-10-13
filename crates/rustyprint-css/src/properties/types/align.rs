//! Value types of box alignment (CSS Box Alignment 3): `align-*`, `justify-*` and the gaps.

use cssparser::{ParseError, Parser};

use super::{
    LengthPercentage, Result, SpecifiedLengthPercentage, fail, length_percentage_css,
    parse_length_percentage,
};
use crate::properties::{ComputeContext, Property};

/// `<self-position>`, `<content-position>` and `left | right`: where a box or its content
/// goes in its alignment container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignPosition {
    Center,
    Start,
    End,
    SelfStart,
    SelfEnd,
    FlexStart,
    FlexEnd,
    Left,
    Right,
}

impl AlignPosition {
    fn as_str(self) -> &'static str {
        match self {
            AlignPosition::Center => "center",
            AlignPosition::Start => "start",
            AlignPosition::End => "end",
            AlignPosition::SelfStart => "self-start",
            AlignPosition::SelfEnd => "self-end",
            AlignPosition::FlexStart => "flex-start",
            AlignPosition::FlexEnd => "flex-end",
            AlignPosition::Left => "left",
            AlignPosition::Right => "right",
        }
    }

    fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "center" => AlignPosition::Center,
            "start" => AlignPosition::Start,
            "end" => AlignPosition::End,
            "self-start" => AlignPosition::SelfStart,
            "self-end" => AlignPosition::SelfEnd,
            "flex-start" => AlignPosition::FlexStart,
            "flex-end" => AlignPosition::FlexEnd,
            "left" => AlignPosition::Left,
            "right" => AlignPosition::Right,
            _ => return None,
        })
    }

    /// Whether the position is a `<content-position>`, which `*-content` take.
    fn is_content(self) -> bool {
        matches!(
            self,
            AlignPosition::Center
                | AlignPosition::Start
                | AlignPosition::End
                | AlignPosition::FlexStart
                | AlignPosition::FlexEnd
        )
    }

    fn is_left_right(self) -> bool {
        matches!(self, AlignPosition::Left | AlignPosition::Right)
    }
}

/// `<overflow-position>`: what happens when the box overflows its alignment container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OverflowPosition {
    /// Neither keyword: the box may overflow, as with `unsafe`.
    #[default]
    Default,
    /// The box keeps to the start edge rather than overflow it.
    Safe,
    Unsafe,
}

/// `<content-distribution>`: how the space between and around boxes is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentDistribution {
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

/// `align-content` and `justify-content`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentAlignment {
    Normal,
    /// `baseline` (first) or `last baseline`.
    Baseline {
        last: bool,
    },
    Distribution(ContentDistribution),
    Position {
        position: AlignPosition,
        overflow: OverflowPosition,
    },
}

/// `align-self`, `justify-self`, `align-items` and `justify-items`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemAlignment {
    /// The parent's `align-items` or `justify-items`; only the `*-self` properties take it.
    Auto,
    Normal,
    Stretch,
    Baseline {
        last: bool,
    },
    Position {
        position: AlignPosition,
        overflow: OverflowPosition,
    },
    AnchorCenter,
    /// `legacy` with `left`, `right` or `center`, which descendants inherit through their
    /// own `legacy`; only `justify-items` takes it. A specified `legacy` on its own is
    /// `Legacy(Start)`, which computes to the parent's value or `normal`.
    Legacy(AlignPosition),
}

impl ItemAlignment {
    /// The value for a box whose parent's `*-items` is `items`: `auto` takes it, without
    /// `legacy`.
    pub fn resolve(self, items: ItemAlignment) -> ItemAlignment {
        match self {
            ItemAlignment::Auto => match items {
                ItemAlignment::Legacy(position) => ItemAlignment::Position {
                    position,
                    overflow: OverflowPosition::Default,
                },
                ItemAlignment::Auto => ItemAlignment::Normal,
                other => other,
            },
            other => other,
        }
    }
}

/// Parses `[ first | last ]? baseline`.
fn parse_baseline(input: &mut Parser<'_>) -> Result<bool> {
    let first = input.expect_ident_cloned()?.to_ascii_lowercase();
    match first.as_str() {
        "baseline" => Ok(false),
        "first" | "last" => {
            input.expect_ident_matching("baseline")?;
            Ok(first == "last")
        }
        _ => fail(),
    }
}

/// Parses `<overflow-position>? <position>`, where `accepts` decides which positions the
/// property takes.
fn parse_position(
    input: &mut Parser<'_>,
    accepts: impl Fn(AlignPosition) -> bool,
) -> Result<(AlignPosition, OverflowPosition)> {
    let overflow = match input.try_parse(|input| {
        let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
        match ident.as_str() {
            "safe" => Ok(OverflowPosition::Safe),
            "unsafe" => Ok(OverflowPosition::Unsafe),
            _ => fail(),
        }
    }) {
        Ok(overflow) => overflow,
        Err(_) => OverflowPosition::Default,
    };
    let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
    match AlignPosition::from_keyword(&ident) {
        Some(position) if accepts(position) => Ok((position, overflow)),
        _ => Err(ParseError::custom(())),
    }
}

fn position_css(position: AlignPosition, overflow: OverflowPosition) -> String {
    match overflow {
        OverflowPosition::Default => position.as_str().into(),
        OverflowPosition::Safe => format!("safe {}", position.as_str()),
        OverflowPosition::Unsafe => format!("unsafe {}", position.as_str()),
    }
}

fn baseline_css(last: bool) -> String {
    if last { "last baseline" } else { "baseline" }.into()
}

fn content_css(value: &ContentAlignment) -> String {
    match *value {
        ContentAlignment::Normal => "normal".into(),
        ContentAlignment::Baseline { last } => baseline_css(last),
        ContentAlignment::Distribution(distribution) => match distribution {
            ContentDistribution::SpaceBetween => "space-between",
            ContentDistribution::SpaceAround => "space-around",
            ContentDistribution::SpaceEvenly => "space-evenly",
            ContentDistribution::Stretch => "stretch",
        }
        .into(),
        ContentAlignment::Position { position, overflow } => position_css(position, overflow),
    }
}

fn item_css(value: &ItemAlignment) -> String {
    match *value {
        ItemAlignment::Auto => "auto".into(),
        ItemAlignment::Normal => "normal".into(),
        ItemAlignment::Stretch => "stretch".into(),
        ItemAlignment::Baseline { last } => baseline_css(last),
        ItemAlignment::Position { position, overflow } => position_css(position, overflow),
        ItemAlignment::AnchorCenter => "anchor-center".into(),
        ItemAlignment::Legacy(position) => format!("legacy {}", position.as_str()),
    }
}

/// `normal | <baseline-position> | <content-distribution> |
/// <overflow-position>? <content-position>`, with `left | right` when `left_right`.
fn parse_content(
    input: &mut Parser<'_>,
    baseline: bool,
    left_right: bool,
) -> Result<ContentAlignment> {
    if input
        .try_parse(|input| input.expect_ident_matching("normal"))
        .is_ok()
    {
        return Ok(ContentAlignment::Normal);
    }
    if baseline && let Ok(last) = input.try_parse(parse_baseline) {
        return Ok(ContentAlignment::Baseline { last });
    }
    if let Ok(distribution) = input.try_parse(|input| {
        Ok::<_, ParseError<()>>(match &*input.expect_ident()?.to_ascii_lowercase() {
            "space-between" => ContentDistribution::SpaceBetween,
            "space-around" => ContentDistribution::SpaceAround,
            "space-evenly" => ContentDistribution::SpaceEvenly,
            "stretch" => ContentDistribution::Stretch,
            _ => return fail(),
        })
    }) {
        return Ok(ContentAlignment::Distribution(distribution));
    }
    let (position, overflow) = parse_position(input, |p| {
        p.is_content() || (left_right && p.is_left_right())
    })?;
    Ok(ContentAlignment::Position { position, overflow })
}

/// What a `*-self` or `*-items` property accepts besides the common values.
#[derive(Clone, Copy)]
struct ItemGrammar {
    auto: bool,
    left_right: bool,
    legacy: bool,
}

fn parse_item(input: &mut Parser<'_>, grammar: ItemGrammar) -> Result<ItemAlignment> {
    if let Ok(keyword) = input.try_parse(|input| {
        let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
        Ok::<_, ParseError<()>>(match ident.as_str() {
            "auto" if grammar.auto => ItemAlignment::Auto,
            "normal" => ItemAlignment::Normal,
            "stretch" => ItemAlignment::Stretch,
            "anchor-center" => ItemAlignment::AnchorCenter,
            _ => return fail(),
        })
    }) {
        return Ok(keyword);
    }
    if let Ok(last) = input.try_parse(parse_baseline) {
        return Ok(ItemAlignment::Baseline { last });
    }
    if grammar.legacy {
        // `legacy && [ left | right | center ]`, or `legacy` alone.
        let side = |input: &mut Parser<'_>| {
            let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
            match AlignPosition::from_keyword(&ident) {
                Some(p @ (AlignPosition::Left | AlignPosition::Right | AlignPosition::Center)) => {
                    Ok(p)
                }
                _ => fail(),
            }
        };
        let legacy = |input: &mut Parser<'_>| input.expect_ident_matching("legacy");
        if input.try_parse(legacy).is_ok() {
            return Ok(match input.try_parse(side) {
                Ok(position) => ItemAlignment::Legacy(position),
                // `legacy` alone; computing it looks at the parent.
                Err(_) => ItemAlignment::Legacy(AlignPosition::Start),
            });
        }
        if let Ok(position) = input.try_parse(|input| {
            let position = side(input)?;
            legacy(input)?;
            Ok::<_, ParseError<()>>(position)
        }) {
            return Ok(ItemAlignment::Legacy(position));
        }
    }
    let (position, overflow) = parse_position(input, |p| !p.is_left_right() || grammar.left_right)?;
    Ok(ItemAlignment::Position { position, overflow })
}

macro_rules! content_property {
    ($(#[$meta:meta])* $name:ident, baseline: $baseline:expr, left_right: $lr:expr) => {
        $(#[$meta])*
        pub struct $name;

        impl Property for $name {
            type Specified = ContentAlignment;
            type Computed = ContentAlignment;

            fn to_css(computed: &ContentAlignment) -> String {
                content_css(computed)
            }

            fn parse(input: &mut Parser<'_>) -> Result<ContentAlignment> {
                parse_content(input, $baseline, $lr)
            }

            fn compute(specified: &ContentAlignment, _ctx: &ComputeContext) -> ContentAlignment {
                *specified
            }
        }
    };
}

content_property!(
    /// `align-content`: how lines (or a block's content) are placed in the cross axis.
    AlignContent, baseline: true, left_right: false
);
content_property!(
    /// `justify-content`: how items are placed in the main axis.
    JustifyContent, baseline: false, left_right: true
);

macro_rules! item_property {
    ($(#[$meta:meta])* $name:ident, $grammar:expr) => {
        $(#[$meta])*
        pub struct $name;

        impl Property for $name {
            type Specified = ItemAlignment;
            type Computed = ItemAlignment;

            fn to_css(computed: &ItemAlignment) -> String {
                item_css(computed)
            }

            fn parse(input: &mut Parser<'_>) -> Result<ItemAlignment> {
                parse_item(input, $grammar)
            }

            fn compute(specified: &ItemAlignment, ctx: &ComputeContext) -> ItemAlignment {
                compute_item(*specified, ctx)
            }
        }
    };
}

/// `legacy` alone computes to the parent's `legacy` value, or else to `normal`.
fn compute_item(specified: ItemAlignment, ctx: &ComputeContext) -> ItemAlignment {
    match specified {
        ItemAlignment::Legacy(AlignPosition::Start) => match ctx.parent.justify_items {
            inherited @ ItemAlignment::Legacy(_) => inherited,
            _ => ItemAlignment::Normal,
        },
        other => other,
    }
}

item_property!(
    /// `align-self`: where a box goes in the cross axis of its line.
    AlignSelf,
    ItemGrammar { auto: true, left_right: false, legacy: false }
);
item_property!(
    /// `justify-self`: where a box goes in the inline axis of its alignment container.
    JustifySelf,
    ItemGrammar { auto: true, left_right: true, legacy: false }
);
item_property!(
    /// `align-items`: the default `align-self` of a box's children.
    AlignItems,
    ItemGrammar { auto: false, left_right: false, legacy: false }
);
item_property!(
    /// `justify-items`: the default `justify-self` of a box's children.
    JustifyItems,
    ItemGrammar { auto: false, left_right: true, legacy: true }
);

/// `row-gap` and `column-gap`: `normal`, or the space between rows or columns.
pub struct Gap;

impl Property for Gap {
    /// `None` for `normal`.
    type Specified = Option<SpecifiedLengthPercentage>;
    type Computed = Option<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "normal".into(),
            Some(value) => length_percentage_css(value),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if input
            .try_parse(|input| input.expect_ident_matching("normal"))
            .is_ok()
        {
            return Ok(None);
        }
        Ok(Some(parse_length_percentage(input, true, true)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.as_ref().map(|value| value.compute(ctx, true))
    }
}
