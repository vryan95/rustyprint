//! CSS values: units, numbers and the functions that compute them.

mod calc;
mod color;
mod length;

pub(crate) use calc::fnv;
pub use calc::{
    AnchorFunction, AnchorQuery, AnchorResolver, AnchorSide, AnchorSize, Base, CalcContext,
    CalcNode, MathFunction, NumericType, RandomFunction, RoundStrategy, Unit, Vars,
    is_math_function, with_anchor_resolver,
};
pub use color::{
    AbsoluteColor, Color, ColorContext, ColorSpace, DEVICE_CMYK, HueMethod, NativeColor,
    interpolate, mix, parse_interpolation,
};
pub use length::{FontMetrics, Length, LengthContext, LengthUnit};
