//! Value types of CSS Tables 3: `border-collapse`, `border-spacing`, `caption-side`,
//! `empty-cells` and `table-layout`.

use cssparser::Parser;

use super::{Result, SpecifiedLengthPercentage, parse_length_percentage};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

keywords!(
    /// `border-collapse`: whether cells share borders.
    BorderCollapse {
        Separate = "separate",
        Collapse = "collapse",
    }
);

keywords!(
    /// `caption-side`: where a table's captions go.
    CaptionSide { Top = "top", Bottom = "bottom" }
);

keywords!(
    /// `empty-cells`: whether cells without content draw their borders and backgrounds.
    EmptyCells { Show = "show", Hide = "hide" }
);

keywords!(
    /// `table-layout`: whether column widths depend on all the content or the first row.
    TableLayout { Auto = "auto", Fixed = "fixed" }
);

/// `border-spacing`: `<length>{1,2}`, the horizontal and vertical space between cells.
pub struct BorderSpacing;

impl Property for BorderSpacing {
    type Specified = (SpecifiedLengthPercentage, SpecifiedLengthPercentage);
    type Computed = (f32, f32);

    fn to_css(computed: &Self::Computed) -> String {
        let (h, v) = computed;
        if h == v {
            serialize::px(*h)
        } else {
            format!("{} {}", serialize::px(*h), serialize::px(*v))
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let h = parse_length_percentage(input, false, true)?;
        let v = input
            .try_parse(|input| parse_length_percentage(input, false, true))
            .unwrap_or_else(|_| h.clone());
        Ok((h, v))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        let length = |l: &SpecifiedLengthPercentage| {
            l.compute(ctx, true).as_length().unwrap_or(0.0).max(0.0)
        };
        (length(&specified.0), length(&specified.1))
    }
}
