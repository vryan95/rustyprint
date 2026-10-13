//! Shorthand properties and their expansion into longhands.

use cssparser::{ParseError, Parser};

use super::types::*;
use super::{LonghandId, Property, SpecifiedValue};
use crate::values::{AbsoluteColor, Color, Length};

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

macro_rules! shorthands {
    ($($id:ident $name:literal;)+) => {
        /// A shorthand property.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ShorthandId {
            $($id),+
        }

        impl ShorthandId {
            pub fn name(self) -> &'static str {
                match self {
                    $(ShorthandId::$id => $name),+
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(ShorthandId::$id),)+
                    _ => None,
                }
            }
        }
    };
}

shorthands! {
    All "all";
    Margin "margin";
    MarginBlock "margin-block";
    MarginInline "margin-inline";
    Inset "inset";
    InsetBlock "inset-block";
    InsetInline "inset-inline";
    PositionTry "position-try";
    Padding "padding";
    PaddingBlock "padding-block";
    PaddingInline "padding-inline";
    BorderWidth "border-width";
    BorderStyle "border-style";
    BorderColor "border-color";
    BorderTop "border-top";
    BorderRight "border-right";
    BorderBottom "border-bottom";
    BorderLeft "border-left";
    Border "border";
    BorderBlock "border-block";
    BorderInline "border-inline";
    BorderBlockStart "border-block-start";
    BorderBlockEnd "border-block-end";
    BorderInlineStart "border-inline-start";
    BorderInlineEnd "border-inline-end";
    BorderBlockWidth "border-block-width";
    BorderBlockStyle "border-block-style";
    BorderBlockColor "border-block-color";
    BorderInlineWidth "border-inline-width";
    BorderInlineStyle "border-inline-style";
    BorderInlineColor "border-inline-color";
    BorderRadius "border-radius";
    ContainIntrinsicSize "contain-intrinsic-size";
    PageBreakBefore "page-break-before";
    PageBreakAfter "page-break-after";
    PageBreakInside "page-break-inside";
    Outline "outline";
    Overflow "overflow";
    Font "font";
    FontVariant "font-variant";
    FontSynthesis "font-synthesis";
    Background "background";
    BackgroundPosition "background-position";
    VerticalAlign "vertical-align";
    TextBox "text-box";
    WhiteSpace "white-space";
    TextWrap "text-wrap";
    TextDecoration "text-decoration";
    TextEmphasis "text-emphasis";
    ListStyle "list-style";
    Gap "gap";
    PlaceContent "place-content";
    PlaceItems "place-items";
    PlaceSelf "place-self";
    Flex "flex";
    FlexFlow "flex-flow";
    GridRow "grid-row";
    GridColumn "grid-column";
    GridArea "grid-area";
    GridTemplate "grid-template";
    Grid "grid";
    Columns "columns";
    ColumnRule "column-rule";
    BorderImage "border-image";
    CornerShape "corner-shape";
    CornerTopShape "corner-top-shape";
    CornerRightShape "corner-right-shape";
    CornerBottomShape "corner-bottom-shape";
    CornerLeftShape "corner-left-shape";
    CornerBlockStartShape "corner-block-start-shape";
    CornerBlockEndShape "corner-block-end-shape";
    CornerInlineStartShape "corner-inline-start-shape";
    CornerInlineEndShape "corner-inline-end-shape";
    Mask "mask";
    MaskBorder "mask-border";
    Offset "offset";
    Container "container";
    Caret "caret";
    InterestDelay "interest-delay";
    OverscrollBehavior "overscroll-behavior";
    ScrollMargin "scroll-margin";
    ScrollMarginBlock "scroll-margin-block";
    ScrollMarginInline "scroll-margin-inline";
    ScrollPadding "scroll-padding";
    ScrollPaddingBlock "scroll-padding-block";
    ScrollPaddingInline "scroll-padding-inline";
    Animation "animation";
    AnimationRange "animation-range";
    Transition "transition";
    ScrollTimeline "scroll-timeline";
    ViewTimeline "view-timeline";
    TimelineTrigger "timeline-trigger";
    TimelineTriggerActivationRange "timeline-trigger-activation-range";
    TimelineTriggerActiveRange "timeline-trigger-active-range";
}

use LonghandId as L;

const MARGIN: [LonghandId; 4] = [L::MarginTop, L::MarginRight, L::MarginBottom, L::MarginLeft];
const INSET: [LonghandId; 4] = [L::Top, L::Right, L::Bottom, L::Left];
const PADDING: [LonghandId; 4] = [
    L::PaddingTop,
    L::PaddingRight,
    L::PaddingBottom,
    L::PaddingLeft,
];
const BORDER_WIDTH: [LonghandId; 4] = [
    L::BorderTopWidth,
    L::BorderRightWidth,
    L::BorderBottomWidth,
    L::BorderLeftWidth,
];
const BORDER_STYLE: [LonghandId; 4] = [
    L::BorderTopStyle,
    L::BorderRightStyle,
    L::BorderBottomStyle,
    L::BorderLeftStyle,
];
const BORDER_COLOR: [LonghandId; 4] = [
    L::BorderTopColor,
    L::BorderRightColor,
    L::BorderBottomColor,
    L::BorderLeftColor,
];
/// Width, style and color of each logical side.
const BLOCK_START: [LonghandId; 3] = [
    L::BorderBlockStartWidth,
    L::BorderBlockStartStyle,
    L::BorderBlockStartColor,
];
const BLOCK_END: [LonghandId; 3] = [
    L::BorderBlockEndWidth,
    L::BorderBlockEndStyle,
    L::BorderBlockEndColor,
];
const INLINE_START: [LonghandId; 3] = [
    L::BorderInlineStartWidth,
    L::BorderInlineStartStyle,
    L::BorderInlineStartColor,
];
const INLINE_END: [LonghandId; 3] = [
    L::BorderInlineEndWidth,
    L::BorderInlineEndStyle,
    L::BorderInlineEndColor,
];

const BORDER_RADIUS: [LonghandId; 4] = [
    L::BorderTopLeftRadius,
    L::BorderTopRightRadius,
    L::BorderBottomRightRadius,
    L::BorderBottomLeftRadius,
];

const BACKGROUND: [LonghandId; 9] = [
    L::BackgroundImage,
    L::BackgroundPositionX,
    L::BackgroundPositionY,
    L::BackgroundSize,
    L::BackgroundRepeat,
    L::BackgroundAttachment,
    L::BackgroundOrigin,
    L::BackgroundClip,
    L::BackgroundColor,
];

const BORDER_IMAGE: [LonghandId; 5] = [
    L::BorderImageSource,
    L::BorderImageSlice,
    L::BorderImageWidth,
    L::BorderImageOutset,
    L::BorderImageRepeat,
];

const CORNER_SHAPE: [LonghandId; 4] = [
    L::CornerTopLeftShape,
    L::CornerTopRightShape,
    L::CornerBottomRightShape,
    L::CornerBottomLeftShape,
];

const MASK: [LonghandId; 8] = [
    L::MaskImage,
    L::MaskPosition,
    L::MaskSize,
    L::MaskRepeat,
    L::MaskOrigin,
    L::MaskClip,
    L::MaskComposite,
    L::MaskMode,
];

const MASK_BORDER: [LonghandId; 6] = [
    L::MaskBorderSource,
    L::MaskBorderSlice,
    L::MaskBorderWidth,
    L::MaskBorderOutset,
    L::MaskBorderRepeat,
    L::MaskBorderMode,
];

const OFFSET: [LonghandId; 5] = [
    L::OffsetPosition,
    L::OffsetPath,
    L::OffsetDistance,
    L::OffsetRotate,
    L::OffsetAnchor,
];

const SCROLL_MARGIN: [LonghandId; 4] = [
    L::ScrollMarginTop,
    L::ScrollMarginRight,
    L::ScrollMarginBottom,
    L::ScrollMarginLeft,
];

const SCROLL_PADDING: [LonghandId; 4] = [
    L::ScrollPaddingTop,
    L::ScrollPaddingRight,
    L::ScrollPaddingBottom,
    L::ScrollPaddingLeft,
];

/// What the `animation` shorthand sets, including the longhands it only resets.
const ANIMATION: [LonghandId; 13] = [
    L::AnimationName,
    L::AnimationDuration,
    L::AnimationTimingFunction,
    L::AnimationDelay,
    L::AnimationIterationCount,
    L::AnimationDirection,
    L::AnimationFillMode,
    L::AnimationPlayState,
    L::AnimationTimeline,
    L::AnimationComposition,
    L::AnimationRangeStart,
    L::AnimationRangeEnd,
    L::AnimationTrigger,
];

const TRANSITION: [LonghandId; 5] = [
    L::TransitionProperty,
    L::TransitionDuration,
    L::TransitionTimingFunction,
    L::TransitionDelay,
    L::TransitionBehavior,
];

const TIMELINE_TRIGGER: [LonghandId; 6] = [
    L::TimelineTriggerName,
    L::TimelineTriggerSource,
    L::TimelineTriggerActivationRangeStart,
    L::TimelineTriggerActivationRangeEnd,
    L::TimelineTriggerActiveRangeStart,
    L::TimelineTriggerActiveRangeEnd,
];

const FONT: [LonghandId; 19] = [
    L::FontStyle,
    L::FontVariantCaps,
    L::FontWeight,
    L::FontWidth,
    L::FontSize,
    L::LineHeight,
    L::FontFamily,
    // Reset to their initial values.
    L::FontVariantLigatures,
    L::FontVariantNumeric,
    L::FontVariantEastAsian,
    L::FontVariantPosition,
    L::FontFeatureSettings,
    L::FontKerning,
    L::FontLanguageOverride,
    L::FontSizeAdjust,
    L::FontVariantAlternates,
    L::FontVariantEmoji,
    L::FontOpticalSizing,
    L::FontVariationSettings,
];

const FONT_VARIANT: [LonghandId; 7] = [
    L::FontVariantLigatures,
    L::FontVariantCaps,
    L::FontVariantNumeric,
    L::FontVariantEastAsian,
    L::FontVariantPosition,
    L::FontVariantAlternates,
    L::FontVariantEmoji,
];

const FONT_SYNTHESIS: [LonghandId; 4] = [
    L::FontSynthesisWeight,
    L::FontSynthesisStyle,
    L::FontSynthesisSmallCaps,
    L::FontSynthesisPosition,
];

const TEXT_DECORATION: [LonghandId; 4] = [
    L::TextDecorationLine,
    L::TextDecorationStyle,
    L::TextDecorationColor,
    L::TextDecorationThickness,
];

