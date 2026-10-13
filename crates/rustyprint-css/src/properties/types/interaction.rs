//! Value types of properties that only matter for interaction, scrolling or browser UI (CSS
//! Basic User Interface 4, CSS Scroll Snap 1, CSS Overflow 4, CSS Scrollbars 1, Pointer
//! Events, CSS Speech 1 and others). A PDF has no pointer, caret or scrolling, so these are
//! parsed and computed without changing what is drawn.

use std::sync::Arc;

use cssparser::Parser;

use super::{
    AccentColor, LengthPercentage, Result, SpecifiedLengthPercentage, fail, layers_css,
    length_percentage_css, next_is, parse_custom_ident, parse_length_percentage,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;
use crate::values::Color;

/// `caret-color`: `auto` or a color, as `accent-color` takes.
pub type CaretColor = AccentColor;

keywords!(CaretAnimation { Auto = "auto", Manual = "manual" });

keywords!(CaretShape { Auto = "auto", Bar = "bar", Block = "block", Underscore = "underscore" });

keywords!(
    /// `<cursor-predefined>`.
    CursorKind {
        Auto = "auto",
        Default = "default",
        None = "none",
        ContextMenu = "context-menu",
        Help = "help",
        Pointer = "pointer",
        Progress = "progress",
        Wait = "wait",
        Cell = "cell",
        Crosshair = "crosshair",
        Text = "text",
        VerticalText = "vertical-text",
        Alias = "alias",
        Copy = "copy",
        Move = "move",
        NoDrop = "no-drop",
        NotAllowed = "not-allowed",
        EResize = "e-resize",
        NResize = "n-resize",
        NeResize = "ne-resize",
        NwResize = "nw-resize",
        SResize = "s-resize",
        SeResize = "se-resize",
        SwResize = "sw-resize",
        WResize = "w-resize",
        EwResize = "ew-resize",
        NsResize = "ns-resize",
        NeswResize = "nesw-resize",
        NwseResize = "nwse-resize",
        ColResize = "col-resize",
        RowResize = "row-resize",
        AllScroll = "all-scroll",
        ZoomIn = "zoom-in",
        ZoomOut = "zoom-out",
        Grab = "grab",
        Grabbing = "grabbing",
    }
);

/// A cursor image's URL and optional hotspot.
pub type CursorImage = (Arc<str>, Option<(f32, f32)>);

/// `cursor`: cursor images with their optional hotspots, then a predefined cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct Cursor {
    pub images: Arc<[CursorImage]>,
    pub kind: CursorKind,
}

