//! Value types of positioned layout (CSS Positioned Layout 3).

use cssparser::Parser;

use super::{Result, parse_integer};
use crate::properties::{ComputeContext, Property};

/// `z-index`: `auto` or the box's stack level in its stacking context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZIndex {
    #[default]
    Auto,
    Integer(i32),
}

impl ZIndex {
    /// The stack level, with `auto` at level 0.
    pub fn level(self) -> i32 {
        match self {
            ZIndex::Auto => 0,
            ZIndex::Integer(level) => level,
        }
    }
}

impl Property for ZIndex {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ZIndex::Auto => "auto".into(),
            ZIndex::Integer(level) => level.to_string(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_ident_matching("auto"))
            .is_ok()
        {
            return Ok(ZIndex::Auto);
        }
        Ok(ZIndex::Integer(parse_integer(input, i32::MIN)?))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

keywords!(
    /// `float`: which side of its containing block a box floats to (CSS 2.1 § 9.5).
    Float {
        None = "none",
        Left = "left",
        Right = "right",
        InlineStart = "inline-start",
        InlineEnd = "inline-end",
    }
);

impl Float {
    /// Whether the box floats to the left, in a containing block of direction `rtl`.
    /// `None` if it does not float.
    pub fn is_left(self, rtl: bool) -> Option<bool> {
        match self {
            Float::None => None,
            Float::Left => Some(true),
            Float::Right => Some(false),
            Float::InlineStart => Some(!rtl),
            Float::InlineEnd => Some(rtl),
        }
    }
}

keywords!(
    /// `clear`: which earlier floats a box goes below (CSS 2.1 § 9.5.2). The block-axis
    /// values are for page floats, which do not exist here, so they clear nothing.
    Clear {
        None = "none",
        Left = "left",
        Right = "right",
        Both = "both",
        InlineStart = "inline-start",
        InlineEnd = "inline-end",
        BothInline = "both-inline",
        BlockStart = "block-start",
        BlockEnd = "block-end",
        Top = "top",
        Bottom = "bottom",
        BothBlock = "both-block",
    }
);

impl Clear {
    /// Whether the box clears left floats and right floats, in a containing block of
    /// direction `rtl`.
    pub fn sides(self, rtl: bool) -> (bool, bool) {
        match self {
            Clear::Left => (true, false),
            Clear::Right => (false, true),
            Clear::Both | Clear::BothInline => (true, true),
            Clear::InlineStart => (!rtl, rtl),
            Clear::InlineEnd => (rtl, !rtl),
            Clear::None
            | Clear::BlockStart
            | Clear::BlockEnd
            | Clear::Top
            | Clear::Bottom
            | Clear::BothBlock => (false, false),
        }
    }
}
