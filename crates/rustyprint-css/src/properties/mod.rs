//! Properties: the longhand table, shorthands and declaration parsing.
//!
//! Each longhand names a value type implementing [`Property`]. `css-properties.md` tracks
//! which properties exist so far.

mod shorthands;
pub mod types;

use std::sync::Arc;

use cssparser::{ParseError, Parser};

use crate::custom::{CustomProperties, has_references};
use crate::values::{AbsoluteColor, CalcContext, ColorContext, FontMetrics, LengthContext};
pub use shorthands::ShorthandId;
use types::*;

/// A property's value type: how it parses and computes.
pub trait Property {
    type Specified: Clone + std::fmt::Debug + PartialEq;
    type Computed: Clone + std::fmt::Debug + PartialEq;

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified, ParseError<()>>;
    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed;
    /// The computed value as CSS text, like `getComputedStyle()` returns it.
    fn to_css(computed: &Self::Computed) -> String;
}

/// What specified values compute against.
pub struct ComputeContext<'a> {
    pub parent: &'a ComputedValues,
    pub root_font_size: f32,
    /// This element's font size (the parent's while `font-size` itself computes).
    pub font_size: f32,
    /// This element's line height in pixels, for `lh` units.
    pub line_height: f32,
    pub root_line_height: f32,
    /// Metrics of this element's first available font (the parent's while `font-size`
    /// computes) and of the root element's.
    pub font_metrics: FontMetrics,
    pub root_font_metrics: FontMetrics,
    /// This element's computed `color`, for `currentcolor`.
    pub color: AbsoluteColor,
    /// The page area, which viewport units refer to.
    pub viewport: (f32, f32),
    pub vertical: bool,
    /// Whether this element is right-to-left.
    pub rtl: bool,
    /// Whether this element's used color scheme is dark.
    pub dark: bool,
    /// The element's effective zoom, which absolute lengths are multiplied by.
    pub zoom: f32,
    /// `(sibling-index(), sibling-count())`.
    pub siblings: (u32, u32),
    /// What `random()` picks values from for this element.
    pub random_seed: Option<u64>,
    /// The size container units refer to: the nearest size containers', or the page area.
    pub container: (f32, f32),
}

impl<'a> ComputeContext<'a> {
    /// A context for values that do not depend on an element, such as the initial values of
    /// registered custom properties: `parent`'s font without its metrics, horizontal and
    /// left to right.
    pub fn initial(parent: &'a ComputedValues, viewport: (f32, f32)) -> Self {
        let font = FontMetrics::fallback(parent.font_size);
        ComputeContext {
            parent,
            root_font_size: parent.font_size,
            font_size: parent.font_size,
            line_height: font.line_height,
            root_line_height: font.line_height,
            font_metrics: font,
            root_font_metrics: font,
            color: parent.color,
            viewport,
            vertical: false,
            rtl: false,
            dark: false,
            zoom: 1.0,
            siblings: (1, 1),
            random_seed: None,
            container: viewport,
        }
    }

    pub fn length(&self) -> LengthContext {
        let mut font = self.font_metrics;
        font.line_height = self.line_height;
        let mut root_font = self.root_font_metrics;
        root_font.line_height = self.root_line_height;
        LengthContext {
            font,
            root_font,
            viewport: self.viewport,
            container: self.container,
            vertical: self.vertical,
            zoom: self.zoom,
            random_seed: self.random_seed,
        }
    }

    /// What colors that depend on the element resolve against.
    pub fn colors(&self) -> ColorContext {
        ColorContext {
            current: self.color,
            dark: self.dark,
        }
    }

    pub fn calc(&self) -> CalcContext<'static> {
        CalcContext {
            length: None,
            percent_basis: None,
            siblings: Some(self.siblings),
            size: None,
        }
    }
}

macro_rules! inherited {
    (inherited) => {
        true
    };
    (reset) => {
        false
    };
}

macro_rules! longhands {
    (
        physical {
            $($id:ident $name:literal $field:ident: $ty:ty = $initial:expr, $inherit:ident;)+
        }
        logical {
            $($lid:ident $lname:literal: $lty:ty => [$($phys:ident),+] $which:ident;)+
        }
    ) => {
        /// A longhand property.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum LonghandId {
            $($id,)+
            $($lid,)+
        }

        impl LonghandId {
            pub const ALL: &'static [LonghandId] = &[$(LonghandId::$id,)+ $(LonghandId::$lid,)+];

            pub fn name(self) -> &'static str {
                match self {
                    $(LonghandId::$id => $name,)+
                    $(LonghandId::$lid => $lname,)+
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(LonghandId::$id),)+
                    $($lname => Some(LonghandId::$lid),)+
                    _ => None,
                }
            }

            /// Whether the property inherits by default.
            pub fn inherited(self) -> bool {
                match self {
                    $(LonghandId::$id => inherited!($inherit),)+
                    // Logical properties share the inheritance of their physical group.
                    $(LonghandId::$lid => [$(LonghandId::$phys),+][0].inherited(),)+
                }
            }

            /// Parses a value of this property.
            pub fn parse(self, input: &mut Parser<'_>) -> Result<SpecifiedValue, ParseError<()>> {
                match self {
                    $(LonghandId::$id => Ok(SpecifiedValue::$id(<$ty as Property>::parse(input)?)),)+
                    $(LonghandId::$lid => Ok(SpecifiedValue::$lid(<$lty as Property>::parse(input)?)),)+
                }
            }

            /// The physical property a logical property maps to for a writing mode and
            /// direction. Physical properties map to themselves.
            pub fn to_physical(self, writing_mode: WritingMode, direction: Direction) -> LonghandId {
                match self {
                    $(LonghandId::$lid => physical_of(&[$(LonghandId::$phys),+], LogicalSide::$which, writing_mode, direction),)+
                    other => other,
                }
            }

            /// Whether this longhand is a logical property that maps to a physical one.
            pub fn is_logical(self) -> bool {
                matches!(self, $(LonghandId::$lid)|+)
            }
        }

        /// A specified value of a longhand.
        #[derive(Debug, Clone, PartialEq)]
        pub enum SpecifiedValue {
            $($id(<$ty as Property>::Specified),)+
            $($lid(<$lty as Property>::Specified),)+
        }

        impl SpecifiedValue {
            pub fn id(&self) -> LonghandId {
                match self {
                    $(SpecifiedValue::$id(_) => LonghandId::$id,)+
                    $(SpecifiedValue::$lid(_) => LonghandId::$lid,)+
                }
            }

            /// A logical value as a value of the physical property `target` from its group.
            /// Physical values are returned unchanged.
            pub fn retarget(&self, target: LonghandId) -> SpecifiedValue {
                match self {
                    $(SpecifiedValue::$lid(v) => match target {
                        $(LonghandId::$phys => SpecifiedValue::$phys(v.clone()),)+
                        _ => unreachable!("{target:?} is not in the group of {}", $lname),
                    },)+
                    other => other.clone(),
                }
            }

            /// A physical value as a value of the logical property `target`, for shorthands
            /// that parse logical values with physical grammars.
            pub fn into_logical(self, target: LonghandId) -> SpecifiedValue {
                match target {
                    $(LonghandId::$lid => match self {
                        $(SpecifiedValue::$phys(v) => SpecifiedValue::$lid(v),)+
                        other => unreachable!("{other:?} is not in the group of {}", $lname),
                    },)+
                    _ => self,
                }
            }
        }

        /// Computed values of every property of an element. Logical properties have no
        /// storage of their own: they read their physical property.
        #[derive(Debug, Clone, PartialEq)]
        pub struct ComputedValues {
            $(pub $field: <$ty as Property>::Computed,)+
            pub custom: CustomProperties,
            /// The document's font rules, which font selection uses.
            pub font_rules: crate::font_rules::FontRulesRef,
            /// The product of the `zoom` of the element and its ancestors, which absolute
            /// lengths are multiplied by.
            pub effective_zoom: f32,
            /// For a style turned into the line-relative axes of a vertical writing mode
            /// (see [`ComputedValues::in_line_axes`]), the style as computed.
            pub unrotated: Option<Arc<ComputedValues>>,
        }

        impl ComputedValues {
            /// Initial values of every property.
            pub fn initial() -> Self {
                Self {
                    $($field: $initial,)+
                    custom: CustomProperties::default(),
                    font_rules: crate::font_rules::FontRulesRef::default(),
                    effective_zoom: 1.0,
                    unrotated: None,
                }
            }

            /// Computes `value` and stores it. Logical values must be retargeted first.
            pub(crate) fn set(&mut self, value: &SpecifiedValue, ctx: &ComputeContext) {
                match value {
                    $(SpecifiedValue::$id(v) => self.$field = <$ty as Property>::compute(v, ctx),)+
                    $(SpecifiedValue::$lid(_) => unreachable!("{} is set through its physical property", $lname),)+
                }
            }

            /// A longhand's computed value as CSS text.
            pub fn serialize(&self, id: LonghandId) -> String {
                match id {
                    $(LonghandId::$id => <$ty as Property>::to_css(&self.$field),)+
                    $(LonghandId::$lid => self.serialize(id.to_physical(self.writing_mode, self.direction)),)+
                }
            }

            /// Whether one property has the same computed value in `other`.
            pub fn same(&self, id: LonghandId, other: &ComputedValues) -> bool {
                match id {
                    $(LonghandId::$id => self.$field == other.$field,)+
                    $(LonghandId::$lid => {
                        let id = id.to_physical(self.writing_mode, self.direction);
                        self.same(id, other)
                    })+
                }
            }

            /// Copies one property's computed value from `other`.
            pub(crate) fn copy(&mut self, id: LonghandId, other: &ComputedValues) {
                match id {
                    $(LonghandId::$id => self.$field = other.$field.clone(),)+
                    $(LonghandId::$lid => unreachable!("{} has no storage", $lname),)+
                }
            }
        }
    };
}

