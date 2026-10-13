//! Value types of the font, inline, text, text decoration and text painting properties.

use std::sync::Arc;

use cssparser::{ParseError, Parser};

use super::*;

/// Parses a `<string>`.
fn parse_string(input: &mut Parser<'_>) -> Result<Arc<str>> {
    Ok(Arc::from(&*input.expect_string_cloned()?))
}

fn string_css(value: &str) -> String {
    let mut out = String::new();
    let _ = cssparser::serialize_string(value, &mut out);
    out
}

/// Parses an `<integer>`, including math functions, of at least `min`.
pub fn parse_integer(input: &mut Parser<'_>, min: i32) -> Result<i32> {
    let value = match input.try_parse(|input| input.expect_integer()) {
        Ok(value) => value,
        Err(_) => {
            let number = parse_number(input)?;
            number.round() as i32
        }
    };
    if value < min {
        return fail();
    }
    Ok(value)
}

/// Whether the next token is the identifier `keyword`, consuming it if so.
fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

/// The next identifier, lowercased, if there is one.
fn ident(input: &mut Parser<'_>) -> Option<String> {
    input
        .try_parse(|input| input.expect_ident_cloned())
        .ok()
        .map(|ident| ident.to_ascii_lowercase())
}

/// Parses `a || b || ...`: identifiers in any order, each at most once. `accept` returns
/// whether it took the keyword; at least one must be taken.
fn any_order(input: &mut Parser<'_>, mut accept: impl FnMut(&str) -> bool) -> Result<()> {
    let mut count = 0;
    loop {
        let state = input.state();
        match ident(input) {
            Some(word) if accept(&word) => count += 1,
            _ => {
                input.reset(&state);
                break;
            }
        }
    }
    if count == 0 { fail() } else { Ok(()) }
}

/// Sets `slot` to `value` if it is empty; returns whether it was.
fn once<T>(slot: &mut Option<T>, value: T) -> bool {
    if slot.is_some() {
        return false;
    }
    *slot = Some(value);
    true
}

fn flag(slot: &mut bool) -> bool {
    if *slot {
        return false;
    }
    *slot = true;
    true
}

/// Joins the non-empty words, or returns `empty` when there are none.
fn words(parts: &[&str], empty: &str) -> String {
    let parts: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        empty.into()
    } else {
        parts.join(" ")
    }
}

/// A property whose specified value is also its computed value.
macro_rules! as_specified {
    ($name:ident, $value:ty, $parse:expr, $css:expr) => {
        impl Property for $name {
            type Specified = $value;
            type Computed = $value;

            fn to_css(computed: &Self::Computed) -> String {
                #[allow(clippy::redundant_closure_call)]
                ($css)(computed)
            }

            fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
                #[allow(clippy::redundant_closure_call)]
                ($parse)(input)
            }

            fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
                specified.clone()
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// Fonts

keywords!(FontKerning { Auto = "auto", Normal = "normal", None = "none" });

keywords!(
    /// `font-synthesis-weight`, `font-synthesis-small-caps` and `font-synthesis-position`.
    Synthesis { Auto = "auto", None = "none" }
);

keywords!(FontSynthesisStyle { Auto = "auto", None = "none", ObliqueOnly = "oblique-only" });

keywords!(FontVariantPosition { Normal = "normal", Sub = "sub", Super = "super" });

/// One `font-feature-settings` entry: an OpenType feature tag and its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeatureSetting {
    pub tag: [u8; 4],
    pub value: u32,
}

/// `font-feature-settings`: `normal | <feature-tag-value>#`.
pub struct FontFeatureSettings;

impl Property for FontFeatureSettings {
    type Specified = Arc<[FeatureSetting]>;
    type Computed = Arc<[FeatureSetting]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "normal".into();
        }
        computed
            .iter()
            .map(|setting| {
                let tag = string_css(&String::from_utf8_lossy(&setting.tag));
                if setting.value == 1 {
                    tag
                } else {
                    format!("{tag} {}", setting.value)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "normal") {
            return Ok(Arc::from([]));
        }
        let settings = input.parse_comma_separated(|input| -> Result<FeatureSetting> {
            let tag = input.expect_string_cloned()?;
            let bytes = tag.as_bytes();
            if bytes.len() != 4 || !bytes.iter().all(|b| (0x20..=0x7e).contains(b)) {
                return fail();
            }
            let value = if keyword(input, "on") {
                1
            } else if keyword(input, "off") {
                0
            } else {
                input
                    .try_parse(|input| parse_integer(input, 0))
                    .map_or(1, |v| v as u32)
            };
            Ok(FeatureSetting {
                tag: [bytes[0], bytes[1], bytes[2], bytes[3]],
                value,
            })
        })?;
        Ok(settings.into())
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

/// `font-language-override`: `normal | <string>`.
pub struct FontLanguageOverride;

as_specified!(
    FontLanguageOverride,
    Option<Arc<str>>,
    |input: &mut Parser<'_>| {
        if keyword(input, "normal") {
            return Ok(None);
        }
        Ok(Some(parse_string(input)?))
    },
    |value: &Option<Arc<str>>| value.as_deref().map_or("normal".into(), string_css)
);

/// The font metric `font-size-adjust` keeps constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontMetric {
    ExHeight,
    CapHeight,
    ChWidth,
    IcWidth,
    IcHeight,
}

impl FontMetric {
    fn as_str(self) -> &'static str {
        match self {
            FontMetric::ExHeight => "ex-height",
            FontMetric::CapHeight => "cap-height",
            FontMetric::ChWidth => "ch-width",
            FontMetric::IcWidth => "ic-width",
            FontMetric::IcHeight => "ic-height",
        }
    }
}

/// `font-size-adjust`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontSizeAdjust {
    None,
    /// The metric and the ratio to keep; `None` for `from-font`.
    Adjust(FontMetric, Option<f32>),
}

impl Property for FontSizeAdjust {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            FontSizeAdjust::None => "none".into(),
            FontSizeAdjust::Adjust(metric, value) => {
                let value = value.map_or("from-font".into(), |v| serialize::number(f64::from(v)));
                if *metric == FontMetric::ExHeight {
                    value
                } else {
                    format!("{} {value}", metric.as_str())
                }
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(FontSizeAdjust::None);
        }
        let metric = match ident(&mut *input).as_deref() {
            Some("ex-height") => FontMetric::ExHeight,
            Some("cap-height") => FontMetric::CapHeight,
            Some("ch-width") => FontMetric::ChWidth,
            Some("ic-width") => FontMetric::IcWidth,
            Some("ic-height") => FontMetric::IcHeight,
            Some("from-font") => return Ok(FontSizeAdjust::Adjust(FontMetric::ExHeight, None)),
            Some(_) => return fail(),
            None => {
                let value = parse_number(input)?;
                if value < 0.0 {
                    return fail();
                }
                return Ok(FontSizeAdjust::Adjust(FontMetric::ExHeight, Some(value)));
            }
        };
        if keyword(input, "from-font") {
            return Ok(FontSizeAdjust::Adjust(metric, None));
        }
        let value = parse_number(input)?;
        if value < 0.0 {
            return fail();
        }
        Ok(FontSizeAdjust::Adjust(metric, Some(value)))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `font-variant-ligatures`. Each group is `None` when unspecified, `Some(true)` when
/// enabled and `Some(false)` for its `no-` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FontVariantLigatures {
    /// `none`: every ligature group is disabled.
    pub none: bool,
    pub common: Option<bool>,
    pub discretionary: Option<bool>,
    pub historical: Option<bool>,
    pub contextual: Option<bool>,
}