impl ShorthandId {
    /// The longhands this shorthand sets.
    pub fn longhands(self) -> Vec<LonghandId> {
        match self {
            // `all` excludes `direction` and `unicode-bidi`.
            ShorthandId::All => LonghandId::ALL
                .iter()
                .copied()
                .filter(|id| !id.is_logical() && !matches!(id, L::Direction | L::UnicodeBidi))
                .collect(),
            ShorthandId::Margin => MARGIN.to_vec(),
            ShorthandId::BorderImage => BORDER_IMAGE.to_vec(),
            ShorthandId::CornerShape => CORNER_SHAPE.to_vec(),
            ShorthandId::CornerTopShape => vec![L::CornerTopLeftShape, L::CornerTopRightShape],
            ShorthandId::CornerRightShape => {
                vec![L::CornerTopRightShape, L::CornerBottomRightShape]
            }
            ShorthandId::CornerBottomShape => {
                vec![L::CornerBottomLeftShape, L::CornerBottomRightShape]
            }
            ShorthandId::CornerLeftShape => vec![L::CornerTopLeftShape, L::CornerBottomLeftShape],
            ShorthandId::CornerBlockStartShape => {
                vec![L::CornerStartStartShape, L::CornerStartEndShape]
            }
            ShorthandId::CornerBlockEndShape => vec![L::CornerEndStartShape, L::CornerEndEndShape],
            ShorthandId::CornerInlineStartShape => {
                vec![L::CornerStartStartShape, L::CornerEndStartShape]
            }
            ShorthandId::CornerInlineEndShape => vec![L::CornerStartEndShape, L::CornerEndEndShape],
            ShorthandId::Mask => [&MASK[..], &MASK_BORDER[..]].concat(),
            ShorthandId::MaskBorder => MASK_BORDER.to_vec(),
            ShorthandId::Offset => OFFSET.to_vec(),
            ShorthandId::Container => vec![L::ContainerName, L::ContainerType],
            ShorthandId::Caret => vec![L::CaretColor, L::CaretAnimation, L::CaretShape],
            ShorthandId::InterestDelay => vec![L::InterestDelayStart, L::InterestDelayEnd],
            ShorthandId::OverscrollBehavior => {
                vec![L::OverscrollBehaviorX, L::OverscrollBehaviorY]
            }
            ShorthandId::ScrollMargin => SCROLL_MARGIN.to_vec(),
            ShorthandId::ScrollMarginBlock => {
                vec![L::ScrollMarginBlockStart, L::ScrollMarginBlockEnd]
            }
            ShorthandId::ScrollMarginInline => {
                vec![L::ScrollMarginInlineStart, L::ScrollMarginInlineEnd]
            }
            ShorthandId::ScrollPadding => SCROLL_PADDING.to_vec(),
            ShorthandId::ScrollPaddingBlock => {
                vec![L::ScrollPaddingBlockStart, L::ScrollPaddingBlockEnd]
            }
            ShorthandId::ScrollPaddingInline => {
                vec![L::ScrollPaddingInlineStart, L::ScrollPaddingInlineEnd]
            }
            ShorthandId::Animation => ANIMATION.to_vec(),
            ShorthandId::AnimationRange => vec![L::AnimationRangeStart, L::AnimationRangeEnd],
            ShorthandId::Transition => TRANSITION.to_vec(),
            ShorthandId::ScrollTimeline => vec![L::ScrollTimelineName, L::ScrollTimelineAxis],
            ShorthandId::ViewTimeline => vec![
                L::ViewTimelineName,
                L::ViewTimelineAxis,
                L::ViewTimelineInset,
            ],
            ShorthandId::TimelineTrigger => TIMELINE_TRIGGER.to_vec(),
            ShorthandId::TimelineTriggerActivationRange => vec![
                L::TimelineTriggerActivationRangeStart,
                L::TimelineTriggerActivationRangeEnd,
            ],
            ShorthandId::TimelineTriggerActiveRange => vec![
                L::TimelineTriggerActiveRangeStart,
                L::TimelineTriggerActiveRangeEnd,
            ],
            ShorthandId::MarginBlock => vec![L::MarginBlockStart, L::MarginBlockEnd],
            ShorthandId::MarginInline => vec![L::MarginInlineStart, L::MarginInlineEnd],
            ShorthandId::Inset => INSET.to_vec(),
            ShorthandId::InsetBlock => vec![L::InsetBlockStart, L::InsetBlockEnd],
            ShorthandId::InsetInline => vec![L::InsetInlineStart, L::InsetInlineEnd],
            ShorthandId::PositionTry => vec![L::PositionTryOrder, L::PositionTryFallbacks],
            ShorthandId::Padding => PADDING.to_vec(),
            ShorthandId::PaddingBlock => vec![L::PaddingBlockStart, L::PaddingBlockEnd],
            ShorthandId::PaddingInline => vec![L::PaddingInlineStart, L::PaddingInlineEnd],
            ShorthandId::BorderWidth => BORDER_WIDTH.to_vec(),
            ShorthandId::BorderStyle => BORDER_STYLE.to_vec(),
            ShorthandId::BorderColor => BORDER_COLOR.to_vec(),
            ShorthandId::BorderTop => vec![L::BorderTopWidth, L::BorderTopStyle, L::BorderTopColor],
            ShorthandId::BorderRight => vec![
                L::BorderRightWidth,
                L::BorderRightStyle,
                L::BorderRightColor,
            ],
            ShorthandId::BorderBottom => vec![
                L::BorderBottomWidth,
                L::BorderBottomStyle,
                L::BorderBottomColor,
            ],
            ShorthandId::BorderLeft => {
                vec![L::BorderLeftWidth, L::BorderLeftStyle, L::BorderLeftColor]
            }
            // `border` resets the border image too.
            ShorthandId::Border => BORDER_WIDTH
                .into_iter()
                .chain(BORDER_STYLE)
                .chain(BORDER_COLOR)
                .chain(BORDER_IMAGE)
                .collect(),
            ShorthandId::BorderBlock => [BLOCK_START, BLOCK_END].concat(),
            ShorthandId::BorderInline => [INLINE_START, INLINE_END].concat(),
            ShorthandId::BorderBlockStart => BLOCK_START.to_vec(),
            ShorthandId::BorderBlockEnd => BLOCK_END.to_vec(),
            ShorthandId::BorderInlineStart => INLINE_START.to_vec(),
            ShorthandId::BorderInlineEnd => INLINE_END.to_vec(),
            ShorthandId::BorderBlockWidth => vec![BLOCK_START[0], BLOCK_END[0]],
            ShorthandId::BorderBlockStyle => vec![BLOCK_START[1], BLOCK_END[1]],
            ShorthandId::BorderBlockColor => vec![BLOCK_START[2], BLOCK_END[2]],
            ShorthandId::BorderInlineWidth => vec![INLINE_START[0], INLINE_END[0]],
            ShorthandId::BorderInlineStyle => vec![INLINE_START[1], INLINE_END[1]],
            ShorthandId::BorderInlineColor => vec![INLINE_START[2], INLINE_END[2]],
            ShorthandId::BorderRadius => BORDER_RADIUS.to_vec(),
            ShorthandId::ContainIntrinsicSize => {
                vec![L::ContainIntrinsicWidth, L::ContainIntrinsicHeight]
            }
            ShorthandId::PageBreakBefore => vec![L::BreakBefore],
            ShorthandId::PageBreakAfter => vec![L::BreakAfter],
            ShorthandId::PageBreakInside => vec![L::BreakInside],
            ShorthandId::Outline => vec![L::OutlineWidth, L::OutlineStyle, L::OutlineColor],
            ShorthandId::Overflow => vec![L::OverflowX, L::OverflowY],
            ShorthandId::Font => FONT.to_vec(),
            ShorthandId::FontVariant => FONT_VARIANT.to_vec(),
            ShorthandId::FontSynthesis => FONT_SYNTHESIS.to_vec(),
            ShorthandId::Background => BACKGROUND.to_vec(),
            ShorthandId::BackgroundPosition => {
                vec![L::BackgroundPositionX, L::BackgroundPositionY]
            }
            ShorthandId::VerticalAlign => {
                vec![L::BaselineSource, L::AlignmentBaseline, L::BaselineShift]
            }
            ShorthandId::TextBox => vec![L::TextBoxTrim, L::TextBoxEdge],
            ShorthandId::WhiteSpace => vec![L::WhiteSpaceCollapse, L::TextWrapMode],
            ShorthandId::TextWrap => vec![L::TextWrapMode, L::TextWrapStyle],
            ShorthandId::TextDecoration => TEXT_DECORATION.to_vec(),
            ShorthandId::TextEmphasis => vec![L::TextEmphasisStyle, L::TextEmphasisColor],
            ShorthandId::ListStyle => {
                vec![L::ListStylePosition, L::ListStyleImage, L::ListStyleType]
            }
            ShorthandId::Gap => vec![L::RowGap, L::ColumnGap],
            ShorthandId::PlaceContent => vec![L::AlignContent, L::JustifyContent],
            ShorthandId::PlaceItems => vec![L::AlignItems, L::JustifyItems],
            ShorthandId::PlaceSelf => vec![L::AlignSelf, L::JustifySelf],
            ShorthandId::Flex => vec![L::FlexGrow, L::FlexShrink, L::FlexBasis],
            ShorthandId::FlexFlow => vec![L::FlexDirection, L::FlexWrap],
            ShorthandId::Columns => vec![L::ColumnWidth, L::ColumnCount, L::ColumnHeight],
            ShorthandId::ColumnRule => {
                vec![L::ColumnRuleWidth, L::ColumnRuleStyle, L::ColumnRuleColor]
            }
            ShorthandId::GridRow => vec![L::GridRowStart, L::GridRowEnd],
            ShorthandId::GridColumn => vec![L::GridColumnStart, L::GridColumnEnd],
            ShorthandId::GridArea => vec![
                L::GridRowStart,
                L::GridColumnStart,
                L::GridRowEnd,
                L::GridColumnEnd,
            ],
            ShorthandId::GridTemplate => vec![
                L::GridTemplateRows,
                L::GridTemplateColumns,
                L::GridTemplateAreas,
            ],
            ShorthandId::Grid => vec![
                L::GridTemplateRows,
                L::GridTemplateColumns,
                L::GridTemplateAreas,
                L::GridAutoRows,
                L::GridAutoColumns,
                L::GridAutoFlow,
            ],
        }
    }