impl Property for Cursor {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let mut parts: Vec<String> = computed
            .images
            .iter()
            .map(|(url, hotspot)| {
                let mut out = String::from("url(");
                let _ = cssparser::serialize_string(url, &mut out);
                out.push(')');
                if let Some((x, y)) = hotspot {
                    out.push_str(&format!(
                        " {} {}",
                        serialize::number(f64::from(*x)),
                        serialize::number(f64::from(*y))
                    ));
                }
                out
            })
            .collect();
        parts.push(computed.kind.as_str().into());
        parts.join(", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let mut images = Vec::new();
        while let Ok(url) = input.try_parse(|input| input.expect_url()) {
            let url: Arc<str> = Arc::from(&*url);
            let hotspot = input
                .try_parse(|input| {
                    let x = super::parse_number(input)?;
                    let y = super::parse_number(input)?;
                    Ok::<_, cssparser::ParseError<()>>((x, y))
                })
                .ok();
            input.expect_comma()?;
            images.push((url, hotspot));
        }
        Ok(Cursor {
            images: images.into(),
            kind: CursorKind::parse(input)?,
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

keywords!(Interactivity { Auto = "auto", Inert = "inert" });

/// `interest-delay-start` and `-end`: `normal` or a delay in seconds.
pub struct InterestDelay;

impl Property for InterestDelay {
    type Specified = Option<f32>;
    type Computed = Option<f32>;

    fn to_css(computed: &Option<f32>) -> String {
        computed.map_or("normal".into(), |s| {
            format!("{}s", serialize::number(f64::from(s)))
        })
    }

    fn parse(input: &mut Parser<'_>) -> Result<Option<f32>> {
        if next_is(input, "normal") {
            return Ok(None);
        }
        super::animation::parse_time(input).map(Some)
    }

    fn compute(specified: &Option<f32>, _ctx: &ComputeContext) -> Option<f32> {
        *specified
    }
}

keywords!(
    /// `pointer-events`. The SVG keywords serialize in lowercase.
    PointerEvents {
        Auto = "auto",
        None = "none",
        VisiblePainted = "visiblepainted",
        VisibleFill = "visiblefill",
        VisibleStroke = "visiblestroke",
        Visible = "visible",
        Painted = "painted",
        Fill = "fill",
        Stroke = "stroke",
        All = "all",
    }
);

keywords!(Resize {
    None = "none",
    Both = "both",
    Horizontal = "horizontal",
    Vertical = "vertical",
    Block = "block",
    Inline = "inline",
});

keywords!(UserSelect { Auto = "auto", Text = "text", None = "none", All = "all" });

keywords!(WindowDrag { None = "none", Move = "move" });

// ---------------------------------------------------------------------------------------------
// Scrolling

keywords!(ScrollBehavior { Auto = "auto", Smooth = "smooth" });

keywords!(OverscrollBehavior { Contain = "contain", None = "none", Auto = "auto" });

keywords!(AutoOrNone { Auto = "auto", None = "none" });

keywords!(Overlay { None = "none", Auto = "auto" });

keywords!(ScrollInitialTarget { None = "none", Nearest = "nearest" });

keywords!(ScrollMarkerGroup { None = "none", Before = "before", After = "after" });

keywords!(ScrollTargetGroup { None = "none", Auto = "auto" });

keywords!(ScrollSnapStop { Normal = "normal", Always = "always" });

keywords!(ScrollbarWidth { Auto = "auto", Thin = "thin", None = "none" });

/// `scroll-padding-*`: `auto` or a non-negative `<length-percentage>`.
pub struct ScrollPadding;

impl Property for ScrollPadding {
    type Specified = Option<SpecifiedLengthPercentage>;
    type Computed = Option<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        computed
            .as_ref()
            .map_or("auto".into(), length_percentage_css)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if next_is(input, "auto") {
            return Ok(None);
        }
        parse_length_percentage(input, true, true).map(Some)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.as_ref().map(|value| value.compute(ctx, true))
    }
}

keywords!(SnapAlignment { None = "none", Start = "start", End = "end", Center = "center" });

/// `scroll-snap-align`: block and inline alignment, the second equal to the first when
/// omitted.
pub struct ScrollSnapAlign;

impl Property for ScrollSnapAlign {
    type Specified = (SnapAlignment, SnapAlignment);
    type Computed = (SnapAlignment, SnapAlignment);

    fn to_css(computed: &Self::Computed) -> String {
        if computed.0 == computed.1 {
            computed.0.as_str().into()
        } else {
            format!("{} {}", computed.0.as_str(), computed.1.as_str())
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        let block = SnapAlignment::parse(input)?;
        let inline = input.try_parse(SnapAlignment::parse).unwrap_or(block);
        Ok((block, inline))
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        *specified
    }
}

keywords!(SnapAxis { X = "x", Y = "y", Block = "block", Inline = "inline", Both = "both" });

keywords!(SnapStrictness { Mandatory = "mandatory", Proximity = "proximity" });

/// `scroll-snap-type`: `none` or an axis with its strictness.
pub struct ScrollSnapType;

impl Property for ScrollSnapType {
    type Specified = Option<(SnapAxis, SnapStrictness)>;
    type Computed = Option<(SnapAxis, SnapStrictness)>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "none".into(),
            Some((axis, SnapStrictness::Proximity)) => axis.as_str().into(),
            Some((axis, strictness)) => format!("{} {}", axis.as_str(), strictness.as_str()),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if next_is(input, "none") {
            return Ok(None);
        }
        let axis = SnapAxis::parse(input)?;
        let strictness = input
            .try_parse(SnapStrictness::parse)
            .unwrap_or(SnapStrictness::Proximity);
        Ok(Some((axis, strictness)))
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        *specified
    }
}

/// `scrollbar-color`: `auto` or the thumb and track colors.
pub struct ScrollbarColor;

impl Property for ScrollbarColor {
    type Specified = Option<(Color, Color)>;
    type Computed = Option<(Color, Color)>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "auto".into(),
            Some((thumb, track)) => {
                format!("{} {}", serialize::color(thumb), serialize::color(track))
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if next_is(input, "auto") {
            return Ok(None);
        }
        Ok(Some((Color::parse(input)?, Color::parse(input)?)))
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        let resolve = |color: &Color| match color {
            Color::Deferred(_) => Color::Absolute(color.resolve_in(&ctx.colors())),
            other => other.clone(),
        };
        specified
            .as_ref()
            .map(|(thumb, track)| (resolve(thumb), resolve(track)))
    }
}

/// `scrollbar-gutter`: `auto`, or `stable` with `both-edges` or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarGutter {
    Auto,
    Stable { both_edges: bool },
}

impl Property for ScrollbarGutter {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ScrollbarGutter::Auto => "auto".into(),
            ScrollbarGutter::Stable { both_edges: false } => "stable".into(),
            ScrollbarGutter::Stable { both_edges: true } => "stable both-edges".into(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if next_is(input, "auto") {
            return Ok(ScrollbarGutter::Auto);
        }
        let both_first = next_is(input, "both-edges");
        if !next_is(input, "stable") {
            return fail();
        }
        let both_edges = both_first || next_is(input, "both-edges");
        Ok(ScrollbarGutter::Stable { both_edges })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

// ---------------------------------------------------------------------------------------------
// Other properties without a visual effect

/// `text-size-adjust`: how mobile browsers inflate text; nothing is inflated on paper.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextSizeAdjust {
    None,
    Auto,
    /// A fraction: `150%` is 1.5.
    Percentage(f32),
}

impl Property for TextSizeAdjust {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            TextSizeAdjust::None => "none".into(),
            TextSizeAdjust::Auto => "auto".into(),
            TextSizeAdjust::Percentage(fraction) => serialize::percentage(*fraction),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if next_is(input, "none") {
            return Ok(TextSizeAdjust::None);
        }
        if next_is(input, "auto") {
            return Ok(TextSizeAdjust::Auto);
        }
        let fraction = input.expect_percentage()?;
        if fraction < 0.0 {
            return fail();
        }
        Ok(TextSizeAdjust::Percentage(fraction))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

/// `speak-as`: how speech reads the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpeakAs {
    pub spell_out: bool,
    pub digits: bool,
    /// `literal-punctuation` (`Some(true)`) or `no-punctuation` (`Some(false)`).
    pub punctuation: Option<bool>,
}

impl Property for SpeakAs {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let mut words = Vec::new();
        if computed.spell_out {
            words.push("spell-out");
        }
        if computed.digits {
            words.push("digits");
        }
        match computed.punctuation {
            Some(true) => words.push("literal-punctuation"),
            Some(false) => words.push("no-punctuation"),
            None => {}
        }
        if words.is_empty() {
            "normal".into()
        } else {
            words.join(" ")
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if next_is(input, "normal") {
            return Ok(SpeakAs::default());
        }
        let mut value = SpeakAs::default();
        let mut any = false;
        while let Ok(word) = input.try_parse(|input| input.expect_ident_cloned()) {
            match &*word.to_ascii_lowercase() {
                "spell-out" if !value.spell_out => value.spell_out = true,
                "digits" if !value.digits => value.digits = true,
                "literal-punctuation" if value.punctuation.is_none() => {
                    value.punctuation = Some(true)
                }
                "no-punctuation" if value.punctuation.is_none() => value.punctuation = Some(false),
                _ => return fail(),
            }
            any = true;
        }
        if !any {
            return fail();
        }
        Ok(value)
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}

keywords!(InterpolateSize { NumericOnly = "numeric-only", AllowKeywords = "allow-keywords" });

/// `will-change`: `auto` (an empty list) or the features the author expects to change.
pub struct WillChange;

impl Property for WillChange {
    type Specified = Arc<[Arc<str>]>;
    type Computed = Arc<[Arc<str>]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            "auto".into()
        } else {
            layers_css(computed, |name| name.to_string())
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if next_is(input, "auto") {
            return Ok(Arc::from([]));
        }
        let features = input.parse_comma_separated(|input| {
            parse_custom_ident(input, &["will-change", "none", "all", "auto"])
        })?;
        Ok(features.into())
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

/// `touch-action`: the touch gestures the browser handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchAction {
    Auto,
    None,
    Manipulation,
    /// Panning on each axis (as its keyword) and pinch zoom.
    Gestures {
        x: Option<&'static str>,
        y: Option<&'static str>,
        pinch_zoom: bool,
    },
}

impl Property for TouchAction {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            TouchAction::Auto => "auto".into(),
            TouchAction::None => "none".into(),
            TouchAction::Manipulation => "manipulation".into(),
            TouchAction::Gestures { x, y, pinch_zoom } => {
                let mut words: Vec<&str> = x.iter().chain(y.iter()).copied().collect();
                if *pinch_zoom {
                    words.push("pinch-zoom");
                }
                words.join(" ")
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        for (word, value) in [
            ("auto", TouchAction::Auto),
            ("none", TouchAction::None),
            ("manipulation", TouchAction::Manipulation),
        ] {
            if next_is(input, word) {
                return Ok(value);
            }
        }
        let (mut x, mut y, mut pinch_zoom) = (None, None, false);
        while let Ok(word) = input.try_parse(|input| input.expect_ident_cloned()) {
            match &*word.to_ascii_lowercase() {
                "pan-x" if x.is_none() => x = Some("pan-x"),
                "pan-left" if x.is_none() => x = Some("pan-left"),
                "pan-right" if x.is_none() => x = Some("pan-right"),
                "pan-y" if y.is_none() => y = Some("pan-y"),
                "pan-up" if y.is_none() => y = Some("pan-up"),
                "pan-down" if y.is_none() => y = Some("pan-down"),
                "pinch-zoom" if !pinch_zoom => pinch_zoom = true,
                _ => return fail(),
            }
        }
        if x.is_none() && y.is_none() && !pinch_zoom {
            return fail();
        }
        Ok(TouchAction::Gestures { x, y, pinch_zoom })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}
