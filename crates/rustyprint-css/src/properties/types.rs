//! Value types of properties: how each parses and computes.

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{ComputeContext, Property};
use crate::serialize;
use crate::values::{
    AbsoluteColor, Base, CalcContext, CalcNode, Color, ColorContext, Length, LengthUnit,
    NumericType, is_math_function,
};

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

/// Defines a property whose value is one of a set of keywords.
macro_rules! keywords {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $keyword:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $keyword,)+
                }
            }

            pub fn from_keyword(keyword: &str) -> Option<Self> {
                match keyword {
                    $($keyword => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl Property for $name {
            type Specified = Self;
            type Computed = Self;

            fn to_css(computed: &Self) -> String {
                computed.as_str().into()
            }

            fn parse(input: &mut Parser<'_>) -> Result<Self> {
                let ident = input.expect_ident()?.to_ascii_lowercase();
                Self::from_keyword(&ident).ok_or_else(|| cssparser::ParseError::custom(()))
            }

            fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
                *specified
            }
        }
    };
}

keywords!(BorderStyle {
    None = "none",
    Hidden = "hidden",
    Dotted = "dotted",
    Dashed = "dashed",
    Solid = "solid",
    Double = "double",
    Groove = "groove",
    Ridge = "ridge",
    Inset = "inset",
    Outset = "outset",
});

keywords!(Visibility { Visible = "visible", Hidden = "hidden", Collapse = "collapse" });

keywords!(BoxSizing { ContentBox = "content-box", BorderBox = "border-box" });

keywords!(Overflow { Visible = "visible", Hidden = "hidden", Clip = "clip", Scroll = "scroll", Auto = "auto" });

keywords!(TextAlign {
    Start = "start",
    End = "end",
    Left = "left",
    Right = "right",
    Center = "center",
    Justify = "justify",
    MatchParent = "match-parent",
    JustifyAll = "justify-all",
});

keywords!(WritingMode {
    HorizontalTb = "horizontal-tb",
    VerticalRl = "vertical-rl",
    VerticalLr = "vertical-lr",
    SidewaysRl = "sideways-rl",
    SidewaysLr = "sideways-lr",
});

keywords!(Direction { Ltr = "ltr", Rtl = "rtl" });

keywords!(UnicodeBidi {
    Normal = "normal",
    Embed = "embed",
    Isolate = "isolate",
    BidiOverride = "bidi-override",
    IsolateOverride = "isolate-override",
    Plaintext = "plaintext",
});

keywords!(TextOrientation { Mixed = "mixed", Upright = "upright", Sideways = "sideways" });

keywords!(FontVariantCaps {
    Normal = "normal",
    SmallCaps = "small-caps",
    AllSmallCaps = "all-small-caps",
    PetiteCaps = "petite-caps",
    AllPetiteCaps = "all-petite-caps",
    Unicase = "unicase",
    TitlingCaps = "titling-caps",
});

impl WritingMode {
    pub fn is_vertical(self) -> bool {
        !matches!(self, WritingMode::HorizontalTb)
    }
}

// ---------------------------------------------------------------------------------------------
// Numbers and lengths

/// A specified `<length-percentage>`.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecifiedLengthPercentage {
    Length(Length),
    /// A fraction: `50%` is 0.5.
    Percentage(f32),
    Calc(Box<CalcNode>),
}

/// A computed `<length-percentage>`; percentages resolve at layout time.
#[derive(Debug, Clone, PartialEq)]
pub enum LengthPercentage {
    /// CSS pixels.
    Length(f32),
    /// A fraction: `50%` is 0.5.
    Percentage(f32),
    /// A math expression with lengths in pixels and percentages left to resolve.
    Calc(Arc<CalcNode>),
}

impl LengthPercentage {
    pub const ZERO: Self = LengthPercentage::Length(0.0);

    /// Resolves against `basis`, what 100% equals.
    pub fn resolve(&self, basis: f32) -> f32 {
        match self {
            LengthPercentage::Length(px) => *px,
            LengthPercentage::Percentage(fraction) => fraction * basis,
            LengthPercentage::Calc(node) => node
                .evaluate(&CalcContext {
                    percent_basis: Some(f64::from(basis)),
                    ..Default::default()
                })
                .map_or(0.0, |v| v as f32),
        }
    }

    /// Resolves against `basis`, or `None` if the value cannot be resolved here, as an
    /// `anchor()` without an anchor or fallback cannot.
    pub fn try_resolve(&self, basis: f32) -> Option<f32> {
        match self {
            LengthPercentage::Calc(node) => node
                .evaluate(&CalcContext {
                    percent_basis: Some(f64::from(basis)),
                    ..Default::default()
                })
                .map(|v| v as f32),
            other => Some(other.resolve(basis)),
        }
    }

    /// The length if it does not depend on a percentage.
    pub fn as_length(&self) -> Option<f32> {
        match self {
            LengthPercentage::Length(px) => Some(*px),
            _ => None,
        }
    }
}