    /// Parses a value into longhand values.
    pub fn parse(self, input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
        match self {
            // `all` takes only CSS-wide keywords, which the declaration parser handles.
            ShorthandId::All => fail(),
            ShorthandId::BorderImage => parse_border_image(input, false),
            ShorthandId::MaskBorder => parse_border_image(input, true),
            ShorthandId::Mask => parse_mask(input),
            ShorthandId::Offset => parse_offset(input),
            ShorthandId::Container => {
                let name = ContainerName::parse(input)?;
                let kind = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
                    ContainerType::parse(input)?
                } else {
                    ContainerType::NORMAL
                };
                Ok(vec![
                    SpecifiedValue::ContainerName(name),
                    SpecifiedValue::ContainerType(kind),
                ])
            }
            ShorthandId::Caret => parse_caret(input),
            ShorthandId::InterestDelay => {
                let start = InterestDelay::parse(input)?;
                let end = input.try_parse(InterestDelay::parse).unwrap_or(start);
                Ok(vec![
                    SpecifiedValue::InterestDelayStart(start),
                    SpecifiedValue::InterestDelayEnd(end),
                ])
            }
            ShorthandId::OverscrollBehavior => {
                let x = OverscrollBehavior::parse(input)?;
                let y = input.try_parse(OverscrollBehavior::parse).unwrap_or(x);
                Ok(vec![
                    SpecifiedValue::OverscrollBehaviorX(x),
                    SpecifiedValue::OverscrollBehaviorY(y),
                ])
            }
            ShorthandId::ScrollMargin => {
                four_sides::<LengthValue>(input, SCROLL_MARGIN, wrap_scroll_margin)
            }
            ShorthandId::ScrollMarginBlock => two_values::<LengthValue>(
                input,
                [L::ScrollMarginBlockStart, L::ScrollMarginBlockEnd],
                wrap_scroll_margin,
            ),
            ShorthandId::ScrollMarginInline => two_values::<LengthValue>(
                input,
                [L::ScrollMarginInlineStart, L::ScrollMarginInlineEnd],
                wrap_scroll_margin,
            ),
            ShorthandId::ScrollPadding => {
                four_sides::<ScrollPadding>(input, SCROLL_PADDING, wrap_scroll_padding)
            }
            ShorthandId::ScrollPaddingBlock => two_values::<ScrollPadding>(
                input,
                [L::ScrollPaddingBlockStart, L::ScrollPaddingBlockEnd],
                wrap_scroll_padding,
            ),
            ShorthandId::ScrollPaddingInline => two_values::<ScrollPadding>(
                input,
                [L::ScrollPaddingInlineStart, L::ScrollPaddingInlineEnd],
                wrap_scroll_padding,
            ),
            ShorthandId::Animation => parse_animation(input),
            ShorthandId::AnimationRange => {
                let (start, end) = parse_ranges(input, false)?;
                Ok(vec![
                    SpecifiedValue::AnimationRangeStart(start.into()),
                    SpecifiedValue::AnimationRangeEnd(end.into()),
                ])
            }
            ShorthandId::Transition => parse_transition(input),
            ShorthandId::ScrollTimeline => {
                let items = input.parse_comma_separated(|input| {
                    let name = TimelineName::parse(input)?;
                    let axis = input
                        .try_parse(TimelineAxis::parse)
                        .unwrap_or(TimelineAxis::Block);
                    Ok((name, axis))
                })?;
                let (names, axes): (Vec<_>, Vec<_>) = items.into_iter().unzip();
                Ok(vec![
                    SpecifiedValue::ScrollTimelineName(names.into()),
                    SpecifiedValue::ScrollTimelineAxis(axes.into()),
                ])
            }
            ShorthandId::ViewTimeline => parse_view_timeline(input),
            ShorthandId::TimelineTrigger => parse_timeline_trigger(input),
            ShorthandId::TimelineTriggerActivationRange => {
                let (start, end) = parse_ranges(input, false)?;
                Ok(vec![
                    SpecifiedValue::TimelineTriggerActivationRangeStart(start.into()),
                    SpecifiedValue::TimelineTriggerActivationRangeEnd(end.into()),
                ])
            }
            ShorthandId::TimelineTriggerActiveRange => {
                let (start, end) = parse_ranges(input, true)?;
                Ok(vec![
                    SpecifiedValue::TimelineTriggerActiveRangeStart(start.into()),
                    SpecifiedValue::TimelineTriggerActiveRangeEnd(end.into()),
                ])
            }
            ShorthandId::CornerShape => {
                four_sides::<CornerShape>(input, CORNER_SHAPE, wrap_corner_shape)
            }
            ShorthandId::CornerTopShape
            | ShorthandId::CornerRightShape
            | ShorthandId::CornerBottomShape
            | ShorthandId::CornerLeftShape
            | ShorthandId::CornerBlockStartShape
            | ShorthandId::CornerBlockEndShape
            | ShorthandId::CornerInlineStartShape
            | ShorthandId::CornerInlineEndShape => {
                let ids = self.longhands();
                two_values::<CornerShape>(input, [ids[0], ids[1]], wrap_corner_shape)
            }
            ShorthandId::Margin => four_sides::<MarginValue>(input, MARGIN, wrap_margin),
            ShorthandId::Inset => four_sides::<MarginValue>(input, INSET, wrap_inset),
            ShorthandId::PositionTry => {
                let order = input
                    .try_parse(PositionTryOrder::parse)
                    .unwrap_or(PositionTryOrder::Normal);
                Ok(vec![
                    SpecifiedValue::PositionTryOrder(order),
                    SpecifiedValue::PositionTryFallbacks(PositionTryFallbacks::parse(input)?),
                ])
            }
            ShorthandId::InsetBlock => {
                two_values::<MarginValue>(input, [L::InsetBlockStart, L::InsetBlockEnd], wrap_inset)
            }
            ShorthandId::InsetInline => two_values::<MarginValue>(
                input,
                [L::InsetInlineStart, L::InsetInlineEnd],
                wrap_inset,
            ),
            ShorthandId::Padding => {
                four_sides::<NonNegativeLengthPercentage>(input, PADDING, wrap_padding)
            }
            ShorthandId::BorderWidth => {
                four_sides::<BorderWidth>(input, BORDER_WIDTH, wrap_border_width)
            }
            ShorthandId::BorderStyle => {
                four_sides::<BorderStyle>(input, BORDER_STYLE, wrap_border_style)
            }
            ShorthandId::BorderColor => {
                four_sides::<ColorValue>(input, BORDER_COLOR, wrap_border_color)
            }
            ShorthandId::MarginBlock => two_values::<MarginValue>(
                input,
                [L::MarginBlockStart, L::MarginBlockEnd],
                wrap_margin,
            ),
            ShorthandId::MarginInline => two_values::<MarginValue>(
                input,
                [L::MarginInlineStart, L::MarginInlineEnd],
                wrap_margin,
            ),
            ShorthandId::PaddingBlock => two_values::<NonNegativeLengthPercentage>(
                input,
                [L::PaddingBlockStart, L::PaddingBlockEnd],
                wrap_padding,
            ),
            ShorthandId::PaddingInline => two_values::<NonNegativeLengthPercentage>(
                input,
                [L::PaddingInlineStart, L::PaddingInlineEnd],
                wrap_padding,
            ),
            ShorthandId::BorderTop => border_side(input, 0),
            ShorthandId::BorderRight => border_side(input, 1),
            ShorthandId::BorderBottom => border_side(input, 2),
            ShorthandId::BorderLeft => border_side(input, 3),
            ShorthandId::Border => {
                let side = border_side(input, 0)?;
                Ok((0..4)
                    .flat_map(|i| {
                        side.iter().map(move |value| match value {
                            SpecifiedValue::BorderTopWidth(w) => {
                                wrap_border_width(BORDER_WIDTH[i], w.clone())
                            }
                            SpecifiedValue::BorderTopStyle(s) => {
                                wrap_border_style(BORDER_STYLE[i], *s)
                            }
                            SpecifiedValue::BorderTopColor(c) => {
                                wrap_border_color(BORDER_COLOR[i], c.clone())
                            }
                            other => other.clone(),
                        })
                    })
                    .chain(border_image_values(None, None, None, None, None))
                    .collect())
            }
            ShorthandId::BorderBlock => logical_border(input, &[BLOCK_START, BLOCK_END]),
            ShorthandId::BorderInline => logical_border(input, &[INLINE_START, INLINE_END]),
            ShorthandId::BorderBlockStart => logical_border(input, &[BLOCK_START]),
            ShorthandId::BorderBlockEnd => logical_border(input, &[BLOCK_END]),
            ShorthandId::BorderInlineStart => logical_border(input, &[INLINE_START]),
            ShorthandId::BorderInlineEnd => logical_border(input, &[INLINE_END]),
            ShorthandId::BorderBlockWidth => {
                two_values::<BorderWidth>(input, [BLOCK_START[0], BLOCK_END[0]], |id, v| {
                    SpecifiedValue::BorderTopWidth(v).into_logical(id)
                })
            }
            ShorthandId::BorderBlockStyle => {
                two_values::<BorderStyle>(input, [BLOCK_START[1], BLOCK_END[1]], |id, v| {
                    SpecifiedValue::BorderTopStyle(v).into_logical(id)
                })
            }
            ShorthandId::BorderBlockColor => {
                two_values::<ColorValue>(input, [BLOCK_START[2], BLOCK_END[2]], |id, v| {
                    SpecifiedValue::BorderTopColor(v).into_logical(id)
                })
            }
            ShorthandId::BorderInlineWidth => {
                two_values::<BorderWidth>(input, [INLINE_START[0], INLINE_END[0]], |id, v| {
                    SpecifiedValue::BorderTopWidth(v).into_logical(id)
                })
            }
            ShorthandId::BorderInlineStyle => {
                two_values::<BorderStyle>(input, [INLINE_START[1], INLINE_END[1]], |id, v| {
                    SpecifiedValue::BorderTopStyle(v).into_logical(id)
                })
            }
            ShorthandId::BorderInlineColor => {
                two_values::<ColorValue>(input, [INLINE_START[2], INLINE_END[2]], |id, v| {
                    SpecifiedValue::BorderTopColor(v).into_logical(id)
                })
            }
            ShorthandId::Overflow => {
                two_values::<Overflow>(input, [L::OverflowX, L::OverflowY], |id, v| {
                    if id == L::OverflowX {
                        SpecifiedValue::OverflowX(v)
                    } else {
                        SpecifiedValue::OverflowY(v)
                    }
                })
            }
            ShorthandId::BorderRadius => parse_border_radius(input),
            ShorthandId::PageBreakBefore => {
                Ok(vec![SpecifiedValue::BreakBefore(parse_page_break(input)?)])
            }
            ShorthandId::PageBreakAfter => {
                Ok(vec![SpecifiedValue::BreakAfter(parse_page_break(input)?)])
            }
            ShorthandId::PageBreakInside => Ok(vec![SpecifiedValue::BreakInside(
                parse_page_break_inside(input)?,
            )]),
            ShorthandId::ContainIntrinsicSize => {
                let width = ContainIntrinsic::parse_one(input)?;
                let height = input
                    .try_parse(ContainIntrinsic::parse_one)
                    .unwrap_or_else(|_| width.clone());
                Ok(vec![
                    SpecifiedValue::ContainIntrinsicWidth(width),
                    SpecifiedValue::ContainIntrinsicHeight(height),
                ])
            }
            ShorthandId::Outline => parse_outline(input),
            ShorthandId::Font => parse_font(input),
            ShorthandId::FontVariant => parse_font_variant(input),
            ShorthandId::FontSynthesis => parse_font_synthesis(input),
            ShorthandId::VerticalAlign => parse_vertical_align(input),
            ShorthandId::TextBox => parse_text_box(input),
            ShorthandId::WhiteSpace => parse_white_space(input),
            ShorthandId::TextWrap => {
                let (mode, style) = two_any::<TextWrapMode, TextWrapStyle>(input)?;
                Ok(vec![
                    SpecifiedValue::TextWrapMode(mode.unwrap_or(TextWrapMode::Wrap)),
                    SpecifiedValue::TextWrapStyle(style.unwrap_or(TextWrapStyle::Auto)),
                ])
            }
            ShorthandId::TextDecoration => parse_text_decoration(input),
            ShorthandId::TextEmphasis => {
                let (style, color) = two_any::<TextEmphasisStyle, ColorValue>(input)?;
                Ok(vec![
                    SpecifiedValue::TextEmphasisStyle(style.unwrap_or((None, None, None))),
                    SpecifiedValue::TextEmphasisColor(color.unwrap_or(Color::CurrentColor)),
                ])
            }
            ShorthandId::Background => parse_background(input),
            ShorthandId::ListStyle => parse_list_style(input),
            ShorthandId::BackgroundPosition => {
                let positions = input.parse_comma_separated(parse_bg_position)?;
                let (x, y): (Vec<_>, Vec<_>) = positions.into_iter().unzip();
                Ok(vec![
                    SpecifiedValue::BackgroundPositionX(x.into()),
                    SpecifiedValue::BackgroundPositionY(y.into()),
                ])
            }
            ShorthandId::Gap => {
                let row = Gap::parse(input)?;
                let column = input.try_parse(Gap::parse).unwrap_or_else(|_| row.clone());
                Ok(vec![
                    SpecifiedValue::RowGap(row),
                    SpecifiedValue::ColumnGap(column),
                ])
            }
            ShorthandId::PlaceContent => {
                let align = AlignContent::parse(input)?;
                // A baseline value is not one `justify-content` takes, which gets `start`.
                let justify = match input.try_parse(JustifyContent::parse) {
                    Ok(justify) => justify,
                    Err(_) if matches!(align, ContentAlignment::Baseline { .. }) => {
                        ContentAlignment::Position {
                            position: AlignPosition::Start,
                            overflow: OverflowPosition::Default,
                        }
                    }
                    Err(_) => align,
                };
                Ok(vec![
                    SpecifiedValue::AlignContent(align),
                    SpecifiedValue::JustifyContent(justify),
                ])
            }
            ShorthandId::PlaceItems => {
                let align = AlignItems::parse(input)?;
                let justify = input.try_parse(JustifyItems::parse).unwrap_or(align);
                Ok(vec![
                    SpecifiedValue::AlignItems(align),
                    SpecifiedValue::JustifyItems(justify),
                ])
            }
            ShorthandId::PlaceSelf => {
                let align = AlignSelf::parse(input)?;
                let justify = input.try_parse(JustifySelf::parse).unwrap_or(align);
                Ok(vec![
                    SpecifiedValue::AlignSelf(align),
                    SpecifiedValue::JustifySelf(justify),
                ])
            }
            ShorthandId::Flex => {
                // `none` is `0 0 auto`.
                let (grow, shrink, basis) = if input
                    .try_parse(|input| input.expect_ident_matching("none"))
                    .is_ok()
                {
                    (0.0, 0.0, FlexBasis::Size(Size::Auto))
                } else {
                    parse_flex(input)?
                };
                Ok(vec![
                    SpecifiedValue::FlexGrow(grow),
                    SpecifiedValue::FlexShrink(shrink),
                    SpecifiedValue::FlexBasis(basis),
                ])
            }
            ShorthandId::FlexFlow => parse_flex_flow(input),
            ShorthandId::Columns => {
                let (width, count, height) = parse_columns(input)?;
                Ok(vec![
                    SpecifiedValue::ColumnWidth(width),
                    SpecifiedValue::ColumnCount(count),
                    SpecifiedValue::ColumnHeight(height),
                ])
            }
            ShorthandId::ColumnRule => parse_column_rule(input),
            ShorthandId::GridRow => {
                let (start, end) = parse_grid_line_pair(input)?;
                Ok(vec![
                    SpecifiedValue::GridRowStart(start),
                    SpecifiedValue::GridRowEnd(end),
                ])
            }
            ShorthandId::GridColumn => {
                let (start, end) = parse_grid_line_pair(input)?;
                Ok(vec![
                    SpecifiedValue::GridColumnStart(start),
                    SpecifiedValue::GridColumnEnd(end),
                ])
            }
            ShorthandId::GridArea => parse_grid_area(input),
            ShorthandId::GridTemplate => {
                let (rows, columns, areas) = parse_grid_template(input)?;
                Ok(vec![
                    SpecifiedValue::GridTemplateRows(rows),
                    SpecifiedValue::GridTemplateColumns(columns),
                    SpecifiedValue::GridTemplateAreas(areas),
                ])
            }
            ShorthandId::Grid => parse_grid(input),
        }
    }
}