impl FontVariantLigatures {
    pub const NORMAL: Self = Self {
        none: false,
        common: None,
        discretionary: None,
        historical: None,
        contextual: None,
    };

    /// Parses the values other than `normal` and `none`.
    pub(crate) fn parse_keyword(&mut self, word: &str) -> bool {
        let (slot, on) = match word {
            "common-ligatures" => (&mut self.common, true),
            "no-common-ligatures" => (&mut self.common, false),
            "discretionary-ligatures" => (&mut self.discretionary, true),
            "no-discretionary-ligatures" => (&mut self.discretionary, false),
            "historical-ligatures" => (&mut self.historical, true),
            "no-historical-ligatures" => (&mut self.historical, false),
            "contextual" => (&mut self.contextual, true),
            "no-contextual" => (&mut self.contextual, false),
            _ => return false,
        };
        once(slot, on)
    }
}

impl Property for FontVariantLigatures {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        if computed.none {
            return "none".into();
        }
        let part = |value: Option<bool>, on: &'static str, off: &'static str| match value {
            Some(true) => on,
            Some(false) => off,
            None => "",
        };
        words(
            &[
                part(computed.common, "common-ligatures", "no-common-ligatures"),
                part(
                    computed.discretionary,
                    "discretionary-ligatures",
                    "no-discretionary-ligatures",
                ),
                part(
                    computed.historical,
                    "historical-ligatures",
                    "no-historical-ligatures",
                ),
                part(computed.contextual, "contextual", "no-contextual"),
            ],
            "normal",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "normal") {
            return Ok(Self::NORMAL);
        }
        if keyword(input, "none") {
            return Ok(Self {
                none: true,
                ..Self::NORMAL
            });
        }
        let mut value = Self::NORMAL;
        any_order(input, |word| value.parse_keyword(word))?;
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `font-variant-numeric`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FontVariantNumeric {
    /// `lining-nums` (`true`) or `oldstyle-nums` (`false`).
    pub lining: Option<bool>,
    /// `proportional-nums` (`true`) or `tabular-nums` (`false`).
    pub proportional: Option<bool>,
    /// `diagonal-fractions` (`true`) or `stacked-fractions` (`false`).
    pub diagonal: Option<bool>,
    pub ordinal: bool,
    pub slashed_zero: bool,
}

impl FontVariantNumeric {
    pub(crate) fn parse_keyword(&mut self, word: &str) -> bool {
        match word {
            "lining-nums" => once(&mut self.lining, true),
            "oldstyle-nums" => once(&mut self.lining, false),
            "proportional-nums" => once(&mut self.proportional, true),
            "tabular-nums" => once(&mut self.proportional, false),
            "diagonal-fractions" => once(&mut self.diagonal, true),
            "stacked-fractions" => once(&mut self.diagonal, false),
            "ordinal" => flag(&mut self.ordinal),
            "slashed-zero" => flag(&mut self.slashed_zero),
            _ => false,
        }
    }
}

impl Property for FontVariantNumeric {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        let pick = |value: Option<bool>, on: &'static str, off: &'static str| match value {
            Some(true) => on,
            Some(false) => off,
            None => "",
        };
        words(
            &[
                pick(c.lining, "lining-nums", "oldstyle-nums"),
                pick(c.proportional, "proportional-nums", "tabular-nums"),
                pick(c.diagonal, "diagonal-fractions", "stacked-fractions"),
                if c.ordinal { "ordinal" } else { "" },
                if c.slashed_zero { "slashed-zero" } else { "" },
            ],
            "normal",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let mut value = Self::default();
        if keyword(input, "normal") {
            return Ok(value);
        }
        any_order(input, |word| value.parse_keyword(word))?;
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `font-variant-east-asian`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FontVariantEastAsian {
    /// `jis78`, `jis83`, `jis90`, `jis04`, `simplified` or `traditional`.
    pub variant: Option<&'static str>,
    /// `full-width` or `proportional-width`.
    pub width: Option<&'static str>,
    pub ruby: bool,
}

impl FontVariantEastAsian {
    pub(crate) fn parse_keyword(&mut self, word: &str) -> bool {
        const VARIANTS: [&str; 6] = [
            "jis78",
            "jis83",
            "jis90",
            "jis04",
            "simplified",
            "traditional",
        ];
        const WIDTHS: [&str; 2] = ["full-width", "proportional-width"];
        if let Some(variant) = VARIANTS.iter().find(|v| **v == word) {
            return once(&mut self.variant, *variant);
        }
        if let Some(width) = WIDTHS.iter().find(|w| **w == word) {
            return once(&mut self.width, *width);
        }
        word == "ruby" && flag(&mut self.ruby)
    }
}

impl Property for FontVariantEastAsian {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        words(
            &[
                c.variant.unwrap_or(""),
                c.width.unwrap_or(""),
                if c.ruby { "ruby" } else { "" },
            ],
            "normal",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let mut value = Self::default();
        if keyword(input, "normal") {
            return Ok(value);
        }
        any_order(input, |word| value.parse_keyword(word))?;
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

// ---------------------------------------------------------------------------------------------
// Inline layout

keywords!(AlignmentBaseline {
    Baseline = "baseline",
    TextBottom = "text-bottom",
    Alphabetic = "alphabetic",
    Ideographic = "ideographic",
    Middle = "middle",
    Central = "central",
    Mathematical = "mathematical",
    TextTop = "text-top",
});

impl AlignmentBaseline {
    /// Also accepts the SVG keywords that CSS Inline 3 maps to its values.
    pub(crate) fn parse_any(word: &str) -> Option<Self> {
        Self::from_keyword(word).or(match word {
            "auto" => Some(Self::Baseline),
            "before-edge" | "text-before-edge" | "hanging" => Some(Self::TextTop),
            "after-edge" | "text-after-edge" => Some(Self::TextBottom),
            _ => None,
        })
    }
}

keywords!(BaselineSource { Auto = "auto", First = "first", Last = "last" });

keywords!(DominantBaseline {
    Auto = "auto",
    TextBottom = "text-bottom",
    Alphabetic = "alphabetic",
    Ideographic = "ideographic",
    Middle = "middle",
    Central = "central",
    Mathematical = "mathematical",
    Hanging = "hanging",
    TextTop = "text-top",
});

/// `baseline-shift`.
#[derive(Debug, Clone, PartialEq)]
pub enum BaselineShift<T> {
    /// A length raises the box; a percentage refers to `line-height`.
    Value(T),
    Sub,
    Super,
    Top,
    Center,
    Bottom,
}

impl BaselineShift<SpecifiedLengthPercentage> {
    pub(crate) fn parse_keyword(word: &str) -> Option<Self> {
        Some(match word {
            "sub" => BaselineShift::Sub,
            "super" => BaselineShift::Super,
            "top" => BaselineShift::Top,
            "center" => BaselineShift::Center,
            "bottom" => BaselineShift::Bottom,
            _ => return None,
        })
    }
}

pub struct BaselineShiftValue;

impl Property for BaselineShiftValue {
    type Specified = BaselineShift<SpecifiedLengthPercentage>;
    type Computed = BaselineShift<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            BaselineShift::Value(value) => length_percentage_css(value),
            BaselineShift::Sub => "sub".into(),
            BaselineShift::Super => "super".into(),
            BaselineShift::Top => "top".into(),
            BaselineShift::Center => "center".into(),
            BaselineShift::Bottom => "bottom".into(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if let Some(word) = ident(input) {
            return BaselineShift::parse_keyword(&word).ok_or_else(|| ParseError::custom(()));
        }
        Ok(BaselineShift::Value(parse_length_percentage(
            input, true, false,
        )?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        match specified {
            BaselineShift::Value(value) => BaselineShift::Value(value.compute(ctx, false)),
            BaselineShift::Sub => BaselineShift::Sub,
            BaselineShift::Super => BaselineShift::Super,
            BaselineShift::Top => BaselineShift::Top,
            BaselineShift::Center => BaselineShift::Center,
            BaselineShift::Bottom => BaselineShift::Bottom,
        }
    }
}

/// How far an initial letter sinks: `<integer>`, `drop` or `raise`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InitialLetterSink {
    Lines(u32),
    Drop,
    Raise,
}

/// `initial-letter`: `normal`, or the letter's size in lines and how far it sinks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InitialLetter {
    Normal,
    Letter { size: f32, sink: InitialLetterSink },
}

impl InitialLetter {
    /// The size in lines and the number of lines it sinks, or `None` for `normal`.
    pub fn lines(&self) -> Option<(f32, u32)> {
        match *self {
            InitialLetter::Normal => None,
            InitialLetter::Letter { size, sink } => Some((
                size,
                match sink {
                    InitialLetterSink::Lines(lines) => lines,
                    InitialLetterSink::Drop => size.floor().max(1.0) as u32,
                    InitialLetterSink::Raise => 1,
                },
            )),
        }
    }
}

impl Property for InitialLetter {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            InitialLetter::Normal => "normal".into(),
            InitialLetter::Letter { size, sink } => {
                let size = serialize::number(f64::from(*size));
                match sink {
                    InitialLetterSink::Lines(lines) => format!("{size} {lines}"),
                    InitialLetterSink::Drop => size,
                    InitialLetterSink::Raise => format!("{size} raise"),
                }
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "normal") {
            return Ok(InitialLetter::Normal);
        }
        // `<number> && [drop | raise]?` or `<number> <integer>?`
        let leading = if keyword(input, "drop") {
            Some(InitialLetterSink::Drop)
        } else if keyword(input, "raise") {
            Some(InitialLetterSink::Raise)
        } else {
            None
        };
        let size = parse_number(input)?;
        if size < 1.0 {
            return fail();
        }
        let sink = match leading {
            Some(sink) => sink,
            None if keyword(input, "drop") => InitialLetterSink::Drop,
            None if keyword(input, "raise") => InitialLetterSink::Raise,
            None => match input.try_parse(|input| parse_integer(input, 1)) {
                Ok(lines) => InitialLetterSink::Lines(lines as u32),
                Err(_) => InitialLetterSink::Drop,
            },
        };
        Ok(InitialLetter::Letter { size, sink })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `initial-letter-align`: `[border-box]? [alphabetic | ideographic | hanging | leading]?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InitialLetterAlign {
    pub border_box: bool,
    /// `alphabetic`, `ideographic`, `hanging` or `leading`; `None` when only `border-box`
    /// is given.
    pub edge: Option<&'static str>,
}

impl Property for InitialLetterAlign {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        words(
            &[
                if c.border_box { "border-box" } else { "" },
                c.edge.unwrap_or(""),
            ],
            "alphabetic",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        const EDGES: [&str; 4] = ["alphabetic", "ideographic", "hanging", "leading"];
        let mut value = Self {
            border_box: false,
            edge: None,
        };
        any_order(input, |word| {
            if word == "border-box" {
                return flag(&mut value.border_box);
            }
            EDGES
                .iter()
                .find(|e| **e == word)
                .is_some_and(|edge| once(&mut value.edge, *edge))
        })?;
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

keywords!(TextBoxTrim {
    None = "none",
    TrimStart = "trim-start",
    TrimEnd = "trim-end",
    TrimBoth = "trim-both",
});

/// A metric of the font that `<text-edge>` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextEdgeMetric {
    Text,
    Ideographic,
    IdeographicInk,
    Cap,
    Ex,
    Alphabetic,
}

impl TextEdgeMetric {
    fn from_keyword(word: &str) -> Option<Self> {
        Some(match word {
            "text" => Self::Text,
            "ideographic" => Self::Ideographic,
            "ideographic-ink" => Self::IdeographicInk,
            "cap" => Self::Cap,
            "ex" => Self::Ex,
            "alphabetic" => Self::Alphabetic,
            _ => return None,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Ideographic => "ideographic",
            Self::IdeographicInk => "ideographic-ink",
            Self::Cap => "cap",
            Self::Ex => "ex",
            Self::Alphabetic => "alphabetic",
        }
    }
}

/// `<text-edge>`: the over (top) and under (bottom) edges of a line of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextEdge {
    pub over: TextEdgeMetric,
    pub under: TextEdgeMetric,
}

impl TextEdge {
    pub const TEXT: Self = Self {
        over: TextEdgeMetric::Text,
        under: TextEdgeMetric::Text,
    };

    pub fn parse(input: &mut Parser<'_>) -> Result<Self> {
        use TextEdgeMetric as M;
        let over = ident(input)
            .and_then(|w| M::from_keyword(&w))
            .filter(|m| *m != M::Alphabetic)
            .ok_or_else(|| ParseError::custom(()))?;
        let under = input
            .try_parse(|input| {
                ident(input)
                    .and_then(|w| M::from_keyword(&w))
                    .filter(|m| !matches!(m, M::Cap | M::Ex))
                    .ok_or_else(|| ParseError::<()>::custom(()))
            })
            .ok();
        Ok(match under {
            Some(under) => Self { over, under },
            // A single keyword sets both edges, with `text` for `cap` and `ex` underneath.
            None if matches!(over, M::Cap | M::Ex) => Self {
                over,
                under: M::Text,
            },
            None => Self { over, under: over },
        })
    }

    pub fn to_css(&self) -> String {
        let single = self.over == self.under
            || (matches!(self.over, TextEdgeMetric::Cap | TextEdgeMetric::Ex)
                && self.under == TextEdgeMetric::Text);
        if single {
            self.over.as_str().into()
        } else {
            format!("{} {}", self.over.as_str(), self.under.as_str())
        }
    }
}

/// `text-box-edge`: `auto | <text-edge>`; `None` is `auto`.
pub struct TextBoxEdge;

as_specified!(
    TextBoxEdge,
    Option<TextEdge>,
    |input: &mut Parser<'_>| {
        if keyword(input, "auto") {
            return Ok(None);
        }
        Ok(Some(TextEdge::parse(input)?))
    },
    |value: &Option<TextEdge>| value.map_or("auto".into(), |edge| edge.to_css())
);

/// What `block-ellipsis` (and `line-clamp`) put at the end of the last line.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockEllipsis {
    NoEllipsis,
    Auto,
    String(Arc<str>),
}

/// `line-clamp`: `none | [<integer [1,∞]> || <'block-ellipsis'>]`.
#[derive(Debug, Clone, PartialEq)]
pub struct LineClamp {
    pub lines: Option<u32>,
    pub ellipsis: BlockEllipsis,
}

impl Property for LineClamp {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let ellipsis = match &computed.ellipsis {
            BlockEllipsis::NoEllipsis => "no-ellipsis".to_owned(),
            BlockEllipsis::Auto => String::new(),
            BlockEllipsis::String(s) => string_css(s),
        };
        match computed.lines {
            None if computed.ellipsis == BlockEllipsis::NoEllipsis => "none".into(),
            None => ellipsis,
            Some(lines) => words(&[&lines.to_string(), &ellipsis], ""),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(Self {
                lines: None,
                ellipsis: BlockEllipsis::NoEllipsis,
            });
        }
        let mut lines = None;
        let mut ellipsis = None;
        for _ in 0..2 {
            if lines.is_none()
                && let Ok(n) = input.try_parse(|input| parse_integer(input, 1))
            {
                lines = Some(n as u32);
            } else if ellipsis.is_none() && keyword(input, "auto") {
                ellipsis = Some(BlockEllipsis::Auto);
            } else if ellipsis.is_none() && keyword(input, "no-ellipsis") {
                ellipsis = Some(BlockEllipsis::NoEllipsis);
            } else if ellipsis.is_none()
                && let Ok(s) = input.try_parse(parse_string)
            {
                ellipsis = Some(BlockEllipsis::String(s));
            } else {
                break;
            }
        }
        if lines.is_none() && ellipsis.is_none() {
            return fail();
        }
        Ok(Self {
            lines,
            ellipsis: ellipsis.unwrap_or(BlockEllipsis::Auto),
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// `max-lines`: `none | <integer [1,∞]>`.
pub struct MaxLines;

as_specified!(
    MaxLines,
    Option<u32>,
    |input: &mut Parser<'_>| {
        if keyword(input, "none") {
            return Ok(None);
        }
        Ok(Some(parse_integer(input, 1)? as u32))
    },
    |value: &Option<u32>| value.map_or("none".into(), |n| n.to_string())
);

/// One side of `text-overflow`.
#[derive(Debug, Clone, PartialEq)]
pub enum TextOverflowSide {
    Clip,
    Ellipsis,
    String(Arc<str>),
}

/// `text-overflow`: one value for the end side, or two for start and end.
#[derive(Debug, Clone, PartialEq)]
pub struct TextOverflow {
    pub start: TextOverflowSide,
    pub end: TextOverflowSide,
    /// Whether two values were given.
    pub both: bool,
}

impl Property for TextOverflow {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let side = |s: &TextOverflowSide| match s {
            TextOverflowSide::Clip => "clip".to_owned(),
            TextOverflowSide::Ellipsis => "ellipsis".to_owned(),
            TextOverflowSide::String(s) => string_css(s),
        };
        if computed.both {
            format!("{} {}", side(&computed.start), side(&computed.end))
        } else {
            side(&computed.end)
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        fn side(input: &mut Parser<'_>) -> Result<TextOverflowSide> {
            if let Ok(s) = input.try_parse(parse_string) {
                return Ok(TextOverflowSide::String(s));
            }
            match ident(input).as_deref() {
                Some("clip") => Ok(TextOverflowSide::Clip),
                Some("ellipsis") => Ok(TextOverflowSide::Ellipsis),
                _ => fail(),
            }
        }
        let first = side(input)?;
        Ok(match input.try_parse(side) {
            Ok(second) => Self {
                start: first,
                end: second,
                both: true,
            },
            Err(_) => Self {
                start: TextOverflowSide::Clip,
                end: first,
                both: false,
            },
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// `line-height-step`: a non-negative `<length>`, computed to pixels.
pub struct LineHeightStep;

impl Property for LineHeightStep {
    type Specified = SpecifiedLengthPercentage;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        serialize::px(*computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_length_percentage(input, false, true)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> f32 {
        specified.compute(ctx, true).as_length().unwrap_or(0.0)
    }
}

// ---------------------------------------------------------------------------------------------
// Text

/// `hanging-punctuation`: `none | [first || [force-end | allow-end] || last]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct HangingPunctuation {
    pub first: bool,
    pub last: bool,
    /// `force-end` (`true`) or `allow-end` (`false`).
    pub force_end: Option<bool>,
}

impl Property for HangingPunctuation {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        words(
            &[
                if c.first { "first" } else { "" },
                match c.force_end {
                    Some(true) => "force-end",
                    Some(false) => "allow-end",
                    None => "",
                },
                if c.last { "last" } else { "" },
            ],
            "none",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let mut value = Self::default();
        if keyword(input, "none") {
            return Ok(value);
        }
        any_order(input, |word| match word {
            "first" => flag(&mut value.first),
            "last" => flag(&mut value.last),
            "force-end" => once(&mut value.force_end, true),
            "allow-end" => once(&mut value.force_end, false),
            _ => false,
        })?;
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `hyphenate-character`: `auto | <string>`; `None` is `auto`.
pub struct HyphenateCharacter;

as_specified!(
    HyphenateCharacter,
    Option<Arc<str>>,
    |input: &mut Parser<'_>| {
        if keyword(input, "auto") {
            return Ok(None);
        }
        Ok(Some(parse_string(input)?))
    },
    |value: &Option<Arc<str>>| value.as_deref().map_or("auto".into(), string_css)
);

/// `hyphenate-limit-chars`: the minimum word length and the minimum characters before and
/// after a hyphen. `None` is `auto`.
pub struct HyphenateLimitChars;

as_specified!(
    HyphenateLimitChars,
    [Option<u32>; 3],
    |input: &mut Parser<'_>| {
        let mut values = Vec::new();
        while values.len() < 3 {
            if keyword(input, "auto") {
                values.push(None);
            } else if let Ok(n) = input.try_parse(|input| parse_integer(input, 1)) {
                values.push(Some(n as u32));
            } else {
                break;
            }
        }
        match values.len() {
            0 => fail(),
            // A missing `after` value copies `before`.
            1 => Ok([values[0], None, None]),
            2 => Ok([values[0], values[1], values[1]]),
            _ => Ok([values[0], values[1], values[2]]),
        }
    },
    |value: &[Option<u32>; 3]| {
        let part = |v: Option<u32>| v.map_or("auto".to_owned(), |n| n.to_string());
        if value[1] == value[2] && value[1].is_none() {
            part(value[0])
        } else if value[1] == value[2] {
            format!("{} {}", part(value[0]), part(value[1]))
        } else {
            format!("{} {} {}", part(value[0]), part(value[1]), part(value[2]))
        }
    }
);

keywords!(Hyphens { None = "none", Manual = "manual", Auto = "auto" });

/// `letter-spacing` and `word-spacing`: `normal | <length-percentage>`. `None` is `normal`.
pub struct Spacing;

impl Property for Spacing {
    type Specified = Option<SpecifiedLengthPercentage>;
    type Computed = Option<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        computed
            .as_ref()
            .map_or("normal".into(), length_percentage_css)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "normal") {
            return Ok(None);
        }
        Ok(Some(parse_length_percentage(input, true, false)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.as_ref().map(|value| value.compute(ctx, false))
    }
}

keywords!(LineBreak {
    Auto = "auto",
    Loose = "loose",
    Normal = "normal",
    Strict = "strict",
    Anywhere = "anywhere",
});

keywords!(OverflowWrap { Normal = "normal", BreakWord = "break-word", Anywhere = "anywhere" });

/// `tab-size`: a multiple of the space advance, or a length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TabSize {
    Spaces(f32),
    Length(f32),
}

impl Property for TabSize {
    type Specified = std::result::Result<f32, SpecifiedLengthPercentage>;
    type Computed = TabSize;

    fn to_css(computed: &TabSize) -> String {
        match computed {
            TabSize::Spaces(n) => serialize::number(f64::from(*n)),
            TabSize::Length(px) => serialize::px(*px),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if let Ok(n) = input.try_parse(parse_number) {
            if n < 0.0 {
                return fail();
            }
            return Ok(Ok(n));
        }
        Ok(Err(parse_length_percentage(input, false, true)?))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> TabSize {
        match specified {
            Ok(n) => TabSize::Spaces(*n),
            Err(length) => TabSize::Length(length.compute(ctx, true).as_length().unwrap_or(0.0)),
        }
    }
}

keywords!(TextAlignLast {
    Auto = "auto",
    Start = "start",
    End = "end",
    Left = "left",
    Right = "right",
    Center = "center",
    Justify = "justify",
    MatchParent = "match-parent",
});

/// `text-autospace`: `normal | auto | no-autospace | [ideograph-alpha || ideograph-numeric ||
/// punctuation] || [insert | replace]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextAutospace {
    Normal,
    Auto,
    NoAutospace,
    Custom {
        ideograph_alpha: bool,
        ideograph_numeric: bool,
        punctuation: bool,
        /// `insert` (`true`) or `replace` (`false`).
        insert: Option<bool>,
    },
}

impl Property for TextAutospace {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        match c {
            TextAutospace::Normal => "normal".into(),
            TextAutospace::Auto => "auto".into(),
            TextAutospace::NoAutospace => "no-autospace".into(),
            TextAutospace::Custom {
                ideograph_alpha,
                ideograph_numeric,
                punctuation,
                insert,
            } => words(
                &[
                    if *ideograph_alpha {
                        "ideograph-alpha"
                    } else {
                        ""
                    },
                    if *ideograph_numeric {
                        "ideograph-numeric"
                    } else {
                        ""
                    },
                    if *punctuation { "punctuation" } else { "" },
                    match insert {
                        Some(true) => "insert",
                        Some(false) => "replace",
                        None => "",
                    },
                ],
                "normal",
            ),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "normal") {
            return Ok(TextAutospace::Normal);
        }
        if keyword(input, "auto") {
            return Ok(TextAutospace::Auto);
        }
        if keyword(input, "no-autospace") {
            return Ok(TextAutospace::NoAutospace);
        }
        let (mut alpha, mut numeric, mut punctuation, mut insert) = (false, false, false, None);
        any_order(input, |word| match word {
            "ideograph-alpha" => flag(&mut alpha),
            "ideograph-numeric" => flag(&mut numeric),
            "punctuation" => flag(&mut punctuation),
            "insert" => once(&mut insert, true),
            "replace" => once(&mut insert, false),
            _ => false,
        })?;
        Ok(TextAutospace::Custom {
            ideograph_alpha: alpha,
            ideograph_numeric: numeric,
            punctuation,
            insert,
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `text-fit`: `none | [grow | shrink] [consistent | per-line | per-line-all]? <percentage>?`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextFit {
    None,
    Fit {
        /// `grow` (`true`) or `shrink` (`false`).
        grow: bool,
        /// `consistent`, `per-line` or `per-line-all`.
        method: &'static str,
        /// The limit on scaling, as a fraction.
        limit: Option<f32>,
    },
}

impl Property for TextFit {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        match c {
            TextFit::None => "none".into(),
            TextFit::Fit {
                grow,
                method,
                limit,
            } => {
                let limit = limit.map(serialize::percentage).unwrap_or_default();
                let method = if *method == "consistent" { "" } else { method };
                words(&[if *grow { "grow" } else { "shrink" }, method, &limit], "")
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(TextFit::None);
        }
        let grow = match ident(input).as_deref() {
            Some("grow") => true,
            Some("shrink") => false,
            _ => return fail(),
        };
        let method = input
            .try_parse(|input| {
                let word = ident(input).ok_or_else(|| ParseError::<()>::custom(()))?;
                ["consistent", "per-line", "per-line-all"]
                    .into_iter()
                    .find(|m| *m == word)
                    .ok_or_else(|| ParseError::custom(()))
            })
            .unwrap_or("consistent");
        let limit = input
            .try_parse(|input| input.expect_percentage())
            .ok()
            .filter(|p| *p >= 0.0);
        Ok(TextFit::Fit {
            grow,
            method,
            limit,
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `text-indent`: `<length-percentage> && hanging? && each-line?`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextIndent<T> {
    pub length: T,
    pub hanging: bool,
    pub each_line: bool,
}

pub struct TextIndentValue;

impl Property for TextIndentValue {
    type Specified = TextIndent<SpecifiedLengthPercentage>;
    type Computed = TextIndent<LengthPercentage>;

    fn to_css(c: &Self::Computed) -> String {
        words(
            &[
                &length_percentage_css(&c.length),
                if c.hanging { "hanging" } else { "" },
                if c.each_line { "each-line" } else { "" },
            ],
            "",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let (mut length, mut hanging, mut each_line) = (None, false, false);
        for _ in 0..3 {
            if length.is_none()
                && let Ok(value) =
                    input.try_parse(|input| parse_length_percentage(input, true, false))
            {
                length = Some(value);
            } else if !hanging && keyword(input, "hanging") {
                hanging = true;
            } else if !each_line && keyword(input, "each-line") {
                each_line = true;
            } else {
                break;
            }
        }
        Ok(TextIndent {
            length: length.ok_or_else(|| ParseError::custom(()))?,
            hanging,
            each_line,
        })
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        TextIndent {
            length: specified.length.compute(ctx, false),
            hanging: specified.hanging,
            each_line: specified.each_line,
        }
    }
}

/// `text-justify`; `distribute` is a legacy alias of `inter-character`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextJustify {
    Auto,
    None,
    InterWord,
    InterCharacter,
}

impl Property for TextJustify {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        match c {
            TextJustify::Auto => "auto",
            TextJustify::None => "none",
            TextJustify::InterWord => "inter-word",
            TextJustify::InterCharacter => "inter-character",
        }
        .into()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Ok(match ident(input).as_deref() {
            Some("auto") => TextJustify::Auto,
            Some("none") => TextJustify::None,
            Some("inter-word") => TextJustify::InterWord,
            Some("inter-character" | "distribute") => TextJustify::InterCharacter,
            _ => return fail(),
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

keywords!(TextSpacingTrim {
    Normal = "normal",
    SpaceAll = "space-all",
    SpaceFirst = "space-first",
    TrimStart = "trim-start",
    TrimBoth = "trim-both",
    TrimAll = "trim-all",
    Auto = "auto",
});

keywords!(TextCase {
    None = "none",
    Capitalize = "capitalize",
    Uppercase = "uppercase",
    Lowercase = "lowercase",
});

/// `text-transform`: `none | [capitalize | uppercase | lowercase] || full-width ||
/// full-size-kana | math-auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextTransform {
    pub case: TextCase,
    pub full_width: bool,
    pub full_size_kana: bool,
    pub math_auto: bool,
}

impl TextTransform {
    pub const NONE: Self = Self {
        case: TextCase::None,
        full_width: false,
        full_size_kana: false,
        math_auto: false,
    };
}

impl Property for TextTransform {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        if c.math_auto {
            return "math-auto".into();
        }
        words(
            &[
                if c.case == TextCase::None {
                    ""
                } else {
                    c.case.as_str()
                },
                if c.full_width { "full-width" } else { "" },
                if c.full_size_kana {
                    "full-size-kana"
                } else {
                    ""
                },
            ],
            "none",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(Self::NONE);
        }
        if keyword(input, "math-auto") {
            return Ok(Self {
                math_auto: true,
                ..Self::NONE
            });
        }
        let mut value = Self::NONE;
        let mut case = None;
        any_order(input, |word| match word {
            "capitalize" => once(&mut case, TextCase::Capitalize),
            "uppercase" => once(&mut case, TextCase::Uppercase),
            "lowercase" => once(&mut case, TextCase::Lowercase),
            "full-width" => flag(&mut value.full_width),
            "full-size-kana" => flag(&mut value.full_size_kana),
            _ => false,
        })?;
        value.case = case.unwrap_or(TextCase::None);
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

keywords!(TextWrapMode { Wrap = "wrap", Nowrap = "nowrap" });

keywords!(TextWrapStyle {
    Auto = "auto",
    Balance = "balance",
    Stable = "stable",
    Pretty = "pretty",
    AvoidOrphans = "avoid-orphans",
});

keywords!(WhiteSpaceCollapse {
    Collapse = "collapse",
    Discard = "discard",
    Preserve = "preserve",
    PreserveBreaks = "preserve-breaks",
    PreserveSpaces = "preserve-spaces",
    BreakSpaces = "break-spaces",
});

keywords!(WordBreak {
    Normal = "normal",
    BreakAll = "break-all",
    KeepAll = "keep-all",
    Manual = "manual",
    AutoPhrase = "auto-phrase",
    BreakWord = "break-word",
});

// ---------------------------------------------------------------------------------------------
// Text decoration

/// `text-decoration-line`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TextDecorationLine {
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
    pub blink: bool,
    pub spelling_error: bool,
    pub grammar_error: bool,
}

impl TextDecorationLine {
    pub const NONE: Self = Self {
        underline: false,
        overline: false,
        line_through: false,
        blink: false,
        spelling_error: false,
        grammar_error: false,
    };

    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }

    pub(crate) fn parse_keyword(&mut self, word: &str) -> bool {
        match word {
            "underline" => flag(&mut self.underline),
            "overline" => flag(&mut self.overline),
            "line-through" => flag(&mut self.line_through),
            "blink" => flag(&mut self.blink),
            _ => false,
        }
    }
}

impl Property for TextDecorationLine {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        if c.spelling_error {
            return "spelling-error".into();
        }
        if c.grammar_error {
            return "grammar-error".into();
        }
        words(
            &[
                if c.underline { "underline" } else { "" },
                if c.overline { "overline" } else { "" },
                if c.line_through { "line-through" } else { "" },
                if c.blink { "blink" } else { "" },
            ],
            "none",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(Self::NONE);
        }
        if keyword(input, "spelling-error") {
            return Ok(Self {
                spelling_error: true,
                ..Self::NONE
            });
        }
        if keyword(input, "grammar-error") {
            return Ok(Self {
                grammar_error: true,
                ..Self::NONE
            });
        }
        let mut value = Self::NONE;
        any_order(input, |word| value.parse_keyword(word))?;
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

keywords!(TextDecorationStyle {
    Solid = "solid",
    Double = "double",
    Dotted = "dotted",
    Dashed = "dashed",
    Wavy = "wavy",
});

/// `text-decoration-thickness` and `text-underline-offset`: `auto`, `from-font` (thickness
/// only) or a length. Percentages refer to 1em and stay percentages.
#[derive(Debug, Clone, PartialEq)]
pub enum DecorationLength<T> {
    Auto,
    FromFont,
    Value(T),
}

impl DecorationLength<SpecifiedLengthPercentage> {
    fn parse(input: &mut Parser<'_>, from_font: bool) -> Result<Self> {
        if keyword(input, "auto") {
            return Ok(DecorationLength::Auto);
        }
        if from_font && keyword(input, "from-font") {
            return Ok(DecorationLength::FromFont);
        }
        Ok(DecorationLength::Value(parse_length_percentage(
            input, true, false,
        )?))
    }

    fn compute(&self, ctx: &ComputeContext) -> DecorationLength<LengthPercentage> {
        match self {
            DecorationLength::Auto => DecorationLength::Auto,
            DecorationLength::FromFont => DecorationLength::FromFont,
            DecorationLength::Value(value) => DecorationLength::Value(value.compute(ctx, false)),
        }
    }
}

fn decoration_length_css(value: &DecorationLength<LengthPercentage>) -> String {
    match value {
        DecorationLength::Auto => "auto".into(),
        DecorationLength::FromFont => "from-font".into(),
        DecorationLength::Value(value) => length_percentage_css(value),
    }
}

pub struct TextDecorationThickness;

impl Property for TextDecorationThickness {
    type Specified = DecorationLength<SpecifiedLengthPercentage>;
    type Computed = DecorationLength<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        decoration_length_css(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        DecorationLength::parse(input, true)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.compute(ctx)
    }
}

pub struct TextUnderlineOffset;

impl Property for TextUnderlineOffset {
    type Specified = DecorationLength<SpecifiedLengthPercentage>;
    type Computed = DecorationLength<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        decoration_length_css(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        DecorationLength::parse(input, false)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.compute(ctx)
    }
}

/// `text-decoration-inset`: `<length>{1,2} | auto`, as start and end insets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextDecorationInset {
    Auto,
    Lengths(f32, f32),
}

impl Property for TextDecorationInset {
    type Specified = Option<(SpecifiedLengthPercentage, Option<SpecifiedLengthPercentage>)>;
    type Computed = TextDecorationInset;

    fn to_css(c: &TextDecorationInset) -> String {
        match c {
            TextDecorationInset::Auto => "auto".into(),
            TextDecorationInset::Lengths(a, b) if a == b => serialize::px(*a),
            TextDecorationInset::Lengths(a, b) => {
                format!("{} {}", serialize::px(*a), serialize::px(*b))
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "auto") {
            return Ok(None);
        }
        let start = parse_length_percentage(input, false, false)?;
        let end = input
            .try_parse(|input| parse_length_percentage(input, false, false))
            .ok();
        Ok(Some((start, end)))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> TextDecorationInset {
        match specified {
            None => TextDecorationInset::Auto,
            Some((start, end)) => {
                let px = |v: &SpecifiedLengthPercentage| {
                    v.compute(ctx, false).as_length().unwrap_or(0.0)
                };
                let start = px(start);
                TextDecorationInset::Lengths(start, end.as_ref().map_or(start, px))
            }
        }
    }
}

/// `text-decoration-skip`: `none | [objects || [spaces | [leading-spaces || trailing-spaces]]
/// || edges || box-decoration]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TextDecorationSkip {
    pub objects: bool,
    pub leading_spaces: bool,
    pub trailing_spaces: bool,
    pub edges: bool,
    pub box_decoration: bool,
}

impl TextDecorationSkip {
    pub const OBJECTS: Self = Self {
        objects: true,
        leading_spaces: false,
        trailing_spaces: false,
        edges: false,
        box_decoration: false,
    };
}

impl Property for TextDecorationSkip {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        let spaces = match (c.leading_spaces, c.trailing_spaces) {
            (true, true) => "spaces",
            (true, false) => "leading-spaces",
            (false, true) => "trailing-spaces",
            (false, false) => "",
        };
        words(
            &[
                if c.objects { "objects" } else { "" },
                spaces,
                if c.edges { "edges" } else { "" },
                if c.box_decoration {
                    "box-decoration"
                } else {
                    ""
                },
            ],
            "none",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let mut value = Self::default();
        if keyword(input, "none") {
            return Ok(value);
        }
        let mut spaces = false;
        any_order(input, |word| match word {
            "objects" => flag(&mut value.objects),
            "spaces" if !spaces && !value.leading_spaces && !value.trailing_spaces => {
                spaces = true;
                value.leading_spaces = true;
                value.trailing_spaces = true;
                true
            }
            "leading-spaces" if !spaces => flag(&mut value.leading_spaces),
            "trailing-spaces" if !spaces => flag(&mut value.trailing_spaces),
            "edges" => flag(&mut value.edges),
            "box-decoration" => flag(&mut value.box_decoration),
            _ => false,
        })?;
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

keywords!(TextDecorationSkipInk { Auto = "auto", None = "none", All = "all" });

keywords!(EmphasisShape {
    Dot = "dot",
    Circle = "circle",
    DoubleCircle = "double-circle",
    Triangle = "triangle",
    Sesame = "sesame",
});

/// `text-emphasis-style`.
#[derive(Debug, Clone, PartialEq)]
pub enum TextEmphasisStyle {
    None,
    /// A shape, filled or open. A missing shape is `circle` in horizontal writing.
    Shape {
        filled: bool,
        shape: EmphasisShape,
    },
    String(Arc<str>),
}

impl Property for TextEmphasisStyle {
    type Specified = (Option<bool>, Option<EmphasisShape>, Option<Arc<str>>);
    type Computed = TextEmphasisStyle;

    fn to_css(c: &TextEmphasisStyle) -> String {
        match c {
            TextEmphasisStyle::None => "none".into(),
            TextEmphasisStyle::Shape { filled, shape } => {
                if *filled {
                    shape.as_str().into()
                } else {
                    format!("open {}", shape.as_str())
                }
            }
            TextEmphasisStyle::String(s) => string_css(s),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok((None, None, None));
        }
        if let Ok(s) = input.try_parse(parse_string) {
            return Ok((None, None, Some(s)));
        }
        let (mut filled, mut shape) = (None, None);
        any_order(input, |word| match word {
            "filled" => once(&mut filled, true),
            "open" => once(&mut filled, false),
            other => EmphasisShape::from_keyword(other).is_some_and(|s| once(&mut shape, s)),
        })?;
        Ok((Some(filled.unwrap_or(true)), shape, None))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> TextEmphasisStyle {
        match specified {
            (_, _, Some(s)) => TextEmphasisStyle::String(s.clone()),
            (None, None, None) => TextEmphasisStyle::None,
            (filled, shape, None) => TextEmphasisStyle::Shape {
                filled: filled.unwrap_or(true),
                shape: shape.unwrap_or(if ctx.vertical {
                    EmphasisShape::Sesame
                } else {
                    EmphasisShape::Circle
                }),
            },
        }
    }
}

/// `text-emphasis-position`: `auto | [over | under] && [right | left]?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextEmphasisPosition {
    Auto,
    Position {
        under: bool,
        /// `left` (`true`), `right` (`false`) or omitted (`None`).
        left: Option<bool>,
    },
}

impl Property for TextEmphasisPosition {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        match c {
            TextEmphasisPosition::Auto => "auto".into(),
            TextEmphasisPosition::Position { under, left } => words(
                &[
                    if *under { "under" } else { "over" },
                    match left {
                        Some(true) => "left",
                        Some(false) => "right",
                        None => "",
                    },
                ],
                "",
            ),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "auto") {
            return Ok(TextEmphasisPosition::Auto);
        }
        let (mut under, mut left) = (None, None);
        any_order(input, |word| match word {
            "over" => once(&mut under, false),
            "under" => once(&mut under, true),
            "left" => once(&mut left, true),
            "right" => once(&mut left, false),
            _ => false,
        })?;
        Ok(TextEmphasisPosition::Position {
            under: under.ok_or_else(|| ParseError::custom(()))?,
            left,
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// One `text-shadow`: offsets and blur radius in pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub x: f32,
    pub y: f32,
    pub blur: f32,
}

/// `text-shadow`: `none | [<color>? && <length>{2,3}]#`.
pub struct TextShadow;

impl Property for TextShadow {
    type Specified = Arc<[(Option<Color>, Vec<SpecifiedLengthPercentage>)]>;
    type Computed = Arc<[Shadow]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        computed
            .iter()
            .map(|s| {
                format!(
                    "{} {} {} {}",
                    serialize::color(&s.color),
                    serialize::px(s.x),
                    serialize::px(s.y),
                    serialize::px(s.blur)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(Arc::from([]));
        }
        let shadows = input.parse_comma_separated(|input| {
            let mut color = input.try_parse(Color::parse).ok();
            let mut lengths = Vec::new();
            while lengths.len() < 3 {
                match input.try_parse(|input| parse_length_percentage(input, false, false)) {
                    Ok(length) => lengths.push(length),
                    Err(_) => break,
                }
            }
            if lengths.len() < 2 {
                return fail();
            }
            if let Some(SpecifiedLengthPercentage::Length(blur)) = lengths.get(2)
                && blur.value < 0.0
            {
                return fail();
            }
            if color.is_none() {
                color = input.try_parse(Color::parse).ok();
            }
            Ok((color, lengths))
        })?;
        Ok(shadows.into())
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        let px = |v: &SpecifiedLengthPercentage| v.compute(ctx, false).as_length().unwrap_or(0.0);
        specified
            .iter()
            .map(|(color, lengths)| Shadow {
                color: match color {
                    Some(Color::Deferred(_)) | None => Color::Absolute(
                        color
                            .as_ref()
                            .map_or(ctx.color, |c| c.resolve_in(&ctx.colors())),
                    ),
                    Some(other) => other.clone(),
                },
                x: px(&lengths[0]),
                y: px(&lengths[1]),
                blur: lengths.get(2).map_or(0.0, |b| px(b).max(0.0)),
            })
            .collect()
    }
}

/// `text-underline-position`: `auto | from-font | [under || [left | right]]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TextUnderlinePosition {
    pub from_font: bool,
    pub under: bool,
    /// `left` (`true`) or `right` (`false`).
    pub left: Option<bool>,
}

impl Property for TextUnderlinePosition {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        words(
            &[
                if c.from_font { "from-font" } else { "" },
                if c.under { "under" } else { "" },
                match c.left {
                    Some(true) => "left",
                    Some(false) => "right",
                    None => "",
                },
            ],
            "auto",
        )
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let mut value = Self::default();
        if keyword(input, "auto") {
            return Ok(value);
        }
        if keyword(input, "from-font") {
            value.from_font = true;
            return Ok(value);
        }
        any_order(input, |word| match word {
            "under" => flag(&mut value.under),
            "left" => once(&mut value.left, true),
            "right" => once(&mut value.left, false),
            _ => false,
        })?;
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

// ---------------------------------------------------------------------------------------------
// Text painting

/// `paint-order`: the order of `fill`, `stroke` and `markers`, as indices 0, 1 and 2.
pub struct PaintOrder;

as_specified!(
    PaintOrder,
    [u8; 3],
    |input: &mut Parser<'_>| {
        if keyword(input, "normal") {
            return Ok([0, 1, 2]);
        }
        let mut order = Vec::new();
        any_order(input, |word| {
            let index = match word {
                "fill" => 0,
                "stroke" => 1,
                "markers" => 2,
                _ => return false,
            };
            if order.contains(&index) {
                return false;
            }
            order.push(index);
            true
        })?;
        // Omitted parts follow in their normal order.
        for index in 0..3 {
            if !order.contains(&index) {
                order.push(index);
            }
        }
        Ok([order[0], order[1], order[2]])
    },
    |value: &[u8; 3]| {
        const NAMES: [&str; 3] = ["fill", "stroke", "markers"];
        if *value == [0, 1, 2] {
            "normal".into()
        } else if value[1] < value[2] {
            // The last two in their normal order can be omitted.
            NAMES[value[0] as usize].into()
        } else {
            format!("{} {}", NAMES[value[0] as usize], NAMES[value[1] as usize])
        }
    }
);

/// `stroke`, an SVG `<paint>`.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    None,
    Color(Color),
    ContextFill,
    ContextStroke,
    /// A paint server reference, which HTML content cannot use, and its fallback.
    Url(Arc<str>, Option<Color>),
}

impl Property for Paint {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        match c {
            Paint::None => "none".into(),
            Paint::Color(color) => serialize::color(color),
            Paint::ContextFill => "context-fill".into(),
            Paint::ContextStroke => "context-stroke".into(),
            Paint::Url(url, fallback) => {
                let mut out = format!("url({})", string_css(url));
                if let Some(color) = fallback {
                    out.push(' ');
                    out.push_str(&serialize::color(color));
                }
                out
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(Paint::None);
        }
        if keyword(input, "context-fill") {
            return Ok(Paint::ContextFill);
        }
        if keyword(input, "context-stroke") {
            return Ok(Paint::ContextStroke);
        }
        if let Ok(url) = input.try_parse(|input| input.expect_url_or_string()) {
            let fallback = if keyword(input, "none") {
                None
            } else {
                input.try_parse(Color::parse).ok()
            };
            return Ok(Paint::Url(Arc::from(&*url), fallback));
        }
        Ok(Paint::Color(Color::parse(input)?))
    }

    fn compute(specified: &Self, ctx: &ComputeContext) -> Self {
        match specified {
            Paint::Color(color @ Color::Deferred(_)) => {
                Paint::Color(Color::Absolute(color.resolve_in(&ctx.colors())))
            }
            other => other.clone(),
        }
    }
}

/// `text-rendering`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextRendering {
    Auto,
    OptimizeSpeed,
    OptimizeLegibility,
    GeometricPrecision,
}

impl Property for TextRendering {
    type Specified = Self;
    type Computed = Self;

    fn to_css(c: &Self) -> String {
        match c {
            TextRendering::Auto => "auto",
            TextRendering::OptimizeSpeed => "optimizeSpeed",
            TextRendering::OptimizeLegibility => "optimizeLegibility",
            TextRendering::GeometricPrecision => "geometricPrecision",
        }
        .into()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Ok(match ident(input).as_deref() {
            Some("auto") => TextRendering::Auto,
            Some("optimizespeed") => TextRendering::OptimizeSpeed,
            Some("optimizelegibility") => TextRendering::OptimizeLegibility,
            Some("geometricprecision") => TextRendering::GeometricPrecision,
            _ => return fail(),
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `text-combine-upright`: `none | all | digits <integer [2,4]>?`, text set upright in the
/// space of one character in vertical writing modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextCombineUpright {
    None,
    /// All the text of the box.
    All,
    /// Runs of at most this many ASCII digits.
    Digits(u8),
}

impl Property for TextCombineUpright {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            TextCombineUpright::None => "none".into(),
            TextCombineUpright::All => "all".into(),
            TextCombineUpright::Digits(2) => "digits".into(),
            TextCombineUpright::Digits(count) => format!("digits {count}"),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if keyword(input, "none") {
            return Ok(TextCombineUpright::None);
        }
        if keyword(input, "all") {
            return Ok(TextCombineUpright::All);
        }
        if !keyword(input, "digits") {
            return fail();
        }
        match input.try_parse(|i| parse_integer(i, 2)) {
            Ok(count @ 2..=4) => Ok(TextCombineUpright::Digits(count as u8)),
            Ok(_) => fail(),
            Err(_) => Ok(TextCombineUpright::Digits(2)),
        }
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}
