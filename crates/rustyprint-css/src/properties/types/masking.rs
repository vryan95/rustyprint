//! Clipping and masking (CSS Masking 1): `clip`, `clip-path` and the `mask-*` properties.

use std::sync::Arc;

use cssparser::Parser;

use super::{
    BasicShape, LengthPercentage, PositionComponent, Result, SpecifiedLengthPercentage,
    SpecifiedPosition, fail, layers, layers_css, length_percentage_css, parse_basic_shape,
    parse_bg_position, parse_length_percentage,
};
use crate::properties::{ComputeContext, Property};
use crate::serialize;

fn keyword(input: &mut Parser<'_>, keyword: &str) -> bool {
    input
        .try_parse(|input| input.expect_ident_matching(keyword))
        .is_ok()
}

keywords!(
    /// `<geometry-box>`: the box a clip path or shape is drawn in. For CSS boxes, `fill-box`
    /// is the content box and `stroke-box` and `view-box` the border box.
    GeometryBox {
        MarginBox = "margin-box",
        BorderBox = "border-box",
        PaddingBox = "padding-box",
        ContentBox = "content-box",
        FillBox = "fill-box",
        StrokeBox = "stroke-box",
        ViewBox = "view-box",
    }
);

/// `clip`: `rect(<top>, <right>, <bottom>, <left>) | auto`, each edge a length or `auto`.
pub struct Clip;

impl Property for Clip {
    type Specified = Option<[Option<SpecifiedLengthPercentage>; 4]>;
    type Computed = Option<[Option<f32>; 4]>;

    fn to_css(computed: &Self::Computed) -> String {
        match computed {
            None => "auto".into(),
            Some(edges) => format!(
                "rect({})",
                edges
                    .iter()
                    .map(|e| e.map_or_else(|| "auto".into(), serialize::px))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "auto") {
            return Ok(None);
        }
        input.expect_function_matching("rect")?;
        input.parse_nested_block(|input| {
            let edge = |input: &mut Parser<'_>| {
                if keyword(input, "auto") {
                    Ok(None)
                } else {
                    parse_length_percentage(input, false, false).map(Some)
                }
            };
            let mut edges = [None, None, None, None];
            for (i, value) in edges.iter_mut().enumerate() {
                // Commas are optional, for compatibility with old content.
                if i > 0 {
                    let _ = input.try_parse(|input| input.expect_comma());
                }
                *value = edge(input)?;
            }
            Ok(Some(edges))
        })
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified.as_ref().map(|edges| {
            edges
                .clone()
                .map(|e| e.map(|e| e.compute(ctx, false).as_length().unwrap_or(0.0)))
        })
    }
}

/// `clip-path`: `<clip-source> | [<basic-shape> || <geometry-box>] | none`.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipPath<L: super::PositionOf> {
    None,
    /// A reference to an SVG `<clipPath>`.
    Url(Arc<str>),
    Shape {
        shape: Option<BasicShape<L>>,
        reference: Option<GeometryBox>,
    },
}

impl Property for ClipPath<LengthPercentage> {
    type Specified = ClipPath<SpecifiedLengthPercentage>;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ClipPath::None => "none".into(),
            ClipPath::Url(url) => format!("url(\"{url}\")"),
            ClipPath::Shape { shape, reference } => {
                let parts: Vec<String> = shape
                    .iter()
                    .map(BasicShape::to_css)
                    .chain(reference.iter().map(|r| r.as_str().into()))
                    .collect();
                parts.join(" ")
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        if keyword(input, "none") {
            return Ok(ClipPath::None);
        }
        if let Ok(url) = input.try_parse(|input| input.expect_url().map(|u| u.to_string())) {
            return Ok(ClipPath::Url(Arc::from(url)));
        }
        let (mut shape, mut reference) = (None, None);
        for _ in 0..2 {
            if shape.is_none()
                && let Ok(value) = input.try_parse(parse_basic_shape)
            {
                shape = Some(value);
            } else if reference.is_none()
                && let Ok(value) = input.try_parse(<GeometryBox as Property>::parse)
            {
                reference = Some(value);
            }
        }
        if shape.is_none() && reference.is_none() {
            return fail();
        }
        Ok(ClipPath::Shape { shape, reference })
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self {
        match specified {
            ClipPath::None => ClipPath::None,
            ClipPath::Url(url) => ClipPath::Url(url.clone()),
            ClipPath::Shape { shape, reference } => ClipPath::Shape {
                shape: shape.as_ref().map(|s| s.compute(ctx)),
                reference: *reference,
            },
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Mask layers

keywords!(
    /// `<masking-mode>`: whether a mask layer's alpha or luminance masks.
    MaskingMode { Alpha = "alpha", Luminance = "luminance", MatchSource = "match-source" }
);

keyword_layers!(
    /// `mask-mode`: `<masking-mode>#`.
    MaskMode: MaskingMode
);

keywords!(
    /// `<coord-box>`: the box a mask layer is positioned in.
    CoordBox {
        ContentBox = "content-box",
        PaddingBox = "padding-box",
        BorderBox = "border-box",
        FillBox = "fill-box",
        StrokeBox = "stroke-box",
        ViewBox = "view-box",
    }
);

keyword_layers!(
    /// `mask-origin`: `<coord-box>#`.
    MaskOrigin: CoordBox
);

keywords!(
    /// The painting area of one mask layer: a `<coord-box>` or `no-clip`.
    MaskClipBox {
        ContentBox = "content-box",
        PaddingBox = "padding-box",
        BorderBox = "border-box",
        FillBox = "fill-box",
        StrokeBox = "stroke-box",
        ViewBox = "view-box",
        NoClip = "no-clip",
    }
);

keyword_layers!(
    /// `mask-clip`: `[<coord-box> | no-clip]#`.
    MaskClip: MaskClipBox
);

keywords!(
    /// `<compositing-operator>`: how a mask layer combines with the layers below it.
    CompositingOperator {
        Add = "add",
        Subtract = "subtract",
        Intersect = "intersect",
        Exclude = "exclude",
    }
);

keyword_layers!(
    /// `mask-composite`: `<compositing-operator>#`.
    MaskComposite: CompositingOperator
);

keywords!(
    /// `mask-type` and `mask-border-mode`.
    MaskType { Luminance = "luminance", Alpha = "alpha" }
);

/// `mask-position`: `<position>#`.
pub struct MaskPosition;

impl MaskPosition {
    pub(crate) fn compute_one(
        position: &(SpecifiedPosition, SpecifiedPosition),
        ctx: &ComputeContext,
    ) -> (PositionComponent, PositionComponent) {
        (
            position.0.compute(ctx, true),
            position.1.compute(ctx, false),
        )
    }
}

impl Property for MaskPosition {
    type Specified = Arc<[(SpecifiedPosition, SpecifiedPosition)]>;
    type Computed = Arc<[(PositionComponent, PositionComponent)]>;

    fn to_css(computed: &Self::Computed) -> String {
        let side = |p: &PositionComponent, end: &str| {
            let offset = length_percentage_css(&p.offset);
            if p.from_end {
                format!("{end} {offset}")
            } else {
                offset
            }
        };
        layers_css(computed, |(x, y)| {
            format!("{} {}", side(x, "right"), side(y, "bottom"))
        })
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
        layers(input, parse_bg_position)
    }

    fn compute(specified: &Self::Specified, ctx: &ComputeContext) -> Self::Computed {
        specified
            .iter()
            .map(|p| Self::compute_one(p, ctx))
            .collect()
    }
}