fn wrap_inset(id: LonghandId, value: MaybeAuto<SpecifiedLengthPercentage>) -> SpecifiedValue {
    match id {
        L::Top => SpecifiedValue::Top(value),
        L::Right => SpecifiedValue::Right(value),
        L::Bottom => SpecifiedValue::Bottom(value),
        L::Left => SpecifiedValue::Left(value),
        L::InsetBlockStart => SpecifiedValue::InsetBlockStart(value),
        L::InsetBlockEnd => SpecifiedValue::InsetBlockEnd(value),
        L::InsetInlineStart => SpecifiedValue::InsetInlineStart(value),
        _ => SpecifiedValue::InsetInlineEnd(value),
    }
}

fn wrap_margin(id: LonghandId, value: MaybeAuto<SpecifiedLengthPercentage>) -> SpecifiedValue {
    match id {
        L::MarginTop => SpecifiedValue::MarginTop(value),
        L::MarginRight => SpecifiedValue::MarginRight(value),
        L::MarginBottom => SpecifiedValue::MarginBottom(value),
        L::MarginLeft => SpecifiedValue::MarginLeft(value),
        L::MarginBlockStart => SpecifiedValue::MarginBlockStart(value),
        L::MarginBlockEnd => SpecifiedValue::MarginBlockEnd(value),
        L::MarginInlineStart => SpecifiedValue::MarginInlineStart(value),
        _ => SpecifiedValue::MarginInlineEnd(value),
    }
}

fn wrap_padding(id: LonghandId, value: SpecifiedLengthPercentage) -> SpecifiedValue {
    match id {
        L::PaddingTop => SpecifiedValue::PaddingTop(value),
        L::PaddingRight => SpecifiedValue::PaddingRight(value),
        L::PaddingBottom => SpecifiedValue::PaddingBottom(value),
        L::PaddingLeft => SpecifiedValue::PaddingLeft(value),
        L::PaddingBlockStart => SpecifiedValue::PaddingBlockStart(value),
        L::PaddingBlockEnd => SpecifiedValue::PaddingBlockEnd(value),
        L::PaddingInlineStart => SpecifiedValue::PaddingInlineStart(value),
        _ => SpecifiedValue::PaddingInlineEnd(value),
    }
}

fn wrap_border_width(id: LonghandId, value: SpecifiedLengthPercentage) -> SpecifiedValue {
    match id {
        L::BorderTopWidth => SpecifiedValue::BorderTopWidth(value),
        L::BorderRightWidth => SpecifiedValue::BorderRightWidth(value),
        L::BorderBottomWidth => SpecifiedValue::BorderBottomWidth(value),
        _ => SpecifiedValue::BorderLeftWidth(value),
    }
}

fn wrap_border_style(id: LonghandId, value: BorderStyle) -> SpecifiedValue {
    match id {
        L::BorderTopStyle => SpecifiedValue::BorderTopStyle(value),
        L::BorderRightStyle => SpecifiedValue::BorderRightStyle(value),
        L::BorderBottomStyle => SpecifiedValue::BorderBottomStyle(value),
        _ => SpecifiedValue::BorderLeftStyle(value),
    }
}

fn wrap_border_color(id: LonghandId, value: Color) -> SpecifiedValue {
    match id {
        L::BorderTopColor => SpecifiedValue::BorderTopColor(value),
        L::BorderRightColor => SpecifiedValue::BorderRightColor(value),
        L::BorderBottomColor => SpecifiedValue::BorderBottomColor(value),
        _ => SpecifiedValue::BorderLeftColor(value),
    }
}

/// One to four values for top, right, bottom and left, as `margin` takes.
fn four_sides<P: Property>(
    input: &mut Parser<'_>,
    ids: [LonghandId; 4],
    wrap: impl Fn(LonghandId, P::Specified) -> SpecifiedValue,
) -> Result<Vec<SpecifiedValue>> {
    let mut values = vec![P::parse(input)?];
    while values.len() < 4 {
        match input.try_parse(P::parse) {
            Ok(value) => values.push(value),
            Err(_) => break,
        }
    }
    let [top, right, bottom, left] = match values.len() {
        1 => [0, 0, 0, 0],
        2 => [0, 1, 0, 1],
        3 => [0, 1, 2, 1],
        _ => [0, 1, 2, 3],
    };
    Ok([top, right, bottom, left]
        .into_iter()
        .zip(ids)
        .map(|(index, id)| wrap(id, values[index].clone()))
        .collect())
}

/// `<line-width> || <line-style> || <color>` applied to logical sides.
fn logical_border(
    input: &mut Parser<'_>,
    sides: &[[LonghandId; 3]],
) -> Result<Vec<SpecifiedValue>> {
    let values = border_side(input, 0)?;
    Ok(sides
        .iter()
        .flat_map(|ids| {
            values
                .iter()
                .zip(ids)
                .map(|(value, id)| value.clone().into_logical(*id))
        })
        .collect())
}

/// One or two values for the start and end sides, as `margin-block` takes.
fn two_values<P: Property>(
    input: &mut Parser<'_>,
    ids: [LonghandId; 2],
    wrap: impl Fn(LonghandId, P::Specified) -> SpecifiedValue,
) -> Result<Vec<SpecifiedValue>> {
    let start = P::parse(input)?;
    let end = input.try_parse(P::parse).unwrap_or_else(|_| start.clone());
    Ok(vec![wrap(ids[0], start), wrap(ids[1], end)])
}

/// `<line-width> || <line-style> || <color>` for one side; omitted parts reset to initial.
fn border_side(input: &mut Parser<'_>, side: usize) -> Result<Vec<SpecifiedValue>> {
    let mut width = None;
    let mut style = None;
    let mut color = None;
    for _ in 0..3 {
        if width.is_none()
            && let Ok(w) = input.try_parse(BorderWidth::parse)
        {
            width = Some(w);
        } else if style.is_none()
            && let Ok(s) = input.try_parse(BorderStyle::parse)
        {
            style = Some(s);
        } else if color.is_none()
            && let Ok(c) = input.try_parse(Color::parse)
        {
            color = Some(c);
        } else {
            break;
        }
    }
    if width.is_none() && style.is_none() && color.is_none() {
        return fail();
    }
    Ok(vec![
        wrap_border_width(
            BORDER_WIDTH[side],
            width.unwrap_or(SpecifiedLengthPercentage::Length(Length::px(3.0))),
        ),
        wrap_border_style(BORDER_STYLE[side], style.unwrap_or(BorderStyle::None)),
        wrap_border_color(BORDER_COLOR[side], color.unwrap_or(Color::CurrentColor)),
    ])
}

/// One to four values for the corners, from top-left clockwise, as `margin` takes for sides.
fn corner_values(input: &mut Parser<'_>) -> Result<[SpecifiedLengthPercentage; 4]> {
    let parse = |input: &mut Parser<'_>| parse_length_percentage(input, true, true);
    let mut values = vec![parse(input)?];
    while values.len() < 4 {
        match input.try_parse(parse) {
            Ok(value) => values.push(value),
            Err(_) => break,
        }
    }
    let pick = match values.len() {
        1 => [0, 0, 0, 0],
        2 => [0, 1, 0, 1],
        3 => [0, 1, 2, 1],
        _ => [0, 1, 2, 3],
    };
    Ok(pick.map(|index| values[index].clone()))
}

/// `border-radius`: `<length-percentage>{1,4} [ / <length-percentage>{1,4} ]?`.
fn parse_border_radius(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let horizontal = corner_values(input)?;
    let vertical = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
        corner_values(input)?
    } else {
        horizontal.clone()
    };
    let [h0, h1, h2, h3] = horizontal;
    let [v0, v1, v2, v3] = vertical;
    Ok(vec![
        SpecifiedValue::BorderTopLeftRadius((h0, v0)),
        SpecifiedValue::BorderTopRightRadius((h1, v1)),
        SpecifiedValue::BorderBottomRightRadius((h2, v2)),
        SpecifiedValue::BorderBottomLeftRadius((h3, v3)),
    ])
}

/// `outline`: `<'outline-width'> || <'outline-style'> || <'outline-color'>`.
fn parse_outline(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let (mut width, mut style, mut color) = (None, None, None);
    for _ in 0..3 {
        if width.is_none()
            && let Ok(value) = input.try_parse(BorderWidth::parse)
        {
            width = Some(value);
        } else if style.is_none()
            && let Ok(value) = input.try_parse(OutlineStyle::parse)
        {
            style = Some(value);
        } else if color.is_none()
            && let Ok(value) = input.try_parse(OutlineColor::parse)
        {
            color = Some(value);
        } else {
            break;
        }
    }
    if width.is_none() && style.is_none() && color.is_none() {
        return fail();
    }
    Ok(vec![
        SpecifiedValue::OutlineWidth(
            width.unwrap_or(SpecifiedLengthPercentage::Length(Length::px(3.0))),
        ),
        SpecifiedValue::OutlineStyle(style.unwrap_or(OutlineStyle::None)),
        SpecifiedValue::OutlineColor(color.unwrap_or(None)),
    ])
}

/// `column-rule`: `<'column-rule-width'> || <'column-rule-style'> || <'column-rule-color'>`.
fn parse_column_rule(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let (mut width, mut style, mut color) = (None, None, None);
    for _ in 0..3 {
        if width.is_none()
            && let Ok(value) = input.try_parse(BorderWidth::parse)
        {
            width = Some(value);
        } else if style.is_none()
            && let Ok(value) = input.try_parse(BorderStyle::parse)
        {
            style = Some(value);
        } else if color.is_none()
            && let Ok(value) = input.try_parse(ColorValue::parse)
        {
            color = Some(value);
        } else {
            break;
        }
    }
    if width.is_none() && style.is_none() && color.is_none() {
        return fail();
    }
    Ok(vec![
        SpecifiedValue::ColumnRuleWidth(
            width.unwrap_or(SpecifiedLengthPercentage::Length(Length::px(3.0))),
        ),
        SpecifiedValue::ColumnRuleStyle(style.unwrap_or(BorderStyle::None)),
        SpecifiedValue::ColumnRuleColor(color.unwrap_or(crate::values::Color::CurrentColor)),
    ])
}

/// One layer of the `background` shorthand.
#[derive(Default)]
struct BackgroundLayer {
    image: Option<ImageValue>,
    position: Option<(SpecifiedPosition, SpecifiedPosition)>,
    size: Option<BackgroundSize<SpecifiedLengthPercentage>>,
    repeat: Option<(Repeat, Repeat)>,
    attachment: Option<Attachment>,
    origin: Option<VisualBox>,
    clip: Option<BackgroundClip>,
    color: Option<Color>,
}