/// Parses `<length-percentage>`, or `<length>` if `percentages` is false.
pub fn parse_length_percentage(
    input: &mut Parser<'_>,
    percentages: bool,
    non_negative: bool,
) -> Result<SpecifiedLengthPercentage> {
    let token = input.next()?.clone();
    let value = match token {
        Token::Dimension {
            value, ref unit, ..
        } => {
            let unit = LengthUnit::from_name(unit).ok_or_else(|| ParseError::custom(()))?;
            SpecifiedLengthPercentage::Length(Length { value, unit })
        }
        Token::Percentage { unit_value, .. } if percentages => {
            SpecifiedLengthPercentage::Percentage(unit_value)
        }
        // Unitless zero is a valid length.
        Token::Number { value: 0.0, .. } => SpecifiedLengthPercentage::Length(Length::px(0.0)),
        Token::Function(ref name) if is_math_function(name) => {
            let node = CalcNode::parse_function(input, name)?;
            let fold = if percentages {
                Some(Base::Length)
            } else {
                None
            };
            if node.numeric_type(fold) != Some(NumericType::of(Base::Length)) {
                return fail();
            }
            return Ok(SpecifiedLengthPercentage::Calc(Box::new(node)));
        }
        _ => return fail(),
    };
    let negative = match &value {
        SpecifiedLengthPercentage::Length(length) => length.value < 0.0,
        SpecifiedLengthPercentage::Percentage(fraction) => *fraction < 0.0,
        SpecifiedLengthPercentage::Calc(_) => false,
    };
    if non_negative && negative {
        return fail();
    }
    Ok(value)
}

impl SpecifiedLengthPercentage {
    pub fn compute(&self, ctx: &ComputeContext, non_negative: bool) -> LengthPercentage {
        let clamp = |v: f32| if non_negative { v.max(0.0) } else { v };
        match self {
            SpecifiedLengthPercentage::Length(length) => {
                LengthPercentage::Length(length.to_px(&ctx.length()))
            }
            SpecifiedLengthPercentage::Percentage(fraction) => {
                LengthPercentage::Percentage(*fraction)
            }
            SpecifiedLengthPercentage::Calc(node) => {
                let node = node.absolutize(&ctx.length());
                if node.has_percentage() || node.has_anchor() {
                    return LengthPercentage::Calc(Arc::new(node));
                }
                let value = node.evaluate(&ctx.calc()).unwrap_or(0.0);
                LengthPercentage::Length(clamp(finite(value)))
            }
        }
    }
}

/// Clamps infinite results of math functions to the largest representable value.
fn finite(value: f64) -> f32 {
    value.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32
}

/// `<length-percentage>` that may be negative (`text-indent`, insets, margins without auto).
pub struct LengthPercentageValue;

impl Property for LengthPercentageValue {
    type Specified = SpecifiedLengthPercentage;
    type Computed = LengthPercentage;

    fn to_css(computed: &Self::Computed) -> String {
        length_percentage_css(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_length_percentage(input, true, false)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.compute(ctx, false)
    }
}

/// Non-negative `<length-percentage>`, as `padding` takes.
pub struct NonNegativeLengthPercentage;

impl Property for NonNegativeLengthPercentage {
    type Specified = SpecifiedLengthPercentage;
    type Computed = LengthPercentage;

