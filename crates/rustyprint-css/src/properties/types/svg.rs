//! The painting properties of SVG shapes and text (SVG 2 § 13 and CSS Fill and Stroke 3).
//! They style the content of an `<svg>` element; HTML boxes ignore them.

use super::*;

/// The width of a stroke or the length of a dash: a length, or a number of user units,
/// which are CSS pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecifiedStrokeLength {
    Number(f32),
    Length(SpecifiedLengthPercentage),
}

impl SpecifiedStrokeLength {
    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if let Ok(number) = input.try_parse(|input| input.expect_number()) {
            return if number < 0.0 {
                fail()
            } else {
                Ok(Self::Number(number))
            };
        }
        parse_length_percentage(input, true, true).map(Self::Length)
    }

    /// The length in CSS pixels. A percentage is of the SVG's diagonal, which is not known
    /// to the cascade, so it is taken of 100 pixels.
    fn compute(&self, ctx: &ComputeContext) -> f32 {
        match self {
            Self::Number(number) => *number,
            Self::Length(length) => match length.compute(ctx, true) {
                LengthPercentage::Length(px) => px,
                other => other.resolve(100.0),
            },
        }
    }
}

/// `stroke-width`: `<length-percentage> | <number>`.
pub struct StrokeWidth;

impl Property for StrokeWidth {
    type Specified = SpecifiedStrokeLength;
    type Computed = f32;

    fn to_css(computed: &f32) -> String {
        serialize::px(*computed)
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        SpecifiedStrokeLength::parse(input)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> f32 {
        specified.compute(ctx)
    }
}

/// `stroke-dasharray`: `none | [<length-percentage> | <number>]+`, with or without commas.
/// The computed value is the dashes and gaps in CSS pixels, empty for `none`.
pub struct StrokeDasharray;

impl Property for StrokeDasharray {
    type Specified = Arc<[SpecifiedStrokeLength]>;
    type Computed = Arc<[f32]>;

    fn to_css(computed: &Self::Computed) -> String {
        if computed.is_empty() {
            return "none".into();
        }
        let lengths: Vec<String> = computed.iter().map(|&px| serialize::px(px)).collect();
        lengths.join(", ")
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if input
            .try_parse(|input| input.expect_ident_matching("none"))
            .is_ok()
        {
            return Ok(Arc::from([]));
        }
        let mut lengths = vec![SpecifiedStrokeLength::parse(input)?];
        while !input.is_exhausted() {
            let _ = input.try_parse(|input| input.expect_comma());
            lengths.push(SpecifiedStrokeLength::parse(input)?);
        }
        Ok(lengths.into())
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.iter().map(|length| length.compute(ctx)).collect()
    }
}

keywords!(
    /// `stroke-linecap`.
    StrokeLinecap {
        Butt = "butt",
        Round = "round",
        Square = "square",
    }
);

keywords!(
    /// `stroke-linejoin`.
    StrokeLinejoin {
        Miter = "miter",
        MiterClip = "miter-clip",
        Round = "round",
        Bevel = "bevel",
        Arcs = "arcs",
    }
);

keywords!(
    /// `text-anchor`: the point of a line of SVG text that sits at its position.
    TextAnchor {
        Start = "start",
        Middle = "middle",
        End = "end",
    }
);

/// `shape-rendering`. Its keywords serialize in the mixed case SVG gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShapeRendering {
    Auto,
    OptimizeSpeed,
    CrispEdges,
    GeometricPrecision,
}

impl Property for ShapeRendering {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ShapeRendering::Auto => "auto",
            ShapeRendering::OptimizeSpeed => "optimizeSpeed",
            ShapeRendering::CrispEdges => "crispEdges",
            ShapeRendering::GeometricPrecision => "geometricPrecision",
        }
        .into()
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        Ok(match &*input.expect_ident()?.to_ascii_lowercase() {
            "auto" => ShapeRendering::Auto,
            "optimizespeed" => ShapeRendering::OptimizeSpeed,
            "crispedges" => ShapeRendering::CrispEdges,
            "geometricprecision" => ShapeRendering::GeometricPrecision,
            _ => return fail(),
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        *specified
    }
}