/// `<bg-layer>`: `<bg-image> || <bg-position> [ / <bg-size> ]? || <repeat-style> ||
/// <attachment> || <visual-box> || <bg-clip>`, and `<'background-color'>` in the final layer.
fn parse_background_layer(input: &mut Parser<'_>) -> Result<BackgroundLayer> {
    let mut layer = BackgroundLayer::default();
    let mut boxes = Vec::new();
    let mut any = false;
    loop {
        if layer.image.is_none()
            && let Ok(value) = input.try_parse(ImageValue::parse)
        {
            layer.image = Some(value);
        } else if layer.position.is_none()
            && let Ok(value) = input.try_parse(parse_bg_position)
        {
            layer.position = Some(value);
            if input.try_parse(|input| input.expect_delim('/')).is_ok() {
                layer.size = Some(BackgroundSize::parse_one(input)?);
            }
        } else if layer.repeat.is_none()
            && let Ok(value) = input.try_parse(BackgroundRepeat::parse_one)
        {
            layer.repeat = Some(value);
        } else if layer.attachment.is_none()
            && let Ok(value) = input.try_parse(<Attachment as Property>::parse)
        {
            layer.attachment = Some(value);
        } else if boxes.len() < 2
            && let Ok(value) = input.try_parse(<BackgroundClip as Property>::parse)
        {
            boxes.push(value);
        } else if layer.color.is_none()
            && let Ok(value) = input.try_parse(Color::parse)
        {
            layer.color = Some(value);
        } else {
            break;
        }
        any = true;
    }
    if !any {
        return fail();
    }
    // One `<visual-box>` sets both boxes; with two, the first is the origin.
    let visual_box = |clip: BackgroundClip| VisualBox::from_keyword(clip.as_str());
    match boxes[..] {
        [] => {}
        [clip] => {
            layer.origin = visual_box(clip);
            layer.clip = Some(clip);
        }
        [origin, clip] => {
            layer.origin = Some(visual_box(origin).ok_or_else(|| ParseError::custom(()))?);
            layer.clip = Some(clip);
        }
        _ => unreachable!("at most two boxes"),
    }
    Ok(layer)
}

/// `background`: `<bg-layer>#? , <final-bg-layer>`.
fn parse_background(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let layers = input.parse_comma_separated(parse_background_layer)?;
    // Only the final layer may have a color.
    if layers[..layers.len() - 1]
        .iter()
        .any(|layer| layer.color.is_some())
    {
        return fail();
    }
    let color = layers
        .last()
        .and_then(|layer| layer.color.clone())
        .unwrap_or(Color::Absolute(AbsoluteColor::TRANSPARENT));
    let start = || SpecifiedPosition {
        edge: PositionEdge::Start,
        offset: Some(SpecifiedLengthPercentage::Percentage(0.0)),
    };
    let mut values = Vec::new();
    values.push(SpecifiedValue::BackgroundImage(
        layers
            .iter()
            .map(|l| l.image.clone().unwrap_or(ImageValue::None))
            .collect(),
    ));
    values.push(SpecifiedValue::BackgroundPositionX(
        layers
            .iter()
            .map(|l| l.position.as_ref().map_or_else(start, |p| p.0.clone()))
            .collect(),
    ));
    values.push(SpecifiedValue::BackgroundPositionY(
        layers
            .iter()
            .map(|l| l.position.as_ref().map_or_else(start, |p| p.1.clone()))
            .collect(),
    ));
    values.push(SpecifiedValue::BackgroundSize(
        layers
            .iter()
            .map(|l| {
                l.size
                    .clone()
                    .unwrap_or(BackgroundSize::Explicit(None, None))
            })
            .collect(),
    ));
    values.push(SpecifiedValue::BackgroundRepeat(
        layers
            .iter()
            .map(|l| l.repeat.unwrap_or((Repeat::Repeat, Repeat::Repeat)))
            .collect(),
    ));
    values.push(SpecifiedValue::BackgroundAttachment(
        layers
            .iter()
            .map(|l| l.attachment.unwrap_or(Attachment::Scroll))
            .collect(),
    ));
    values.push(SpecifiedValue::BackgroundOrigin(
        layers
            .iter()
            .map(|l| l.origin.unwrap_or(VisualBox::PaddingBox))
            .collect(),
    ));
    values.push(SpecifiedValue::BackgroundClip(
        layers
            .iter()
            .map(|l| l.clip.unwrap_or(BackgroundClip::BorderBox))
            .collect(),
    ));
    values.push(SpecifiedValue::BackgroundColor(color));
    Ok(values)
}

/// `font`: `[<style> || <variant-css2> || <weight> || <width-css3>]? <size> [/ <line-height>]?
/// <family>#`, or a system font keyword.
fn parse_font(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let system = input.try_parse(|input| {
        let keyword = input.expect_ident_cloned()?.to_ascii_lowercase();
        input.expect_exhausted()?;
        match keyword.as_str() {
            "caption" | "icon" | "menu" | "message-box" | "small-caption" | "status-bar" => Ok(()),
            _ => fail(),
        }
    });
    if system.is_ok() {
        // System fonts resolve to the default font at its default size.
        return Ok(font_values(
            None,
            None,
            None,
            None,
            FontSize::Absolute(1.0),
            None,
            None,
        ));
    }
    let (mut style, mut caps, mut weight, mut width) = (None, None, None, None);
    for _ in 0..4 {
        // `normal` may stand for any of the four.
        if input
            .try_parse(|input| input.expect_ident_matching("normal"))
            .is_ok()
        {
            continue;
        }
        if style.is_none()
            && let Ok(value) = input.try_parse(FontStyle::parse)
        {
            style = Some(value);
        } else if caps.is_none()
            && input
                .try_parse(|input| input.expect_ident_matching("small-caps"))
                .is_ok()
        {
            caps = Some(FontVariantCaps::SmallCaps);
        } else if weight.is_none()
            && let Ok(value) = input.try_parse(FontWeight::parse)
        {
            weight = Some(value);
        } else if width.is_none()
            && let Ok(value) = input.try_parse(|input| {
                // Only keywords: percentages are not allowed in the shorthand.
                let state = input.state();
                input.expect_ident()?;
                input.reset(&state);
                FontWidth::parse(input)
            })
        {
            width = Some(value);
        } else {
            break;
        }
    }
    let size = FontSize::parse(input)?;
    let line_height = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
        Some(LineHeight::parse(input)?)
    } else {
        None
    };
    let family = FontFamily::parse(input)?;
    Ok(font_values(
        style,
        caps,
        weight,
        width,
        size,
        line_height,
        Some(family),
    ))
}

#[allow(clippy::too_many_arguments)]
fn font_values(
    style: Option<FontStyle>,
    caps: Option<FontVariantCaps>,
    weight: Option<FontWeight>,
    width: Option<f32>,
    size: FontSize,
    line_height: Option<SpecifiedLineHeight>,
    family: Option<<FontFamily as Property>::Specified>,
) -> Vec<SpecifiedValue> {
    vec![
        SpecifiedValue::FontStyle(style.unwrap_or(FontStyle::Normal)),
        SpecifiedValue::FontVariantCaps(caps.unwrap_or(FontVariantCaps::Normal)),
        SpecifiedValue::FontWeight(weight.unwrap_or(FontWeight::Absolute(400.0))),
        SpecifiedValue::FontWidth(width.unwrap_or(100.0)),
        SpecifiedValue::FontSize(size),
        SpecifiedValue::LineHeight(line_height.unwrap_or(SpecifiedLineHeight::Normal)),
        SpecifiedValue::FontFamily(
            family.unwrap_or_else(|| {
                std::sync::Arc::from([FamilyName::Generic(GenericFamily::Serif)])
            }),
        ),
        SpecifiedValue::FontVariantLigatures(FontVariantLigatures::NORMAL),
        SpecifiedValue::FontVariantNumeric(FontVariantNumeric::default()),
        SpecifiedValue::FontVariantEastAsian(FontVariantEastAsian::default()),
        SpecifiedValue::FontVariantPosition(FontVariantPosition::Normal),
        SpecifiedValue::FontFeatureSettings(std::sync::Arc::from([])),
        SpecifiedValue::FontKerning(FontKerning::Auto),
        SpecifiedValue::FontLanguageOverride(None),
        SpecifiedValue::FontSizeAdjust(FontSizeAdjust::None),
        SpecifiedValue::FontVariantAlternates(FontVariantAlternates::default()),
        SpecifiedValue::FontVariantEmoji(FontVariantEmoji::Normal),
        SpecifiedValue::FontOpticalSizing(FontOpticalSizing::Auto),
        SpecifiedValue::FontVariationSettings(std::sync::Arc::from([])),
    ]
}

/// Values of two properties, either of which may be missing.
type Pair<A, B> = (
    Option<<A as Property>::Specified>,
    Option<<B as Property>::Specified>,
);

/// `a || b`: each of two property values at most once, in any order, at least one of them.
fn two_any<A: Property, B: Property>(input: &mut Parser<'_>) -> Result<Pair<A, B>> {
    let (mut a, mut b) = (None, None);
    for _ in 0..2 {
        if a.is_none()
            && let Ok(value) = input.try_parse(A::parse)
        {
            a = Some(value);
        } else if b.is_none()
            && let Ok(value) = input.try_parse(B::parse)
        {
            b = Some(value);
        } else {
            break;
        }
    }
    if a.is_none() && b.is_none() {
        return fail();
    }
    Ok((a, b))
}

/// `list-style`: `<position> || <image> || <type>`. A `none` sets whichever of the image
/// and type is not otherwise given, or both.
fn parse_list_style(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let (mut position, mut image, mut kind) = (None, None, None);
    let mut nones = 0usize;
    loop {
        if input
            .try_parse(|input| input.expect_ident_matching("none"))
            .is_ok()
        {
            nones += 1;
        } else if position.is_none()
            && let Ok(value) = input.try_parse(ListStylePosition::parse)
        {
            position = Some(value);
        } else if image.is_none()
            && let Ok(value) = input.try_parse(ListStyleImage::parse)
        {
            image = Some(value);
        } else if kind.is_none()
            && let Ok(value) = input.try_parse(ListStyleType::parse)
        {
            kind = Some(value);
        } else {
            break;
        }
    }
    let unset = usize::from(image.is_none()) + usize::from(kind.is_none());
    if nones > unset || (position.is_none() && image.is_none() && kind.is_none() && nones == 0) {
        return fail();
    }
    if nones > 0 {
        // A single `none` with neither the image nor the type sets both.
        if image.is_none() {
            image = Some(ImageValue::None);
        }
        if kind.is_none() {
            kind = Some(ListStyleType::None);
        }
    }
    Ok(vec![
        SpecifiedValue::ListStylePosition(position.unwrap_or(ListStylePosition::Outside)),
        SpecifiedValue::ListStyleImage(image.unwrap_or(ImageValue::None)),
        SpecifiedValue::ListStyleType(kind.unwrap_or_else(|| {
            ListStyleType::Style(crate::counter_style::CounterStyleRef::Name(
                std::sync::Arc::from("disc"),
            ))
        })),
    ])
}

/// The next identifier, lowercased, if there is one.
fn next_ident(input: &mut Parser<'_>) -> Option<String> {
    input
        .try_parse(|input| input.expect_ident_cloned())
        .ok()
        .map(|ident| ident.to_ascii_lowercase())
}

/// `font-variant`: `normal | none | [<caps> || <ligatures> || <numeric> || <east-asian> ||
/// <position>]`.
fn parse_font_variant(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let mut ligatures = FontVariantLigatures::NORMAL;
    let mut caps = None;
    let mut numeric = FontVariantNumeric::default();
    let mut east_asian = FontVariantEastAsian::default();
    let mut position = None;
    let mut alternates = FontVariantAlternates::default();
    let mut emoji = None;
    let state = input.state();
    match next_ident(input).as_deref() {
        Some("normal") => {}
        Some("none") => ligatures.none = true,
        _ => {
            input.reset(&state);
            let mut count = 0;
            loop {
                if input
                    .try_parse(|input| alternates.parse_item(input))
                    .is_ok()
                {
                    count += 1;
                    continue;
                }
                let state = input.state();
                let Some(word) = next_ident(input) else {
                    break;
                };
                let emoji_value = FontVariantEmoji::from_keyword(&word)
                    .filter(|e| *e != FontVariantEmoji::Normal);
                let caps_value =
                    FontVariantCaps::from_keyword(&word).filter(|c| *c != FontVariantCaps::Normal);
                let taken = if let Some(value) = emoji_value {
                    emoji.replace(value).is_none()
                } else if let Some(value) = caps_value {
                    caps.replace(value).is_none()
                } else if let Some(value) = FontVariantPosition::from_keyword(&word)
                    .filter(|p| *p != FontVariantPosition::Normal)
                {
                    position.replace(value).is_none()
                } else {
                    ligatures.parse_keyword(&word)
                        || numeric.parse_keyword(&word)
                        || east_asian.parse_keyword(&word)
                };
                if !taken {
                    input.reset(&state);
                    break;
                }
                count += 1;
            }
            if count == 0 {
                return fail();
            }
        }
    }
    Ok(vec![
        SpecifiedValue::FontVariantLigatures(ligatures),
        SpecifiedValue::FontVariantCaps(caps.unwrap_or(FontVariantCaps::Normal)),
        SpecifiedValue::FontVariantNumeric(numeric),
        SpecifiedValue::FontVariantEastAsian(east_asian),
        SpecifiedValue::FontVariantPosition(position.unwrap_or(FontVariantPosition::Normal)),
        SpecifiedValue::FontVariantAlternates(alternates),
        SpecifiedValue::FontVariantEmoji(emoji.unwrap_or(FontVariantEmoji::Normal)),
    ])
}