    fn to_css(computed: &Self::Computed) -> String {
        length_percentage_css(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_length_percentage(input, true, true)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.compute(ctx, true)
    }
}

/// `auto | <length-percentage>`, as `margin` takes.
#[derive(Debug, Clone, PartialEq)]
pub enum MaybeAuto<T> {
    Auto,
    Value(T),
}

pub struct MarginValue;

impl Property for MarginValue {
    type Specified = MaybeAuto<SpecifiedLengthPercentage>;
    type Computed = MaybeAuto<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            MaybeAuto::Auto => "auto".into(),
            MaybeAuto::Value(value) => length_percentage_css(value),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if input
            .try_parse(|input| input.expect_ident_matching("auto"))
            .is_ok()
        {
            return Ok(MaybeAuto::Auto);
        }
        Ok(MaybeAuto::Value(parse_length_percentage(
            input, true, false,
        )?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        match specified {
            MaybeAuto::Auto => MaybeAuto::Auto,
            MaybeAuto::Value(value) => MaybeAuto::Value(value.compute(ctx, false)),
        }
    }
}

/// `width`, `height` and their logical and minimum forms.
#[derive(Debug, Clone, PartialEq)]
pub enum Size<T> {
    Auto,
    /// `none`, for maximum sizes only.
    None,
    MinContent,
    MaxContent,
    FitContent(Option<T>),
    Stretch,
    Value(T),
    /// `calc-size(basis, expression)`: the expression of the basis's size.
    CalcSize(Arc<CalcSize<T>>),
}

/// The value of `calc-size()` (CSS Values 5 § 10.1).
#[derive(Debug, Clone, PartialEq)]
pub struct CalcSize<T> {
    pub basis: CalcSizeBasis<T>,
    /// The expression, where `size` is the basis's size; lengths in pixels once computed.
    pub expression: Arc<CalcNode>,
}

/// What `size` refers to in `calc-size()`.
#[derive(Debug, Clone, PartialEq)]
pub enum CalcSizeBasis<T> {
    Auto,
    MinContent,
    MaxContent,
    FitContent,
    /// `any`: no size, which the expression does not use.
    Any,
    Value(T),
}

/// Parses the arguments of `calc-size()`.
fn parse_calc_size(input: &mut Parser<'_>) -> Result<CalcSize<SpecifiedLengthPercentage>> {
    let basis = match input.try_parse(|input| input.expect_ident_cloned()) {
        Ok(keyword) => match &*keyword.to_ascii_lowercase() {
            "auto" => CalcSizeBasis::Auto,
            "min-content" => CalcSizeBasis::MinContent,
            "max-content" => CalcSizeBasis::MaxContent,
            "fit-content" => CalcSizeBasis::FitContent,
            "any" => CalcSizeBasis::Any,
            _ => return fail(),
        },
        Err(_) => CalcSizeBasis::Value(parse_length_percentage(input, true, false)?),
    };
    input.expect_comma()?;
    let expression = CalcNode::parse_calc_size(input)?;
    let valid = expression.numeric_type(Some(Base::Length)) == Some(NumericType::of(Base::Length))
        && !(basis == CalcSizeBasis::Any && expression.has_size());
    if !valid {
        return fail();
    }
    Ok(CalcSize {
        basis,
        expression: Arc::new(expression),
    })
}

fn parse_size(
    input: &mut Parser<'_>,
    allow_none: bool,
    allow_auto: bool,
) -> Result<Size<SpecifiedLengthPercentage>> {
    if let Ok(keyword) = input.try_parse(|input| input.expect_ident_cloned()) {
        return Ok(match &*keyword.to_ascii_lowercase() {
            "auto" if allow_auto => Size::Auto,
            "none" if allow_none => Size::None,
            "min-content" => Size::MinContent,
            "max-content" => Size::MaxContent,
            "fit-content" => Size::FitContent(None),
            "stretch" => Size::Stretch,
            _ => return fail(),
        });
    }
    if input
        .try_parse(|input| input.expect_function_matching("calc-size"))
        .is_ok()
    {
        return input
            .parse_nested_block(|input| Ok(Size::CalcSize(Arc::new(parse_calc_size(input)?))));
    }
    if input
        .try_parse(|input| input.expect_function_matching("fit-content"))
        .is_ok()
    {
        return input.parse_nested_block(|input| {
            Ok(Size::FitContent(Some(parse_length_percentage(
                input, true, true,
            )?)))
        });
    }
    Ok(Size::Value(parse_length_percentage(input, true, true)?))
}

fn compute_size(
    specified: &Size<SpecifiedLengthPercentage>,
    ctx: &ComputeContext,
) -> Size<LengthPercentage> {
    match specified {
        Size::Auto => Size::Auto,
        Size::None => Size::None,
        Size::MinContent => Size::MinContent,
        Size::MaxContent => Size::MaxContent,
        Size::FitContent(value) => Size::FitContent(value.as_ref().map(|v| v.compute(ctx, true))),
        Size::Stretch => Size::Stretch,
        Size::Value(value) => Size::Value(value.compute(ctx, true)),
        Size::CalcSize(calc) => Size::CalcSize(Arc::new(CalcSize {
            basis: match &calc.basis {
                CalcSizeBasis::Auto => CalcSizeBasis::Auto,
                CalcSizeBasis::MinContent => CalcSizeBasis::MinContent,
                CalcSizeBasis::MaxContent => CalcSizeBasis::MaxContent,
                CalcSizeBasis::FitContent => CalcSizeBasis::FitContent,
                CalcSizeBasis::Any => CalcSizeBasis::Any,
                CalcSizeBasis::Value(value) => CalcSizeBasis::Value(value.compute(ctx, true)),
            },
            expression: Arc::new(calc.expression.absolutize(&ctx.length())),
        })),
    }
}

/// `width` and `height`: `auto | <length-percentage> | min-content | ...`.
pub struct SizeValue;

impl Property for SizeValue {
    type Specified = Size<SpecifiedLengthPercentage>;
    type Computed = Size<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        size_css(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_size(input, false, true)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        compute_size(specified, ctx)
    }
}

/// `max-width` and `max-height`: `none | <length-percentage> | ...`.
pub struct MaxSizeValue;

impl Property for MaxSizeValue {
    type Specified = Size<SpecifiedLengthPercentage>;
    type Computed = Size<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        size_css(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_size(input, true, false)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        compute_size(specified, ctx)
    }
}

/// `border-*-width`: `thin | medium | thick | <length>`, computed to pixels. The cascade sets
/// it to 0 when the border style is `none` or `hidden`.
pub struct BorderWidth;

impl Property for BorderWidth {
    type Specified = SpecifiedLengthPercentage;
    type Computed = f32;