longhands! {
    physical {
        // Early: other properties depend on these.
        WritingMode "writing-mode" writing_mode: WritingMode = WritingMode::HorizontalTb, inherited;
        Direction "direction" direction: Direction = Direction::Ltr, inherited;
        TextOrientation "text-orientation" text_orientation: TextOrientation = TextOrientation::Mixed, inherited;
        TextCombineUpright "text-combine-upright" text_combine_upright: TextCombineUpright = TextCombineUpright::None, inherited;
        Zoom "zoom" zoom: Zoom = 1.0, reset;
        FontFamily "font-family" font_family: FontFamily = Arc::from([FamilyName::Generic(GenericFamily::Serif)]), inherited;
        FontStyle "font-style" font_style: FontStyle = FontStyle::Normal, inherited;
        FontWeight "font-weight" font_weight: FontWeight = 400.0, inherited;
        FontWidth "font-width" font_width: FontWidth = 100.0, inherited;
        FontSize "font-size" font_size: FontSize = MEDIUM_FONT_SIZE, inherited;
        LineHeight "line-height" line_height: LineHeight = LineHeight::Normal, inherited;
        ColorScheme "color-scheme" color_scheme: ColorScheme = ColorScheme::normal(), inherited;
        Color "color" color: ColorProperty = AbsoluteColor::BLACK, inherited;
        // Color adjustment
        PrintColorAdjust "print-color-adjust" print_color_adjust: PrintColorAdjust = PrintColorAdjust::Economy, inherited;
        ForcedColorAdjust "forced-color-adjust" forced_color_adjust: ForcedColorAdjust = ForcedColorAdjust::Auto, inherited;
        DynamicRangeLimit "dynamic-range-limit" dynamic_range_limit: DynamicRangeLimit = DynamicRangeLimit::NoLimit, inherited;
        // Fonts
        FontVariantCaps "font-variant-caps" font_variant_caps: FontVariantCaps = FontVariantCaps::Normal, inherited;
        FontVariantLigatures "font-variant-ligatures" font_variant_ligatures: FontVariantLigatures = FontVariantLigatures::NORMAL, inherited;
        FontVariantNumeric "font-variant-numeric" font_variant_numeric: FontVariantNumeric = FontVariantNumeric::default(), inherited;
        FontVariantEastAsian "font-variant-east-asian" font_variant_east_asian: FontVariantEastAsian = FontVariantEastAsian::default(), inherited;
        FontVariantPosition "font-variant-position" font_variant_position: FontVariantPosition = FontVariantPosition::Normal, inherited;
        FontFeatureSettings "font-feature-settings" font_feature_settings: FontFeatureSettings = Arc::from([]), inherited;
        FontKerning "font-kerning" font_kerning: FontKerning = FontKerning::Auto, inherited;
        FontLanguageOverride "font-language-override" font_language_override: FontLanguageOverride = None, inherited;
        FontSizeAdjust "font-size-adjust" font_size_adjust: FontSizeAdjust = FontSizeAdjust::None, inherited;
        FontSynthesisWeight "font-synthesis-weight" font_synthesis_weight: Synthesis = Synthesis::Auto, inherited;
        FontSynthesisStyle "font-synthesis-style" font_synthesis_style: FontSynthesisStyle = FontSynthesisStyle::Auto, inherited;
        FontSynthesisSmallCaps "font-synthesis-small-caps" font_synthesis_small_caps: Synthesis = Synthesis::Auto, inherited;
        FontSynthesisPosition "font-synthesis-position" font_synthesis_position: Synthesis = Synthesis::Auto, inherited;
        FontVariantAlternates "font-variant-alternates" font_variant_alternates: FontVariantAlternates = FontVariantAlternates::default(), inherited;
        FontVariantEmoji "font-variant-emoji" font_variant_emoji: FontVariantEmoji = FontVariantEmoji::Normal, inherited;
        FontVariationSettings "font-variation-settings" font_variation_settings: FontVariationSettings = Arc::from([]), inherited;
        FontOpticalSizing "font-optical-sizing" font_optical_sizing: FontOpticalSizing = FontOpticalSizing::Auto, inherited;
        FontPalette "font-palette" font_palette: FontPalette = FontPalette::Normal, inherited;
        // Inline layout
        AlignmentBaseline "alignment-baseline" alignment_baseline: AlignmentBaseline = AlignmentBaseline::Baseline, reset;
        BaselineShift "baseline-shift" baseline_shift: BaselineShiftValue = BaselineShift::Value(LengthPercentage::ZERO), reset;
        BaselineSource "baseline-source" baseline_source: BaselineSource = BaselineSource::Auto, reset;
        DominantBaseline "dominant-baseline" dominant_baseline: DominantBaseline = DominantBaseline::Auto, inherited;
        InitialLetter "initial-letter" initial_letter: InitialLetter = InitialLetter::Normal, reset;
        InitialLetterAlign "initial-letter-align" initial_letter_align: InitialLetterAlign = InitialLetterAlign { border_box: false, edge: None }, inherited;
        TextBoxTrim "text-box-trim" text_box_trim: TextBoxTrim = TextBoxTrim::None, reset;
        TextBoxEdge "text-box-edge" text_box_edge: TextBoxEdge = None, inherited;
        LineHeightStep "line-height-step" line_height_step: LineHeightStep = 0.0, inherited;
        LineClamp "line-clamp" line_clamp: LineClamp = LineClamp { lines: None, ellipsis: BlockEllipsis::NoEllipsis }, reset;
        MaxLines "max-lines" max_lines: MaxLines = None, reset;
        TextOverflow "text-overflow" text_overflow: TextOverflow = TextOverflow { start: TextOverflowSide::Clip, end: TextOverflowSide::Clip, both: false }, reset;
        // Text
        WhiteSpaceCollapse "white-space-collapse" white_space_collapse: WhiteSpaceCollapse = WhiteSpaceCollapse::Collapse, inherited;
        TextWrapMode "text-wrap-mode" text_wrap_mode: TextWrapMode = TextWrapMode::Wrap, inherited;
        TextWrapStyle "text-wrap-style" text_wrap_style: TextWrapStyle = TextWrapStyle::Auto, inherited;
        TextTransform "text-transform" text_transform: TextTransform = TextTransform::NONE, inherited;
        TabSize "tab-size" tab_size: TabSize = TabSize::Spaces(8.0), inherited;
        WordBreak "word-break" word_break: WordBreak = WordBreak::Normal, inherited;
        LineBreak "line-break" line_break: LineBreak = LineBreak::Auto, inherited;
        // Ruby
        RubyAlign "ruby-align" ruby_align: RubyAlign = RubyAlign::SpaceAround, inherited;
        RubyMerge "ruby-merge" ruby_merge: RubyMerge = RubyMerge::Separate, inherited;
        RubyOverhang "ruby-overhang" ruby_overhang: RubyOverhang = RubyOverhang::Auto, inherited;
        RubyPosition "ruby-position" ruby_position: RubyPosition = RubyPosition::INITIAL, inherited;
        OverflowWrap "overflow-wrap" overflow_wrap: OverflowWrap = OverflowWrap::Normal, inherited;
        Hyphens "hyphens" hyphens: Hyphens = Hyphens::Manual, inherited;
        HyphenateCharacter "hyphenate-character" hyphenate_character: HyphenateCharacter = None, inherited;
        HyphenateLimitChars "hyphenate-limit-chars" hyphenate_limit_chars: HyphenateLimitChars = [None; 3], inherited;
        LetterSpacing "letter-spacing" letter_spacing: Spacing = None, inherited;
        WordSpacing "word-spacing" word_spacing: Spacing = None, inherited;
        TextIndent "text-indent" text_indent: TextIndentValue = TextIndent { length: LengthPercentage::ZERO, hanging: false, each_line: false }, inherited;
        TextAlignLast "text-align-last" text_align_last: TextAlignLast = TextAlignLast::Auto, inherited;
        TextJustify "text-justify" text_justify: TextJustify = TextJustify::Auto, inherited;
        HangingPunctuation "hanging-punctuation" hanging_punctuation: HangingPunctuation = HangingPunctuation::default(), inherited;
        TextAutospace "text-autospace" text_autospace: TextAutospace = TextAutospace::Normal, inherited;
        TextSpacingTrim "text-spacing-trim" text_spacing_trim: TextSpacingTrim = TextSpacingTrim::Normal, inherited;
        TextFit "text-fit" text_fit: TextFit = TextFit::None, reset;
        // Text decoration
        TextDecorationLine "text-decoration-line" text_decoration_line: TextDecorationLine = TextDecorationLine::NONE, reset;
        TextDecorationStyle "text-decoration-style" text_decoration_style: TextDecorationStyle = TextDecorationStyle::Solid, reset;
        TextDecorationColor "text-decoration-color" text_decoration_color: ColorValue = crate::values::Color::CurrentColor, reset;
        TextDecorationThickness "text-decoration-thickness" text_decoration_thickness: TextDecorationThickness = DecorationLength::Auto, reset;
        TextDecorationInset "text-decoration-inset" text_decoration_inset: TextDecorationInset = TextDecorationInset::Lengths(0.0, 0.0), reset;
        TextDecorationSkip "text-decoration-skip" text_decoration_skip: TextDecorationSkip = TextDecorationSkip::OBJECTS, inherited;
        TextDecorationSkipInk "text-decoration-skip-ink" text_decoration_skip_ink: TextDecorationSkipInk = TextDecorationSkipInk::Auto, inherited;
        TextUnderlineOffset "text-underline-offset" text_underline_offset: TextUnderlineOffset = DecorationLength::Auto, inherited;
        TextUnderlinePosition "text-underline-position" text_underline_position: TextUnderlinePosition = TextUnderlinePosition::default(), inherited;
        TextEmphasisStyle "text-emphasis-style" text_emphasis_style: TextEmphasisStyle = TextEmphasisStyle::None, inherited;
        TextEmphasisColor "text-emphasis-color" text_emphasis_color: ColorValue = crate::values::Color::CurrentColor, inherited;
        TextEmphasisPosition "text-emphasis-position" text_emphasis_position: TextEmphasisPosition = TextEmphasisPosition::Position { under: false, left: Some(false) }, inherited;
        TextShadow "text-shadow" text_shadow: TextShadow = Arc::from([]), inherited;
        // Text painting
        PaintOrder "paint-order" paint_order: PaintOrder = [0, 1, 2], inherited;
        Stroke "stroke" stroke: Paint = Paint::None, inherited;
        StrokeColor "stroke-color" stroke_color: ColorValue = crate::values::Color::Absolute(AbsoluteColor::TRANSPARENT), inherited;
        TextRendering "text-rendering" text_rendering: TextRendering = TextRendering::Auto, inherited;
        // Display and box
        Display "display" display: Display = Display::INLINE, reset;
        Visibility "visibility" visibility: Visibility = Visibility::Visible, inherited;
        BoxSizing "box-sizing" box_sizing: BoxSizing = BoxSizing::ContentBox, reset;
        OverflowX "overflow-x" overflow_x: Overflow = Overflow::Visible, reset;
        OverflowY "overflow-y" overflow_y: Overflow = Overflow::Visible, reset;
        UnicodeBidi "unicode-bidi" unicode_bidi: UnicodeBidi = UnicodeBidi::Normal, reset;
        TextAlign "text-align" text_align: TextAlign = TextAlign::Start, inherited;
        // Backgrounds
        BackgroundColor "background-color" background_color: ColorValue = crate::values::Color::Absolute(AbsoluteColor::TRANSPARENT), reset;
        BackgroundImage "background-image" background_image: BackgroundImage = Arc::from([ImageValue::None]), reset;
        BackgroundPositionX "background-position-x" background_position_x: BackgroundPositionX = Arc::from([PositionComponent::START]), reset;
        BackgroundPositionY "background-position-y" background_position_y: BackgroundPositionY = Arc::from([PositionComponent::START]), reset;
        BackgroundSize "background-size" background_size: BackgroundSizeList = Arc::from([BackgroundSize::Explicit(None, None)]), reset;
        BackgroundRepeat "background-repeat" background_repeat: BackgroundRepeat = Arc::from([(Repeat::Repeat, Repeat::Repeat)]), reset;
        BackgroundAttachment "background-attachment" background_attachment: BackgroundAttachment = Arc::from([Attachment::Scroll]), reset;
        BackgroundOrigin "background-origin" background_origin: BackgroundOrigin = Arc::from([VisualBox::PaddingBox]), reset;
        BackgroundClip "background-clip" background_clip: BackgroundClipList = Arc::from([BackgroundClip::BorderBox]), reset;
        // Margins
        MarginTop "margin-top" margin_top: MarginValue = MaybeAuto::Value(LengthPercentage::ZERO), reset;
        MarginRight "margin-right" margin_right: MarginValue = MaybeAuto::Value(LengthPercentage::ZERO), reset;
        MarginBottom "margin-bottom" margin_bottom: MarginValue = MaybeAuto::Value(LengthPercentage::ZERO), reset;
        MarginLeft "margin-left" margin_left: MarginValue = MaybeAuto::Value(LengthPercentage::ZERO), reset;
        // Padding
        PaddingTop "padding-top" padding_top: NonNegativeLengthPercentage = LengthPercentage::ZERO, reset;
        PaddingRight "padding-right" padding_right: NonNegativeLengthPercentage = LengthPercentage::ZERO, reset;
        PaddingBottom "padding-bottom" padding_bottom: NonNegativeLengthPercentage = LengthPercentage::ZERO, reset;
        PaddingLeft "padding-left" padding_left: NonNegativeLengthPercentage = LengthPercentage::ZERO, reset;
        // Borders
        BorderTopWidth "border-top-width" border_top_width: BorderWidth = 3.0, reset;
        BorderRightWidth "border-right-width" border_right_width: BorderWidth = 3.0, reset;
        BorderBottomWidth "border-bottom-width" border_bottom_width: BorderWidth = 3.0, reset;
        BorderLeftWidth "border-left-width" border_left_width: BorderWidth = 3.0, reset;
        BorderTopStyle "border-top-style" border_top_style: BorderStyle = BorderStyle::None, reset;
        BorderRightStyle "border-right-style" border_right_style: BorderStyle = BorderStyle::None, reset;
        BorderBottomStyle "border-bottom-style" border_bottom_style: BorderStyle = BorderStyle::None, reset;
        BorderLeftStyle "border-left-style" border_left_style: BorderStyle = BorderStyle::None, reset;
        BorderTopColor "border-top-color" border_top_color: ColorValue = crate::values::Color::CurrentColor, reset;
        BorderRightColor "border-right-color" border_right_color: ColorValue = crate::values::Color::CurrentColor, reset;
        BorderBottomColor "border-bottom-color" border_bottom_color: ColorValue = crate::values::Color::CurrentColor, reset;
        BorderLeftColor "border-left-color" border_left_color: ColorValue = crate::values::Color::CurrentColor, reset;
        BorderTopLeftRadius "border-top-left-radius" border_top_left_radius: BorderRadius = (LengthPercentage::ZERO, LengthPercentage::ZERO), reset;
        BorderTopRightRadius "border-top-right-radius" border_top_right_radius: BorderRadius = (LengthPercentage::ZERO, LengthPercentage::ZERO), reset;
        BorderBottomRightRadius "border-bottom-right-radius" border_bottom_right_radius: BorderRadius = (LengthPercentage::ZERO, LengthPercentage::ZERO), reset;
        BorderBottomLeftRadius "border-bottom-left-radius" border_bottom_left_radius: BorderRadius = (LengthPercentage::ZERO, LengthPercentage::ZERO), reset;
        // Outlines
        OutlineWidth "outline-width" outline_width: BorderWidth = 3.0, reset;
        OutlineStyle "outline-style" outline_style: OutlineStyle = OutlineStyle::None, reset;
        OutlineColor "outline-color" outline_color: OutlineColor = None, reset;
        OutlineOffset "outline-offset" outline_offset: LengthValue = 0.0, reset;
        // Sizing
        Width "width" width: SizeValue = Size::Auto, reset;
        Height "height" height: SizeValue = Size::Auto, reset;
        MinWidth "min-width" min_width: SizeValue = Size::Auto, reset;
        MinHeight "min-height" min_height: SizeValue = Size::Auto, reset;
        MaxWidth "max-width" max_width: MaxSizeValue = Size::None, reset;
        MaxHeight "max-height" max_height: MaxSizeValue = Size::None, reset;
        AspectRatio "aspect-ratio" aspect_ratio: AspectRatio = AspectRatio::AUTO, reset;
        ContainIntrinsicWidth "contain-intrinsic-width" contain_intrinsic_width: ContainIntrinsic = ContainIntrinsicSize { auto: false, length: None }, reset;
        ContainIntrinsicHeight "contain-intrinsic-height" contain_intrinsic_height: ContainIntrinsic = ContainIntrinsicSize { auto: false, length: None }, reset;
        FrameSizing "frame-sizing" frame_sizing: FrameSizing = FrameSizing::Auto, reset;
        MarginTrim "margin-trim" margin_trim: MarginTrim = MarginTrim::default(), reset;
        OverflowClipMargin "overflow-clip-margin" overflow_clip_margin: OverflowClipMarginValue = OverflowClipMargin { visual_box: VisualBox::PaddingBox, margin: 0.0 }, reset;
        // Fragmentation
        BreakBefore "break-before" break_before: BreakBetween = BreakBetween::Auto, reset;
        BreakAfter "break-after" break_after: BreakBetween = BreakBetween::Auto, reset;
        BreakInside "break-inside" break_inside: BreakInside = BreakInside::Auto, reset;
        Orphans "orphans" orphans: LineCount = 2, inherited;
        Widows "widows" widows: LineCount = 2, inherited;
        BoxDecorationBreak "box-decoration-break" box_decoration_break: BoxDecorationBreak = BoxDecorationBreak::Slice, reset;
        Page "page" page: PageName = None, reset;
        // Generated content
        Content "content" content: Content = Content::Normal, reset;
        Quotes "quotes" quotes: Quotes = Quotes::Auto, inherited;
        CounterReset "counter-reset" counter_reset: CounterReset = Arc::from([]), reset;
        CounterIncrement "counter-increment" counter_increment: CounterIncrement = Arc::from([]), reset;
        CounterSet "counter-set" counter_set: CounterSet = Arc::from([]), reset;
        // Lists
        ListStyleType "list-style-type" list_style_type: ListStyleType = ListStyleType::Style(crate::counter_style::CounterStyleRef::Name(Arc::from("disc"))), inherited;
        ListStylePosition "list-style-position" list_style_position: ListStylePosition = ListStylePosition::Outside, inherited;
        ListStyleImage "list-style-image" list_style_image: ListStyleImage = ImageValue::None, inherited;
        // Tables
        BorderCollapse "border-collapse" border_collapse: BorderCollapse = BorderCollapse::Separate, inherited;
        BorderSpacing "border-spacing" border_spacing: BorderSpacing = (0.0, 0.0), inherited;
        CaptionSide "caption-side" caption_side: CaptionSide = CaptionSide::Top, inherited;
        EmptyCells "empty-cells" empty_cells: EmptyCells = EmptyCells::Show, inherited;
        TableLayout "table-layout" table_layout: TableLayout = TableLayout::Auto, reset;
        // Images and replaced elements
        ObjectFit "object-fit" object_fit: ObjectFit = ObjectFit::Fill, reset;
        ObjectPosition "object-position" object_position: ObjectPosition = (PositionComponent { from_end: false, offset: LengthPercentage::Percentage(0.5) }, PositionComponent { from_end: false, offset: LengthPercentage::Percentage(0.5) }), reset;
        ObjectViewBox "object-view-box" object_view_box: ObjectViewBox = ObjectViewBox::None, reset;
        ImageRendering "image-rendering" image_rendering: ImageRendering = ImageRendering::Auto, inherited;
        ImageOrientation "image-orientation" image_orientation: ImageOrientation = ImageOrientation::FromImage, inherited;
        ImageResolution "image-resolution" image_resolution: ImageResolution = ImageResolution { from_image: false, resolution: Some(1.0), snap: false }, inherited;
        LinkParameters "link-parameters" link_parameters: LinkParameters = Arc::from([]), reset;
        Position "position" position: Position = Position::Static, reset;
        Top "top" top: MarginValue = MaybeAuto::Auto, reset;
        Right "right" right: MarginValue = MaybeAuto::Auto, reset;
        Bottom "bottom" bottom: MarginValue = MaybeAuto::Auto, reset;
        Left "left" left: MarginValue = MaybeAuto::Auto, reset;
        ZIndex "z-index" z_index: ZIndex = ZIndex::Auto, reset;
        Float "float" float: Float = Float::None, reset;
        Clear "clear" clear: Clear = Clear::None, reset;
        ShapeOutside "shape-outside" shape_outside: ShapeOutside<LengthPercentage> = ShapeOutside::None, reset;
        ShapeMargin "shape-margin" shape_margin: NonNegativeLengthPercentage = LengthPercentage::ZERO, reset;
        ShapeImageThreshold "shape-image-threshold" shape_image_threshold: ShapeImageThreshold = 0.0, reset;
        // Box alignment
        AlignContent "align-content" align_content: AlignContent = ContentAlignment::Normal, reset;
        JustifyContent "justify-content" justify_content: JustifyContent = ContentAlignment::Normal, reset;
        AlignItems "align-items" align_items: AlignItems = ItemAlignment::Normal, reset;
        JustifyItems "justify-items" justify_items: JustifyItems = ItemAlignment::Normal, reset;
        AlignSelf "align-self" align_self: AlignSelf = ItemAlignment::Auto, reset;
        JustifySelf "justify-self" justify_self: JustifySelf = ItemAlignment::Auto, reset;
        RowGap "row-gap" row_gap: Gap = None, reset;
        ColumnGap "column-gap" column_gap: Gap = None, reset;
        // Flex layout
        FlexDirection "flex-direction" flex_direction: FlexDirection = FlexDirection::Row, reset;
        FlexWrap "flex-wrap" flex_wrap: FlexWrap = FlexWrap::NOWRAP, reset;
        FlexGrow "flex-grow" flex_grow: FlexFactor = 0.0, reset;
        FlexShrink "flex-shrink" flex_shrink: FlexFactor = 1.0, reset;
        FlexBasis "flex-basis" flex_basis: FlexBasis<LengthPercentage> = FlexBasis::Size(Size::Auto), reset;
        FlexLineCount "flex-line-count" flex_line_count: FlexLineCount = 1, reset;
        Order "order" order: Order = 0, reset;
        ReadingFlow "reading-flow" reading_flow: ReadingFlow = ReadingFlow::Normal, reset;
        ReadingOrder "reading-order" reading_order: Order = 0, reset;
        // Grid layout
        GridTemplateColumns "grid-template-columns" grid_template_columns: GridTemplate<LengthPercentage> = GridTemplate::None, reset;
        GridTemplateRows "grid-template-rows" grid_template_rows: GridTemplate<LengthPercentage> = GridTemplate::None, reset;
        GridTemplateAreas "grid-template-areas" grid_template_areas: GridAreas = GridAreas::default(), reset;
        GridAutoColumns "grid-auto-columns" grid_auto_columns: GridAutoTracks = Arc::from([TrackSize::Breadth(TrackBreadth::Auto)]), reset;
        GridAutoRows "grid-auto-rows" grid_auto_rows: GridAutoTracks = Arc::from([TrackSize::Breadth(TrackBreadth::Auto)]), reset;
        GridAutoFlow "grid-auto-flow" grid_auto_flow: GridAutoFlow = GridAutoFlow { column: false, dense: false }, reset;
        GridRowStart "grid-row-start" grid_row_start: GridLine = GridLine::Auto, reset;
        GridRowEnd "grid-row-end" grid_row_end: GridLine = GridLine::Auto, reset;
        GridColumnStart "grid-column-start" grid_column_start: GridLine = GridLine::Auto, reset;
        GridColumnEnd "grid-column-end" grid_column_end: GridLine = GridLine::Auto, reset;
        // Multi-column layout
        ColumnCount "column-count" column_count: ColumnCount = None, reset;
        ColumnWidth "column-width" column_width: ColumnLength = None, reset;
        ColumnHeight "column-height" column_height: ColumnLength = None, reset;
        ColumnFill "column-fill" column_fill: ColumnFill = ColumnFill::Balance, reset;
        ColumnSpan "column-span" column_span: ColumnSpan = ColumnSpan::None, reset;
        ColumnWrap "column-wrap" column_wrap: ColumnWrap = ColumnWrap::Auto, reset;
        ColumnRuleWidth "column-rule-width" column_rule_width: BorderWidth = 3.0, reset;
        ColumnRuleStyle "column-rule-style" column_rule_style: BorderStyle = BorderStyle::None, reset;
        ColumnRuleColor "column-rule-color" column_rule_color: ColorValue = crate::values::Color::CurrentColor, reset;
        // Anchor positioning
        AnchorName "anchor-name" anchor_name: AnchorName = Arc::from([]), reset;
        AnchorScope "anchor-scope" anchor_scope: AnchorScope = AnchorScope::None, reset;
        PositionAnchor "position-anchor" position_anchor: PositionAnchor = PositionAnchor::Auto, reset;
        PositionArea "position-area" position_area: PositionAreaValue = None, reset;
        PositionTryFallbacks "position-try-fallbacks" position_try_fallbacks: PositionTryFallbacks = Arc::from([]), reset;
        PositionTryOrder "position-try-order" position_try_order: PositionTryOrder = PositionTryOrder::Normal, reset;
        PositionVisibility "position-visibility" position_visibility: PositionVisibility = PositionVisibility::INITIAL, reset;
        // Form controls
        AccentColor "accent-color" accent_color: AccentColor = AccentColor::Auto, inherited;
        Appearance "appearance" appearance: Appearance = Appearance::None, reset;
        FieldSizing "field-sizing" field_sizing: FieldSizing = FieldSizing::Fixed, reset;
        // Border images, corner shapes and border shapes
        BorderImageSource "border-image-source" border_image_source: ListStyleImage = ImageValue::None, reset;
        BorderImageSlice "border-image-slice" border_image_slice: BorderImageSlice = ImageSlice { offsets: [NumberOrPercentage::Percentage(1.0); 4], fill: false }, reset;
        BorderImageWidth "border-image-width" border_image_width: BorderImageWidth = ImageSideWidth::numbers(1.0), reset;
        BorderImageOutset "border-image-outset" border_image_outset: BorderImageOutset = ImageSideWidth::numbers(0.0), reset;
        BorderImageRepeat "border-image-repeat" border_image_repeat: BorderImageRepeat = (ImageRepeat::Stretch, ImageRepeat::Stretch), reset;
        CornerTopLeftShape "corner-top-left-shape" corner_top_left_shape: CornerShape = CornerShape::Round, reset;
        CornerTopRightShape "corner-top-right-shape" corner_top_right_shape: CornerShape = CornerShape::Round, reset;
        CornerBottomRightShape "corner-bottom-right-shape" corner_bottom_right_shape: CornerShape = CornerShape::Round, reset;
        CornerBottomLeftShape "corner-bottom-left-shape" corner_bottom_left_shape: CornerShape = CornerShape::Round, reset;
        BorderShape "border-shape" border_shape: BorderShape = Arc::from([]), reset;
        BoxShadow "box-shadow" box_shadow: BoxShadow = Arc::from([]), reset;
        // Compositing and filters
        Opacity "opacity" opacity: Opacity = 1.0, reset;
        MixBlendMode "mix-blend-mode" mix_blend_mode: BlendMode = BlendMode::Normal, reset;
        BackgroundBlendMode "background-blend-mode" background_blend_mode: BackgroundBlendMode = Arc::from([BlendMode::Normal]), reset;
        Isolation "isolation" isolation: Isolation = Isolation::Auto, reset;
        Filter "filter" filter: FilterList = Arc::from([]), reset;
        BackdropFilter "backdrop-filter" backdrop_filter: FilterList = Arc::from([]), reset;
        // Clipping and masking
        Clip "clip" clip: Clip = None, reset;
        ClipPath "clip-path" clip_path: ClipPath<LengthPercentage> = ClipPath::None, reset;
        MaskImage "mask-image" mask_image: BackgroundImage = Arc::from([ImageValue::None]), reset;
        MaskMode "mask-mode" mask_mode: MaskMode = Arc::from([MaskingMode::MatchSource]), reset;
        MaskRepeat "mask-repeat" mask_repeat: BackgroundRepeat = Arc::from([(Repeat::Repeat, Repeat::Repeat)]), reset;
        MaskPosition "mask-position" mask_position: MaskPosition = Arc::from([(PositionComponent::START, PositionComponent::START)]), reset;
        MaskClip "mask-clip" mask_clip: MaskClip = Arc::from([MaskClipBox::BorderBox]), reset;
        MaskOrigin "mask-origin" mask_origin: MaskOrigin = Arc::from([CoordBox::BorderBox]), reset;
        MaskSize "mask-size" mask_size: BackgroundSizeList = Arc::from([BackgroundSize::Explicit(None, None)]), reset;
        MaskComposite "mask-composite" mask_composite: MaskComposite = Arc::from([CompositingOperator::Add]), reset;
        MaskType "mask-type" mask_type: MaskType = MaskType::Luminance, reset;
        MaskBorderSource "mask-border-source" mask_border_source: ListStyleImage = ImageValue::None, reset;
        MaskBorderSlice "mask-border-slice" mask_border_slice: BorderImageSlice = ImageSlice { offsets: [NumberOrPercentage::Number(0.0); 4], fill: false }, reset;
        MaskBorderWidth "mask-border-width" mask_border_width: BorderImageWidth = ImageSideWidth::auto(), reset;
        MaskBorderOutset "mask-border-outset" mask_border_outset: BorderImageOutset = ImageSideWidth::numbers(0.0), reset;
        MaskBorderRepeat "mask-border-repeat" mask_border_repeat: BorderImageRepeat = (ImageRepeat::Stretch, ImageRepeat::Stretch), reset;
        MaskBorderMode "mask-border-mode" mask_border_mode: MaskType = MaskType::Alpha, reset;
        // Transforms
        Transform "transform" transform: Transform = Arc::from([]), reset;
        Translate "translate" translate: Translate = None, reset;
        Rotate "rotate" rotate: Rotate = None, reset;
        Scale "scale" scale: Scale = None, reset;
        TransformOrigin "transform-origin" transform_origin: TransformOrigin = (LengthPercentage::Percentage(0.5), LengthPercentage::Percentage(0.5), 0.0), reset;
        TransformBox "transform-box" transform_box: TransformBox = TransformBox::ViewBox, reset;
        TransformStyle "transform-style" transform_style: TransformStyle = TransformStyle::Flat, reset;
        Perspective "perspective" perspective: Perspective = None, reset;
        PerspectiveOrigin "perspective-origin" perspective_origin: ObjectPosition = (PositionComponent { from_end: false, offset: LengthPercentage::Percentage(0.5) }, PositionComponent { from_end: false, offset: LengthPercentage::Percentage(0.5) }), reset;
        BackfaceVisibility "backface-visibility" backface_visibility: BackfaceVisibility = BackfaceVisibility::Visible, reset;
        // Motion path
        OffsetPath "offset-path" offset_path: OffsetPath<LengthPercentage, PositionComponent> = OffsetPath::None, reset;
        OffsetDistance "offset-distance" offset_distance: LengthPercentageValue = LengthPercentage::ZERO, reset;
        OffsetPosition "offset-position" offset_position: OffsetPosition<PositionComponent> = OffsetPosition::Normal, reset;
        OffsetAnchor "offset-anchor" offset_anchor: OffsetAnchor = None, reset;
        OffsetRotate "offset-rotate" offset_rotate: OffsetRotate = OffsetRotate::AUTO, reset;
        // Containment and container queries
        Contain "contain" contain: Contain = Contain::NONE, reset;
        ContentVisibility "content-visibility" content_visibility: ContentVisibility = ContentVisibility::Visible, reset;
        ContainerType "container-type" container_type: ContainerType = ContainerType::NORMAL, reset;
        ContainerName "container-name" container_name: ContainerName = Arc::from([]), reset;
        // Animations, at time zero
        AnimationName "animation-name" animation_name: AnimationNames = Arc::from([AnimationName::None]), reset;
        AnimationDuration "animation-duration" animation_duration: AnimationDurations = Arc::from([Some(0.0)]), reset;
        AnimationTimingFunction "animation-timing-function" animation_timing_function: TimingFunctions = Arc::from([Easing::Ease]), reset;
        AnimationDelay "animation-delay" animation_delay: Times = Arc::from([0.0]), reset;
        AnimationIterationCount "animation-iteration-count" animation_iteration_count: IterationCounts = Arc::from([1.0]), reset;
        AnimationDirection "animation-direction" animation_direction: AnimationDirections = Arc::from([AnimationDirection::Normal]), reset;
        AnimationFillMode "animation-fill-mode" animation_fill_mode: AnimationFillModes = Arc::from([AnimationFillMode::None]), reset;
        AnimationPlayState "animation-play-state" animation_play_state: AnimationPlayStates = Arc::from([AnimationPlayState::Running]), reset;
        AnimationComposition "animation-composition" animation_composition: AnimationCompositions = Arc::from([AnimationComposition::Replace]), reset;
        AnimationTimeline "animation-timeline" animation_timeline: AnimationTimelines = Arc::from([AnimationTimeline::Auto]), reset;
        AnimationTrigger "animation-trigger" animation_trigger: AnimationTriggers = Arc::from([TriggerItem::from([])]), reset;
        AnimationRangeStart "animation-range-start" animation_range_start: RangeStarts = Arc::from([RangePoint::Normal]), reset;
        AnimationRangeEnd "animation-range-end" animation_range_end: RangeEnds = Arc::from([RangePoint::Normal]), reset;
        TimelineTriggerName "timeline-trigger-name" timeline_trigger_name: DashedNames = Arc::from([]), reset;
        TimelineTriggerSource "timeline-trigger-source" timeline_trigger_source: AnimationTimelines = Arc::from([AnimationTimeline::Auto]), reset;
        TimelineTriggerActivationRangeStart "timeline-trigger-activation-range-start" timeline_trigger_activation_range_start: RangeStarts = Arc::from([RangePoint::Normal]), reset;
        TimelineTriggerActivationRangeEnd "timeline-trigger-activation-range-end" timeline_trigger_activation_range_end: RangeEnds = Arc::from([RangePoint::Normal]), reset;
        TimelineTriggerActiveRangeStart "timeline-trigger-active-range-start" timeline_trigger_active_range_start: ActiveRangeStarts = Arc::from([RangePoint::Auto]), reset;
        TimelineTriggerActiveRangeEnd "timeline-trigger-active-range-end" timeline_trigger_active_range_end: ActiveRangeEnds = Arc::from([RangePoint::Auto]), reset;
        TriggerScope "trigger-scope" trigger_scope: TriggerScope = TriggerScope::None, reset;
        // Transitions, which never run
        TransitionProperty "transition-property" transition_property: TransitionProperties = Arc::from([TransitionProperty::All]), reset;
        TransitionDuration "transition-duration" transition_duration: NonNegativeTimes = Arc::from([0.0]), reset;
        TransitionTimingFunction "transition-timing-function" transition_timing_function: TimingFunctions = Arc::from([Easing::Ease]), reset;
        TransitionDelay "transition-delay" transition_delay: Times = Arc::from([0.0]), reset;
        TransitionBehavior "transition-behavior" transition_behavior: TransitionBehaviors = Arc::from([TransitionBehavior::Normal]), reset;
        // Scroll-driven animations
        ScrollTimelineName "scroll-timeline-name" scroll_timeline_name: TimelineNames = Arc::from([TimelineName(None)]), reset;
        ScrollTimelineAxis "scroll-timeline-axis" scroll_timeline_axis: TimelineAxes = Arc::from([TimelineAxis::Block]), reset;
        ViewTimelineName "view-timeline-name" view_timeline_name: TimelineNames = Arc::from([TimelineName(None)]), reset;
        ViewTimelineAxis "view-timeline-axis" view_timeline_axis: TimelineAxes = Arc::from([TimelineAxis::Block]), reset;
        ViewTimelineInset "view-timeline-inset" view_timeline_inset: ViewInsets = Arc::from([(InsetSide::Auto, InsetSide::Auto)]), reset;
        TimelineScope "timeline-scope" timeline_scope: DashedNames = Arc::from([]), reset;
        // View transitions
        ViewTransitionName "view-transition-name" view_transition_name: ViewTransitionName = ViewTransitionName::None, reset;
        ViewTransitionClass "view-transition-class" view_transition_class: ViewTransitionClass = Arc::from([]), reset;
        ViewTransitionScope "view-transition-scope" view_transition_scope: ViewTransitionScope = ViewTransitionScope::None, reset;
        // Interaction, without a visual effect
        CaretColor "caret-color" caret_color: CaretColor = AccentColor::Auto, inherited;
        CaretAnimation "caret-animation" caret_animation: CaretAnimation = CaretAnimation::Auto, inherited;
        CaretShape "caret-shape" caret_shape: CaretShape = CaretShape::Auto, inherited;
        Cursor "cursor" cursor: Cursor = Cursor { images: Arc::from([]), kind: CursorKind::Auto }, inherited;
        Interactivity "interactivity" interactivity: Interactivity = Interactivity::Auto, inherited;
        InterestDelayStart "interest-delay-start" interest_delay_start: InterestDelay = None, inherited;
        InterestDelayEnd "interest-delay-end" interest_delay_end: InterestDelay = None, inherited;
        PointerEvents "pointer-events" pointer_events: PointerEvents = PointerEvents::Auto, inherited;
        Resize "resize" resize: Resize = Resize::None, reset;
        UserSelect "user-select" user_select: UserSelect = UserSelect::Auto, reset;
        WindowDrag "window-drag" window_drag: WindowDrag = WindowDrag::None, inherited;
        TouchAction "touch-action" touch_action: TouchAction = TouchAction::Auto, reset;
        WillChange "will-change" will_change: WillChange = Arc::from([]), reset;
        InterpolateSize "interpolate-size" interpolate_size: InterpolateSize = InterpolateSize::NumericOnly, inherited;
        TextSizeAdjust "text-size-adjust" text_size_adjust: TextSizeAdjust = TextSizeAdjust::Auto, inherited;
        SpeakAs "speak-as" speak_as: SpeakAs = SpeakAs { spell_out: false, digits: false, punctuation: None }, inherited;
        // Scrolling, without a visual effect
        ScrollBehavior "scroll-behavior" scroll_behavior: ScrollBehavior = ScrollBehavior::Auto, reset;
        ScrollAxisLock "scroll-axis-lock" scroll_axis_lock: AutoOrNone = AutoOrNone::Auto, reset;
        ScrollInitialTarget "scroll-initial-target" scroll_initial_target: ScrollInitialTarget = ScrollInitialTarget::None, reset;
        ScrollMarkerGroup "scroll-marker-group" scroll_marker_group: ScrollMarkerGroup = ScrollMarkerGroup::None, reset;
        ScrollTargetGroup "scroll-target-group" scroll_target_group: ScrollTargetGroup = ScrollTargetGroup::None, reset;
        ScrollMarginTop "scroll-margin-top" scroll_margin_top: LengthValue = 0.0, reset;
        ScrollMarginRight "scroll-margin-right" scroll_margin_right: LengthValue = 0.0, reset;
        ScrollMarginBottom "scroll-margin-bottom" scroll_margin_bottom: LengthValue = 0.0, reset;
        ScrollMarginLeft "scroll-margin-left" scroll_margin_left: LengthValue = 0.0, reset;
        ScrollPaddingTop "scroll-padding-top" scroll_padding_top: ScrollPadding = None, reset;
        ScrollPaddingRight "scroll-padding-right" scroll_padding_right: ScrollPadding = None, reset;
        ScrollPaddingBottom "scroll-padding-bottom" scroll_padding_bottom: ScrollPadding = None, reset;
        ScrollPaddingLeft "scroll-padding-left" scroll_padding_left: ScrollPadding = None, reset;
        ScrollSnapAlign "scroll-snap-align" scroll_snap_align: ScrollSnapAlign = (SnapAlignment::None, SnapAlignment::None), reset;
        ScrollSnapStop "scroll-snap-stop" scroll_snap_stop: ScrollSnapStop = ScrollSnapStop::Normal, reset;
        ScrollSnapType "scroll-snap-type" scroll_snap_type: ScrollSnapType = None, reset;
        OverscrollBehaviorX "overscroll-behavior-x" overscroll_behavior_x: OverscrollBehavior = OverscrollBehavior::Auto, reset;
        OverscrollBehaviorY "overscroll-behavior-y" overscroll_behavior_y: OverscrollBehavior = OverscrollBehavior::Auto, reset;
        OverflowAnchor "overflow-anchor" overflow_anchor: AutoOrNone = AutoOrNone::Auto, reset;
        Overlay "overlay" overlay: Overlay = Overlay::None, reset;
        ScrollbarColor "scrollbar-color" scrollbar_color: ScrollbarColor = None, inherited;
        ScrollbarWidth "scrollbar-width" scrollbar_width: ScrollbarWidth = ScrollbarWidth::Auto, reset;
        ScrollbarGutter "scrollbar-gutter" scrollbar_gutter: ScrollbarGutter = ScrollbarGutter::Auto, reset;
    }
    logical {
        MarginBlockStart "margin-block-start": MarginValue => [MarginTop, MarginRight, MarginBottom, MarginLeft] BlockStart;
        MarginBlockEnd "margin-block-end": MarginValue => [MarginTop, MarginRight, MarginBottom, MarginLeft] BlockEnd;
        MarginInlineStart "margin-inline-start": MarginValue => [MarginTop, MarginRight, MarginBottom, MarginLeft] InlineStart;
        MarginInlineEnd "margin-inline-end": MarginValue => [MarginTop, MarginRight, MarginBottom, MarginLeft] InlineEnd;
        PaddingBlockStart "padding-block-start": NonNegativeLengthPercentage => [PaddingTop, PaddingRight, PaddingBottom, PaddingLeft] BlockStart;
        PaddingBlockEnd "padding-block-end": NonNegativeLengthPercentage => [PaddingTop, PaddingRight, PaddingBottom, PaddingLeft] BlockEnd;
        PaddingInlineStart "padding-inline-start": NonNegativeLengthPercentage => [PaddingTop, PaddingRight, PaddingBottom, PaddingLeft] InlineStart;
        PaddingInlineEnd "padding-inline-end": NonNegativeLengthPercentage => [PaddingTop, PaddingRight, PaddingBottom, PaddingLeft] InlineEnd;
        BorderBlockStartWidth "border-block-start-width": BorderWidth => [BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth] BlockStart;
        BorderBlockEndWidth "border-block-end-width": BorderWidth => [BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth] BlockEnd;
        BorderInlineStartWidth "border-inline-start-width": BorderWidth => [BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth] InlineStart;
        BorderInlineEndWidth "border-inline-end-width": BorderWidth => [BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth] InlineEnd;
        BorderBlockStartStyle "border-block-start-style": BorderStyle => [BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle] BlockStart;
        BorderBlockEndStyle "border-block-end-style": BorderStyle => [BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle] BlockEnd;
        BorderInlineStartStyle "border-inline-start-style": BorderStyle => [BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle] InlineStart;
        BorderInlineEndStyle "border-inline-end-style": BorderStyle => [BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle] InlineEnd;
        BorderBlockStartColor "border-block-start-color": ColorValue => [BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor] BlockStart;
        BorderBlockEndColor "border-block-end-color": ColorValue => [BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor] BlockEnd;
        BorderInlineStartColor "border-inline-start-color": ColorValue => [BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor] InlineStart;
        BorderInlineEndColor "border-inline-end-color": ColorValue => [BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor] InlineEnd;
        BorderStartStartRadius "border-start-start-radius": BorderRadius => [BorderTopLeftRadius, BorderTopRightRadius, BorderBottomRightRadius, BorderBottomLeftRadius] StartStart;
        BorderStartEndRadius "border-start-end-radius": BorderRadius => [BorderTopLeftRadius, BorderTopRightRadius, BorderBottomRightRadius, BorderBottomLeftRadius] StartEnd;
        BorderEndStartRadius "border-end-start-radius": BorderRadius => [BorderTopLeftRadius, BorderTopRightRadius, BorderBottomRightRadius, BorderBottomLeftRadius] EndStart;
        BorderEndEndRadius "border-end-end-radius": BorderRadius => [BorderTopLeftRadius, BorderTopRightRadius, BorderBottomRightRadius, BorderBottomLeftRadius] EndEnd;
        InlineSize "inline-size": SizeValue => [Width, Height] Inline;
        BlockSize "block-size": SizeValue => [Width, Height] Block;
        MinInlineSize "min-inline-size": SizeValue => [MinWidth, MinHeight] Inline;
        MinBlockSize "min-block-size": SizeValue => [MinWidth, MinHeight] Block;
        MaxInlineSize "max-inline-size": MaxSizeValue => [MaxWidth, MaxHeight] Inline;
        MaxBlockSize "max-block-size": MaxSizeValue => [MaxWidth, MaxHeight] Block;
        ContainIntrinsicInlineSize "contain-intrinsic-inline-size": ContainIntrinsic => [ContainIntrinsicWidth, ContainIntrinsicHeight] Inline;
        ContainIntrinsicBlockSize "contain-intrinsic-block-size": ContainIntrinsic => [ContainIntrinsicWidth, ContainIntrinsicHeight] Block;
        OverflowInline "overflow-inline": Overflow => [OverflowX, OverflowY] Inline;
        OverflowBlock "overflow-block": Overflow => [OverflowX, OverflowY] Block;
        InsetBlockStart "inset-block-start": MarginValue => [Top, Right, Bottom, Left] BlockStart;
        InsetBlockEnd "inset-block-end": MarginValue => [Top, Right, Bottom, Left] BlockEnd;
        InsetInlineStart "inset-inline-start": MarginValue => [Top, Right, Bottom, Left] InlineStart;
        InsetInlineEnd "inset-inline-end": MarginValue => [Top, Right, Bottom, Left] InlineEnd;
        CornerStartStartShape "corner-start-start-shape": CornerShape => [CornerTopLeftShape, CornerTopRightShape, CornerBottomRightShape, CornerBottomLeftShape] StartStart;
        CornerStartEndShape "corner-start-end-shape": CornerShape => [CornerTopLeftShape, CornerTopRightShape, CornerBottomRightShape, CornerBottomLeftShape] StartEnd;
        CornerEndStartShape "corner-end-start-shape": CornerShape => [CornerTopLeftShape, CornerTopRightShape, CornerBottomRightShape, CornerBottomLeftShape] EndStart;
        CornerEndEndShape "corner-end-end-shape": CornerShape => [CornerTopLeftShape, CornerTopRightShape, CornerBottomRightShape, CornerBottomLeftShape] EndEnd;
        ScrollMarginBlockStart "scroll-margin-block-start": LengthValue => [ScrollMarginTop, ScrollMarginRight, ScrollMarginBottom, ScrollMarginLeft] BlockStart;
        ScrollMarginBlockEnd "scroll-margin-block-end": LengthValue => [ScrollMarginTop, ScrollMarginRight, ScrollMarginBottom, ScrollMarginLeft] BlockEnd;
        ScrollMarginInlineStart "scroll-margin-inline-start": LengthValue => [ScrollMarginTop, ScrollMarginRight, ScrollMarginBottom, ScrollMarginLeft] InlineStart;
        ScrollMarginInlineEnd "scroll-margin-inline-end": LengthValue => [ScrollMarginTop, ScrollMarginRight, ScrollMarginBottom, ScrollMarginLeft] InlineEnd;
        ScrollPaddingBlockStart "scroll-padding-block-start": ScrollPadding => [ScrollPaddingTop, ScrollPaddingRight, ScrollPaddingBottom, ScrollPaddingLeft] BlockStart;
        ScrollPaddingBlockEnd "scroll-padding-block-end": ScrollPadding => [ScrollPaddingTop, ScrollPaddingRight, ScrollPaddingBottom, ScrollPaddingLeft] BlockEnd;
        ScrollPaddingInlineStart "scroll-padding-inline-start": ScrollPadding => [ScrollPaddingTop, ScrollPaddingRight, ScrollPaddingBottom, ScrollPaddingLeft] InlineStart;
        ScrollPaddingInlineEnd "scroll-padding-inline-end": ScrollPadding => [ScrollPaddingTop, ScrollPaddingRight, ScrollPaddingBottom, ScrollPaddingLeft] InlineEnd;
        OverscrollBehaviorInline "overscroll-behavior-inline": OverscrollBehavior => [OverscrollBehaviorX, OverscrollBehaviorY] Inline;
        OverscrollBehaviorBlock "overscroll-behavior-block": OverscrollBehavior => [OverscrollBehaviorX, OverscrollBehaviorY] Block;
    }
}