/// `font-synthesis`: `none | [weight || style || small-caps || position]`.
fn parse_font_synthesis(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let mut on = [false; 4];
    if input
        .try_parse(|input| input.expect_ident_matching("none"))
        .is_err()
    {
        let mut count = 0;
        loop {
            let state = input.state();
            let index = match next_ident(input).as_deref() {
                Some("weight") => 0,
                Some("style") => 1,
                Some("small-caps") => 2,
                Some("position") => 3,
                _ => {
                    input.reset(&state);
                    break;
                }
            };
            if on[index] {
                return fail();
            }
            on[index] = true;
            count += 1;
        }
        if count == 0 {
            return fail();
        }
    }
    let pick = |b: bool| if b { Synthesis::Auto } else { Synthesis::None };
    Ok(vec![
        SpecifiedValue::FontSynthesisWeight(pick(on[0])),
        SpecifiedValue::FontSynthesisStyle(if on[1] {
            FontSynthesisStyle::Auto
        } else {
            FontSynthesisStyle::None
        }),
        SpecifiedValue::FontSynthesisSmallCaps(pick(on[2])),
        SpecifiedValue::FontSynthesisPosition(pick(on[3])),
    ])
}

/// `vertical-align`: `[first | last] || <'alignment-baseline'> || <'baseline-shift'>`.
fn parse_vertical_align(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let (mut source, mut align, mut shift) = (None, None, None);
    for _ in 0..3 {
        if source.is_none()
            && let Ok(value) = input.try_parse(|input| match next_ident(input).as_deref() {
                Some("first") => Ok(BaselineSource::First),
                Some("last") => Ok(BaselineSource::Last),
                _ => fail(),
            })
        {
            source = Some(value);
        } else if align.is_none()
            && let Ok(value) = input.try_parse(|input| {
                next_ident(input)
                    .and_then(|word| AlignmentBaseline::parse_any(&word))
                    .ok_or_else(|| ParseError::<()>::custom(()))
            })
        {
            align = Some(value);
        } else if shift.is_none()
            && let Ok(value) = input.try_parse(BaselineShiftValue::parse)
        {
            shift = Some(value);
        } else {
            break;
        }
    }
    if source.is_none() && align.is_none() && shift.is_none() {
        return fail();
    }
    Ok(vec![
        SpecifiedValue::BaselineSource(source.unwrap_or(BaselineSource::Auto)),
        SpecifiedValue::AlignmentBaseline(align.unwrap_or(AlignmentBaseline::Baseline)),
        SpecifiedValue::BaselineShift(shift.unwrap_or(BaselineShift::Value(
            SpecifiedLengthPercentage::Length(Length::px(0.0)),
        ))),
    ])
}

/// `text-box`: `normal | <'text-box-trim'> || <'text-box-edge'>`.
fn parse_text_box(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    if input
        .try_parse(|input| input.expect_ident_matching("normal"))
        .is_ok()
    {
        return Ok(vec![
            SpecifiedValue::TextBoxTrim(TextBoxTrim::None),
            SpecifiedValue::TextBoxEdge(None),
        ]);
    }
    let (trim, edge) = two_any::<TextBoxTrim, TextBoxEdge>(input)?;
    Ok(vec![
        SpecifiedValue::TextBoxTrim(trim.unwrap_or(TextBoxTrim::TrimBoth)),
        SpecifiedValue::TextBoxEdge(edge.unwrap_or(None)),
    ])
}

/// `white-space`: `normal | pre | pre-wrap | pre-line | <'white-space-collapse'> ||
/// <'text-wrap-mode'>`.
fn parse_white_space(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let values = |collapse, mode| {
        vec![
            SpecifiedValue::WhiteSpaceCollapse(collapse),
            SpecifiedValue::TextWrapMode(mode),
        ]
    };
    let state = input.state();
    let (collapse, mode) = match next_ident(input).as_deref() {
        Some("normal") => (WhiteSpaceCollapse::Collapse, TextWrapMode::Wrap),
        Some("pre") => (WhiteSpaceCollapse::Preserve, TextWrapMode::Nowrap),
        Some("pre-wrap") => (WhiteSpaceCollapse::Preserve, TextWrapMode::Wrap),
        Some("pre-line") => (WhiteSpaceCollapse::PreserveBreaks, TextWrapMode::Wrap),
        _ => {
            input.reset(&state);
            let (collapse, mode) = two_any::<WhiteSpaceCollapse, TextWrapMode>(input)?;
            (
                collapse.unwrap_or(WhiteSpaceCollapse::Collapse),
                mode.unwrap_or(TextWrapMode::Wrap),
            )
        }
    };
    Ok(values(collapse, mode))
}

/// `text-decoration`: `<line> || <style> || <color> || <thickness>`.
fn parse_text_decoration(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let (mut line, mut style, mut color, mut thickness) = (None, None, None, None);
    for _ in 0..4 {
        if line.is_none()
            && let Ok(value) = input.try_parse(TextDecorationLine::parse)
        {
            line = Some(value);
        } else if style.is_none()
            && let Ok(value) = input.try_parse(TextDecorationStyle::parse)
        {
            style = Some(value);
        } else if thickness.is_none()
            && let Ok(value) = input.try_parse(TextDecorationThickness::parse)
        {
            thickness = Some(value);
        } else if color.is_none()
            && let Ok(value) = input.try_parse(Color::parse)
        {
            color = Some(value);
        } else {
            break;
        }
    }
    if line.is_none() && style.is_none() && color.is_none() && thickness.is_none() {
        return fail();
    }
    Ok(vec![
        SpecifiedValue::TextDecorationLine(line.unwrap_or(TextDecorationLine::NONE)),
        SpecifiedValue::TextDecorationStyle(style.unwrap_or(TextDecorationStyle::Solid)),
        SpecifiedValue::TextDecorationColor(color.unwrap_or(Color::CurrentColor)),
        SpecifiedValue::TextDecorationThickness(thickness.unwrap_or(DecorationLength::Auto)),
    ])
}

/// `flex-flow`: `<'flex-direction'> || <'flex-wrap'>`, with the keywords of `flex-wrap` in
/// any order around the direction.
fn parse_flex_flow(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let (mut direction, mut wrap, mut balance) = (None, None, false);
    let mut any = false;
    loop {
        if direction.is_none()
            && let Ok(value) = input.try_parse(FlexDirection::parse)
        {
            direction = Some(value);
        } else if input
            .try_parse(|input| FlexWrap::parse_keyword(input, &mut wrap, &mut balance))
            .is_err()
        {
            break;
        }
        any = true;
    }
    if !any {
        return fail();
    }
    Ok(vec![
        SpecifiedValue::FlexDirection(direction.unwrap_or(FlexDirection::Row)),
        SpecifiedValue::FlexWrap(FlexWrap::from_keywords(wrap, balance)?),
    ])
}

/// `grid-row` and `grid-column`: `<grid-line> [ / <grid-line> ]?`. Without an end, a name
/// alone is copied to it, and anything else leaves it `auto`.
fn parse_grid_line_pair(input: &mut Parser<'_>) -> Result<(GridLine, GridLine)> {
    let start = GridLine::parse(input)?;
    let end = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
        GridLine::parse(input)?
    } else if start.is_ident() {
        start.clone()
    } else {
        GridLine::Auto
    };
    Ok((start, end))
}

/// `grid-area`: up to four `<grid-line>`s separated by `/`: row-start, column-start,
/// row-end and column-end, with omitted ones copied from names or `auto`.
fn parse_grid_area(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let mut lines = vec![GridLine::parse(input)?];
    while lines.len() < 4 && input.try_parse(|input| input.expect_delim('/')).is_ok() {
        lines.push(GridLine::parse(input)?);
    }
    let copy = |line: &GridLine| {
        if line.is_ident() {
            line.clone()
        } else {
            GridLine::Auto
        }
    };
    let row_start = lines[0].clone();
    let column_start = lines.get(1).cloned().unwrap_or_else(|| copy(&row_start));
    let row_end = lines.get(2).cloned().unwrap_or_else(|| copy(&row_start));
    let column_end = lines.get(3).cloned().unwrap_or_else(|| copy(&column_start));
    Ok(vec![
        SpecifiedValue::GridRowStart(row_start),
        SpecifiedValue::GridColumnStart(column_start),
        SpecifiedValue::GridRowEnd(row_end),
        SpecifiedValue::GridColumnEnd(column_end),
    ])
}

/// The rows, columns and areas that `grid-template` sets.
type Template = (
    GridTemplate<SpecifiedLengthPercentage>,
    GridTemplate<SpecifiedLengthPercentage>,
    GridAreas,
);

/// `grid-template`: `none | [ <'grid-template-rows'> / <'grid-template-columns'> ] |
/// [ <line-names>? <string> <track-size>? <line-names>? ]+ [ / <explicit-track-list> ]?`.
fn parse_grid_template(input: &mut Parser<'_>) -> Result<Template> {
    if input
        .try_parse(|input| {
            input.expect_ident_matching("none")?;
            input.expect_exhausted()
        })
        .is_ok()
    {
        return Ok((GridTemplate::None, GridTemplate::None, GridAreas::default()));
    }
    if let Ok(template) = input.try_parse(|input| {
        let rows = GridTemplate::<LengthPercentage>::parse(input)?;
        input.expect_delim('/')?;
        let columns = GridTemplate::<LengthPercentage>::parse(input)?;
        Ok::<_, ParseError<()>>((rows, columns, GridAreas::default()))
    }) {
        return Ok(template);
    }
    // Rows of areas, each with its size and the names of the lines around it.
    let mut names: Vec<Vec<std::sync::Arc<str>>> = vec![Vec::new()];
    let mut sizes = Vec::new();
    let mut rows = Vec::new();
    loop {
        let row = input.try_parse(|input| {
            let before = input.try_parse(parse_line_names_list).unwrap_or_default();
            let row = GridAreas::parse_row(input)?;
            Ok::<_, ParseError<()>>((before, row))
        });
        let Ok((before, row)) = row else {
            break;
        };
        names.last_mut().expect("names has a slot").extend(before);
        rows.push(row);
        sizes.push(
            input
                .try_parse(parse_track_size)
                .unwrap_or(TrackSize::Breadth(TrackBreadth::Auto)),
        );
        names.push(input.try_parse(parse_line_names_list).unwrap_or_default());
    }
    if rows.is_empty() {
        return fail();
    }
    let areas = GridAreas::from_rows(rows).ok_or_else(|| ParseError::custom(()))?;
    let columns = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
        GridTemplate::Tracks(parse_track_list(input, false)?)
    } else {
        GridTemplate::None
    };
    let rows = GridTemplate::Tracks(TrackList {
        names,
        entries: sizes.into_iter().map(TrackEntry::Track).collect(),
    });
    Ok((rows, columns, areas))
}

/// `<line-names>`, for `grid-template`'s rows of areas.
fn parse_line_names_list(input: &mut Parser<'_>) -> Result<Vec<std::sync::Arc<str>>> {
    // Names come through a track list of nothing but names, which the track list parser
    // rejects; parse the brackets here instead.
    input.expect_square_bracket_block()?;
    input.parse_nested_block(|input| {
        let mut names = Vec::new();
        while !input.is_exhausted() {
            let ident = input.expect_ident_cloned()?;
            if matches!(&*ident.to_ascii_lowercase(), "span" | "auto" | "default") {
                return fail();
            }
            names.push(std::sync::Arc::from(&*ident));
        }
        Ok(names)
    })
}

