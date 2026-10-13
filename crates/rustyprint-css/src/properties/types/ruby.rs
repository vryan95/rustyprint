//! Value types of ruby layout (CSS Ruby Annotation Layout 1).

use cssparser::{ParseError, Parser};

use super::{Result, fail};
use crate::properties::{ComputeContext, Property};

keywords!(
    /// `ruby-align`: how ruby content is spread when it is narrower than its base or
    /// annotation.
    RubyAlign {
        SpaceAround = "space-around",
        Start = "start",
        Center = "center",
        SpaceBetween = "space-between",
    }
);

keywords!(
    /// `ruby-merge`: whether annotations of a ruby are placed over their own bases or merged
    /// over all of them.
    RubyMerge {
        Separate = "separate",
        Merge = "merge",
        Auto = "auto",
    }
);

keywords!(
    /// `ruby-overhang`: whether an annotation wider than its base may hang over the text
    /// next to the ruby.
    RubyOverhang { Auto = "auto", None = "none" }
);

/// `ruby-position`: `[ alternate || [ over | under ] ] | inter-character`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RubyPosition {
    /// Annotations go on the over side, or the under side.
    Side { under: bool, alternate: bool },
    /// Annotations go beside each base character, in vertical text (for Bopomofo).
    InterCharacter,
}

impl RubyPosition {
    pub const INITIAL: Self = RubyPosition::Side {
        under: false,
        alternate: true,
    };

    /// Whether annotation level `level` (from 0) goes on the under side.
    pub fn under(self, level: usize) -> bool {
        match self {
            RubyPosition::Side { under, alternate } => under != (alternate && level % 2 == 1),
            RubyPosition::InterCharacter => false,
        }
    }
}

impl Property for RubyPosition {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match *computed {
            RubyPosition::InterCharacter => "inter-character".into(),
            RubyPosition::Side {
                under: false,
                alternate: true,
            } => "alternate".into(),
            RubyPosition::Side { under, alternate } => {
                let side = if under { "under" } else { "over" };
                if alternate {
                    format!("alternate {side}")
                } else {
                    side.into()
                }
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|i| i.expect_ident_matching("inter-character"))
            .is_ok()
        {
            return Ok(RubyPosition::InterCharacter);
        }
        let (mut side, mut alternate) = (None, false);
        while let Ok(word) = input.try_parse(|i| i.expect_ident().map(|w| w.to_ascii_lowercase())) {
            match &*word {
                "alternate" if !alternate => alternate = true,
                "over" | "under" if side.is_none() => side = Some(word == "under"),
                _ => return Err(ParseError::custom(())),
            }
        }
        if side.is_none() && !alternate {
            return fail();
        }
        Ok(RubyPosition::Side {
            under: side.unwrap_or(false),
            alternate,
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}