/// Longhands computed before the rest, in this order: later ones depend on earlier ones.
pub(crate) const EARLY: &[LonghandId] = &[
    LonghandId::WritingMode,
    LonghandId::Direction,
    LonghandId::TextOrientation,
    // Zoom scales the absolute lengths of every other property.
    LonghandId::Zoom,
    // The font properties pick the font that font-relative units use.
    LonghandId::FontFamily,
    LonghandId::FontStyle,
    LonghandId::FontWeight,
    LonghandId::FontWidth,
    LonghandId::FontSize,
    LonghandId::LineHeight,
    // The color scheme decides `light-dark()` and system colors.
    LonghandId::ColorScheme,
    LonghandId::Color,
];

/// Which member of its physical group a logical property maps to.
#[derive(Debug, Clone, Copy)]
enum LogicalSide {
    BlockStart,
    BlockEnd,
    InlineStart,
    InlineEnd,
    /// The inline axis of a `[horizontal, vertical]` pair.
    Inline,
    /// The block axis of a `[horizontal, vertical]` pair.
    Block,
    /// Corners of a four-corner group (top-left, top-right, bottom-right, bottom-left), named
    /// by their block side and then their inline side.
    StartStart,
    StartEnd,
    EndStart,
    EndEnd,
}

/// Picks the physical property from `group`: four sides (top, right, bottom, left) or two
/// axes (horizontal, vertical).
fn physical_of(
    group: &[LonghandId],
    side: LogicalSide,
    writing_mode: WritingMode,
    direction: Direction,
) -> LonghandId {
    let vertical = writing_mode.is_vertical();
    let (block_start, block_end, inline_start, inline_end) = logical_sides(writing_mode, direction);
    group[match side {
        LogicalSide::BlockStart => block_start,
        LogicalSide::BlockEnd => block_end,
        LogicalSide::InlineStart => inline_start,
        LogicalSide::InlineEnd => inline_end,
        LogicalSide::Inline => usize::from(vertical),
        LogicalSide::Block => usize::from(!vertical),
        LogicalSide::StartStart => corner(block_start, inline_start),
        LogicalSide::StartEnd => corner(block_start, inline_end),
        LogicalSide::EndStart => corner(block_end, inline_start),
        LogicalSide::EndEnd => corner(block_end, inline_end),
    }]
}