fn template_values(
    (rows, columns, areas): Template,
    auto_rows: Vec<TrackSize<SpecifiedLengthPercentage>>,
    auto_columns: Vec<TrackSize<SpecifiedLengthPercentage>>,
    flow: GridAutoFlow,
) -> Vec<SpecifiedValue> {
    vec![
        SpecifiedValue::GridTemplateRows(rows),
        SpecifiedValue::GridTemplateColumns(columns),
        SpecifiedValue::GridTemplateAreas(areas),
        SpecifiedValue::GridAutoRows(auto_rows.into()),
        SpecifiedValue::GridAutoColumns(auto_columns.into()),
        SpecifiedValue::GridAutoFlow(flow),
    ]
}

/// `[ auto-flow && dense? ]`, as `grid` takes it; the axis comes from the side of the `/`.
fn parse_auto_flow(input: &mut Parser<'_>, column: bool) -> Result<GridAutoFlow> {
    let mut auto_flow = false;
    let mut dense = false;
    loop {
        let parsed = input.try_parse(|input| {
            let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
            match ident.as_str() {
                "auto-flow" if !auto_flow => auto_flow = true,
                "dense" if !dense => dense = true,
                _ => return fail(),
            }
            Ok::<_, ParseError<()>>(())
        });
        if parsed.is_err() {
            break;
        }
    }
    if !auto_flow {
        return fail();
    }
    Ok(GridAutoFlow { column, dense })
}

fn parse_auto_tracks(input: &mut Parser<'_>) -> Vec<TrackSize<SpecifiedLengthPercentage>> {
    let mut sizes = Vec::new();
    while let Ok(size) = input.try_parse(parse_track_size) {
        sizes.push(size);
    }
    if sizes.is_empty() {
        sizes.push(TrackSize::Breadth(TrackBreadth::Auto));
    }
    sizes
}

/// `grid`: `<'grid-template'> | <'grid-template-rows'> / [ auto-flow && dense? ]
/// <'grid-auto-columns'>? | [ auto-flow && dense? ] <'grid-auto-rows'>? /
/// <'grid-template-columns'>`. It resets every grid property it does not set.
fn parse_grid(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let auto = || vec![TrackSize::Breadth(TrackBreadth::Auto)];
    if let Ok(template) = input.try_parse(|input| {
        let template = parse_grid_template(input)?;
        input.expect_exhausted()?;
        Ok::<_, ParseError<()>>(template)
    }) {
        return Ok(template_values(
            template,
            auto(),
            auto(),
            GridAutoFlow::default(),
        ));
    }
    if let Ok(values) = input.try_parse(|input| {
        let rows = GridTemplate::<LengthPercentage>::parse(input)?;
        input.expect_delim('/')?;
        let flow = parse_auto_flow(input, true)?;
        let columns = parse_auto_tracks(input);
        Ok::<_, ParseError<()>>(template_values(
            (rows, GridTemplate::None, GridAreas::default()),
            auto(),
            columns,
            flow,
        ))
    }) {
        return Ok(values);
    }
    let flow = parse_auto_flow(input, false)?;
    let rows = parse_auto_tracks(input);
    input.expect_delim('/')?;
    let columns = GridTemplate::<LengthPercentage>::parse(input)?;
    Ok(template_values(
        (GridTemplate::None, columns, GridAreas::default()),
        rows,
        auto(),
        flow,
    ))
}

fn wrap_corner_shape(id: LonghandId, value: CornerShape) -> SpecifiedValue {
    match id {
        L::CornerTopLeftShape => SpecifiedValue::CornerTopLeftShape(value),
        L::CornerTopRightShape => SpecifiedValue::CornerTopRightShape(value),
        L::CornerBottomRightShape => SpecifiedValue::CornerBottomRightShape(value),
        L::CornerBottomLeftShape => SpecifiedValue::CornerBottomLeftShape(value),
        L::CornerStartStartShape => SpecifiedValue::CornerStartStartShape(value),
        L::CornerStartEndShape => SpecifiedValue::CornerStartEndShape(value),
        L::CornerEndStartShape => SpecifiedValue::CornerEndStartShape(value),
        _ => SpecifiedValue::CornerEndEndShape(value),
    }
}

type ImageSides = [ImageSideWidth<SpecifiedLengthPercentage>; 4];

/// The `border-image-*` values, with omitted ones at their initial values.
fn border_image_values(
    source: Option<ImageValue>,
    slice: Option<ImageSlice>,
    width: Option<ImageSides>,
    outset: Option<ImageSides>,
    repeat: Option<(ImageRepeat, ImageRepeat)>,
) -> Vec<SpecifiedValue> {
    vec![
        SpecifiedValue::BorderImageSource(source.unwrap_or(ImageValue::None)),
        SpecifiedValue::BorderImageSlice(slice.unwrap_or(ImageSlice {
            offsets: [NumberOrPercentage::Percentage(1.0); 4],
            fill: false,
        })),
        SpecifiedValue::BorderImageWidth(width.unwrap_or_else(|| ImageSideWidth::numbers(1.0))),
        SpecifiedValue::BorderImageOutset(outset.unwrap_or_else(|| ImageSideWidth::numbers(0.0))),
        SpecifiedValue::BorderImageRepeat(
            repeat.unwrap_or((ImageRepeat::Stretch, ImageRepeat::Stretch)),
        ),
    ]
}

/// The `mask-border-*` values, with omitted ones at their initial values.
fn mask_border_values(
    source: Option<ImageValue>,
    slice: Option<ImageSlice>,
    width: Option<ImageSides>,
    outset: Option<ImageSides>,
    repeat: Option<(ImageRepeat, ImageRepeat)>,
    mode: Option<MaskType>,
) -> Vec<SpecifiedValue> {
    vec![
        SpecifiedValue::MaskBorderSource(source.unwrap_or(ImageValue::None)),
        SpecifiedValue::MaskBorderSlice(slice.unwrap_or(ImageSlice {
            offsets: [NumberOrPercentage::Number(0.0); 4],
            fill: false,
        })),
        SpecifiedValue::MaskBorderWidth(width.unwrap_or_else(ImageSideWidth::auto)),
        SpecifiedValue::MaskBorderOutset(outset.unwrap_or_else(|| ImageSideWidth::numbers(0.0))),
        SpecifiedValue::MaskBorderRepeat(
            repeat.unwrap_or((ImageRepeat::Stretch, ImageRepeat::Stretch)),
        ),
        SpecifiedValue::MaskBorderMode(mode.unwrap_or(MaskType::Alpha)),
    ]
}

/// `border-image`: `<'border-image-source'> || <'border-image-slice'> [/ <'border-image-width'>
/// | / <'border-image-width'>? / <'border-image-outset'>]? || <'border-image-repeat'>`, and
/// `mask-border`, which adds `<'mask-border-mode'>`.
fn parse_border_image(input: &mut Parser<'_>, mask: bool) -> Result<Vec<SpecifiedValue>> {
    let (mut source, mut slice, mut width, mut outset, mut repeat, mut mode) =
        (None, None, None, None, None, None);
    let mut any = false;
    loop {
        if source.is_none()
            && let Ok(value) = input.try_parse(ImageValue::parse)
        {
            source = Some(value);
        } else if slice.is_none()
            && let Ok(value) = input.try_parse(BorderImageSlice::parse_slice)
        {
            slice = Some(value);
            if input.try_parse(|input| input.expect_delim('/')).is_ok() {
                width = input.try_parse(BorderImageWidth::parse_widths).ok();
                if input.try_parse(|input| input.expect_delim('/')).is_ok() {
                    outset = Some(BorderImageOutset::parse_outsets(input)?);
                } else if width.is_none() {
                    return fail();
                }
            }
        } else if repeat.is_none()
            && let Ok(value) = input.try_parse(BorderImageRepeat::parse_repeat)
        {
            repeat = Some(value);
        } else if mask
            && mode.is_none()
            && let Ok(value) = input.try_parse(<MaskType as Property>::parse)
        {
            mode = Some(value);
        } else {
            break;
        }
        any = true;
    }
    if !any {
        return fail();
    }
    Ok(if mask {
        mask_border_values(source, slice, width, outset, repeat, mode)
    } else {
        border_image_values(source, slice, width, outset, repeat)
    })
}

/// One layer of `mask`.
#[derive(Default)]
struct MaskLayer {
    image: Option<ImageValue>,
    position: Option<(SpecifiedPosition, SpecifiedPosition)>,
    size: Option<BackgroundSize<SpecifiedLengthPercentage>>,
    repeat: Option<(Repeat, Repeat)>,
    origin: Option<CoordBox>,
    clip: Option<MaskClipBox>,
    composite: Option<CompositingOperator>,
    mode: Option<MaskingMode>,
}

/// `<mask-layer>`: `<mask-reference> || <position> [/ <bg-size>]? || <repeat-style> ||
/// <geometry-box> || [<geometry-box> | no-clip] || <compositing-operator> || <masking-mode>`.
fn parse_mask_layer(input: &mut Parser<'_>) -> Result<MaskLayer> {
    let mut layer = MaskLayer::default();
    let mut boxes = Vec::new();
    let mut any = false;
    loop {
        if layer.image.is_none()
            && let Ok(value) = input.try_parse(ImageValue::parse)
        {
            layer.image = Some(value);
        } else if layer.position.is_none()
            && let Ok(value) = input.try_parse(parse_bg_position)
        {
            layer.position = Some(value);
            if input.try_parse(|input| input.expect_delim('/')).is_ok() {
                layer.size = Some(BackgroundSize::parse_one(input)?);
            }
        } else if layer.repeat.is_none()
            && let Ok(value) = input.try_parse(BackgroundRepeat::parse_one)
        {
            layer.repeat = Some(value);
        } else if boxes.len() < 2
            && let Ok(value) = input.try_parse(<MaskClipBox as Property>::parse)
        {
            boxes.push(value);
        } else if layer.composite.is_none()
            && let Ok(value) = input.try_parse(<CompositingOperator as Property>::parse)
        {
            layer.composite = Some(value);
        } else if layer.mode.is_none()
            && let Ok(value) = input.try_parse(<MaskingMode as Property>::parse)
        {
            layer.mode = Some(value);
        } else {
            break;
        }
        any = true;
    }
    if !any {
        return fail();
    }
    // One box sets both the origin and the clip; with two, the first is the origin.
    let coord = |b: MaskClipBox| CoordBox::from_keyword(b.as_str());
    match boxes[..] {
        [] => {}
        [MaskClipBox::NoClip] => layer.clip = Some(MaskClipBox::NoClip),
        [one] => {
            layer.origin = coord(one);
            layer.clip = Some(one);
        }
        [origin, clip] => {
            layer.origin = Some(coord(origin).ok_or_else(|| ParseError::custom(()))?);
            layer.clip = Some(clip);
        }
        _ => unreachable!("at most two boxes"),
    }
    Ok(layer)
}

/// `mask`: `<mask-layer>#`. It also resets `mask-border`.
fn parse_mask(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let layers = input.parse_comma_separated(parse_mask_layer)?;
    let start = || SpecifiedPosition {
        edge: PositionEdge::Start,
        offset: Some(SpecifiedLengthPercentage::Percentage(0.0)),
    };
    let mut values = vec![
        SpecifiedValue::MaskImage(
            layers
                .iter()
                .map(|l| l.image.clone().unwrap_or(ImageValue::None))
                .collect(),
        ),
        SpecifiedValue::MaskPosition(
            layers
                .iter()
                .map(|l| l.position.clone().unwrap_or_else(|| (start(), start())))
                .collect(),
        ),
        SpecifiedValue::MaskSize(
            layers
                .iter()
                .map(|l| {
                    l.size
                        .clone()
                        .unwrap_or(BackgroundSize::Explicit(None, None))
                })
                .collect(),
        ),
        SpecifiedValue::MaskRepeat(
            layers
                .iter()
                .map(|l| l.repeat.unwrap_or((Repeat::Repeat, Repeat::Repeat)))
                .collect(),
        ),
        SpecifiedValue::MaskOrigin(
            layers
                .iter()
                .map(|l| l.origin.unwrap_or(CoordBox::BorderBox))
                .collect(),
        ),
        SpecifiedValue::MaskClip(
            layers
                .iter()
                .map(|l| l.clip.unwrap_or(MaskClipBox::BorderBox))
                .collect(),
        ),
        SpecifiedValue::MaskComposite(
            layers
                .iter()
                .map(|l| l.composite.unwrap_or(CompositingOperator::Add))
                .collect(),
        ),
        SpecifiedValue::MaskMode(
            layers
                .iter()
                .map(|l| l.mode.unwrap_or(MaskingMode::MatchSource))
                .collect(),
        ),
    ];
    values.extend(mask_border_values(None, None, None, None, None, None));
    Ok(values)
}

