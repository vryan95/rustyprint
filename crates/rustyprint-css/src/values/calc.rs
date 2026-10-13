//! Math functions: `calc()`, `min()`, `round()`, trigonometry and the rest (CSS Values 4 § 10).

use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;
use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{LengthContext, LengthUnit};

type Result<T> = std::result::Result<T, ParseError<()>>;

/// Resolves keywords that are not math constants, such as relative color channel names.
pub type Vars<'a> = &'a dyn Fn(&str) -> Option<f64>;

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

/// A non-length unit allowed in math expressions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Length(LengthUnit),
    Deg,
    Grad,
    Rad,
    Turn,
    S,
    Ms,
    Hz,
    KHz,
    Dppx,
    Dpi,
    Dpcm,
    Fr,
}

impl Unit {
    pub fn from_name(name: &str) -> Option<Self> {
        if let Some(length) = LengthUnit::from_name(name) {
            return Some(Unit::Length(length));
        }
        Some(match &*name.to_ascii_lowercase() {
            "deg" => Unit::Deg,
            "grad" => Unit::Grad,
            "rad" => Unit::Rad,
            "turn" => Unit::Turn,
            "s" => Unit::S,
            "ms" => Unit::Ms,
            "hz" => Unit::Hz,
            "khz" => Unit::KHz,
            "dppx" | "x" => Unit::Dppx,
            "dpi" => Unit::Dpi,
            "dpcm" => Unit::Dpcm,
            "fr" => Unit::Fr,
            _ => return None,
        })
    }

    fn base(self) -> Base {
        match self {
            Unit::Length(_) => Base::Length,
            Unit::Deg | Unit::Grad | Unit::Rad | Unit::Turn => Base::Angle,
            Unit::S | Unit::Ms => Base::Time,
            Unit::Hz | Unit::KHz => Base::Frequency,
            Unit::Dppx | Unit::Dpi | Unit::Dpcm => Base::Resolution,
            Unit::Fr => Base::Flex,
        }
    }

    /// Converts to the canonical unit: px, deg, s, Hz, dppx or fr.
    fn canonical(self, value: f64, ctx: Option<&LengthContext>) -> Option<f64> {
        Some(match self {
            Unit::Length(unit) => {
                let length = super::Length { value: 1.0, unit };
                match unit.absolute_px() {
                    Some(px) => value * f64::from(px),
                    None => value * f64::from(length.to_px(ctx?)),
                }
            }
            Unit::Deg => value,
            Unit::Grad => value * 0.9,
            Unit::Rad => value.to_degrees(),
            Unit::Turn => value * 360.0,
            Unit::S => value,
            Unit::Ms => value / 1000.0,
            Unit::Hz => value,
            Unit::KHz => value * 1000.0,
            Unit::Dppx => value,
            Unit::Dpi => value / 96.0,
            Unit::Dpcm => value * 2.54 / 96.0,
            Unit::Fr => value,
        })
    }
}

/// The base types of CSS numeric types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    Length,
    Angle,
    Time,
    Frequency,
    Resolution,
    Flex,
    Percent,
}

/// A CSS numeric type: the exponent of each base type. `<number>` has all zeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NumericType([i8; 7]);

impl NumericType {
    pub const NUMBER: Self = Self([0; 7]);

    pub fn of(base: Base) -> Self {
        let mut exponents = [0; 7];
        exponents[base as usize] = 1;
        Self(exponents)
    }

