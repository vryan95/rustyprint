//! Value types of flex layout (CSS Flexible Box Layout 1 and 2) and of reading order
//! (CSS Display 4 § 5).

use cssparser::{ParseError, Parser};

use super::{
    LengthPercentage, Result, Size, SpecifiedLengthPercentage, compute_size, fail, parse_integer,
    parse_number, parse_size, size_css,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

keywords!(
    /// `flex-direction`: the main axis, and which way items go along it.
    FlexDirection {
        Row = "row",
        RowReverse = "row-reverse",
        Column = "column",
        ColumnReverse = "column-reverse",
    }
);

impl FlexDirection {
    pub fn is_column(self) -> bool {
        matches!(self, FlexDirection::Column | FlexDirection::ColumnReverse)
    }

    pub fn is_reverse(self) -> bool {
        matches!(
            self,
            FlexDirection::RowReverse | FlexDirection::ColumnReverse
        )
    }
}

/// Whether items wrap onto several lines, in which order the lines go, and whether they
/// are balanced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Wrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

/// `flex-wrap`: `nowrap | [ wrap | wrap-reverse ] || balance` (CSS Flexbox 2 § 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlexWrap {
    pub wrap: Wrap,
    /// Lines are as similar in length as possible, rather than each filled in turn.
    pub balance: bool,
}

impl FlexWrap {
    pub const NOWRAP: Self = FlexWrap {
        wrap: Wrap::NoWrap,
        balance: false,
    };

    /// Parses one keyword of the value into `self`, as `flex-flow` interleaves them with
    /// the direction.
    pub(crate) fn parse_keyword(
        input: &mut Parser<'_>,
        wrap: &mut Option<Wrap>,
        balance: &mut bool,
    ) -> Result<()> {
        let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
        match ident.as_str() {
            "nowrap" if wrap.is_none() => *wrap = Some(Wrap::NoWrap),
            "wrap" if wrap.is_none() => *wrap = Some(Wrap::Wrap),
            "wrap-reverse" if wrap.is_none() => *wrap = Some(Wrap::WrapReverse),
            "balance" if !*balance => *balance = true,
            _ => return fail(),
        }
        Ok(())
    }

    /// The value from its keywords; `balance` alone wraps, and cannot go with `nowrap`.
    pub(crate) fn from_keywords(wrap: Option<Wrap>, balance: bool) -> Result<Self> {
        let wrap = match (wrap, balance) {
            (Some(Wrap::NoWrap), true) => return fail(),
            (None, true) => Wrap::Wrap,
            (wrap, _) => wrap.unwrap_or(Wrap::NoWrap),
        };
        Ok(FlexWrap { wrap, balance })
    }
}

impl Property for FlexWrap {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match (computed.wrap, computed.balance) {
            (Wrap::NoWrap, _) => "nowrap".into(),
            (Wrap::Wrap, false) => "wrap".into(),
            (Wrap::WrapReverse, false) => "wrap-reverse".into(),
            (Wrap::Wrap, true) => "balance".into(),
            (Wrap::WrapReverse, true) => "wrap-reverse balance".into(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let (mut wrap, mut balance) = (None, false);
        FlexWrap::parse_keyword(input, &mut wrap, &mut balance)?;
        let _ = input.try_parse(|input| FlexWrap::parse_keyword(input, &mut wrap, &mut balance));
        FlexWrap::from_keywords(wrap, balance)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `flex-grow` and `flex-shrink`: a non-negative number.
pub struct FlexFactor;

impl Property for FlexFactor {
    type Specified = f32;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        serialize::number(f64::from(*computed))
    }

    fn parse(input: &mut Parser<'_>) -> Result<f32> {
        let value = parse_number(input)?;
        if value < 0.0 {
            return fail();
        }
        Ok(value)
    }

    fn compute(specified: &f32, _ctx: &ComputeContext) -> f32 {
        *specified
    }
}

/// `flex-basis`: `content | <'width'>`.
#[derive(Debug, Clone, PartialEq)]
pub enum FlexBasis<T> {
    /// The size of the item's content.
    Content,
    Size(Size<T>),
}

impl Property for FlexBasis<LengthPercentage> {
    type Specified = FlexBasis<SpecifiedLengthPercentage>;
    type Computed = FlexBasis<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            FlexBasis::Content => "content".into(),
            FlexBasis::Size(size) => size_css(size),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if input
            .try_parse(|input| input.expect_ident_matching("content"))
            .is_ok()
        {
            return Ok(FlexBasis::Content);
        }
        Ok(FlexBasis::Size(parse_size(input, false, true)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        match specified {
            FlexBasis::Content => FlexBasis::Content,
            FlexBasis::Size(size) => FlexBasis::Size(compute_size(size, ctx)),
        }
    }
}

/// `flex-line-count`: the least number of lines a balanced flex container has.
pub struct FlexLineCount;

impl Property for FlexLineCount {
    type Specified = u32;
    type Computed = u32;

    fn to_css(computed: &u32) -> String {
        computed.to_string()
    }

    fn parse(input: &mut Parser<'_>) -> Result<u32> {
        Ok(parse_integer(input, 1)? as u32)
    }

    fn compute(specified: &u32, _ctx: &ComputeContext) -> u32 {
        *specified
    }
}

/// `order` and `reading-order`: an integer that reorders boxes.
pub struct Order;

impl Property for Order {
    type Specified = i32;
    type Computed = i32;

    fn to_css(computed: &i32) -> String {
        computed.to_string()
    }

    fn parse(input: &mut Parser<'_>) -> Result<i32> {
        parse_integer(input, i32::MIN)
    }

    fn compute(specified: &i32, _ctx: &ComputeContext) -> i32 {
        *specified
    }
}

keywords!(
    /// `reading-flow`: the order in which a container's children are read, for speech and
    /// keyboard navigation.
    ReadingFlow {
        Normal = "normal",
        SourceOrder = "source-order",
        FlexVisual = "flex-visual",
        FlexFlow = "flex-flow",
        GridRows = "grid-rows",
        GridColumns = "grid-columns",
        GridOrder = "grid-order",
    }
);

/// Parses the value of `flex`, other than `none`: `<'flex-grow'> <'flex-shrink'>? ||
/// <'flex-basis'>`. An omitted factor is 1 and an omitted basis 0%. A unitless zero is a
/// factor unless both factors come before it.
pub(crate) fn parse_flex(
    input: &mut Parser<'_>,
) -> Result<(f32, f32, FlexBasis<SpecifiedLengthPercentage>)> {
    let (mut grow, mut shrink, mut basis) = (None, None, None);
    loop {
        if grow.is_none()
            && let Ok(value) = input.try_parse(FlexFactor::parse)
        {
            grow = Some(value);
            shrink = input.try_parse(FlexFactor::parse).ok();
            continue;
        }
        if basis.is_none()
            && let Ok(value) = input.try_parse(FlexBasis::<LengthPercentage>::parse)
        {
            basis = Some(value);
            // Factors after the basis must come together.
            if grow.is_some() {
                break;
            }
            continue;
        }
        break;
    }
    if grow.is_none() && basis.is_none() {
        return Err(ParseError::custom(()));
    }
    Ok((
        grow.unwrap_or(1.0),
        shrink.unwrap_or(1.0),
        basis.unwrap_or(FlexBasis::Size(Size::Value(
            SpecifiedLengthPercentage::Percentage(0.0),
        ))),
    ))
}