/// `offset`: `[<'offset-position'>? [<'offset-path'> [<'offset-distance'> ||
/// <'offset-rotate'>]?]?]! [/ <'offset-anchor'>]?`.
fn parse_offset(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let position = input.try_parse(OffsetPosition::parse_value).ok();
    let path = input.try_parse(OffsetPath::parse_value).ok();
    if position.is_none() && path.is_none() {
        return fail();
    }
    let (mut distance, mut rotate) = (None, None);
    if path.is_some() {
        for _ in 0..2 {
            if distance.is_none()
                && let Ok(value) =
                    input.try_parse(|input| parse_length_percentage(input, true, false))
            {
                distance = Some(value);
            } else if rotate.is_none()
                && let Ok(value) = input.try_parse(OffsetRotate::parse_value)
            {
                rotate = Some(value);
            }
        }
    }
    let anchor = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
        OffsetAnchor::parse_value(input)?
    } else {
        None
    };
    Ok(vec![
        SpecifiedValue::OffsetPosition(position.unwrap_or(OffsetPosition::Normal)),
        SpecifiedValue::OffsetPath(path.unwrap_or(OffsetPath::None)),
        SpecifiedValue::OffsetDistance(
            distance.unwrap_or(SpecifiedLengthPercentage::Length(Length::px(0.0))),
        ),
        SpecifiedValue::OffsetRotate(rotate.unwrap_or(OffsetRotate::AUTO)),
        SpecifiedValue::OffsetAnchor(anchor),
    ])
}

fn wrap_scroll_margin(id: LonghandId, value: SpecifiedLengthPercentage) -> SpecifiedValue {
    match id {
        L::ScrollMarginTop => SpecifiedValue::ScrollMarginTop(value),
        L::ScrollMarginRight => SpecifiedValue::ScrollMarginRight(value),
        L::ScrollMarginBottom => SpecifiedValue::ScrollMarginBottom(value),
        L::ScrollMarginLeft => SpecifiedValue::ScrollMarginLeft(value),
        logical => SpecifiedValue::ScrollMarginTop(value).into_logical(logical),
    }
}

fn wrap_scroll_padding(id: LonghandId, value: Option<SpecifiedLengthPercentage>) -> SpecifiedValue {
    match id {
        L::ScrollPaddingTop => SpecifiedValue::ScrollPaddingTop(value),
        L::ScrollPaddingRight => SpecifiedValue::ScrollPaddingRight(value),
        L::ScrollPaddingBottom => SpecifiedValue::ScrollPaddingBottom(value),
        L::ScrollPaddingLeft => SpecifiedValue::ScrollPaddingLeft(value),
        logical => SpecifiedValue::ScrollPaddingTop(value).into_logical(logical),
    }
}

/// `caret`: `<'caret-color'> || <'caret-animation'> || <'caret-shape'>`.
fn parse_caret(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let (mut color, mut animation, mut shape) = (None, None, None);
    loop {
        // `auto` is a value of all three; it goes to the first one still unset.
        if color.is_none()
            && let Ok(value) = input.try_parse(CaretColor::parse)
        {
            color = Some(value);
        } else if animation.is_none()
            && let Ok(value) = input.try_parse(CaretAnimation::parse)
        {
            animation = Some(value);
        } else if shape.is_none()
            && let Ok(value) = input.try_parse(CaretShape::parse)
        {
            shape = Some(value);
        } else {
            break;
        }
    }
    if color.is_none() && animation.is_none() && shape.is_none() {
        return fail();
    }
    Ok(vec![
        SpecifiedValue::CaretColor(color.unwrap_or(AccentColor::Auto)),
        SpecifiedValue::CaretAnimation(animation.unwrap_or(CaretAnimation::Auto)),
        SpecifiedValue::CaretShape(shape.unwrap_or(CaretShape::Auto)),
    ])
}

/// `animation`: a list of single animations. It also resets the longhands it cannot set.
fn parse_animation(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    use std::sync::Arc;
    let items = input.parse_comma_separated(parse_single_animation)?;
    let count = items.len();
    let mut names = Vec::with_capacity(count);
    let mut durations = Vec::with_capacity(count);
    let mut easings = Vec::with_capacity(count);
    let mut delays = Vec::with_capacity(count);
    let mut iterations = Vec::with_capacity(count);
    let mut directions = Vec::with_capacity(count);
    let mut fill_modes = Vec::with_capacity(count);
    let mut play_states = Vec::with_capacity(count);
    let mut timelines = Vec::with_capacity(count);
    for item in items {
        names.push(item.name.unwrap_or(AnimationName::None));
        durations.push(item.duration.unwrap_or(Some(0.0)));
        easings.push(item.easing.unwrap_or(Easing::Ease));
        delays.push(item.delay.unwrap_or(0.0));
        iterations.push(item.iterations.unwrap_or(1.0));
        directions.push(item.direction.unwrap_or(AnimationDirection::Normal));
        fill_modes.push(item.fill_mode.unwrap_or(AnimationFillMode::None));
        play_states.push(item.play_state.unwrap_or(AnimationPlayState::Running));
        timelines.push(item.timeline.unwrap_or(AnimationTimeline::Auto));
    }
    Ok(vec![
        SpecifiedValue::AnimationName(names.into()),
        SpecifiedValue::AnimationDuration(durations.into()),
        SpecifiedValue::AnimationTimingFunction(easings.into()),
        SpecifiedValue::AnimationDelay(delays.into()),
        SpecifiedValue::AnimationIterationCount(iterations.into()),
        SpecifiedValue::AnimationDirection(directions.into()),
        SpecifiedValue::AnimationFillMode(fill_modes.into()),
        SpecifiedValue::AnimationPlayState(play_states.into()),
        SpecifiedValue::AnimationTimeline(timelines.into()),
        SpecifiedValue::AnimationComposition(Arc::from([AnimationComposition::Replace])),
        SpecifiedValue::AnimationRangeStart(Arc::from([RangePoint::Normal])),
        SpecifiedValue::AnimationRangeEnd(Arc::from([RangePoint::Normal])),
        SpecifiedValue::AnimationTrigger(Arc::from([TriggerItem::from([])])),
    ])
}

/// `transition`: a list of single transitions, where `none` must be the only one.
fn parse_transition(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let items = input.parse_comma_separated(parse_single_transition)?;
    let count = items.len();
    let mut properties = Vec::with_capacity(count);
    let mut durations = Vec::with_capacity(count);
    let mut easings = Vec::with_capacity(count);
    let mut delays = Vec::with_capacity(count);
    let mut behaviors = Vec::with_capacity(count);
    let mut none = false;
    for item in items {
        match item.property {
            Some(None) => none = true,
            Some(Some(property)) => properties.push(property),
            None => properties.push(TransitionProperty::All),
        }
        durations.push(item.duration.unwrap_or(0.0));
        easings.push(item.easing.unwrap_or(Easing::Ease));
        delays.push(item.delay.unwrap_or(0.0));
        behaviors.push(item.behavior.unwrap_or(TransitionBehavior::Normal));
    }
    if none && count > 1 {
        return fail();
    }
    Ok(vec![
        SpecifiedValue::TransitionProperty(properties.into()),
        SpecifiedValue::TransitionDuration(durations.into()),
        SpecifiedValue::TransitionTimingFunction(easings.into()),
        SpecifiedValue::TransitionDelay(delays.into()),
        SpecifiedValue::TransitionBehavior(behaviors.into()),
    ])
}

type Ranges = (
    Vec<RangePoint<SpecifiedLengthPercentage>>,
    Vec<RangePoint<SpecifiedLengthPercentage>>,
);

/// `[ <start> <end>? ]#`, as `animation-range` and the trigger ranges take; `auto` allows
/// `auto`.
fn parse_ranges(input: &mut Parser<'_>, auto: bool) -> Result<Ranges> {
    let pairs = input.parse_comma_separated(|input| {
        let start = parse_range_point(input, false, auto)?;
        let end = match input.try_parse(|input| parse_range_point(input, true, auto)) {
            Ok(end) => end,
            Err(_) => implied_range_end(&start, auto),
        };
        Ok((start, end))
    })?;
    Ok(pairs.into_iter().unzip())
}

/// `view-timeline`: `[ <'view-timeline-name'> [ <'view-timeline-axis'> ||
/// <'view-timeline-inset'> ]? ]#`.
fn parse_view_timeline(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    let items = input.parse_comma_separated(|input| {
        let name = TimelineName::parse(input)?;
        let (mut axis, mut inset) = (None, None);
        loop {
            if axis.is_none()
                && let Ok(value) = input.try_parse(TimelineAxis::parse)
            {
                axis = Some(value);
            } else if inset.is_none()
                && let Ok(value) = input.try_parse(ViewInsetValue::parse)
            {
                inset = Some(value);
            } else {
                break;
            }
        }
        Ok((
            name,
            axis.unwrap_or(TimelineAxis::Block),
            inset.unwrap_or((InsetSide::Auto, InsetSide::Auto)),
        ))
    })?;
    let mut names = Vec::new();
    let mut axes = Vec::new();
    let mut insets = Vec::new();
    for (name, axis, inset) in items {
        names.push(name);
        axes.push(axis);
        insets.push(inset);
    }
    Ok(vec![
        SpecifiedValue::ViewTimelineName(names.into()),
        SpecifiedValue::ViewTimelineAxis(axes.into()),
        SpecifiedValue::ViewTimelineInset(insets.into()),
    ])
}

/// `timeline-trigger`: `none | [ <'timeline-trigger-name'> <'timeline-trigger-source'>
/// <'timeline-trigger-activation-range'> [ '/' <'timeline-trigger-active-range'> ]? ]#`.
fn parse_timeline_trigger(input: &mut Parser<'_>) -> Result<Vec<SpecifiedValue>> {
    use std::sync::Arc;
    if input
        .try_parse(|input| input.expect_ident_matching("none"))
        .is_ok()
    {
        return Ok(vec![
            SpecifiedValue::TimelineTriggerName(Arc::from([])),
            SpecifiedValue::TimelineTriggerSource(Arc::from([AnimationTimeline::Auto])),
            SpecifiedValue::TimelineTriggerActivationRangeStart(Arc::from([RangePoint::Normal])),
            SpecifiedValue::TimelineTriggerActivationRangeEnd(Arc::from([RangePoint::Normal])),
            SpecifiedValue::TimelineTriggerActiveRangeStart(Arc::from([RangePoint::Auto])),
            SpecifiedValue::TimelineTriggerActiveRangeEnd(Arc::from([RangePoint::Auto])),
        ]);
    }
    let items = input.parse_comma_separated(|input| {
        let name = parse_dashed_ident(input)?;
        let source = TimelineValue::parse(input)?;
        let start = parse_range_point(input, false, false)?;
        let end = input
            .try_parse(|input| parse_range_point(input, true, false))
            .unwrap_or_else(|_| implied_range_end(&start, false));
        let active = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
            let active_start = parse_range_point(input, false, true)?;
            let active_end = input
                .try_parse(|input| parse_range_point(input, true, true))
                .unwrap_or_else(|_| implied_range_end(&active_start, true));
            (active_start, active_end)
        } else {
            (RangePoint::Auto, RangePoint::Auto)
        };
        Ok((name, source, start, end, active))
    })?;
    let mut names = Vec::new();
    let mut sources = Vec::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut active_starts = Vec::new();
    let mut active_ends = Vec::new();
    for (name, source, start, end, (active_start, active_end)) in items {
        names.push(name);
        sources.push(source);
        starts.push(start);
        ends.push(end);
        active_starts.push(active_start);
        active_ends.push(active_end);
    }
    Ok(vec![
        SpecifiedValue::TimelineTriggerName(names.into()),
        SpecifiedValue::TimelineTriggerSource(sources.into()),
        SpecifiedValue::TimelineTriggerActivationRangeStart(starts.into()),
        SpecifiedValue::TimelineTriggerActivationRangeEnd(ends.into()),
        SpecifiedValue::TimelineTriggerActiveRangeStart(active_starts.into()),
        SpecifiedValue::TimelineTriggerActiveRangeEnd(active_ends.into()),
    ])
}