/// The index of the corner where two sides (top 0, right 1, bottom 2, left 3) meet, counting
/// from top-left clockwise.
fn corner(a: usize, b: usize) -> usize {
    let (vertical_side, horizontal_side) = if a.is_multiple_of(2) { (a, b) } else { (b, a) };
    match (vertical_side, horizontal_side) {
        (0, 3) => 0,
        (0, _) => 1,
        (_, 1) => 2,
        _ => 3,
    }
}

/// Indices (top 0, right 1, bottom 2, left 3) of the block-start, block-end, inline-start and
/// inline-end sides.
fn logical_sides(writing_mode: WritingMode, direction: Direction) -> (usize, usize, usize, usize) {
    let (block_start, block_end, inline_start, inline_end) = match writing_mode {
        WritingMode::HorizontalTb => (0, 2, 3, 1),
        WritingMode::VerticalRl | WritingMode::SidewaysRl => (1, 3, 0, 2),
        WritingMode::VerticalLr => (3, 1, 0, 2),
        WritingMode::SidewaysLr => (3, 1, 2, 0),
    };
    if direction == Direction::Rtl {
        (block_start, block_end, inline_end, inline_start)
    } else {
        (block_start, block_end, inline_start, inline_end)
    }
}

/// `initial`, `inherit`, `unset`, `revert` and `revert-layer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssWideKeyword {
    Initial,
    Inherit,
    Unset,
    Revert,
    RevertLayer,
}

