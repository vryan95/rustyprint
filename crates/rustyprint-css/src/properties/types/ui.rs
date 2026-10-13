//! Value types of form control styling (CSS Basic User Interface 4 § 6, CSS Forms 1).

use cssparser::Parser;

use super::Result;
use crate::properties::{ComputeContext, Property};
use crate::serialize;
use crate::values::Color;

/// `accent-color`: `auto` or a color for the checked and filled parts of form controls.
/// `currentcolor` stays a keyword, resolved where it is used.
#[derive(Debug, Clone, PartialEq)]
pub enum AccentColor {
    Auto,
    Color(Color),
}

impl Property for AccentColor {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            AccentColor::Auto => "auto".into(),
            AccentColor::Color(color) => serialize::color(color),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_ident_matching("auto"))
            .is_ok()
        {
            return Ok(AccentColor::Auto);
        }
        Ok(AccentColor::Color(Color::parse(input)?))
    }

    fn compute(specified: &Self, ctx: &ComputeContext) -> Self {
        match specified {
            AccentColor::Color(color @ Color::Deferred(_)) => {
                AccentColor::Color(Color::Absolute(color.resolve_in(&ctx.colors())))
            }
            other => other.clone(),
        }
    }
}

keywords!(
    /// `appearance`: whether form controls look native (`auto` and the compatibility
    /// keywords, which behave like it), are plain boxes (`none`), or use the basic,
    /// styleable appearance (`base`, `base-select`).
    Appearance {
        None = "none",
        Auto = "auto",
        Base = "base",
        BaseSelect = "base-select",
        Textfield = "textfield",
        MenulistButton = "menulist-button",
        Searchfield = "searchfield",
        Textarea = "textarea",
        PushButton = "push-button",
        SliderHorizontal = "slider-horizontal",
        Checkbox = "checkbox",
        Radio = "radio",
        SquareButton = "square-button",
        Menulist = "menulist",
        Listbox = "listbox",
        Meter = "meter",
        ProgressBar = "progress-bar",
        Button = "button",
    }
);

impl Appearance {
    /// Whether the control is drawn as a native widget, rather than as a plain box.
    pub fn is_native(self) -> bool {
        !matches!(
            self,
            Appearance::None | Appearance::Base | Appearance::BaseSelect
        )
    }
}

keywords!(
    /// `field-sizing`: whether a form control has a fixed default size or sizes to its
    /// content.
    FieldSizing { Fixed = "fixed", Content = "content" }
);
