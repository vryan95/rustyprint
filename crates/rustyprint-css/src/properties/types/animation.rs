//! Value types of animations, transitions, easing and timelines (CSS Animations 1 and 2, CSS
//! Transitions 1 and 2, CSS Easing 2, Scroll-driven Animations 1, CSS View Transitions 2).
//!
//! The PDF is a snapshot at time zero: animations apply their state at that time, and
//! transitions, timelines and triggers never start.

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{
    LengthPercentage, List, Result, SpecifiedLengthPercentage, fail, layers_css,
    length_percentage_css, names_or_none, next_is, parse_custom_ident, parse_length_percentage,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;
use crate::values::{Base, CalcContext, CalcNode, NumericType, Unit, is_math_function};

use super::anchor::parse_dashed_ident;

// ---------------------------------------------------------------------------------------------
// Times

/// Parses a `<time>` in seconds.
pub(crate) fn parse_time(input: &mut Parser<'_>) -> Result<f32> {
    match input.next()?.clone() {
        Token::Dimension { value, unit, .. } => match Unit::from_name(&unit) {
            Some(Unit::S) => Ok(value),
            Some(Unit::Ms) => Ok(value / 1000.0),
            _ => fail(),
        },
        Token::Function(name) if is_math_function(&name) => {
            let node = CalcNode::parse_function(input, &name)?;
            if node.numeric_type(None) != Some(NumericType::of(Base::Time)) {
                return fail();
            }
            node.evaluate(&CalcContext::default())
                .map(|seconds| seconds as f32)
                .ok_or_else(|| ParseError::custom(()))
        }
        _ => fail(),
    }
}

fn time_css(seconds: f32) -> String {
    format!("{}s", serialize::number(f64::from(seconds)))
}

/// `<time>`, computed to seconds; may be negative, as delays are.
pub struct Time;

impl Property for Time {
    type Specified = f32;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        time_css(*computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<f32> {
        parse_time(input)
    }

    fn compute(specified: &f32, _ctx: &ComputeContext) -> f32 {
        *specified
    }
}

/// A non-negative `<time>`, as `transition-duration` takes.
pub struct NonNegativeTime;

impl Property for NonNegativeTime {
    type Specified = f32;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        time_css(*computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<f32> {
        let seconds = parse_time(input)?;
        if seconds < 0.0 {
            return fail();
        }
        Ok(seconds)
    }

    fn compute(specified: &f32, _ctx: &ComputeContext) -> f32 {
        *specified
    }
}

/// `auto | <time [0s,∞]>`, an animation's duration; `None` is `auto`, which is `0s` for
/// animations on the document timeline.
pub struct AnimationDurationValue;

impl Property for AnimationDurationValue {
    type Specified = Option<f32>;
    type Computed = Option<f32>;

    fn to_css(computed: &Option<f32>) -> String {
        computed.map_or("auto".into(), time_css)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Option<f32>> {
        if next_is(input, "auto") {
            return Ok(None);
        }
        NonNegativeTime::parse(input).map(Some)
    }

    fn compute(specified: &Option<f32>, _ctx: &ComputeContext) -> Option<f32> {
        *specified
    }
}

// ---------------------------------------------------------------------------------------------
// Easing

keywords!(
    /// Where the jumps of `steps()` go.
    StepPosition {
        JumpStart = "jump-start",
        JumpEnd = "jump-end",
        JumpNone = "jump-none",
        JumpBoth = "jump-both",
        Start = "start",
        End = "end",
    }
);

/// An `<easing-function>` (CSS Easing 2).
#[derive(Debug, Clone, PartialEq)]
pub enum Easing {
    /// The `linear` keyword.
    Linear,
    /// `linear()` with its control points as `(output, input)` pairs, every input filled in.
    LinearPoints(Arc<[(f32, f32)]>),
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier([f32; 4]),
    StepStart,
    StepEnd,
    Steps(u32, StepPosition),
}

impl Easing {
    /// The output progress for an input progress `x`.
    pub fn apply(&self, x: f32) -> f32 {
        match self {
            Easing::Linear => x,
            Easing::LinearPoints(points) => linear_points(points, x),
            Easing::Ease => cubic_bezier([0.25, 0.1, 0.25, 1.0], x),
            Easing::EaseIn => cubic_bezier([0.42, 0.0, 1.0, 1.0], x),
            Easing::EaseOut => cubic_bezier([0.0, 0.0, 0.58, 1.0], x),
            Easing::EaseInOut => cubic_bezier([0.42, 0.0, 0.58, 1.0], x),
            Easing::CubicBezier(points) => cubic_bezier(*points, x),
            Easing::StepStart => steps(1, StepPosition::JumpStart, x),
            Easing::StepEnd => steps(1, StepPosition::JumpEnd, x),
            Easing::Steps(count, position) => steps(*count, *position, x),
        }
    }
}

/// `linear()`: straight lines between the control points, extended past the first and last.
fn linear_points(points: &[(f32, f32)], x: f32) -> f32 {
    let [first, .., last] = points else {
        return points.first().map_or(x, |p| p.0);
    };
    // The segment containing `x`: the last point at or before it starts it.
    let start = match points.iter().rposition(|p| p.1 <= x) {
        None => 0,
        Some(index) if index == points.len() - 1 => {
            if x == last.1 {
                return last.0;
            }
            index - 1
        }
        Some(index) => index,
    };
    let (a, b) = (points[start], points[start + 1]);
    if x < first.1 && first.1 == b.1 {
        return first.0;
    }
    if b.1 == a.1 {
        return b.0;
    }
    a.0 + (b.0 - a.0) * (x - a.1) / (b.1 - a.1)
}

/// A cubic Bézier curve from `(0, 0)` to `(1, 1)` with control points `(x1, y1)` and
/// `(x2, y2)`, evaluated at `x`.
fn cubic_bezier([x1, y1, x2, y2]: [f32; 4], x: f32) -> f32 {
    // Outside 0..1 the curve extends along its end tangents.
    if x <= 0.0 {
        let slope = if x1 > 0.0 {
            y1 / x1
        } else if y1 == 0.0 && x2 > 0.0 {
            y2 / x2
        } else {
            0.0
        };
        return slope * x;
    }
    if x >= 1.0 {
        let slope = if x2 < 1.0 {
            (y2 - 1.0) / (x2 - 1.0)
        } else if y2 == 1.0 && x1 < 1.0 {
            (y1 - 1.0) / (x1 - 1.0)
        } else {
            0.0
        };
        return 1.0 + slope * (x - 1.0);
    }
    let curve = |t: f64, p1: f64, p2: f64| {
        let u = 1.0 - t;
        3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
    };
    let (x1, y1, x2, y2) = (f64::from(x1), f64::from(y1), f64::from(x2), f64::from(y2));
    // x(t) increases on 0..1 because x1 and x2 are in 0..1, so bisection finds t.
    let (mut low, mut high) = (0.0f64, 1.0f64);
    let target = f64::from(x);
    for _ in 0..64 {
        let mid = (low + high) / 2.0;
        if curve(mid, x1, x2) < target {
            low = mid;
        } else {
            high = mid;
        }
    }
    curve((low + high) / 2.0, y1, y2) as f32
}

/// `steps()` at `x`, before any jump at `x` itself is taken (CSS Easing 2 § 2.3.1).
fn steps(count: u32, position: StepPosition, x: f32) -> f32 {
    let count = count.max(1) as f32;
    let mut step = (x * count).floor();
    if matches!(
        position,
        StepPosition::JumpStart | StepPosition::Start | StepPosition::JumpBoth
    ) {
        step += 1.0;
    }
    let jumps = match position {
        StepPosition::JumpBoth => count + 1.0,
        StepPosition::JumpNone => count - 1.0,
        _ => count,
    };
    if x >= 0.0 && step < 0.0 {
        step = 0.0;
    }
    if x <= 1.0 && step > jumps {
        step = jumps;
    }
    step / jumps
}

/// Parses the arguments of `linear()`: `[ <number> && <percentage>{0,2} ]#`.
fn parse_linear_points(input: &mut Parser<'_>) -> Result<Vec<(f32, f32)>> {
    let mut points: Vec<(f32, Option<f32>)> = Vec::new();
    input.parse_comma_separated(|input| {
        let mut output = None;
        let mut inputs = Vec::new();
        while !input.is_exhausted() {
            match input.next()?.clone() {
                Token::Number { value, .. } if output.is_none() => output = Some(value),
                Token::Percentage { unit_value, .. } if inputs.len() < 2 => inputs.push(unit_value),
                _ => return fail(),
            }
        }
        let output = output.ok_or_else(|| ParseError::custom(()))?;
        if inputs.is_empty() {
            points.push((output, None));
        }
        for at in inputs {
            points.push((output, Some(at)));
        }
        Ok(())
    })?;
    if points.len() < 2 {
        return fail();
    }
    // Missing inputs: 0 for the first point, at least 1 for the last, then never decreasing,
    // with runs of missing ones spread evenly (CSS Easing 2 § 2.2.1).
    if points[0].1.is_none() {
        points[0].1 = Some(0.0);
    }
    let last = points.len() - 1;
    if points[last].1.is_none() {
        let largest = points.iter().filter_map(|p| p.1).fold(1.0f32, f32::max);
        points[last].1 = Some(largest);
    }
    let mut largest = f32::MIN;
    for point in &mut points {
        if let Some(at) = &mut point.1 {
            *at = at.max(largest);
            largest = *at;
        }
    }
    let mut index = 0;
    while index < points.len() {
        if points[index].1.is_some() {
            index += 1;
            continue;
        }
        let start = index - 1;
        let end = (index..points.len())
            .find(|&i| points[i].1.is_some())
            .expect("the last point has an input");
        let (from, to) = (points[start].1.unwrap(), points[end].1.unwrap());
        let span = (end - start) as f32;
        for (i, point) in points.iter_mut().enumerate().take(end).skip(index) {
            point.1 = Some(from + (to - from) * (i - start) as f32 / span);
        }
        index = end;
    }
    Ok(points.into_iter().map(|(o, i)| (o, i.unwrap())).collect())
}

impl Property for Easing {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        let number = |v: f32| serialize::number(f64::from(v));
        match computed {
            Easing::Linear => "linear".into(),
            Easing::LinearPoints(points) => format!(
                "linear({})",
                points
                    .iter()
                    .map(|(output, at)| format!(
                        "{} {}",
                        number(*output),
                        serialize::percentage(*at)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Easing::Ease => "ease".into(),
            Easing::EaseIn => "ease-in".into(),
            Easing::EaseOut => "ease-out".into(),
            Easing::EaseInOut => "ease-in-out".into(),
            Easing::CubicBezier(p) => format!(
                "cubic-bezier({}, {}, {}, {})",
                number(p[0]),
                number(p[1]),
                number(p[2]),
                number(p[3])
            ),
            Easing::StepStart => "steps(1, start)".into(),
            Easing::StepEnd => "steps(1)".into(),
            Easing::Steps(count, StepPosition::End | StepPosition::JumpEnd) => {
                format!("steps({count})")
            }
            Easing::Steps(count, position) => format!("steps({count}, {})", position.as_str()),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) {
            return Ok(match &*ident.to_ascii_lowercase() {
                "linear" => Easing::Linear,
                "ease" => Easing::Ease,
                "ease-in" => Easing::EaseIn,
                "ease-out" => Easing::EaseOut,
                "ease-in-out" => Easing::EaseInOut,
                "step-start" => Easing::StepStart,
                "step-end" => Easing::StepEnd,
                _ => return fail(),
            });
        }
        let name = input.expect_function()?.to_ascii_lowercase();
        input.parse_nested_block(|input| match &*name {
            "linear" => Ok(Easing::LinearPoints(parse_linear_points(input)?.into())),
            "cubic-bezier" => {
                let mut values = [0.0; 4];
                for (index, value) in values.iter_mut().enumerate() {
                    if index > 0 {
                        input.expect_comma()?;
                    }
                    *value = super::parse_number(input)?;
                }
                // The x coordinates must stay within 0..1 so the curve is a function of x.
                if !(0.0..=1.0).contains(&values[0]) || !(0.0..=1.0).contains(&values[2]) {
                    return fail();
                }
                Ok(Easing::CubicBezier(values))
            }
            "steps" => {
                let count = super::parse_number(input)?;
                if count.fract() != 0.0 || count < 1.0 {
                    return fail();
                }
                let position = if input.try_parse(|input| input.expect_comma()).is_ok() {
                    StepPosition::parse(input)?
                } else {
                    StepPosition::End
                };
                if position == StepPosition::JumpNone && count < 2.0 {
                    return fail();
                }
                Ok(Easing::Steps(count as u32, position))
            }
            _ => fail(),
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

// ---------------------------------------------------------------------------------------------
// Animations

/// One item of `animation-name`: `none` or the name of an `@keyframes` rule.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationName {
    None,
    Ident(Arc<str>),
    String(Arc<str>),
}

impl AnimationName {
    /// The `@keyframes` name this refers to, if any.
    pub fn name(&self) -> Option<&str> {
        match self {
            AnimationName::None => None,
            AnimationName::Ident(name) | AnimationName::String(name) => Some(name),
        }
    }
}

impl Property for AnimationName {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            AnimationName::None => "none".into(),
            AnimationName::Ident(name) => name.to_string(),
            AnimationName::String(name) => {
                let mut quoted = String::new();
                let _ = cssparser::serialize_string(name, &mut quoted);
                quoted
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if next_is(input, "none") {
            return Ok(AnimationName::None);
        }
        if let Ok(name) = input.try_parse(|input| input.expect_string_cloned()) {
            return Ok(AnimationName::String(Arc::from(&*name)));
        }
        Ok(AnimationName::Ident(parse_custom_ident(input, &["none"])?))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// `animation-iteration-count`: `infinite` or a non-negative number of iterations.
pub struct IterationCount;

impl Property for IterationCount {
    type Specified = f32;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        if computed.is_infinite() {
            "infinite".into()
        } else {
            serialize::number(f64::from(*computed))
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<f32> {
        if next_is(input, "infinite") {
            return Ok(f32::INFINITY);
        }
        let count = super::parse_number(input)?;
        if count < 0.0 {
            return fail();
        }
        Ok(count)
    }

    fn compute(specified: &f32, _ctx: &ComputeContext) -> f32 {
        *specified
    }
}

keywords!(AnimationDirection {
    Normal = "normal",
    Reverse = "reverse",
    Alternate = "alternate",
    AlternateReverse = "alternate-reverse",
});

keywords!(AnimationFillMode {
    None = "none",
    Forwards = "forwards",
    Backwards = "backwards",
    Both = "both",
});

keywords!(AnimationPlayState { Running = "running", Paused = "paused" });

keywords!(AnimationComposition { Replace = "replace", Add = "add", Accumulate = "accumulate" });

keywords!(ScrollerKind { Nearest = "nearest", Root = "root", SelfElement = "self" });

keywords!(TimelineAxis { Block = "block", Inline = "inline", X = "x", Y = "y" });

/// `auto | <length-percentage>`: one side of a view timeline's inset.
#[derive(Debug, Clone, PartialEq)]
pub enum InsetSide<L> {
    Auto,
    Value(L),
}

fn parse_inset_side(input: &mut Parser<'_>) -> Result<InsetSide<SpecifiedLengthPercentage>> {
    if next_is(input, "auto") {
        return Ok(InsetSide::Auto);
    }
    Ok(InsetSide::Value(parse_length_percentage(
        input, true, false,
    )?))
}

fn compute_inset_side(
    side: &InsetSide<SpecifiedLengthPercentage>,
    ctx: &ComputeContext,
) -> InsetSide<LengthPercentage> {
    match side {
        InsetSide::Auto => InsetSide::Auto,
        InsetSide::Value(value) => InsetSide::Value(value.compute(ctx, false)),
    }
}

fn inset_side_css(side: &InsetSide<LengthPercentage>) -> String {
    match side {
        InsetSide::Auto => "auto".into(),
        InsetSide::Value(value) => length_percentage_css(value),
    }
}

/// A view timeline's inset: start and end, the end equal to the start when omitted.
pub type ViewInset<L> = (InsetSide<L>, InsetSide<L>);

fn parse_view_inset(input: &mut Parser<'_>) -> Result<ViewInset<SpecifiedLengthPercentage>> {
    let start = parse_inset_side(input)?;
    let end = input
        .try_parse(parse_inset_side)
        .unwrap_or_else(|_| start.clone());
    Ok((start, end))
}

fn view_inset_css((start, end): &ViewInset<LengthPercentage>) -> String {
    if start == end {
        inset_side_css(start)
    } else {
        format!("{} {}", inset_side_css(start), inset_side_css(end))
    }
}

/// `<single-animation-timeline>`: what drives an animation.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationTimeline<L> {
    /// The document timeline, at time zero in the PDF.
    Auto,
    None,
    /// A named scroll or view timeline.
    Named(Arc<str>),
    /// `scroll()`: the progress of scrolling a scroller.
    Scroll {
        scroller: ScrollerKind,
        axis: TimelineAxis,
    },
    /// `view()`: the progress of the element through its scrollport.
    View {
        axis: TimelineAxis,
        inset: ViewInset<L>,
    },
}

pub(crate) fn parse_timeline(
    input: &mut Parser<'_>,
) -> Result<AnimationTimeline<SpecifiedLengthPercentage>> {
    if next_is(input, "auto") {
        return Ok(AnimationTimeline::Auto);
    }
    if next_is(input, "none") {
        return Ok(AnimationTimeline::None);
    }
    if let Ok(name) = input.try_parse(parse_dashed_ident) {
        return Ok(AnimationTimeline::Named(name));
    }
    let function = input.expect_function()?.to_ascii_lowercase();
    input.parse_nested_block(|input| match &*function {
        "scroll" => {
            let (mut scroller, mut axis) = (None, None);
            while !input.is_exhausted() {
                if scroller.is_none()
                    && let Ok(kind) = input.try_parse(ScrollerKind::parse)
                {
                    scroller = Some(kind);
                    continue;
                }
                if axis.is_none()
                    && let Ok(value) = input.try_parse(TimelineAxis::parse)
                {
                    axis = Some(value);
                    continue;
                }
                return fail();
            }
            Ok(AnimationTimeline::Scroll {
                scroller: scroller.unwrap_or(ScrollerKind::Nearest),
                axis: axis.unwrap_or(TimelineAxis::Block),
            })
        }
        "view" => {
            let (mut axis, mut inset) = (None, None);
            while !input.is_exhausted() {
                if axis.is_none()
                    && let Ok(value) = input.try_parse(TimelineAxis::parse)
                {
                    axis = Some(value);
                    continue;
                }
                if inset.is_none()
                    && let Ok(value) = input.try_parse(parse_view_inset)
                {
                    inset = Some(value);
                    continue;
                }
                return fail();
            }
            Ok(AnimationTimeline::View {
                axis: axis.unwrap_or(TimelineAxis::Block),
                inset: inset.unwrap_or((InsetSide::Auto, InsetSide::Auto)),
            })
        }
        _ => fail(),
    })
}

/// `<single-animation-timeline>`.
pub struct TimelineValue;

impl Property for TimelineValue {
    type Specified = AnimationTimeline<SpecifiedLengthPercentage>;
    type Computed = AnimationTimeline<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            AnimationTimeline::Auto => "auto".into(),
            AnimationTimeline::None => "none".into(),
            AnimationTimeline::Named(name) => name.to_string(),
            AnimationTimeline::Scroll { scroller, axis } => {
                let mut parts = Vec::new();
                if *scroller != ScrollerKind::Nearest {
                    parts.push(scroller.as_str());
                }
                if *axis != TimelineAxis::Block {
                    parts.push(axis.as_str());
                }
                format!("scroll({})", parts.join(" "))
            }
            AnimationTimeline::View { axis, inset } => {
                let mut parts = Vec::new();
                if *axis != TimelineAxis::Block {
                    parts.push(axis.as_str().to_owned());
                }
                if *inset != (InsetSide::Auto, InsetSide::Auto) {
                    parts.push(view_inset_css(inset));
                }
                format!("view({})", parts.join(" "))
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_timeline(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        match specified {
            AnimationTimeline::Auto => AnimationTimeline::Auto,
            AnimationTimeline::None => AnimationTimeline::None,
            AnimationTimeline::Named(name) => AnimationTimeline::Named(name.clone()),
            AnimationTimeline::Scroll { scroller, axis } => AnimationTimeline::Scroll {
                scroller: *scroller,
                axis: *axis,
            },
            AnimationTimeline::View { axis, inset } => AnimationTimeline::View {
                axis: *axis,
                inset: (
                    compute_inset_side(&inset.0, ctx),
                    compute_inset_side(&inset.1, ctx),
                ),
            },
        }
    }
}

keywords!(
    /// What a trigger does to an animation.
    AnimationAction {
        None = "none",
        Play = "play",
        PlayOnce = "play-once",
        PlayForwards = "play-forwards",
        PlayBackwards = "play-backwards",
        Pause = "pause",
        Reset = "reset",
        Replay = "replay",
    }
);

/// One item of `animation-trigger`: the triggers an animation listens to, each with its
/// actions; empty for `none`.
pub type TriggerItem = Arc<[(Arc<str>, Arc<[AnimationAction]>)]>;

/// `animation-trigger`.
pub struct AnimationTriggerItem;

impl Property for AnimationTriggerItem {
    type Specified = TriggerItem;
    type Computed = TriggerItem;

    fn to_css(computed: &TriggerItem) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        computed
            .iter()
            .map(|(name, actions)| {
                let actions: Vec<&str> = actions.iter().map(|a| a.as_str()).collect();
                format!("{name} {}", actions.join(" "))
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<TriggerItem> {
        if next_is(input, "none") {
            return Ok(Arc::from([]));
        }
        let mut triggers = Vec::new();
        while let Ok(name) = input.try_parse(parse_dashed_ident) {
            let mut actions = Vec::new();
            while let Ok(action) = input.try_parse(AnimationAction::parse) {
                actions.push(action);
            }
            if actions.is_empty() {
                return fail();
            }
            triggers.push((name, actions.into()));
        }
        if triggers.is_empty() {
            return fail();
        }
        Ok(triggers.into())
    }

    fn compute(specified: &TriggerItem, _ctx: &ComputeContext) -> TriggerItem {
        specified.clone()
    }
}

pub type AnimationNames = List<AnimationName>;
pub type AnimationDurations = List<AnimationDurationValue>;
pub type TimingFunctions = List<Easing>;
pub type Times = List<Time>;
pub type NonNegativeTimes = List<NonNegativeTime>;
pub type IterationCounts = List<IterationCount>;
pub type AnimationDirections = List<AnimationDirection>;
pub type AnimationFillModes = List<AnimationFillMode>;
pub type AnimationPlayStates = List<AnimationPlayState>;
pub type AnimationCompositions = List<AnimationComposition>;
pub type AnimationTimelines = List<TimelineValue>;
pub type AnimationTriggers = List<AnimationTriggerItem>;

/// One `animation` in the shorthand: values for each longhand, `None` where omitted.
#[derive(Default)]
pub(crate) struct SingleAnimation {
    pub duration: Option<Option<f32>>,
    pub easing: Option<Easing>,
    pub delay: Option<f32>,
    pub iterations: Option<f32>,
    pub direction: Option<AnimationDirection>,
    pub fill_mode: Option<AnimationFillMode>,
    pub play_state: Option<AnimationPlayState>,
    pub name: Option<AnimationName>,
    pub timeline: Option<AnimationTimeline<SpecifiedLengthPercentage>>,
}

/// Parses one item of the `animation` shorthand. Keywords of the other longhands take
/// precedence over names, as long as their longhand is not set yet.
pub(crate) fn parse_single_animation(input: &mut Parser<'_>) -> Result<SingleAnimation> {
    let mut a = SingleAnimation::default();
    let mut any = false;
    loop {
        if a.duration.is_none()
            && let Ok(time) = input.try_parse(AnimationDurationValue::parse)
        {
            a.duration = Some(time);
        } else if a.duration.is_some()
            && a.delay.is_none()
            && let Ok(time) = input.try_parse(parse_time)
        {
            a.delay = Some(time);
        } else if a.easing.is_none()
            && let Ok(easing) = input.try_parse(Easing::parse)
        {
            a.easing = Some(easing);
        } else if a.iterations.is_none()
            && let Ok(count) = input.try_parse(IterationCount::parse)
        {
            a.iterations = Some(count);
        } else if a.direction.is_none()
            && let Ok(direction) = input.try_parse(AnimationDirection::parse)
        {
            a.direction = Some(direction);
        } else if a.fill_mode.is_none()
            && let Ok(mode) = input.try_parse(AnimationFillMode::parse)
        {
            a.fill_mode = Some(mode);
        } else if a.play_state.is_none()
            && let Ok(state) = input.try_parse(AnimationPlayState::parse)
        {
            a.play_state = Some(state);
        } else if a.timeline.is_none()
            && let Ok(timeline) = input.try_parse(|input| {
                // Only the functions: identifiers here are animation names.
                match parse_timeline(input)? {
                    t @ (AnimationTimeline::Scroll { .. } | AnimationTimeline::View { .. }) => {
                        Ok(t)
                    }
                    _ => fail(),
                }
            })
        {
            a.timeline = Some(timeline);
        } else if a.name.is_none()
            && let Ok(name) = input.try_parse(AnimationName::parse)
        {
            a.name = Some(name);
        } else {
            break;
        }
        any = true;
    }
    if !any {
        return fail();
    }
    Ok(a)
}

// ---------------------------------------------------------------------------------------------
// Transitions

/// One item of `transition-property`.
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionProperty {
    All,
    Name(Arc<str>),
}

fn parse_transition_property(input: &mut Parser<'_>) -> Result<TransitionProperty> {
    if next_is(input, "all") {
        return Ok(TransitionProperty::All);
    }
    Ok(TransitionProperty::Name(parse_custom_ident(
        input,
        &["none"],
    )?))
}

/// `transition-property`: `none` (an empty list) or the properties that transition.
pub struct TransitionProperties;

impl Property for TransitionProperties {
    type Specified = Arc<[TransitionProperty]>;
    type Computed = Arc<[TransitionProperty]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        layers_css(computed, |p| match p {
            TransitionProperty::All => "all".into(),
            TransitionProperty::Name(name) => name.to_string(),
        })
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if next_is(input, "none") {
            return Ok(Arc::from([]));
        }
        super::layers(input, parse_transition_property)
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

keywords!(TransitionBehavior { Normal = "normal", AllowDiscrete = "allow-discrete" });

pub type TransitionBehaviors = List<TransitionBehavior>;

/// One `transition` in the shorthand. `property` is `Some(None)` for `none`.
#[derive(Default)]
pub(crate) struct SingleTransition {
    pub property: Option<Option<TransitionProperty>>,
    pub duration: Option<f32>,
    pub easing: Option<Easing>,
    pub delay: Option<f32>,
    pub behavior: Option<TransitionBehavior>,
}

pub(crate) fn parse_single_transition(input: &mut Parser<'_>) -> Result<SingleTransition> {
    let mut t = SingleTransition::default();
    let mut any = false;
    loop {
        if t.duration.is_none()
            && let Ok(time) = input.try_parse(NonNegativeTime::parse)
        {
            t.duration = Some(time);
        } else if t.duration.is_some()
            && t.delay.is_none()
            && let Ok(time) = input.try_parse(parse_time)
        {
            t.delay = Some(time);
        } else if t.easing.is_none()
            && let Ok(easing) = input.try_parse(Easing::parse)
        {
            t.easing = Some(easing);
        } else if t.behavior.is_none()
            && let Ok(behavior) = input.try_parse(TransitionBehavior::parse)
        {
            t.behavior = Some(behavior);
        } else if t.property.is_none() && next_is(input, "none") {
            t.property = Some(None);
        } else if t.property.is_none()
            && let Ok(property) = input.try_parse(parse_transition_property)
        {
            t.property = Some(Some(property));
        } else {
            break;
        }
        any = true;
    }
    if !any {
        return fail();
    }
    Ok(t)
}

// ---------------------------------------------------------------------------------------------
// Timelines and ranges

keywords!(TimelineRangeName {
    Cover = "cover",
    Contain = "contain",
    Entry = "entry",
    Exit = "exit",
    EntryCrossing = "entry-crossing",
    ExitCrossing = "exit-crossing",
});

/// A point of an animation or trigger range on a timeline.
#[derive(Debug, Clone, PartialEq)]
pub enum RangePoint<L> {
    /// `auto`, for trigger active ranges only.
    Auto,
    Normal,
    Offset(L),
    /// A named range, with the offset within it.
    Named(TimelineRangeName, L),
}

/// `animation-range-start` (`END` false) or `-end` items; `AUTO` allows `auto`.
pub struct RangeValue<const END: bool, const AUTO: bool>;

pub(crate) fn parse_range_point(
    input: &mut Parser<'_>,
    end: bool,
    auto: bool,
) -> Result<RangePoint<SpecifiedLengthPercentage>> {
    if auto && next_is(input, "auto") {
        return Ok(RangePoint::Auto);
    }
    if next_is(input, "normal") {
        return Ok(RangePoint::Normal);
    }
    if let Ok(name) = input.try_parse(TimelineRangeName::parse) {
        // A name alone is its start (0%) or end (100%).
        let offset = input
            .try_parse(|input| parse_length_percentage(input, true, false))
            .unwrap_or(SpecifiedLengthPercentage::Percentage(if end {
                1.0
            } else {
                0.0
            }));
        return Ok(RangePoint::Named(name, offset));
    }
    Ok(RangePoint::Offset(parse_length_percentage(
        input, true, false,
    )?))
}

impl<const END: bool, const AUTO: bool> Property for RangeValue<END, AUTO> {
    type Specified = RangePoint<SpecifiedLengthPercentage>;
    type Computed = RangePoint<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            RangePoint::Auto => "auto".into(),
            RangePoint::Normal => "normal".into(),
            RangePoint::Offset(offset) => length_percentage_css(offset),
            RangePoint::Named(name, offset) => {
                format!("{} {}", name.as_str(), length_percentage_css(offset))
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_range_point(input, END, AUTO)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        match specified {
            RangePoint::Auto => RangePoint::Auto,
            RangePoint::Normal => RangePoint::Normal,
            RangePoint::Offset(offset) => RangePoint::Offset(offset.compute(ctx, false)),
            RangePoint::Named(name, offset) => RangePoint::Named(*name, offset.compute(ctx, false)),
        }
    }
}

pub type RangeStarts = List<RangeValue<false, false>>;
pub type RangeEnds = List<RangeValue<true, false>>;
pub type ActiveRangeStarts = List<RangeValue<false, true>>;
pub type ActiveRangeEnds = List<RangeValue<true, true>>;

/// The end of a range whose shorthand omits it: the end of a named start's range, or
/// `normal`.
pub(crate) fn implied_range_end(
    start: &RangePoint<SpecifiedLengthPercentage>,
    auto: bool,
) -> RangePoint<SpecifiedLengthPercentage> {
    match start {
        RangePoint::Named(name, _) => {
            RangePoint::Named(*name, SpecifiedLengthPercentage::Percentage(1.0))
        }
        _ if auto => RangePoint::Auto,
        _ => RangePoint::Normal,
    }
}

/// `none` or a `<dashed-ident>` naming a timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineName(pub Option<Arc<str>>);

impl Property for TimelineName {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        computed.0.as_deref().unwrap_or("none").into()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if next_is(input, "none") {
            return Ok(TimelineName(None));
        }
        Ok(TimelineName(Some(parse_dashed_ident(input)?)))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

pub type TimelineNames = List<TimelineName>;
pub type TimelineAxes = List<TimelineAxis>;

/// `view-timeline-inset` items.
pub struct ViewInsetValue;

impl Property for ViewInsetValue {
    type Specified = ViewInset<SpecifiedLengthPercentage>;
    type Computed = ViewInset<LengthPercentage>;

    fn to_css(computed: &Self::Computed) -> String {
        view_inset_css(computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        parse_view_inset(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        (
            compute_inset_side(&specified.0, ctx),
            compute_inset_side(&specified.1, ctx),
        )
    }
}

pub type ViewInsets = List<ViewInsetValue>;

/// `none | <dashed-ident>#`, as `timeline-scope` and `timeline-trigger-name` take; empty for
/// `none`.
pub struct DashedNames;

impl Property for DashedNames {
    type Specified = Arc<[Arc<str>]>;
    type Computed = Arc<[Arc<str>]>;

    fn to_css(computed: &Self::Computed) -> String {
        names_or_none(computed, ", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if next_is(input, "none") {
            return Ok(Arc::from([]));
        }
        Ok(input.parse_comma_separated(parse_dashed_ident)?.into())
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

/// `trigger-scope`: `none`, `all` or the trigger names scoped to the element.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerScope {
    None,
    All,
    Names(Arc<[Arc<str>]>),
}

impl Property for TriggerScope {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            TriggerScope::None => "none".into(),
            TriggerScope::All => "all".into(),
            TriggerScope::Names(names) => names_or_none(names, ", "),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if next_is(input, "none") {
            return Ok(TriggerScope::None);
        }
        if next_is(input, "all") {
            return Ok(TriggerScope::All);
        }
        Ok(TriggerScope::Names(
            input.parse_comma_separated(parse_dashed_ident)?.into(),
        ))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

// ---------------------------------------------------------------------------------------------
// View transitions

/// `view-transition-name`: `none`, `match-element` or a name.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewTransitionName {
    None,
    MatchElement,
    Name(Arc<str>),
}

impl Property for ViewTransitionName {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ViewTransitionName::None => "none".into(),
            ViewTransitionName::MatchElement => "match-element".into(),
            ViewTransitionName::Name(name) => name.to_string(),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if next_is(input, "none") {
            return Ok(ViewTransitionName::None);
        }
        if next_is(input, "match-element") {
            return Ok(ViewTransitionName::MatchElement);
        }
        Ok(ViewTransitionName::Name(parse_custom_ident(input, &[])?))
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// `none | <custom-ident>+`, as `view-transition-class` and `container-name` take; empty for
/// `none`.
pub struct IdentList<const RESERVED: u8>;

/// Identifiers reserved by an [`IdentList`] property besides `none`.
fn reserved(kind: u8) -> &'static [&'static str] {
    match kind {
        // `container-name` cannot use the words of container queries.
        1 => &["none", "and", "or", "not"],
        _ => &["none"],
    }
}

impl<const RESERVED: u8> Property for IdentList<RESERVED> {
    type Specified = Arc<[Arc<str>]>;
    type Computed = Arc<[Arc<str>]>;

    fn to_css(computed: &Self::Computed) -> String {
        names_or_none(computed, " ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if next_is(input, "none") {
            return Ok(Arc::from([]));
        }
        let mut names = vec![parse_custom_ident(input, reserved(RESERVED))?];
        while let Ok(name) = input.try_parse(|input| parse_custom_ident(input, reserved(RESERVED)))
        {
            names.push(name);
        }
        Ok(names.into())
    }

    fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
        specified.clone()
    }
}

pub type ViewTransitionClass = IdentList<0>;

keywords!(ViewTransitionScope { None = "none", All = "all" });

#[cfg(test)]
mod tests {
    use super::*;

    fn easing(text: &str) -> Easing {
        Parser::new(text).parse_entirely(Easing::parse).unwrap()
    }

    #[test]
    fn cubic_beziers() {
        let ease = easing("ease");
        assert_eq!(ease.apply(0.0), 0.0);
        assert!((ease.apply(1.0) - 1.0).abs() < 1e-6);
        assert!((ease.apply(0.5) - 0.8024).abs() < 1e-3);
        assert!((easing("linear").apply(0.3) - 0.3).abs() < 1e-6);
        assert!((easing("cubic-bezier(0, 0, 1, 1)").apply(0.25) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn steps_jump() {
        assert_eq!(easing("steps(4)").apply(0.3), 0.25);
        assert_eq!(easing("steps(4, start)").apply(0.3), 0.5);
        assert_eq!(easing("step-start").apply(0.0), 1.0);
        assert_eq!(easing("step-end").apply(0.99), 0.0);
        assert_eq!(easing("steps(2, jump-both)").apply(0.0), 1.0 / 3.0);
        assert_eq!(easing("steps(3, jump-none)").apply(0.5), 0.5);
    }

    #[test]
    fn linear_functions() {
        let e = easing("linear(0, 0.25 75%, 1)");
        assert_eq!(
            e,
            Easing::LinearPoints(Arc::from([(0.0, 0.0), (0.25, 0.75), (1.0, 1.0)]))
        );
        assert!((e.apply(0.375) - 0.125).abs() < 1e-6);
        let spread = easing("linear(0, 0.5, 0.5, 1)");
        let Easing::LinearPoints(points) = &spread else {
            panic!()
        };
        assert!((points[1].1 - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(
            easing("linear(0, 1 50% 75%, 0)"),
            Easing::LinearPoints(Arc::from([(0.0, 0.0), (1.0, 0.5), (1.0, 0.75), (0.0, 1.0)]))
        );
    }
}