    fn multiply(self, other: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + other.0[i]))
    }

    fn invert(self) -> Self {
        Self(self.0.map(|e| -e))
    }

    fn is_number(self) -> bool {
        self == Self::NUMBER
    }

    fn is_angle_or_number(self) -> bool {
        self.is_number() || self == Self::of(Base::Angle)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundStrategy {
    Nearest,
    Up,
    Down,
    ToZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathFunction {
    Min,
    Max,
    Clamp,
    Round(RoundStrategy),
    Mod,
    Rem,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Pow,
    Sqrt,
    Hypot,
    Log,
    Exp,
    Abs,
    Sign,
    Progress,
}

/// A math expression tree.
#[derive(Debug, Clone, PartialEq)]
pub enum CalcNode {
    Number(f64),
    /// `50%` is `Percentage(50.0)`.
    Percentage(f64),
    Dimension(f64, Unit),
    /// `sibling-index()`
    SiblingIndex,
    /// `sibling-count()`
    SiblingCount,
    Sum(Vec<CalcNode>),
    Product(Vec<CalcNode>),
    Negate(Box<CalcNode>),
    Invert(Box<CalcNode>),
    Function(MathFunction, Vec<CalcNode>),
    /// `anchor()` or `anchor-size()`, which layout resolves.
    Anchor(Box<AnchorFunction>),
    /// `random()`, which resolves when relative lengths do.
    Random(Box<RandomFunction>),
    /// `size` in the expression of `calc-size()`: the size of its basis.
    SizeRef,
}

/// `random()`: a value from `min` to `max`, a whole number of `step`s above `min` when
/// given (CSS Values 5 § 8).
#[derive(Debug, Clone, PartialEq)]
pub struct RandomFunction {
    /// The `<dashed-ident>` that shares the value between `random()`s of the element.
    pub name: Option<Arc<str>>,
    /// Whether every element gets the same value.
    pub element_shared: bool,
    /// `fixed <number>`: the value to use instead of a random one, from 0 to 1.
    pub fixed: Option<f64>,
    pub min: CalcNode,
    pub max: CalcNode,
    pub step: Option<CalcNode>,
}

impl RandomFunction {
    /// What identifies the function for picking its value: its name, or its arguments.
    fn key(&self) -> u64 {
        let text = match &self.name {
            Some(name) => name.to_string(),
            None => format!("{:?}{:?}{:?}", self.min, self.max, self.step),
        };
        fnv(text.as_bytes())
    }

    /// The random fraction from 0 to 1 for an element with `seed`.
    fn fraction(&self, seed: u64) -> f64 {
        if let Some(fixed) = self.fixed {
            return fixed;
        }
        let seed = if self.element_shared { 0 } else { seed };
        // SplitMix64 of the seed and key, as a fraction in [0, 1).
        let mut z = seed ^ self.key().rotate_left(17);
        z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }

    /// The value for `fraction`, from `min` and `max` in their canonical unit.
    fn pick(fraction: f64, min: f64, max: f64, step: Option<f64>) -> f64 {
        if max < min {
            return min;
        }
        match step.filter(|s| *s > 0.0) {
            Some(step) => {
                let steps = ((max - min) / step).floor();
                (min + step * (fraction * (steps + 1.0)).floor()).min(min + step * steps)
            }
            None => min + fraction * (max - min),
        }
    }
}

/// FNV-1a: a fixed hash, so random values are the same in every run.
pub(crate) fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// `anchor()` or `anchor-size()`: a length from the position or size of an anchor box, which
/// only layout knows, and a fallback for when there is none (CSS Anchor Positioning 1 §§ 5,
/// 6).
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorFunction {
    /// The anchor's name, or `None` for the box's default anchor.
    pub name: Option<Arc<str>>,
    pub query: AnchorQuery,
    pub fallback: Option<CalcNode>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnchorQuery {
    /// `anchor()`: where a side of the anchor is.
    Side(AnchorSide),
    /// `anchor-size()`: a size of the anchor, or by default the one along the axis of the
    /// property it is in.
    Size(Option<AnchorSize>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnchorSide {
    Inside,
    Outside,
    Top,
    Left,
    Right,
    Bottom,
    Start,
    End,
    SelfStart,
    SelfEnd,
    Center,
    /// A fraction of the way from the start side to the end side.
    Percentage(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSize {
    Width,
    Height,
    Block,
    Inline,
    SelfBlock,
    SelfInline,
}

impl AnchorSide {
    fn as_str(self) -> String {
        match self {
            AnchorSide::Inside => "inside".into(),
            AnchorSide::Outside => "outside".into(),
            AnchorSide::Top => "top".into(),
            AnchorSide::Left => "left".into(),
            AnchorSide::Right => "right".into(),
            AnchorSide::Bottom => "bottom".into(),
            AnchorSide::Start => "start".into(),
            AnchorSide::End => "end".into(),
            AnchorSide::SelfStart => "self-start".into(),
            AnchorSide::SelfEnd => "self-end".into(),
            AnchorSide::Center => "center".into(),
            AnchorSide::Percentage(fraction) => {
                format!("{}%", crate::serialize::number(fraction * 100.0))
            }
        }
    }
}

impl AnchorSize {
    fn as_str(self) -> &'static str {
        match self {
            AnchorSize::Width => "width",
            AnchorSize::Height => "height",
            AnchorSize::Block => "block",
            AnchorSize::Inline => "inline",
            AnchorSize::SelfBlock => "self-block",
            AnchorSize::SelfInline => "self-inline",
        }
    }
}

impl AnchorFunction {
    pub fn to_css(&self) -> String {
        let mut parts = Vec::new();
        if let Some(name) = &self.name {
            parts.push(name.to_string());
        }
        let function = match self.query {
            AnchorQuery::Side(side) => {
                parts.push(side.as_str());
                "anchor"
            }
            AnchorQuery::Size(size) => {
                parts.extend(size.map(|s| s.as_str().to_owned()));
                "anchor-size"
            }
        };
        let mut out = format!("{function}({}", parts.join(" "));
        if let Some(fallback) = &self.fallback {
            if !parts.is_empty() {
                out.push_str(", ");
            }
            out.push_str(&crate::serialize::calc_expression(fallback));
        }
        out.push(')');
        out
    }
}

/// What resolves `anchor()` and `anchor-size()` while layout places a positioned box.
pub type AnchorResolver = Rc<dyn Fn(&AnchorFunction) -> Option<f64>>;

thread_local! {
    static ANCHORS: RefCell<Vec<AnchorResolver>> = const { RefCell::new(Vec::new()) };
    /// Whether the expression of a `calc-size()` is being parsed, where `size` is a value.
    static CALC_SIZE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with `resolver` resolving anchor functions, as layout does for the box it
/// places. Outside it, anchor functions take their fallback.
pub fn with_anchor_resolver<T>(resolver: AnchorResolver, f: impl FnOnce() -> T) -> T {
    ANCHORS.with(|anchors| anchors.borrow_mut().push(resolver));
    let result = f();
    ANCHORS.with(|anchors| anchors.borrow_mut().pop());
    result
}

fn resolve_anchor(function: &AnchorFunction) -> Option<f64> {
    let resolver = ANCHORS.with(|anchors| anchors.borrow().last().cloned())?;
    resolver(function)
}

/// What a math expression is evaluated against.
#[derive(Debug, Clone, Copy, Default)]
pub struct CalcContext<'a> {
    /// Resolves relative lengths; without it they cannot be evaluated.
    pub length: Option<&'a LengthContext>,
    /// What 100% equals, in the canonical unit of the type percentages resolve to.
    pub percent_basis: Option<f64>,
    /// `(sibling-index(), sibling-count())` for the element being styled.
    pub siblings: Option<(u32, u32)>,
    /// The size `size` refers to in the expression of `calc-size()`, in pixels.
    pub size: Option<f64>,
}

/// Whether `name` is a math function this parser accepts.
pub fn is_math_function(name: &str) -> bool {
    matches!(
        &*name.to_ascii_lowercase(),
        "calc"
            | "anchor"
            | "anchor-size"
            | "min"
            | "max"
            | "clamp"
            | "round"
            | "mod"
            | "rem"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "pow"
            | "sqrt"
            | "hypot"
            | "log"
            | "exp"
            | "abs"
            | "sign"
            | "progress"
            | "sibling-index"
            | "sibling-count"
            | "random"
    )
}

impl CalcNode {
    /// Parses the arguments of math function `name`; the parser is positioned after its
    /// `Function` token.
    pub fn parse_function(input: &mut Parser<'_>, name: &str) -> Result<Self> {
        Self::parse_function_with(input, name, &|_| None)
    }

    /// Like [`CalcNode::parse_function`], resolving extra keywords to numbers with `vars`, as
    /// relative color syntax does for channel names.
    pub fn parse_function_with(input: &mut Parser<'_>, name: &str, vars: Vars) -> Result<Self> {
        let name = name.to_ascii_lowercase();
        input.parse_nested_block(|input| parse_function_arguments(input, &name, vars))
    }

    /// The expression's type, folding percentages into `percent_as` when given (for example
    /// `<length-percentage>` folds them into lengths). `None` if the expression is invalid.
    pub fn numeric_type(&self, percent_as: Option<Base>) -> Option<NumericType> {
        let same = |args: &[CalcNode]| -> Option<NumericType> {
            let first = args.first()?.numeric_type(percent_as)?;
            args[1..]
                .iter()
                .all(|a| a.numeric_type(percent_as) == Some(first))
                .then_some(first)
        };
        match self {
            CalcNode::Number(_) | CalcNode::SiblingIndex | CalcNode::SiblingCount => {
                Some(NumericType::NUMBER)
            }
            CalcNode::Anchor(_) | CalcNode::SizeRef => Some(NumericType::of(Base::Length)),
            CalcNode::Random(random) => {
                let ty = random.min.numeric_type(percent_as)?;
                let same = random.max.numeric_type(percent_as) == Some(ty)
                    && random
                        .step
                        .as_ref()
                        .is_none_or(|step| step.numeric_type(percent_as) == Some(ty));
                same.then_some(ty)
            }
            CalcNode::Percentage(_) => Some(NumericType::of(percent_as.unwrap_or(Base::Percent))),
            CalcNode::Dimension(_, unit) => Some(NumericType::of(unit.base())),
            CalcNode::Sum(terms) => same(terms),
            CalcNode::Product(factors) => factors.iter().try_fold(NumericType::NUMBER, |acc, f| {
                Some(acc.multiply(f.numeric_type(percent_as)?))
            }),
            CalcNode::Negate(inner) => inner.numeric_type(percent_as),
            CalcNode::Invert(inner) => Some(inner.numeric_type(percent_as)?.invert()),
            CalcNode::Function(function, args) => {
                let ty = |i: usize| args[i].numeric_type(percent_as);
                match function {
                    MathFunction::Min
                    | MathFunction::Max
                    | MathFunction::Clamp
                    | MathFunction::Round(_)
                    | MathFunction::Mod
                    | MathFunction::Rem
                    | MathFunction::Hypot => same(args),
                    MathFunction::Abs => ty(0),
                    MathFunction::Sign => ty(0).map(|_| NumericType::NUMBER),
                    MathFunction::Progress => same(args).map(|_| NumericType::NUMBER),
                    MathFunction::Sin | MathFunction::Cos | MathFunction::Tan => {
                        ty(0)?.is_angle_or_number().then_some(NumericType::NUMBER)
                    }
                    MathFunction::Asin | MathFunction::Acos | MathFunction::Atan => {
                        ty(0)?.is_number().then(|| NumericType::of(Base::Angle))
                    }
                    MathFunction::Atan2 => same(args).map(|_| NumericType::of(Base::Angle)),
                    MathFunction::Pow
                    | MathFunction::Sqrt
                    | MathFunction::Log
                    | MathFunction::Exp => args
                        .iter()
                        .all(|a| a.numeric_type(percent_as) == Some(NumericType::NUMBER))
                        .then_some(NumericType::NUMBER),
                }
            }
        }
    }

    /// Evaluates to a number in the canonical unit of the expression's type (px, deg, s, Hz,
    /// dppx or fr). Percentages need `percent_basis`, unless the whole expression is a
    /// percentage, which evaluates to the percentage number. Returns `None` if something
    /// cannot be resolved with this context.
    pub fn evaluate(&self, ctx: &CalcContext) -> Option<f64> {
        let value = self.eval(ctx)?;
        // NaN in a top-level calculation becomes 0 (CSS Values 4 § 10.12).
        Some(if value.is_nan() { 0.0 } else { value })
    }

    fn eval(&self, ctx: &CalcContext) -> Option<f64> {
        let arg = |args: &[CalcNode], i: usize| args[i].eval(ctx);
        Some(match self {
            CalcNode::Number(n) => *n,
            CalcNode::Percentage(p) => match ctx.percent_basis {
                Some(basis) => p / 100.0 * basis,
                None => *p,
            },
            CalcNode::Dimension(value, unit) => unit.canonical(*value, ctx.length)?,
            CalcNode::SiblingIndex => f64::from(ctx.siblings?.0),
            CalcNode::SizeRef => ctx.size?,
            CalcNode::SiblingCount => f64::from(ctx.siblings?.1),
            CalcNode::Anchor(function) => match resolve_anchor(function) {
                Some(value) => value,
                None => function.fallback.as_ref()?.eval(ctx)?,
            },
            // Random values need the element, which making lengths absolute brings.
            CalcNode::Random(random) => {
                let seed = ctx.length?.random_seed?;
                let step = match &random.step {
                    Some(step) => Some(step.eval(ctx)?),
                    None => None,
                };
                RandomFunction::pick(
                    random.fraction(seed),
                    random.min.eval(ctx)?,
                    random.max.eval(ctx)?,
                    step,
                )
            }
            CalcNode::Sum(terms) => terms.iter().map(|t| t.eval(ctx)).sum::<Option<f64>>()?,
            CalcNode::Product(factors) => factors
                .iter()
                .map(|f| f.eval(ctx))
                .product::<Option<f64>>()?,
            CalcNode::Negate(inner) => -inner.eval(ctx)?,
            CalcNode::Invert(inner) => 1.0 / inner.eval(ctx)?,
            CalcNode::Function(function, args) => match function {
                MathFunction::Min => args
                    .iter()
                    .map(|a| a.eval(ctx))
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .fold(f64::INFINITY, nan_min),
                MathFunction::Max => args
                    .iter()
                    .map(|a| a.eval(ctx))
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .fold(f64::NEG_INFINITY, nan_max),
                MathFunction::Clamp => {
                    let (min, value, max) = (arg(args, 0)?, arg(args, 1)?, arg(args, 2)?);
                    nan_max(min, nan_min(value, max))
                }
                MathFunction::Round(strategy) => round(*strategy, arg(args, 0)?, arg(args, 1)?),
                MathFunction::Mod => {
                    let (a, b) = (arg(args, 0)?, arg(args, 1)?);
                    if b.is_infinite() && a.is_finite() {
                        if a == 0.0 || a.signum() == b.signum() {
                            a
                        } else {
                            b
                        }
                    } else {
                        a - b * (a / b).floor()
                    }
                }
                MathFunction::Rem => {
                    let (a, b) = (arg(args, 0)?, arg(args, 1)?);
                    if b.is_infinite() && a.is_finite() {
                        a
                    } else {
                        a % b
                    }
                }
                MathFunction::Sin => to_radians(&args[0], ctx)?.sin(),
                MathFunction::Cos => to_radians(&args[0], ctx)?.cos(),
                MathFunction::Tan => to_radians(&args[0], ctx)?.tan(),
                MathFunction::Asin => arg(args, 0)?.asin().to_degrees(),
                MathFunction::Acos => arg(args, 0)?.acos().to_degrees(),
                MathFunction::Atan => arg(args, 0)?.atan().to_degrees(),
                MathFunction::Atan2 => arg(args, 0)?.atan2(arg(args, 1)?).to_degrees(),
                MathFunction::Pow => arg(args, 0)?.powf(arg(args, 1)?),
                MathFunction::Sqrt => arg(args, 0)?.sqrt(),
                MathFunction::Hypot => args
                    .iter()
                    .map(|a| a.eval(ctx).map(|v| v * v))
                    .sum::<Option<f64>>()?
                    .sqrt(),
                MathFunction::Log => {
                    let value = arg(args, 0)?;
                    match args.get(1) {
                        Some(base) => value.ln() / base.eval(ctx)?.ln(),
                        None => value.ln(),
                    }
                }
                MathFunction::Exp => arg(args, 0)?.exp(),
                MathFunction::Abs => arg(args, 0)?.abs(),
                MathFunction::Sign => {
                    let value = arg(args, 0)?;
                    if value == 0.0 || value.is_nan() {
                        value
                    } else {
                        value.signum()
                    }
                }
                MathFunction::Progress => {
                    let (value, start, end) = (arg(args, 0)?, arg(args, 1)?, arg(args, 2)?);
                    ((value - start) / (end - start)).clamp(0.0, 1.0)
                }
            },
        })
    }

    /// Replaces relative lengths with pixels, keeping percentages, so the expression can be
    /// stored as a computed value and finished once the percentage basis is known.
    pub fn absolutize(&self, ctx: &LengthContext) -> CalcNode {
        let map = |nodes: &[CalcNode]| nodes.iter().map(|n| n.absolutize(ctx)).collect();
        match self {
            CalcNode::Dimension(value, Unit::Length(unit))
                if unit.absolute_px().is_none() || ctx.zoom != 1.0 =>
            {
                CalcNode::Dimension(
                    value
                        * f64::from(
                            super::Length {
                                value: 1.0,
                                unit: *unit,
                            }
                            .to_px(ctx),
                        ),
                    Unit::Length(LengthUnit::Px),
                )
            }
            CalcNode::Sum(terms) => CalcNode::Sum(map(terms)),
            CalcNode::Product(factors) => CalcNode::Product(map(factors)),
            CalcNode::Negate(inner) => CalcNode::Negate(Box::new(inner.absolutize(ctx))),
            CalcNode::Invert(inner) => CalcNode::Invert(Box::new(inner.absolutize(ctx))),
            CalcNode::Function(function, args) => CalcNode::Function(*function, map(args)),
            CalcNode::Anchor(function) => CalcNode::Anchor(Box::new(AnchorFunction {
                fallback: function.fallback.as_ref().map(|f| f.absolutize(ctx)),
                ..(**function).clone()
            })),
            CalcNode::Random(random) => {
                let Some(seed) = ctx.random_seed else {
                    return self.clone();
                };
                let fraction = random.fraction(seed);
                let (min, max) = (random.min.absolutize(ctx), random.max.absolutize(ctx));
                let step = random.step.as_ref().map(|s| s.absolutize(ctx));
                let unit = |node: &CalcNode| {
                    let ty = node.numeric_type(None)?;
                    let base = [
                        Base::Length,
                        Base::Angle,
                        Base::Time,
                        Base::Frequency,
                        Base::Resolution,
                        Base::Flex,
                    ]
                    .into_iter()
                    .find(|&b| ty == NumericType::of(b));
                    Some(match base {
                        Some(Base::Length) => Some(Unit::Length(LengthUnit::Px)),
                        Some(Base::Angle) => Some(Unit::Deg),
                        Some(Base::Time) => Some(Unit::S),
                        Some(Base::Frequency) => Some(Unit::Hz),
                        Some(Base::Resolution) => Some(Unit::Dppx),
                        Some(Base::Flex) => Some(Unit::Fr),
                        _ if ty == NumericType::NUMBER => None,
                        _ => return Option::<Option<Unit>>::None,
                    })
                };
                let plain = CalcContext::default();
                let values = (|| {
                    let unit = unit(&min)?;
                    let step = match &step {
                        Some(step) => Some(step.evaluate(&plain)?),
                        None => None,
                    };
                    let value = RandomFunction::pick(
                        fraction,
                        min.evaluate(&plain)?,
                        max.evaluate(&plain)?,
                        step,
                    );
                    Some(match unit {
                        Some(unit) => CalcNode::Dimension(value, unit),
                        None => CalcNode::Number(value),
                    })
                })();
                // With percentages left, the value is an expression of them.
                values.unwrap_or_else(|| {
                    let range =
                        CalcNode::Sum(vec![max.clone(), CalcNode::Negate(Box::new(min.clone()))]);
                    let offset = match step {
                        Some(step) => CalcNode::Function(
                            MathFunction::Min,
                            vec![
                                CalcNode::Function(
                                    MathFunction::Round(RoundStrategy::Down),
                                    vec![
                                        CalcNode::Product(vec![
                                            CalcNode::Number(fraction),
                                            CalcNode::Sum(vec![range.clone(), step.clone()]),
                                        ]),
                                        step,
                                    ],
                                ),
                                range,
                            ],
                        ),
                        None => CalcNode::Product(vec![CalcNode::Number(fraction), range]),
                    };
                    CalcNode::Sum(vec![min, offset])
                })
            }
            other => other.clone(),
        }
    }

    /// Whether the expression contains `anchor()` or `anchor-size()`, which only layout can
    /// resolve.
    pub fn has_anchor(&self) -> bool {
        match self {
            CalcNode::Anchor(_) => true,
            CalcNode::Sum(nodes) | CalcNode::Product(nodes) | CalcNode::Function(_, nodes) => {
                nodes.iter().any(CalcNode::has_anchor)
            }
            CalcNode::Negate(inner) | CalcNode::Invert(inner) => inner.has_anchor(),
            _ => false,
        }
    }

    /// Whether the expression refers to the basis of `calc-size()`.
    pub fn has_size(&self) -> bool {
        match self {
            CalcNode::SizeRef => true,
            CalcNode::Sum(nodes) | CalcNode::Product(nodes) | CalcNode::Function(_, nodes) => {
                nodes.iter().any(CalcNode::has_size)
            }
            CalcNode::Negate(inner) | CalcNode::Invert(inner) => inner.has_size(),
            _ => false,
        }
    }

    /// Parses the expression of `calc-size()`, where `size` refers to its basis.
    pub fn parse_calc_size(input: &mut Parser<'_>) -> Result<Self> {
        let outer = CALC_SIZE.with(|flag| flag.replace(true));
        let result = parse_sum(input, &|_| None);
        CALC_SIZE.with(|flag| flag.set(outer));
        result
    }

    /// Whether the expression contains a percentage.
    pub fn has_percentage(&self) -> bool {
        match self {
            CalcNode::Percentage(_) => true,
            CalcNode::Sum(nodes) | CalcNode::Product(nodes) | CalcNode::Function(_, nodes) => {
                nodes.iter().any(CalcNode::has_percentage)
            }
            CalcNode::Negate(inner) | CalcNode::Invert(inner) => inner.has_percentage(),
            CalcNode::Random(random) => {
                random.min.has_percentage()
                    || random.max.has_percentage()
                    || random.step.as_ref().is_some_and(CalcNode::has_percentage)
            }
            _ => false,
        }
    }
}

/// `min()` that propagates NaN.
fn nan_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// `max()` that propagates NaN.
fn nan_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn to_radians(node: &CalcNode, ctx: &CalcContext) -> Option<f64> {
    let value = node.eval(ctx)?;
    Some(
        if node.numeric_type(None) == Some(NumericType::of(Base::Angle)) {
            value.to_radians()
        } else {
            value
        },
    )
}

fn round(strategy: RoundStrategy, value: f64, step: f64) -> f64 {
    if step == 0.0 {
        return f64::NAN;
    }
    if value.is_infinite() {
        return if step.is_infinite() { f64::NAN } else { value };
    }
    if step.is_infinite() {
        return match strategy {
            RoundStrategy::Up if value > 0.0 => f64::INFINITY,
            RoundStrategy::Down if value < 0.0 => f64::NEG_INFINITY,
            _ => 0.0_f64.copysign(value),
        };
    }
    let step = step.abs();
    let down = (value / step).floor() * step;
    let up = (value / step).ceil() * step;
    match strategy {
        RoundStrategy::Up => up,
        RoundStrategy::Down => down,
        RoundStrategy::ToZero => {
            if value >= 0.0 {
                down
            } else {
                up
            }
        }
        RoundStrategy::Nearest => {
            if value - down < up - value {
                down
            } else {
                up
            }
        }
    }
}

fn parse_function_arguments(input: &mut Parser<'_>, name: &str, vars: Vars) -> Result<CalcNode> {
    let args =
        |input: &mut Parser<'_>, count: std::ops::RangeInclusive<usize>| -> Result<Vec<CalcNode>> {
            let args = input.parse_comma_separated(|input| parse_sum(input, vars))?;
            if count.contains(&args.len()) {
                Ok(args)
            } else {
                fail()
            }
        };
    let function = |f: MathFunction, args: Vec<CalcNode>| CalcNode::Function(f, args);
    let node = match name {
        "calc" => parse_sum(input, vars)?,
        "min" => function(MathFunction::Min, args(input, 1..=usize::MAX)?),
        "max" => function(MathFunction::Max, args(input, 1..=usize::MAX)?),
        "clamp" => function(MathFunction::Clamp, args(input, 3..=3)?),
        "round" => {
            let strategy = input
                .try_parse(|input| -> Result<RoundStrategy> {
                    let ident = input.expect_ident_cloned()?;
                    let strategy = match &*ident.to_ascii_lowercase() {
                        "nearest" => RoundStrategy::Nearest,
                        "up" => RoundStrategy::Up,
                        "down" => RoundStrategy::Down,
                        "to-zero" => RoundStrategy::ToZero,
                        _ => return fail(),
                    };
                    input.expect_comma()?;
                    Ok(strategy)
                })
                .unwrap_or(RoundStrategy::Nearest);
            let mut args = args(input, 1..=2)?;
            if args.len() == 1 {
                // The step may be omitted only for numbers.
                if args[0].numeric_type(None) != Some(NumericType::NUMBER) {
                    return fail();
                }
                args.push(CalcNode::Number(1.0));
            }
            function(MathFunction::Round(strategy), args)
        }
        "mod" => function(MathFunction::Mod, args(input, 2..=2)?),
        "rem" => function(MathFunction::Rem, args(input, 2..=2)?),
        "sin" => function(MathFunction::Sin, args(input, 1..=1)?),
        "cos" => function(MathFunction::Cos, args(input, 1..=1)?),
        "tan" => function(MathFunction::Tan, args(input, 1..=1)?),
        "asin" => function(MathFunction::Asin, args(input, 1..=1)?),
        "acos" => function(MathFunction::Acos, args(input, 1..=1)?),
        "atan" => function(MathFunction::Atan, args(input, 1..=1)?),
        "atan2" => function(MathFunction::Atan2, args(input, 2..=2)?),
        "pow" => function(MathFunction::Pow, args(input, 2..=2)?),
        "sqrt" => function(MathFunction::Sqrt, args(input, 1..=1)?),
        "hypot" => function(MathFunction::Hypot, args(input, 1..=usize::MAX)?),
        "log" => function(MathFunction::Log, args(input, 1..=2)?),
        "exp" => function(MathFunction::Exp, args(input, 1..=1)?),
        "abs" => function(MathFunction::Abs, args(input, 1..=1)?),
        "sign" => function(MathFunction::Sign, args(input, 1..=1)?),
        "progress" => function(MathFunction::Progress, args(input, 3..=3)?),
        "sibling-index" => CalcNode::SiblingIndex,
        "sibling-count" => CalcNode::SiblingCount,
        "random" => parse_random(input, vars)?,
        "anchor" => parse_anchor(input, false, vars)?,
        "anchor-size" => parse_anchor(input, true, vars)?,
        _ => return fail(),
    };
    input.expect_exhausted()?;
    // Reject expressions whose types do not combine under any percentage resolution, such as
    // `1px + 2s`. The property's grammar checks the final type.
    const FOLDS: [Option<Base>; 7] = [
        None,
        Some(Base::Length),
        Some(Base::Angle),
        Some(Base::Time),
        Some(Base::Frequency),
        Some(Base::Resolution),
        Some(Base::Flex),
    ];
    if FOLDS.iter().all(|&fold| node.numeric_type(fold).is_none()) {
        return fail();
    }
    Ok(node)
}

/// The arguments of `random()`: `[ <random-value-sharing> , ]? <calc-sum>, <calc-sum>
/// [, by? <calc-sum> ]?`, where the sharing is `[ [ auto | <dashed-ident> ] ||
/// element-shared ] | fixed <number [0,1]>`.
fn parse_random(input: &mut Parser<'_>, vars: Vars) -> Result<CalcNode> {
    let (name, element_shared, fixed) = input
        .try_parse(|input| -> Result<(Option<Arc<str>>, bool, Option<f64>)> {
            if input
                .try_parse(|input| input.expect_ident_matching("fixed"))
                .is_ok()
            {
                let value = f64::from(input.expect_number()?);
                if !(0.0..=1.0).contains(&value) {
                    return fail();
                }
                input.expect_comma()?;
                return Ok((None, false, Some(value)));
            }
            let (mut name, mut auto, mut shared) = (None, false, false);
            loop {
                let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) else {
                    break;
                };
                match &*ident.to_ascii_lowercase() {
                    "auto" if name.is_none() && !auto => auto = true,
                    "element-shared" if !shared => shared = true,
                    _ if ident.starts_with("--") && name.is_none() && !auto => {
                        name = Some(Arc::from(&*ident))
                    }
                    _ => return fail(),
                }
            }
            if name.is_none() && !auto && !shared {
                return fail();
            }
            input.expect_comma()?;
            Ok((name, shared, None))
        })
        .unwrap_or((None, false, None));
    let min = parse_sum(input, vars)?;
    input.expect_comma()?;
    let max = parse_sum(input, vars)?;
    let step = if input.try_parse(|input| input.expect_comma()).is_ok() {
        let _ = input.try_parse(|input| input.expect_ident_matching("by"));
        Some(parse_sum(input, vars)?)
    } else {
        None
    };
    Ok(CalcNode::Random(Box::new(RandomFunction {
        name,
        element_shared,
        fixed,
        min,
        max,
        step,
    })))
}

/// The arguments of `anchor()` or, with `size`, `anchor-size()`.
fn parse_anchor(input: &mut Parser<'_>, size: bool, vars: Vars) -> Result<CalcNode> {
    let dashed = |input: &mut Parser<'_>| -> Result<Arc<str>> {
        let ident = input.expect_ident()?;
        if ident.starts_with("--") && ident.len() > 2 {
            Ok(Arc::from(&**ident))
        } else {
            fail()
        }
    };
    let mut name = input.try_parse(dashed).ok();
    let query = if size {
        let size = input
            .try_parse(|input| {
                Ok::<_, ParseError<()>>(match &*input.expect_ident()?.to_ascii_lowercase() {
                    "width" => AnchorSize::Width,
                    "height" => AnchorSize::Height,
                    "block" => AnchorSize::Block,
                    "inline" => AnchorSize::Inline,
                    "self-block" => AnchorSize::SelfBlock,
                    "self-inline" => AnchorSize::SelfInline,
                    _ => return fail(),
                })
            })
            .ok();
        AnchorQuery::Size(size)
    } else {
        let side = input.try_parse(|input| {
            let token = input.next()?.clone();
            Ok::<_, ParseError<()>>(match token {
                Token::Percentage { unit_value, .. } => {
                    AnchorSide::Percentage(f64::from(unit_value))
                }
                Token::Ident(ident) => match &*ident.to_ascii_lowercase() {
                    "inside" => AnchorSide::Inside,
                    "outside" => AnchorSide::Outside,
                    "top" => AnchorSide::Top,
                    "left" => AnchorSide::Left,
                    "right" => AnchorSide::Right,
                    "bottom" => AnchorSide::Bottom,
                    "start" => AnchorSide::Start,
                    "end" => AnchorSide::End,
                    "self-start" => AnchorSide::SelfStart,
                    "self-end" => AnchorSide::SelfEnd,
                    "center" => AnchorSide::Center,
                    _ => return fail(),
                },
                _ => return fail(),
            })
        });
        match side {
            Ok(side) => AnchorQuery::Side(side),
            // `anchor()` needs a side.
            Err(_) => return fail(),
        }
    };
    if name.is_none() {
        name = input.try_parse(dashed).ok();
    }
    let fallback = if input.try_parse(|input| input.expect_comma()).is_ok() {
        let fallback = parse_sum(input, vars)?;
        if fallback.numeric_type(Some(Base::Length)) != Some(NumericType::of(Base::Length)) {
            return fail();
        }
        Some(fallback)
    } else {
        None
    };
    Ok(CalcNode::Anchor(Box::new(AnchorFunction {
        name,
        query,
        fallback,
    })))
}

/// `<calc-sum>`: products joined by `+` and `-`, which must be surrounded by whitespace.
fn parse_sum(input: &mut Parser<'_>, vars: Vars) -> Result<CalcNode> {
    let mut terms = vec![parse_product(input, vars)?];
    loop {
        let state = input.state();
        let Ok(Token::WhiteSpace(_)) = input.next_including_whitespace() else {
            input.reset(&state);
            break;
        };
        let negate = match input.next_including_whitespace() {
            Ok(Token::Delim('+')) => false,
            Ok(Token::Delim('-')) => true,
            _ => {
                input.reset(&state);
                break;
            }
        };
        input.expect_whitespace()?;
        let term = parse_product(input, vars)?;
        terms.push(if negate {
            CalcNode::Negate(Box::new(term))
        } else {
            term
        });
    }
    Ok(if terms.len() == 1 {
        terms.pop().expect("one term")
    } else {
        CalcNode::Sum(terms)
    })
}

fn parse_product(input: &mut Parser<'_>, vars: Vars) -> Result<CalcNode> {
    let mut factors = vec![parse_value(input, vars)?];
    loop {
        let state = input.state();
        match input.next() {
            Ok(Token::Delim('*')) => factors.push(parse_value(input, vars)?),
            Ok(Token::Delim('/')) => {
                factors.push(CalcNode::Invert(Box::new(parse_value(input, vars)?)))
            }
            _ => {
                input.reset(&state);
                break;
            }
        }
    }
    Ok(if factors.len() == 1 {
        factors.pop().expect("one factor")
    } else {
        CalcNode::Product(factors)
    })
}

fn parse_value(input: &mut Parser<'_>, vars: Vars) -> Result<CalcNode> {
    match input.next()?.clone() {
        Token::Number { value, .. } => Ok(CalcNode::Number(f64::from(value))),
        Token::Percentage { unit_value, .. } => {
            Ok(CalcNode::Percentage(f64::from(unit_value) * 100.0))
        }
        Token::Dimension { value, unit, .. } => match Unit::from_name(&unit) {
            Some(unit) => Ok(CalcNode::Dimension(f64::from(value), unit)),
            None => fail(),
        },
        Token::Ident(ident)
            if ident.eq_ignore_ascii_case("size") && CALC_SIZE.with(std::cell::Cell::get) =>
        {
            Ok(CalcNode::SizeRef)
        }
        Token::Ident(ident) => Ok(CalcNode::Number(match &*ident.to_ascii_lowercase() {
            "e" => std::f64::consts::E,
            "pi" => PI,
            "infinity" => f64::INFINITY,
            "-infinity" => f64::NEG_INFINITY,
            "nan" => f64::NAN,
            other => match vars(other) {
                Some(value) => value,
                None => return fail(),
            },
        })),
        Token::ParenthesisBlock => input.parse_nested_block(|input| parse_sum(input, vars)),
        Token::Function(name) if is_math_function(&name) => {
            CalcNode::parse_function_with(input, &name, vars)
        }
        _ => fail(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Option<CalcNode> {
        let mut input = Parser::new(text);
        input
            .parse_entirely(|input| {
                let name = input.expect_function()?.clone();
                CalcNode::parse_function(input, &name)
            })
            .ok()
    }

    fn eval(text: &str) -> f64 {
        let ctx = LengthContext::initial((800.0, 600.0));
        let node = parse(text).unwrap_or_else(|| panic!("{text:?} should parse"));
        node.evaluate(&CalcContext {
            length: Some(&ctx),
            percent_basis: Some(200.0),
            siblings: Some((3, 5)),
            size: None,
        })
        .unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn arithmetic() {
        assert_eq!(eval("calc(1px + 2px)"), 3.0);
        assert_eq!(eval("calc(1in - 6px * 2)"), 84.0);
        assert_eq!(eval("calc((1em + 4px) / 2)"), 10.0);
        assert_eq!(eval("calc(50% - 10px)"), 90.0);
        assert_eq!(eval("calc(2 * (3 + 4))"), 14.0);
        assert_eq!(eval("calc(10vw)"), 80.0);
    }

    #[test]
    fn whitespace_rules_for_plus_and_minus() {
        assert!(parse("calc(1px+2px)").is_none());
        assert!(parse("calc(1px -2px)").is_none());
        assert!(parse("calc(1px - 2px)").is_some());
        assert!(parse("calc(1px*2)").is_some());
    }

    #[test]
    fn type_checking() {
        assert!(parse("calc(1px + 2s)").is_none());
        assert!(
            parse("calc(1px * 2px)").is_some(),
            "length² is a valid intermediate type"
        );
        assert_eq!(
            parse("calc(1px * 2px)").unwrap().numeric_type(None),
            Some(NumericType([2, 0, 0, 0, 0, 0, 0]))
        );
        assert_eq!(
            parse("calc(10px / 2px)").unwrap().numeric_type(None),
            Some(NumericType::NUMBER)
        );
        assert!(parse("calc(1px + 2)").is_none());
        assert!(parse("sqrt(4px)").is_none());
        assert!(parse("calc(unknown)").is_none());
        assert_eq!(
            parse("calc(50% + 1px)")
                .unwrap()
                .numeric_type(Some(Base::Length)),
            Some(NumericType::of(Base::Length))
        );
    }

    #[test]
    fn comparison_functions() {
        assert_eq!(eval("min(10px, 2em, 50%)"), 10.0);
        assert_eq!(eval("max(10px, 2em)"), 32.0);
        assert_eq!(eval("clamp(10px, 50%, 40px)"), 40.0);
        assert_eq!(eval("clamp(10px, 1px, 40px)"), 10.0);
    }

    #[test]
    fn stepped_functions() {
        assert_eq!(eval("round(2.5)"), 3.0);
        assert_eq!(eval("round(down, 17px, 5px)"), 15.0);
        assert_eq!(eval("round(up, 11px, 5px)"), 15.0);
        assert_eq!(eval("round(to-zero, -7px, 5px)"), -5.0);
        assert_eq!(eval("mod(-18px, 5px)"), 2.0);
        assert_eq!(eval("rem(-18px, 5px)"), -3.0);
        assert!(
            parse("round(1px)").is_none(),
            "step is required for dimensions"
        );
    }

    #[test]
    fn trigonometry_and_exponents() {
        assert!(close(eval("sin(90deg)"), 1.0));
        assert!(close(eval("cos(pi)"), -1.0));
        assert!(close(eval("tan(0.25turn / 2)"), 1.0));
        assert!(close(eval("asin(1)"), 90.0));
        assert!(close(eval("atan2(1px, 1px)"), 45.0));
        assert_eq!(eval("pow(2, 10)"), 1024.0);
        assert_eq!(eval("sqrt(16)"), 4.0);
        assert_eq!(eval("hypot(3px, 4px)"), 5.0);
        assert!(close(eval("log(8, 2)"), 3.0));
        assert!(close(eval("exp(1)"), std::f64::consts::E));
        assert_eq!(eval("abs(-3px)"), 3.0);
        assert_eq!(eval("sign(-3px)"), -1.0);
    }

    #[test]
    fn other_functions() {
        assert_eq!(eval("progress(15px, 10px, 20px)"), 0.5);
        assert_eq!(eval("calc(sibling-index() * 10px)"), 30.0);
        assert_eq!(eval("calc(sibling-count())"), 5.0);
        assert_eq!(eval("calc(1turn)"), 360.0);
        assert_eq!(eval("calc(2s + 500ms)"), 2.5);
        assert_eq!(eval("calc(96dpi)"), 1.0);
    }

    #[test]
    fn special_values() {
        assert_eq!(eval("calc(infinity * 1px)"), f64::INFINITY);
        assert_eq!(eval("calc(NaN)"), 0.0);
        assert_eq!(eval("calc(1 / 0)"), f64::INFINITY);
    }

    #[test]
    fn relative_lengths_need_a_context() {
        let node = parse("calc(1em + 1px)").unwrap();
        assert_eq!(node.evaluate(&CalcContext::default()), None);
        let ctx = LengthContext::initial((800.0, 600.0));
        let absolute = node.absolutize(&ctx);
        assert_eq!(absolute.evaluate(&CalcContext::default()), Some(17.0));
    }

    #[test]
    fn percentages_without_basis() {
        let node = parse("calc(50% * 2)").unwrap();
        assert!(node.has_percentage());
        assert_eq!(node.evaluate(&CalcContext::default()), Some(100.0));
    }
}
