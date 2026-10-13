//! Value types of multi-column layout (CSS Multi-column Layout 1 and 2).

use cssparser::Parser;

use super::{Result, SpecifiedLengthPercentage, fail, parse_integer, parse_length_percentage};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

/// `column-count`: `auto | <integer [1,∞]>`, `None` for `auto`.
pub struct ColumnCount;

impl Property for ColumnCount {
    type Specified = Option<u32>;
    type Computed = Option<u32>;

    fn to_css(computed: &Option<u32>) -> String {
        match computed {
            None => "auto".into(),
            Some(count) => count.to_string(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Option<u32>> {
        if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
            return Ok(None);
        }
        Ok(Some(parse_integer(input, 1)? as u32))
    }

    fn compute(specified: &Option<u32>, _ctx: &ComputeContext) -> Option<u32> {
        *specified
    }
}

/// `column-width` and `column-height`: `auto | <length [0,∞]>`, `None` for `auto`.
pub struct ColumnLength;

impl Property for ColumnLength {
    type Specified = Option<SpecifiedLengthPercentage>;
    type Computed = Option<f32>;

    fn to_css(computed: &Option<f32>) -> String {
        match computed {
            None => "auto".into(),
            Some(px) => serialize::px(*px),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
            return Ok(None);
        }
        Ok(Some(parse_length_percentage(input, false, true)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Option<f32> {
        specified
            .as_ref()
            .map(|value| value.compute(ctx, true).as_length().unwrap_or(0.0).max(0.0))
    }
}

keywords!(
    /// `column-fill`: whether columns are balanced or filled one after another.
    ColumnFill {
        Auto = "auto",
        Balance = "balance",
        BalanceAll = "balance-all",
    }
);

keywords!(
    /// `column-span`: whether a box spans all the columns of its multi-column container.
    ColumnSpan { None = "none", All = "all" }
);

keywords!(
    /// `column-wrap`: whether columns that do not fit in a row start a new row below it.
    ColumnWrap {
        Auto = "auto",
        Nowrap = "nowrap",
        Wrap = "wrap",
    }
);

/// Parses the `columns` shorthand: `[ <'column-width'> || <'column-count'> ]
/// [ / <'column-height'> ]?`, as width, count and height.
#[allow(clippy::type_complexity)]
pub fn parse_columns(
    input: &mut Parser<'_>,
) -> Result<(
    Option<SpecifiedLengthPercentage>,
    Option<u32>,
    Option<SpecifiedLengthPercentage>,
)> {
    let (mut width, mut count) = (None, None);
    let mut autos = 0;
    let mut any = false;
    for _ in 0..2 {
        if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
            autos += 1;
        } else if width.is_none()
            && let Ok(value) = input.try_parse(|i| parse_length_percentage(i, false, true))
        {
            width = Some(value);
        } else if count.is_none()
            && let Ok(value) = input.try_parse(|i| parse_integer(i, 1))
        {
            count = Some(value as u32);
        } else {
            break;
        }
        any = true;
    }
    if !any || autos + usize::from(width.is_some()) + usize::from(count.is_some()) > 2 {
        return fail();
    }
    let height = if input.try_parse(|i| i.expect_delim('/')).is_ok() {
        ColumnLength::parse(input)?
    } else {
        None
    };
    Ok((width, count, height))
}