    fn to_css(computed: &Self::Computed) -> String {
        serialize::px(*computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if let Ok(keyword) = input.try_parse(|input| input.expect_ident_cloned()) {
            let px = match &*keyword.to_ascii_lowercase() {
                "thin" => 1.0,
                "medium" => 3.0,
                "thick" => 5.0,
                _ => return fail(),
            };
            return Ok(SpecifiedLengthPercentage::Length(Length::px(px)));
        }
        parse_length_percentage(input, false, true)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> f32 {
        let px = specified.compute(ctx, true).as_length().unwrap_or(0.0);
        // Widths between 0 and 1px round up to 1px; others floor to whole pixels.
        if px > 0.0 && px < 1.0 {
            1.0
        } else {
            px.floor()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Colors

/// The `color` property: computed to an absolute color, with `currentcolor` meaning the
/// inherited color.
pub struct ColorProperty;

impl Property for ColorProperty {
    type Specified = Color;
    type Computed = AbsoluteColor;

    fn to_css(computed: &Self::Computed) -> String {
        serialize::absolute_color(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Color> {
        Color::parse(input)
    }

    fn compute(specified: &Color, ctx: &ComputeContext) -> AbsoluteColor {
        specified.resolve_in(&ColorContext {
            current: ctx.parent.color,
            dark: ctx.dark,
        })
    }
}

/// Other color properties: `currentcolor` stays a keyword until used.
pub struct ColorValue;

impl Property for ColorValue {
    type Specified = Color;
    type Computed = Color;

    fn to_css(computed: &Self::Computed) -> String {
        serialize::color(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Color> {
        Color::parse(input)
    }

    fn compute(specified: &Color, ctx: &ComputeContext) -> Color {
        match specified {
            Color::Deferred(_) => Color::Absolute(specified.resolve_in(&ctx.colors())),
            other => other.clone(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Display

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayOutside {
    Block,
    Inline,
    RunIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayInside {
    Flow,
    FlowRoot,
    Table,
    Flex,
    Grid,
    Ruby,
    Math,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayInternal {
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableCell,
    TableColumnGroup,
    TableColumn,
    TableCaption,
    RubyBase,
    RubyText,
    RubyBaseContainer,
    RubyTextContainer,
}

/// The `display` property (CSS Display 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Display {
    None,
    Contents,
    Normal {
        outside: DisplayOutside,
        inside: DisplayInside,
        list_item: bool,
    },
    Internal(DisplayInternal),
}

impl Display {
    pub const INLINE: Self = Display::Normal {
        outside: DisplayOutside::Inline,
        inside: DisplayInside::Flow,
        list_item: false,
    };
    pub const BLOCK: Self = Display::Normal {
        outside: DisplayOutside::Block,
        inside: DisplayInside::Flow,
        list_item: false,
    };

    fn keyword(keyword: &str) -> Option<Self> {
        use DisplayInside as I;
        use DisplayInternal as T;
        use DisplayOutside as O;
        let normal = |outside, inside| Display::Normal {
            outside,
            inside,
            list_item: false,
        };
        Some(match keyword {
            "none" => Display::None,
            "contents" => Display::Contents,
            "inline-block" => normal(O::Inline, I::FlowRoot),
            "inline-table" => normal(O::Inline, I::Table),
            "inline-flex" => normal(O::Inline, I::Flex),
            "inline-grid" => normal(O::Inline, I::Grid),
            "table-row-group" => Display::Internal(T::TableRowGroup),
            "table-header-group" => Display::Internal(T::TableHeaderGroup),
            "table-footer-group" => Display::Internal(T::TableFooterGroup),
            "table-row" => Display::Internal(T::TableRow),
            "table-cell" => Display::Internal(T::TableCell),
            "table-column-group" => Display::Internal(T::TableColumnGroup),
            "table-column" => Display::Internal(T::TableColumn),
            "table-caption" => Display::Internal(T::TableCaption),
            "ruby-base" => Display::Internal(T::RubyBase),
            "ruby-text" => Display::Internal(T::RubyText),
            "ruby-base-container" => Display::Internal(T::RubyBaseContainer),
            "ruby-text-container" => Display::Internal(T::RubyTextContainer),
            _ => return None,
        })
    }
}

impl Property for Display {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self::Computed) -> String {
        display_css(*computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let first = input.expect_ident()?.to_ascii_lowercase();
        if let Some(display) = Display::keyword(&first) {
            return Ok(display);
        }
        // `[<display-outside> || <display-inside>] | <display-listitem>`
        let mut outside = None;
        let mut inside = None;
        let mut list_item = false;
        let mut keyword = Some(first);
        while let Some(word) = keyword.take() {
            match word.as_str() {
                "block" if outside.is_none() => outside = Some(DisplayOutside::Block),
                "inline" if outside.is_none() => outside = Some(DisplayOutside::Inline),
                "run-in" if outside.is_none() => outside = Some(DisplayOutside::RunIn),
                "flow" if inside.is_none() => inside = Some(DisplayInside::Flow),
                "flow-root" if inside.is_none() => inside = Some(DisplayInside::FlowRoot),
                "table" if inside.is_none() => inside = Some(DisplayInside::Table),
                "flex" if inside.is_none() => inside = Some(DisplayInside::Flex),
                "grid" if inside.is_none() => inside = Some(DisplayInside::Grid),
                "ruby" if inside.is_none() => inside = Some(DisplayInside::Ruby),
                "math" if inside.is_none() => inside = Some(DisplayInside::Math),
                "list-item" if !list_item => list_item = true,
                _ => return fail(),
            }
            keyword = input
                .try_parse(|input| input.expect_ident_cloned())
                .ok()
                .map(|k| k.to_ascii_lowercase());
        }
        if list_item
            && !matches!(
                inside,
                None | Some(DisplayInside::Flow | DisplayInside::FlowRoot)
            )
        {
            return fail();
        }
        let inside_value = inside.unwrap_or(DisplayInside::Flow);
        let outside = outside.unwrap_or(if inside == Some(DisplayInside::Ruby) {
            DisplayOutside::Inline
        } else {
            DisplayOutside::Block
        });
        Ok(Display::Normal {
            outside,
            inside: inside_value,
            list_item,
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

// ---------------------------------------------------------------------------------------------
// Fonts

/// `font-size`, computed to pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum FontSize {
    /// An absolute-size keyword, as a factor of `medium` (16px).
    Absolute(f32),
    Larger,
    Smaller,
    Value(SpecifiedLengthPercentage),
}

/// `medium` in CSS pixels.
pub const MEDIUM_FONT_SIZE: f32 = 16.0;

impl Property for FontSize {
    type Specified = Self;
    type Computed = f32;

    fn to_css(computed: &Self::Computed) -> String {
        serialize::px(*computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if let Ok(keyword) = input.try_parse(|input| input.expect_ident_cloned()) {
            // CSS Fonts 4 § 2.5 scaling factors.
            return Ok(match &*keyword.to_ascii_lowercase() {
                "xx-small" => FontSize::Absolute(3.0 / 5.0),
                "x-small" => FontSize::Absolute(3.0 / 4.0),
                "small" => FontSize::Absolute(8.0 / 9.0),
                "medium" => FontSize::Absolute(1.0),
                "large" => FontSize::Absolute(6.0 / 5.0),
                "x-large" => FontSize::Absolute(3.0 / 2.0),
                "xx-large" => FontSize::Absolute(2.0),
                "xxx-large" => FontSize::Absolute(3.0),
                "larger" => FontSize::Larger,
                "smaller" => FontSize::Smaller,
                _ => return fail(),
            });
        }
        Ok(FontSize::Value(parse_length_percentage(input, true, true)?))
    }

    /// `ctx.font_size` is the parent's font size while `font-size` computes. Sizes relative
    /// to the parent's take the element's own `zoom`; absolute ones its effective zoom.
    fn compute(specified: &Self, ctx: &ComputeContext) -> f32 {
        let own_zoom = ctx.zoom / ctx.parent.effective_zoom;
        let parent = ctx.parent.font_size * own_zoom;
        match specified {
            FontSize::Absolute(factor) => MEDIUM_FONT_SIZE * factor * ctx.zoom,
            FontSize::Larger => parent * 1.2,
            FontSize::Smaller => parent / 1.2,
            FontSize::Value(value) => {
                let font_relative = matches!(
                    value,
                    SpecifiedLengthPercentage::Length(length) if length.unit.absolute_px().is_none()
                );
                let px = value.compute(ctx, true).resolve(parent);
                (if font_relative { px * own_zoom } else { px }).max(0.0)
            }
        }
    }
}

/// `font-weight`, computed to a number from 1 to 1000.
#[derive(Debug, Clone, PartialEq)]
pub enum FontWeight {
    Absolute(f32),
    Bolder,
    Lighter,
}

impl Property for FontWeight {
    type Specified = Self;
    type Computed = f32;

    fn to_css(computed: &Self::Computed) -> String {
        serialize::number(f64::from(*computed))
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if let Ok(keyword) = input.try_parse(|input| input.expect_ident_cloned()) {
            return Ok(match &*keyword.to_ascii_lowercase() {
                "normal" => FontWeight::Absolute(400.0),
                "bold" => FontWeight::Absolute(700.0),
                "bolder" => FontWeight::Bolder,
                "lighter" => FontWeight::Lighter,
                _ => return fail(),
            });
        }
        let weight = parse_number(input)?;
        if !(1.0..=1000.0).contains(&weight) {
            return fail();
        }
        Ok(FontWeight::Absolute(weight))
    }

    fn compute(specified: &Self, ctx: &ComputeContext) -> f32 {
        let parent = ctx.parent.font_weight;
        // CSS Fonts 4 § 2.2.1 relative weights.
        match specified {
            FontWeight::Absolute(weight) => *weight,
            FontWeight::Bolder if parent < 350.0 => 400.0,
            FontWeight::Bolder if parent < 550.0 => 700.0,
            FontWeight::Bolder => parent.max(900.0),
            FontWeight::Lighter if parent < 100.0 => parent,
            FontWeight::Lighter if parent < 550.0 => 100.0,
            FontWeight::Lighter if parent < 750.0 => 400.0,
            FontWeight::Lighter => 700.0,
        }
    }
}

/// Parses a `<number>`, including math functions.
pub fn parse_number(input: &mut Parser<'_>) -> Result<f32> {
    match input.next()?.clone() {
        Token::Number { value, .. } => Ok(value),
        Token::Function(name) if is_math_function(&name) => {
            let node = CalcNode::parse_function(input, &name)?;
            if node.numeric_type(None) != Some(NumericType::NUMBER) {
                return fail();
            }
            node.evaluate(&CalcContext::default())
                .map(finite)
                .ok_or_else(|| ParseError::custom(()))
        }
        _ => fail(),
    }
}

/// `font-style`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
    /// `oblique` with its angle in degrees.
    Oblique(f32),
}

impl Property for FontStyle {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            FontStyle::Normal => "normal".into(),
            FontStyle::Italic => "italic".into(),
            FontStyle::Oblique(angle) if *angle == 14.0 => "oblique".into(),
            FontStyle::Oblique(angle) => {
                format!("oblique {}deg", serialize::number(f64::from(*angle)))
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Ok(match &*input.expect_ident()?.to_ascii_lowercase() {
            "normal" => FontStyle::Normal,
            "italic" => FontStyle::Italic,
            "oblique" => {
                let angle = input
                    .try_parse(|input| -> Result<f32> {
                        let degrees = match input.next()?.clone() {
                            Token::Dimension { value, unit, .. } => {
                                let unit = crate::values::Unit::from_name(&unit)
                                    .ok_or_else(|| ParseError::custom(()))?;
                                let node = CalcNode::Dimension(f64::from(value), unit);
                                if node.numeric_type(None) != Some(NumericType::of(Base::Angle)) {
                                    return fail();
                                }
                                node.evaluate(&CalcContext::default()).unwrap_or(0.0) as f32
                            }
                            _ => return fail(),
                        };
                        if !(-90.0..=90.0).contains(&degrees) {
                            return fail();
                        }
                        Ok(degrees)
                    })
                    .unwrap_or(14.0);
                FontStyle::Oblique(angle)
            }
            _ => return fail(),
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `font-width` (formerly `font-stretch`), computed to a percentage.
pub struct FontWidth;

impl Property for FontWidth {
    type Specified = f32;
    type Computed = f32;

    fn to_css(computed: &Self::Computed) -> String {
        format!("{}%", serialize::number(f64::from(*computed)))
    }

    fn parse(input: &mut Parser<'_>) -> Result<f32> {
        if let Ok(keyword) = input.try_parse(|input| input.expect_ident_cloned()) {
            return Ok(match &*keyword.to_ascii_lowercase() {
                "ultra-condensed" => 50.0,
                "extra-condensed" => 62.5,
                "condensed" => 75.0,
                "semi-condensed" => 87.5,
                "normal" => 100.0,
                "semi-expanded" => 112.5,
                "expanded" => 125.0,
                "extra-expanded" => 150.0,
                "ultra-expanded" => 200.0,
                _ => return fail(),
            });
        }
        let percent = input.expect_percentage()? * 100.0;
        if percent < 0.0 {
            return fail();
        }
        Ok(percent)
    }

    fn compute(specified: &f32, _ctx: &ComputeContext) -> f32 {
        *specified
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
    UiSerif,
    UiSansSerif,
    UiMonospace,
    UiRounded,
    Math,
    Emoji,
    Fangsong,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FamilyName {
    Named(String),
    Generic(GenericFamily),
}

/// `font-family`: a list of family names.
pub struct FontFamily;

impl Property for FontFamily {
    type Specified = Arc<[FamilyName]>;
    type Computed = Arc<[FamilyName]>;

    fn to_css(computed: &Self::Computed) -> String {
        computed
            .iter()
            .map(|family| match family {
                FamilyName::Named(name) => {
                    let mut quoted = String::new();
                    let _ = cssparser::serialize_string(name, &mut quoted);
                    quoted
                }
                FamilyName::Generic(generic) => generic.as_str().into(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let families = input.parse_comma_separated(|input| -> Result<FamilyName> {
            if let Ok(name) = input.try_parse(|input| input.expect_string_cloned()) {
                return Ok(FamilyName::Named(name.to_string()));
            }
            let first = input.expect_ident_cloned()?;
            let lower = first.to_ascii_lowercase();
            let mut words = vec![first.to_string()];
            while let Ok(word) = input.try_parse(|input| input.expect_ident_cloned()) {
                words.push(word.to_string());
            }
            if words.len() == 1 {
                let generic = match lower.as_str() {
                    "serif" => Some(GenericFamily::Serif),
                    "sans-serif" => Some(GenericFamily::SansSerif),
                    "monospace" => Some(GenericFamily::Monospace),
                    "cursive" => Some(GenericFamily::Cursive),
                    "fantasy" => Some(GenericFamily::Fantasy),
                    "system-ui" => Some(GenericFamily::SystemUi),
                    "ui-serif" => Some(GenericFamily::UiSerif),
                    "ui-sans-serif" => Some(GenericFamily::UiSansSerif),
                    "ui-monospace" => Some(GenericFamily::UiMonospace),
                    "ui-rounded" => Some(GenericFamily::UiRounded),
                    "math" => Some(GenericFamily::Math),
                    "emoji" => Some(GenericFamily::Emoji),
                    "fangsong" => Some(GenericFamily::Fangsong),
                    // CSS-wide keywords and `default` cannot be unquoted family names.
                    "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default" => {
                        return fail();
                    }
                    _ => None,
                };
                if let Some(generic) = generic {
                    return Ok(FamilyName::Generic(generic));
                }
            }
            Ok(FamilyName::Named(words.join(" ")))
        })?;
        Ok(families.into())
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

/// `line-height`.
#[derive(Debug, Clone, PartialEq)]
pub enum LineHeight {
    Normal,
    /// A multiple of the font size; inherited as a number.
    Number(f32),
    /// CSS pixels.
    Length(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpecifiedLineHeight {
    Normal,
    Number(f32),
    Value(SpecifiedLengthPercentage),
}

impl Property for LineHeight {
    type Specified = SpecifiedLineHeight;
    type Computed = LineHeight;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            LineHeight::Normal => "normal".into(),
            LineHeight::Number(n) => serialize::number(f64::from(*n)),
            LineHeight::Length(px) => serialize::px(*px),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<SpecifiedLineHeight> {
        if input
            .try_parse(|input| input.expect_ident_matching("normal"))
            .is_ok()
        {
            return Ok(SpecifiedLineHeight::Normal);
        }
        if let Ok(number) = input.try_parse(parse_number) {
            if number < 0.0 {
                return fail();
            }
            return Ok(SpecifiedLineHeight::Number(number));
        }
        Ok(SpecifiedLineHeight::Value(parse_length_percentage(
            input, true, true,
        )?))
    }

    fn compute(specified: &SpecifiedLineHeight, ctx: &ComputeContext) -> LineHeight {
        match specified {
            SpecifiedLineHeight::Normal => LineHeight::Normal,
            SpecifiedLineHeight::Number(n) => LineHeight::Number(*n),
            SpecifiedLineHeight::Value(value) => {
                LineHeight::Length(value.compute(ctx, true).resolve(ctx.font_size))
            }
        }
    }
}

impl LineHeight {
    /// The line height in pixels for a font size. `normal` uses 1.2 until font metrics are
    /// available.
    pub fn to_px(&self, font_size: f32) -> f32 {
        match self {
            LineHeight::Normal => font_size * 1.2,
            LineHeight::Number(n) => font_size * n,
            LineHeight::Length(px) => *px,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Serialization helpers

pub(crate) fn length_percentage_css(value: &LengthPercentage) -> String {
    match value {
        LengthPercentage::Length(px) => serialize::px(*px),
        LengthPercentage::Percentage(fraction) => serialize::percentage(*fraction),
        LengthPercentage::Calc(node) => serialize::calc(node),
    }
}

fn size_css(size: &Size<LengthPercentage>) -> String {
    match size {
        Size::Auto => "auto".into(),
        Size::None => "none".into(),
        Size::MinContent => "min-content".into(),
        Size::MaxContent => "max-content".into(),
        Size::FitContent(None) => "fit-content".into(),
        Size::FitContent(Some(value)) => format!("fit-content({})", length_percentage_css(value)),
        Size::Stretch => "stretch".into(),
        Size::Value(value) => length_percentage_css(value),
        Size::CalcSize(calc) => {
            let basis = match &calc.basis {
                CalcSizeBasis::Auto => "auto".to_owned(),
                CalcSizeBasis::MinContent => "min-content".to_owned(),
                CalcSizeBasis::MaxContent => "max-content".to_owned(),
                CalcSizeBasis::FitContent => "fit-content".to_owned(),
                CalcSizeBasis::Any => "any".to_owned(),
                CalcSizeBasis::Value(value) => length_percentage_css(value),
            };
            format!(
                "calc-size({basis}, {})",
                serialize::calc_expression(&calc.expression)
            )
        }
    }
}

/// The shortest keywords for a display value.
fn display_css(display: Display) -> String {
    use DisplayInside as I;
    use DisplayInternal as T;
    use DisplayOutside as O;
    match display {
        Display::None => "none".into(),
        Display::Contents => "contents".into(),
        Display::Internal(internal) => match internal {
            T::TableRowGroup => "table-row-group",
            T::TableHeaderGroup => "table-header-group",
            T::TableFooterGroup => "table-footer-group",
            T::TableRow => "table-row",
            T::TableCell => "table-cell",
            T::TableColumnGroup => "table-column-group",
            T::TableColumn => "table-column",
            T::TableCaption => "table-caption",
            T::RubyBase => "ruby-base",
            T::RubyText => "ruby-text",
            T::RubyBaseContainer => "ruby-base-container",
            T::RubyTextContainer => "ruby-text-container",
        }
        .into(),
        Display::Normal {
            outside,
            inside,
            list_item,
        } => {
            let short = match (outside, inside, list_item) {
                (O::Block, I::Flow, false) => Some("block"),
                (O::Inline, I::Flow, false) => Some("inline"),
                (O::RunIn, I::Flow, false) => Some("run-in"),
                (O::Block, I::Flow, true) => Some("list-item"),
                (O::Block, I::FlowRoot, false) => Some("flow-root"),
                (O::Inline, I::FlowRoot, false) => Some("inline-block"),
                (O::Block, I::Table, false) => Some("table"),
                (O::Inline, I::Table, false) => Some("inline-table"),
                (O::Block, I::Flex, false) => Some("flex"),
                (O::Inline, I::Flex, false) => Some("inline-flex"),
                (O::Block, I::Grid, false) => Some("grid"),
                (O::Inline, I::Grid, false) => Some("inline-grid"),
                (O::Inline, I::Ruby, false) => Some("ruby"),
                (O::Block, I::Math, false) => Some("block math"),
                (O::Inline, I::Math, false) => Some("math"),
                _ => None,
            };
            if let Some(short) = short {
                return short.into();
            }
            let outside = match outside {
                O::Block => "block",
                O::Inline => "inline",
                O::RunIn => "run-in",
            };
            let inside = match inside {
                I::Flow => "flow",
                I::FlowRoot => "flow-root",
                I::Table => "table",
                I::Flex => "flex",
                I::Grid => "grid",
                I::Ruby => "ruby",
                I::Math => "math",
            };
            if list_item {
                format!("{outside} {inside} list-item")
            } else {
                format!("{outside} {inside}")
            }
        }
    }
}

impl GenericFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            GenericFamily::Serif => "serif",
            GenericFamily::SansSerif => "sans-serif",
            GenericFamily::Monospace => "monospace",
            GenericFamily::Cursive => "cursive",
            GenericFamily::Fantasy => "fantasy",
            GenericFamily::SystemUi => "system-ui",
            GenericFamily::UiSerif => "ui-serif",
            GenericFamily::UiSansSerif => "ui-sans-serif",
            GenericFamily::UiMonospace => "ui-monospace",
            GenericFamily::UiRounded => "ui-rounded",
            GenericFamily::Math => "math",
            GenericFamily::Emoji => "emoji",
            GenericFamily::Fangsong => "fangsong",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Layered lists

/// A comma-separated list of layer values.
pub(crate) fn layers<T>(
    input: &mut Parser<'_>,
    parse: impl Fn(&mut Parser<'_>) -> Result<T>,
) -> Result<Arc<[T]>> {
    Ok(input.parse_comma_separated(|input| parse(input))?.into())
}

pub(crate) fn layers_css<T>(list: &[T], css: impl Fn(&T) -> String) -> String {
    list.iter().map(css).collect::<Vec<_>>().join(", ")
}

/// A list property whose items are keywords.
macro_rules! keyword_layers {
    ($(#[$meta:meta])* $name:ident: $item:ty) => {
        $(#[$meta])*
        pub struct $name;

        impl Property for $name {
            type Specified = Arc<[$item]>;
            type Computed = Arc<[$item]>;

            fn to_css(computed: &Self::Computed) -> String {
                $crate::properties::types::layers_css(computed, |item| item.as_str().into())
            }

            fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
                $crate::properties::types::layers(input, <$item as Property>::parse)
            }

            fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
                specified.clone()
            }
        }
    };
}

/// A comma-separated list of values of `P`, computed item by item, such as the
/// `animation-*` properties take.
pub struct List<P>(std::marker::PhantomData<P>);

impl<P: Property> Property for List<P> {
    type Specified = Arc<[P::Specified]>;
    type Computed = Arc<[P::Computed]>;

    fn to_css(computed: &Self::Computed) -> String {
        layers_css(computed, P::to_css)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        layers(input, P::parse)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified
            .iter()
            .map(|value| P::compute(value, ctx))
            .collect()
    }
}

/// Consumes the identifier `keyword` if it comes next.
pub(crate) fn next_is(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

/// A `<custom-ident>`: an identifier that is not a CSS-wide keyword, `default`, or one of
/// `excluded` (lowercase), which the property reserves.
pub(crate) fn parse_custom_ident(input: &mut Parser<'_>, excluded: &[&str]) -> Result<Arc<str>> {
    let ident = input.expect_ident_cloned()?;
    let lower = ident.to_ascii_lowercase();
    let reserved = matches!(
        &*lower,
        "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default"
    );
    if reserved || excluded.contains(&&*lower) {
        return fail();
    }
    Ok(Arc::from(&*ident))
}

/// Names separated by spaces, or `none` for no names.
pub(crate) fn names_or_none(names: &[Arc<str>], separator: &str) -> String {
    if names.is_empty() {
        "none".into()
    } else {
        names
            .iter()
            .map(|n| &**n)
            .collect::<Vec<_>>()
            .join(separator)
    }
}

mod align;
mod anchor;
mod animation;
mod backgrounds;
mod borders;
mod color_adjust;
mod containment;
mod effects;
mod flex;
mod fonts;
mod fragmentation;
mod generated;
mod gradients;
mod grid;
mod images;
mod interaction;
mod masking;
mod motion;
mod multicol;
mod position;
mod replaced;
mod ruby;
mod shapes;
mod sizing;
mod tables;
mod text;
mod transforms;
mod ui;
pub use align::*;
pub use anchor::*;
pub use animation::*;
pub use backgrounds::*;
pub use borders::*;
pub use color_adjust::*;
pub use containment::*;
pub use effects::*;
pub use flex::*;
pub use fonts::*;
pub use fragmentation::*;
pub use generated::*;
pub use gradients::*;
pub use grid::*;
pub use images::*;
pub use interaction::*;
pub use masking::*;
pub use motion::*;
pub use multicol::*;
pub use position::*;
pub use replaced::*;
pub use ruby::*;
pub use shapes::*;
pub use sizing::*;
pub use tables::*;
pub use text::*;
pub use transforms::*;
pub use ui::*;