impl CssWideKeyword {
    fn parse(value: &str) -> Option<Self> {
        let mut input = Parser::new(value);
        input
            .parse_entirely(|input| {
                let ident = input.expect_ident()?;
                Ok::<_, ParseError<()>>(match &*ident.to_ascii_lowercase() {
                    "initial" => Some(CssWideKeyword::Initial),
                    "inherit" => Some(CssWideKeyword::Inherit),
                    "unset" => Some(CssWideKeyword::Unset),
                    "revert" => Some(CssWideKeyword::Revert),
                    "revert-layer" => Some(CssWideKeyword::RevertLayer),
                    _ => None,
                })
            })
            .ok()
            .flatten()
    }
}

/// A parsed declaration for one longhand or custom property.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyDeclaration {
    Value(SpecifiedValue),
    CssWide(LonghandId, CssWideKeyword),
    /// A value with `var()`, `env()` or `attr()`, parsed after substitution. For a shorthand,
    /// the substituted value is parsed as the shorthand and this longhand taken from it.
    Unparsed {
        id: LonghandId,
        shorthand: Option<ShorthandId>,
        value: Arc<str>,
    },
    /// A custom property. `None` is the guaranteed-invalid value.
    Custom {
        name: Arc<str>,
        value: CustomValue,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CustomValue {
    Value(Arc<str>),
    CssWide(CssWideKeyword),
}

impl PropertyDeclaration {
    /// The longhand this declaration sets, or `None` for custom properties.
    pub fn id(&self) -> Option<LonghandId> {
        match self {
            PropertyDeclaration::Value(value) => Some(value.id()),
            PropertyDeclaration::CssWide(id, _) | PropertyDeclaration::Unparsed { id, .. } => {
                Some(*id)
            }
            PropertyDeclaration::Custom { .. } => None,
        }
    }
}

/// The property a legacy name is an alias of, or the name itself.
pub fn resolve_alias(name: &str) -> &str {
    match name {
        "font-stretch" => "font-width",
        "word-wrap" => "overflow-wrap",
        "grid-row-gap" => "row-gap",
        "grid-column-gap" => "column-gap",
        "grid-gap" => "gap",
        other => other,
    }
}

/// Parses a declaration into longhand (and custom property) declarations. Returns an error
/// message if the property is unknown or the value invalid.
pub fn parse_declaration(name: &str, value: &str) -> Result<Vec<PropertyDeclaration>, String> {
    let name = resolve_alias(name);
    if name.starts_with("--") {
        let value = match CssWideKeyword::parse(value) {
            Some(keyword) => CustomValue::CssWide(keyword),
            None => CustomValue::Value(Arc::from(value)),
        };
        return Ok(vec![PropertyDeclaration::Custom {
            name: Arc::from(name),
            value,
        }]);
    }
    let longhand = LonghandId::from_name(name);
    let shorthand = if longhand.is_none() {
        ShorthandId::from_name(name)
    } else {
        None
    };
    let longhands: Vec<LonghandId> = match (longhand, shorthand) {
        (Some(id), _) => vec![id],
        (None, Some(shorthand)) => shorthand.longhands(),
        (None, None) => return Err(format!("unsupported property `{name}`")),
    };
    if let Some(keyword) = CssWideKeyword::parse(value) {
        return Ok(longhands
            .into_iter()
            .map(|id| PropertyDeclaration::CssWide(id, keyword))
            .collect());
    }
    if has_references(value) {
        if shorthand == Some(ShorthandId::All) {
            return Err("`all` only accepts CSS-wide keywords".to_owned());
        }
        let value: Arc<str> = Arc::from(value);
        return Ok(longhands
            .into_iter()
            .map(|id| PropertyDeclaration::Unparsed {
                id,
                shorthand,
                value: value.clone(),
            })
            .collect());
    }
    let parsed = match (longhand, shorthand) {
        (Some(id), _) => parse_value(value, |input| id.parse(input)).map(|v| vec![v]),
        (None, Some(shorthand)) => parse_value(value, |input| shorthand.parse(input)),
        (None, None) => unreachable!("handled above"),
    };
    parsed
        .map(|values| values.into_iter().map(PropertyDeclaration::Value).collect())
        .map_err(|()| format!("invalid value for `{name}`: `{value}`"))
}

fn parse_value<T>(
    value: &str,
    parse: impl FnOnce(&mut Parser<'_>) -> Result<T, ParseError<()>>,
) -> Result<T, ()> {
    let mut input = Parser::new(value);
    input.parse_entirely(parse).map_err(|_| ())
}

/// Parses a substituted value for `id`, as a longhand or from `shorthand`.
pub(crate) fn parse_substituted(
    id: LonghandId,
    shorthand: Option<ShorthandId>,
    value: &str,
) -> Option<SpecifiedValue> {
    if let Some(keyword) = CssWideKeyword::parse(value) {
        let _ = keyword;
        return None;
    }
    match shorthand {
        None => parse_value(value, |input| id.parse(input)).ok(),
        Some(shorthand) => parse_value(value, |input| shorthand.parse(input))
            .ok()?
            .into_iter()
            .find(|v| v.id() == id),
    }
}

impl ComputedValues {
    /// The style of an anonymous box: inherited properties from `parent`, initial values for
    /// the rest.
    pub fn inherit_from(parent: &ComputedValues) -> Self {
        let mut style = Self::initial();
        for &id in LonghandId::ALL {
            if !id.is_logical() && id.inherited() {
                style.copy(id, parent);
            }
        }
        style.custom = parent.custom.clone();
        style.font_rules = parent.font_rules.clone();
        style.effective_zoom = parent.effective_zoom;
        // Borders and outlines keep their initial `none` style, so they compute to no width.
        style.border_top_width = 0.0;
        style.border_right_width = 0.0;
        style.border_bottom_width = 0.0;
        style.border_left_width = 0.0;
        style.outline_width = 0.0;
        style.column_rule_width = 0.0;
        style
    }

    /// This style with its physical box properties turned into the line-relative axes of
    /// `writing_mode`, where lines run left to right and stack from top to bottom: `top` is
    /// the block-start side, `left` the line-left side, `width` the inline size, and so on.
    /// Layout in vertical writing modes runs in these axes; `unrotated` keeps this style.
    pub fn in_line_axes(self: &Arc<Self>, writing_mode: WritingMode) -> Arc<ComputedValues> {
        if !writing_mode.is_vertical() {
            return self.clone();
        }
        let (block_start, block_end, line_left, line_right) =
            logical_sides(writing_mode, Direction::Ltr);
        // The physical side each line-relative side (top, right, bottom, left) comes from.
        let from = [block_start, line_right, block_end, line_left];
        let mut style = (**self).clone();
        macro_rules! sides {
            ($($top:ident, $right:ident, $bottom:ident, $left:ident;)+) => {$(
                let physical = [
                    self.$top.clone(),
                    self.$right.clone(),
                    self.$bottom.clone(),
                    self.$left.clone(),
                ];
                style.$top = physical[from[0]].clone();
                style.$right = physical[from[1]].clone();
                style.$bottom = physical[from[2]].clone();
                style.$left = physical[from[3]].clone();
            )+};
        }
        sides! {
            margin_top, margin_right, margin_bottom, margin_left;
            padding_top, padding_right, padding_bottom, padding_left;
            border_top_width, border_right_width, border_bottom_width, border_left_width;
            border_top_style, border_right_style, border_bottom_style, border_left_style;
            border_top_color, border_right_color, border_bottom_color, border_left_color;
            top, right, bottom, left;
        }
        // Corner radii: each corner's horizontal and vertical radii trade places.
        let radii = [
            self.border_top_left_radius.clone(),
            self.border_top_right_radius.clone(),
            self.border_bottom_right_radius.clone(),
            self.border_bottom_left_radius.clone(),
        ];
        let radius = |a: usize, b: usize| {
            let (horizontal, vertical) = radii[corner(from[a], from[b])].clone();
            (vertical, horizontal)
        };
        style.border_top_left_radius = radius(0, 3);
        style.border_top_right_radius = radius(0, 1);
        style.border_bottom_right_radius = radius(2, 1);
        style.border_bottom_left_radius = radius(2, 3);
        let shapes = [
            self.corner_top_left_shape,
            self.corner_top_right_shape,
            self.corner_bottom_right_shape,
            self.corner_bottom_left_shape,
        ];
        let shape = |a: usize, b: usize| shapes[corner(from[a], from[b])];
        style.corner_top_left_shape = shape(0, 3);
        style.corner_top_right_shape = shape(0, 1);
        style.corner_bottom_right_shape = shape(2, 1);
        style.corner_bottom_left_shape = shape(2, 3);
        std::mem::swap(&mut style.width, &mut style.height);
        std::mem::swap(&mut style.min_width, &mut style.min_height);
        std::mem::swap(&mut style.max_width, &mut style.max_height);
        std::mem::swap(&mut style.overflow_x, &mut style.overflow_y);
        std::mem::swap(
            &mut style.contain_intrinsic_width,
            &mut style.contain_intrinsic_height,
        );
        style.border_spacing = (self.border_spacing.1, self.border_spacing.0);
        style.aspect_ratio.ratio = self.aspect_ratio.ratio.map(|(w, h)| (h, w));
        style.unrotated = Some(self.clone());
        Arc::new(style)
    }
}

impl ComputedValues {
    /// The containment in effect: `contain`, with the size and style containment of a size
    /// container, and the containment `content-visibility` adds. Every box is relevant to
    /// the reader of a PDF, so `auto` adds no size containment (CSS Containment 2 § 4).
    pub fn used_contain(&self) -> Contain {
        let size = match self.container_type.size {
            Some(ContainerSize::Both) => Contain {
                size: true,
                style: true,
                ..Contain::NONE
            },
            Some(ContainerSize::Inline) => Contain {
                inline_size: true,
                style: true,
                ..Contain::NONE
            },
            None => Contain::NONE,
        };
        let visibility = match self.content_visibility {
            ContentVisibility::Hidden => Contain::STRICT,
            ContentVisibility::Auto => Contain::CONTENT,
            ContentVisibility::Visible => Contain::NONE,
        };
        self.contain.union(size).union(visibility)
    }

    /// The content-box size a box with size containment in the block axis takes when its
    /// block size is `auto`, from `contain-intrinsic-height` in line-relative styles.
    pub fn contained_height(&self) -> Option<f32> {
        (self.size_containable() && self.used_contain().size && self.height == Size::Auto)
            .then(|| self.contain_intrinsic_height.length.unwrap_or(0.0))
    }

    /// The min- and max-content inline size of a box with size containment in the inline
    /// axis: `contain-intrinsic-width` in line-relative styles.
    pub fn contained_width(&self) -> Option<f32> {
        let contain = self.used_contain();
        (self.size_containable() && (contain.size || contain.inline_size))
            .then(|| self.contain_intrinsic_width.length.unwrap_or(0.0))
    }

    /// Whether size containment applies: not to the internal boxes of tables and ruby
    /// (CSS Containment 2 § 3.1).
    fn size_containable(&self) -> bool {
        !matches!(self.display, Display::Internal(_))
    }

    /// The style of content on the first line of `origin`, an element with the `::first-line`
    /// style `first_line`: the inherited properties that `::first-line` changes replace this
    /// style's values where they came from `origin` (CSS Pseudo 4 § 2.3).
    pub fn on_first_line(&self, origin: &ComputedValues, first_line: &ComputedValues) -> Self {
        let mut style = self.clone();
        for &id in LonghandId::ALL {
            if !id.is_logical()
                && id.inherited()
                && !origin.same(id, first_line)
                && self.same(id, origin)
            {
                style.copy(id, first_line);
            }
        }
        style
    }

    /// A property's computed value as CSS text, by name. Custom properties return their
    /// substituted value; unknown properties return `None`.
    pub fn get_property(&self, name: &str) -> Option<String> {
        if name.starts_with("--") {
            return self.custom.get(name).map(|value| value.replace("/**/", ""));
        }
        LonghandId::from_name(resolve_alias(name)).map(|id| self.serialize(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(declarations: &[PropertyDeclaration]) -> Vec<&'static str> {
        declarations
            .iter()
            .filter_map(|d| d.id())
            .map(LonghandId::name)
            .collect()
    }

    #[test]
    fn longhands_parse() {
        assert!(parse_declaration("color", "red").is_ok());
        assert!(parse_declaration("width", "calc(50% - 1em)").is_ok());
        assert!(parse_declaration("width", "-1px").is_err());
        assert!(parse_declaration("margin-top", "-1px").is_ok());
        assert!(parse_declaration("display", "inline flow-root").is_ok());
        assert!(parse_declaration("display", "list-item table").is_err());
        assert!(parse_declaration("font-weight", "1001").is_err());
        assert!(
            parse_declaration(
                "font-family",
                "\"Helvetica Neue\", Arial Narrow, sans-serif"
            )
            .is_ok()
        );
        assert!(parse_declaration("font-family", "inherit, serif").is_err());
        assert!(
            parse_declaration("made-up", "1")
                .unwrap_err()
                .contains("unsupported")
        );
    }

    #[test]
    fn shorthands_expand() {
        let margin = parse_declaration("margin", "1px 2px").unwrap();
        assert_eq!(
            ids(&margin),
            ["margin-top", "margin-right", "margin-bottom", "margin-left"]
        );
        let border = parse_declaration("border", "1px solid red").unwrap();
        // Four sides of width, style and color, and the border image reset.
        assert_eq!(border.len(), 17);
        assert!(parse_declaration("border", "solid solid").is_err());
    }

    #[test]
    fn css_wide_keywords_and_references() {
        let unset = parse_declaration("padding", "UNSET").unwrap();
        assert!(
            unset
                .iter()
                .all(|d| matches!(d, PropertyDeclaration::CssWide(_, CssWideKeyword::Unset)))
        );
        let unparsed = parse_declaration("margin", "var(--m)").unwrap();
        assert_eq!(unparsed.len(), 4);
        assert!(matches!(
            unparsed[0],
            PropertyDeclaration::Unparsed {
                shorthand: Some(ShorthandId::Margin),
                ..
            }
        ));
        let all = parse_declaration("all", "initial").unwrap();
        assert!(!ids(&all).contains(&"direction"));
        assert!(parse_declaration("all", "red").is_err());
    }

    #[test]
    fn flex_and_alignment_shorthands() {
        let flex = |value: &str| -> Option<(f32, f32, FlexBasis<SpecifiedLengthPercentage>)> {
            let values = parse_declaration("flex", value).ok()?;
            let mut out = (0.0, 0.0, FlexBasis::Content);
            for value in values {
                match value {
                    PropertyDeclaration::Value(SpecifiedValue::FlexGrow(v)) => out.0 = v,
                    PropertyDeclaration::Value(SpecifiedValue::FlexShrink(v)) => out.1 = v,
                    PropertyDeclaration::Value(SpecifiedValue::FlexBasis(v)) => out.2 = v,
                    _ => return None,
                }
            }
            Some(out)
        };
        let zero = FlexBasis::Size(Size::Value(SpecifiedLengthPercentage::Percentage(0.0)));
        assert_eq!(flex("2"), Some((2.0, 1.0, zero.clone())));
        assert_eq!(flex("none"), Some((0.0, 0.0, FlexBasis::Size(Size::Auto))));
        assert_eq!(flex("auto"), Some((1.0, 1.0, FlexBasis::Size(Size::Auto))));
        assert_eq!(flex("1 0"), Some((1.0, 0.0, zero)));
        // A unitless zero after two factors is the basis.
        let zero_px = FlexBasis::Size(Size::Value(SpecifiedLengthPercentage::Length(
            crate::values::Length::px(0.0),
        )));
        assert_eq!(flex("1 2 0"), Some((1.0, 2.0, zero_px)));
        assert_eq!(flex("content 3").map(|f| f.0), Some(3.0));
        assert!(flex("1 2 3").is_none());
        assert!(flex("none 1").is_none());

        assert!(parse_declaration("flex-wrap", "balance").is_ok());
        assert!(parse_declaration("flex-wrap", "wrap-reverse balance").is_ok());
        assert!(parse_declaration("flex-wrap", "nowrap balance").is_err());
        assert!(parse_declaration("flex-flow", "balance column wrap").is_ok());
        assert!(parse_declaration("flex-flow", "row column").is_err());

        let place = parse_declaration("place-content", "last baseline").unwrap();
        assert!(matches!(
            place[1],
            PropertyDeclaration::Value(SpecifiedValue::JustifyContent(
                ContentAlignment::Position {
                    position: AlignPosition::Start,
                    ..
                }
            ))
        ));
        assert!(parse_declaration("justify-content", "baseline").is_err());
        assert!(parse_declaration("align-content", "left").is_err());
        assert!(parse_declaration("justify-items", "center legacy").is_ok());
        assert!(parse_declaration("justify-items", "legacy start").is_err());
        assert!(parse_declaration("align-self", "safe self-end").is_ok());
        assert!(parse_declaration("align-items", "auto").is_err());
        assert!(parse_declaration("gap", "1px 2%").is_ok());
        assert!(parse_declaration("row-gap", "-1px").is_err());
    }

    #[test]
    fn grid_values() {
        let ok = |name: &str, value: &str| parse_declaration(name, value).is_ok();
        let columns = "grid-template-columns";
        assert!(ok(
            columns,
            "[a] 100px [b c] 1fr minmax(10px, auto) fit-content(20%)"
        ));
        assert!(ok(columns, "repeat(3, [x] 1fr) 10px"));
        assert!(ok(
            columns,
            "repeat(3, [x] 10%) repeat(auto-fill, 50px) 10px"
        ));
        assert!(!ok(columns, "repeat(3, 1fr) repeat(auto-fill, 50px)"));
        assert!(ok(columns, "subgrid [a] repeat(auto-fill, [b] [c])"));
        // Repeating to fill the space needs fixed sizes, everywhere in the list.
        assert!(!ok(columns, "repeat(auto-fill, 1fr)"));
        assert!(!ok(columns, "repeat(auto-fit, 10px) auto"));
        assert!(!ok(
            columns,
            "repeat(auto-fill, 10px) repeat(auto-fit, 10px)"
        ));
        assert!(!ok(columns, "minmax(1fr, 10px)"));
        assert!(!ok(columns, "[a] [b] 10px"));
        assert!(!ok(columns, "-1px"));
        assert!(ok("grid-row-start", "span 2 a"));
        assert!(ok("grid-row-start", "-1"));
        assert!(!ok("grid-row-start", "0"));
        assert!(!ok("grid-row-start", "span -1"));
        assert!(!ok("grid-row-start", "span"));
        assert!(!ok("grid-row-start", "auto 1"));
        assert!(ok("grid-template-areas", "'a a b' 'a a c'"));
        assert!(!ok("grid-template-areas", "'a b a'"));
        assert!(!ok("grid-template-areas", "'a b' 'c'"));
        assert!(ok(
            "grid-template",
            "[top] 'a b' 20px [mid] 'c d' / 1fr 2fr"
        ));
        assert!(ok("grid-template", "100px auto / repeat(2, 1fr)"));
        assert!(ok("grid", "auto-flow dense 30px / 1fr 1fr"));
        assert!(ok("grid", "100px / auto-flow 20px"));
        assert!(!ok("grid", "auto-flow / auto-flow"));
        assert!(ok("grid-area", "a / 2 / span 3 / b"));
        assert!(ok("grid-gap", "1px 2px"));
        assert!(ok("grid-auto-flow", "dense column"));
    }

    #[test]
    fn logical_mapping() {
        use LonghandId::{
            MarginBlockStart, MarginInlineStart, MarginLeft, MarginRight, MarginTop, PaddingBottom,
            PaddingInlineEnd, PaddingInlineStart, Width,
        };
        let map = |id: LonghandId, wm, dir| id.to_physical(wm, dir);
        assert_eq!(
            map(MarginInlineStart, WritingMode::HorizontalTb, Direction::Ltr),
            MarginLeft
        );
        assert_eq!(
            map(MarginInlineStart, WritingMode::HorizontalTb, Direction::Rtl),
            MarginRight
        );
        assert_eq!(
            map(MarginBlockStart, WritingMode::VerticalRl, Direction::Ltr),
            MarginRight
        );
        assert_eq!(
            map(PaddingInlineEnd, WritingMode::VerticalLr, Direction::Ltr),
            PaddingBottom
        );
        assert_eq!(
            map(PaddingInlineStart, WritingMode::SidewaysLr, Direction::Ltr),
            PaddingBottom
        );
        assert_eq!(map(Width, WritingMode::VerticalRl, Direction::Ltr), Width);
        assert!(MarginBlockStart.is_logical() && !MarginTop.is_logical());
    }

    #[test]
    fn styles_turn_into_line_axes() {
        let px = |v: f32| MaybeAuto::Value(LengthPercentage::Length(v));
        let mut style = ComputedValues::initial();
        style.width = Size::Value(LengthPercentage::Length(10.0));
        style.margin_top = px(1.0);
        style.margin_right = px(2.0);
        style.margin_bottom = px(3.0);
        style.margin_left = px(4.0);
        let style = Arc::new(style);
        // In vertical-rl, the block-start side is the right and line-left is the top.
        let rl = style.in_line_axes(WritingMode::VerticalRl);
        assert_eq!(rl.height, style.width);
        assert_eq!(rl.width, Size::Auto);
        assert_eq!(
            [
                &rl.margin_top,
                &rl.margin_right,
                &rl.margin_bottom,
                &rl.margin_left
            ],
            [&px(2.0), &px(3.0), &px(4.0), &px(1.0)]
        );
        assert_eq!(rl.unrotated.as_deref(), Some(&*style));
        // In vertical-lr, the block-start side is the left.
        let lr = style.in_line_axes(WritingMode::VerticalLr);
        assert_eq!(lr.margin_top, px(4.0));
        assert_eq!(lr.margin_left, px(1.0));
        // In sideways-lr, line-left is the bottom.
        let slr = style.in_line_axes(WritingMode::SidewaysLr);
        assert_eq!(slr.margin_left, px(3.0));
        assert!(Arc::ptr_eq(
            &style.in_line_axes(WritingMode::HorizontalTb),
            &style
        ));
    }
}
