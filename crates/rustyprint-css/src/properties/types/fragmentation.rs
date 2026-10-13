//! Value types of CSS Fragmentation 3: breaks between and inside boxes.

use cssparser::Parser;

use super::{Result, fail, parse_integer};
use crate::properties::{ComputeContext, Property};

keywords!(
    /// `break-before` and `break-after`: whether a break between boxes is forced or avoided.
    BreakBetween {
        Auto = "auto",
        Avoid = "avoid",
        Always = "always",
        All = "all",
        AvoidPage = "avoid-page",
        Page = "page",
        Left = "left",
        Right = "right",
        Recto = "recto",
        Verso = "verso",
        AvoidColumn = "avoid-column",
        Column = "column",
        AvoidRegion = "avoid-region",
        Region = "region",
    }
);

/// The side of a spread a page is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageSide {
    Left,
    Right,
}

impl BreakBetween {
    /// Whether the value forces a page break. Column and region breaks have no effect on
    /// pages.
    pub fn forces_page(self) -> bool {
        matches!(
            self,
            BreakBetween::Always
                | BreakBetween::All
                | BreakBetween::Page
                | BreakBetween::Left
                | BreakBetween::Right
                | BreakBetween::Recto
                | BreakBetween::Verso
        )
    }

    /// Whether the value avoids a page break.
    pub fn avoids_page(self) -> bool {
        matches!(self, BreakBetween::Avoid | BreakBetween::AvoidPage)
    }

    /// Whether the value forces a break between columns, inside a multi-column container.
    /// Page breaks break columns too.
    pub fn forces_column(self) -> bool {
        self == BreakBetween::Column || self.forces_page()
    }

    /// Whether the value avoids a break between columns.
    pub fn avoids_column(self) -> bool {
        matches!(self, BreakBetween::Avoid | BreakBetween::AvoidColumn)
    }

    /// The side the page after a forced break must be on. Pages progress left to right, so
    /// recto pages are right pages.
    pub fn side(self) -> Option<PageSide> {
        match self {
            BreakBetween::Left | BreakBetween::Verso => Some(PageSide::Left),
            BreakBetween::Right | BreakBetween::Recto => Some(PageSide::Right),
            _ => None,
        }
    }
}

keywords!(
    /// `break-inside`: whether breaks inside a box are avoided.
    BreakInside {
        Auto = "auto",
        Avoid = "avoid",
        AvoidPage = "avoid-page",
        AvoidColumn = "avoid-column",
        AvoidRegion = "avoid-region",
    }
);

impl BreakInside {
    pub fn avoids_page(self) -> bool {
        matches!(self, BreakInside::Avoid | BreakInside::AvoidPage)
    }

    pub fn avoids_column(self) -> bool {
        matches!(self, BreakInside::Avoid | BreakInside::AvoidColumn)
    }
}

keywords!(
    /// `box-decoration-break`: whether a box's borders and padding wrap each fragment.
    BoxDecorationBreak { Slice = "slice", Clone = "clone" }
);

/// `page`: `auto | <custom-ident>`, the named page a box goes on.
pub struct PageName;

impl Property for PageName {
    type Specified = Option<std::sync::Arc<str>>;
    type Computed = Option<std::sync::Arc<str>>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "auto".into(),
            Some(name) => {
                let mut out = String::new();
                let _ = cssparser::serialize_identifier(name, &mut out);
                out
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let ident = input.expect_ident_cloned()?;
        match &*ident.to_ascii_lowercase() {
            "auto" => Ok(None),
            "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default" => fail(),
            _ => Ok(Some(std::sync::Arc::from(&*ident))),
        }
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

/// `orphans` and `widows`: `<integer [1,∞]>`.
pub struct LineCount;

impl Property for LineCount {
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

/// `page-break-before` and `page-break-after`: `auto | always | avoid | left | right`, as
/// `break-before` and `break-after` values.
pub(crate) fn parse_page_break(input: &mut Parser<'_>) -> Result<BreakBetween> {
    Ok(match &*input.expect_ident()?.to_ascii_lowercase() {
        "auto" => BreakBetween::Auto,
        "always" => BreakBetween::Page,
        "avoid" => BreakBetween::Avoid,
        "left" => BreakBetween::Left,
        "right" => BreakBetween::Right,
        _ => return fail(),
    })
}

/// `page-break-inside`: `auto | avoid`.
pub(crate) fn parse_page_break_inside(input: &mut Parser<'_>) -> Result<BreakInside> {
    Ok(match &*input.expect_ident()?.to_ascii_lowercase() {
        "auto" => BreakInside::Auto,
        "avoid" => BreakInside::Avoid,
        _ => return fail(),
    })
}
