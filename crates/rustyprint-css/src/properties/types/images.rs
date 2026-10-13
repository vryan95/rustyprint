//! The `<image>` type (CSS Images 4 § 2): URLs, gradients and the image functions
//! `image()`, `image-set()`, `cross-fade()`, `element()` and `paint()`.

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{Gradient, Result, SpecifiedGradient, fail, is_gradient, parse_gradient, parse_typed};
use crate::properties::ComputeContext;
use crate::serialize;
use crate::values::{Base, Color};

/// An `<image>`.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageValue {
    None,
    /// A URL as written, resolved against the request's assets when used.
    Url(Arc<str>),
    Gradient(Arc<Gradient>),
    /// A gradient before its lengths and colors compute. Only specified values have it.
    SpecifiedGradient(Arc<SpecifiedGradient>),
    /// `image()`.
    Image(Arc<ImageFunction>),
    /// `image-set()`: the options in order. The one used is chosen when the image is.
    ImageSet(Arc<[ImageSetOption]>),
    /// `cross-fade()`: images or colors and the fraction of each, if given.
    CrossFade(Arc<[(FadeSource, Option<f32>)]>),
    /// `element()`: the ID of the element painted as the image.
    Element(Arc<str>),
    /// `paint()`: a paint worklet's name and arguments, as written. Worklets need
    /// scripting, so it is always an invalid image (CSS Painting API 1 § 5).
    Paint(Arc<str>),
}

/// `image()`: an image with a directionality and a solid color to show if it fails, or a
/// solid color image (CSS Images 4 § 2.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageFunction {
    /// `ltr` (false) or `rtl` (true): the image flips horizontally in elements of the other
    /// direction.
    pub rtl: Option<bool>,
    /// The URL, which may end in a media fragment such as `#xywh=0,0,10,10`.
    pub src: Option<Arc<str>>,
    /// Computed values always have an absolute color.
    pub color: Option<Color>,
}

/// One option of `image-set()`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSetOption {
    pub image: ImageValue,
    /// The image's resolution, in image pixels per CSS pixel.
    pub resolution: f32,
    /// The MIME type from `type()`.
    pub mime: Option<Arc<str>>,
}

/// An input of `cross-fade()`.
#[derive(Debug, Clone, PartialEq)]
pub enum FadeSource {
    Image(ImageValue),
    /// A solid color image without a natural size.
    Color(Color),
}

impl ImageValue {
    pub(crate) fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_ident_matching("none"))
            .is_ok()
        {
            return Ok(ImageValue::None);
        }
        Self::parse_image(input, true)
    }

    /// Parses an image that is not `none`; `image-set()` cannot nest, so `sets` is false
    /// inside one.
    fn parse_image(input: &mut Parser<'_>, sets: bool) -> Result<Self> {
        if let Ok(url) = input.try_parse(|input| input.expect_url()) {
            return Ok(ImageValue::Url(Arc::from(&*url)));
        }
        let name = input.expect_function()?.to_ascii_lowercase();
        if is_gradient(&name) {
            return Ok(ImageValue::SpecifiedGradient(parse_gradient(&name, input)?));
        }
        match &*name {
            "image" => input.parse_nested_block(parse_image_function),
            "image-set" | "-webkit-image-set" if sets => input.parse_nested_block(|input| {
                let options = input.parse_comma_separated(parse_set_option)?;
                Ok(ImageValue::ImageSet(options.into()))
            }),
            "cross-fade" => input.parse_nested_block(|input| {
                let inputs = input.parse_comma_separated(parse_fade_input)?;
                Ok(ImageValue::CrossFade(inputs.into()))
            }),
            "element" | "-moz-element" => input.parse_nested_block(|input| {
                let id = match input.next()? {
                    Token::IDHash(id) => Arc::from(&**id),
                    _ => return fail(),
                };
                Ok(ImageValue::Element(id))
            }),
            "paint" => input.parse_nested_block(|input| {
                let start = input.position();
                input.expect_ident()?;
                while input.next().is_ok() {}
                Ok(ImageValue::Paint(Arc::from(input.slice_from(start).trim())))
            }),
            _ => fail(),
        }
    }

    /// The computed image: lengths and colors compute, in nested images too.
    pub(crate) fn compute(&self, ctx: &ComputeContext) -> Self {
        let color = |color: &Color| Color::Absolute(color.resolve_in(&ctx.colors()));
        match self {
            ImageValue::SpecifiedGradient(gradient) => {
                ImageValue::Gradient(Arc::new(gradient.compute(ctx)))
            }
            ImageValue::Image(function) => ImageValue::Image(Arc::new(ImageFunction {
                rtl: function.rtl,
                src: function.src.clone(),
                color: function.color.as_ref().map(color),
            })),
            ImageValue::ImageSet(options) => ImageValue::ImageSet(
                options
                    .iter()
                    .map(|option| ImageSetOption {
                        image: option.image.compute(ctx),
                        resolution: option.resolution,
                        mime: option.mime.clone(),
                    })
                    .collect(),
            ),
            ImageValue::CrossFade(inputs) => ImageValue::CrossFade(
                inputs
                    .iter()
                    .map(|(source, fraction)| {
                        let source = match source {
                            FadeSource::Image(image) => FadeSource::Image(image.compute(ctx)),
                            FadeSource::Color(c) => FadeSource::Color(color(c)),
                        };
                        (source, *fraction)
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    pub(crate) fn to_css(&self) -> String {
        match self {
            ImageValue::None => "none".into(),
            ImageValue::Url(url) => url_css(url),
            ImageValue::Gradient(gradient) => gradient.to_css(),
            ImageValue::SpecifiedGradient(_) => "none".into(),
            ImageValue::Image(function) => {
                let mut parts = Vec::new();
                let mut source = Vec::new();
                if let Some(rtl) = function.rtl {
                    source.push(if rtl { "rtl" } else { "ltr" }.to_owned());
                }
                if let Some(src) = &function.src {
                    source.push(url_css(src));
                }
                if !source.is_empty() {
                    parts.push(source.join(" "));
                }
                if let Some(color) = &function.color {
                    parts.push(serialize::color(color));
                }
                format!("image({})", parts.join(", "))
            }
            ImageValue::ImageSet(options) => {
                let options: Vec<String> = options
                    .iter()
                    .map(|option| {
                        let mut out = format!(
                            "{} {}x",
                            option.image.to_css(),
                            serialize::number(f64::from(option.resolution))
                        );
                        if let Some(mime) = &option.mime {
                            out.push_str(" type(");
                            let _ = cssparser::serialize_string(mime, &mut out);
                            out.push(')');
                        }
                        out
                    })
                    .collect();
                format!("image-set({})", options.join(", "))
            }
            ImageValue::CrossFade(inputs) => {
                let inputs: Vec<String> = inputs
                    .iter()
                    .map(|(source, fraction)| {
                        let source = match source {
                            FadeSource::Image(image) => image.to_css(),
                            FadeSource::Color(color) => serialize::color(color),
                        };
                        match fraction {
                            Some(f) => format!("{source} {}", serialize::percentage(*f)),
                            None => source,
                        }
                    })
                    .collect();
                format!("cross-fade({})", inputs.join(", "))
            }
            ImageValue::Element(id) => {
                let mut out = String::from("element(#");
                let _ = cssparser::serialize_identifier(id, &mut out);
                out.push(')');
                out
            }
            ImageValue::Paint(arguments) => format!("paint({arguments})"),
        }
    }
}

fn url_css(url: &str) -> String {
    let mut out = String::from("url(");
    let _ = cssparser::serialize_string(url, &mut out);
    out.push(')');
    out
}

/// A URL or a string, which `image()` and `image-set()` take as a URL.
fn parse_src(input: &mut Parser<'_>) -> Result<Arc<str>> {
    let url = input.expect_url_or_string()?;
    Ok(Arc::from(&*url))
}

/// The arguments of `image()`: `<image-tags>? [ <image-src>? , <color>? ]!`.
fn parse_image_function(input: &mut Parser<'_>) -> Result<ImageValue> {
    let rtl = input
        .try_parse(|input| -> Result<bool> {
            let ident = input.expect_ident()?;
            match &*ident.to_ascii_lowercase() {
                "ltr" => Ok(false),
                "rtl" => Ok(true),
                _ => Err(ParseError::custom(())),
            }
        })
        .ok();
    let src = input.try_parse(parse_src).ok();
    let color = if src.is_none() {
        // Without a source, a color must follow, after a comma only if there are tags.
        if rtl.is_some() {
            return fail();
        }
        Some(Color::parse(input)?)
    } else if input.try_parse(|input| input.expect_comma()).is_ok() {
        Some(Color::parse(input)?)
    } else {
        None
    };
    input.expect_exhausted()?;
    Ok(ImageValue::Image(Arc::new(ImageFunction {
        rtl,
        src,
        color,
    })))
}

/// One option of `image-set()`: `[ <image> | <string> ] [ <resolution> || type(<string>) ]?`.
fn parse_set_option(input: &mut Parser<'_>) -> Result<ImageSetOption> {
    let image = match input.try_parse(|input| input.expect_string_cloned()) {
        Ok(url) => ImageValue::Url(Arc::from(&*url)),
        Err(_) => ImageValue::parse_image(input, false)?,
    };
    let mut resolution = None;
    let mut mime = None;
    loop {
        if resolution.is_none()
            && let Ok(dppx) = input.try_parse(|input| parse_typed(input, Base::Resolution))
        {
            if dppx < 0.0 {
                return fail();
            }
            resolution = Some(dppx);
        } else if mime.is_none()
            && let Ok(value) = input.try_parse(|input| -> Result<Arc<str>> {
                input.expect_function_matching("type")?;
                input.parse_nested_block(|input| {
                    let value = input.expect_string_cloned()?;
                    Ok::<_, ParseError<()>>(Arc::<str>::from(&*value))
                })
            })
        {
            mime = Some(value);
        } else {
            break;
        }
    }
    Ok(ImageSetOption {
        image,
        resolution: resolution.unwrap_or(1.0),
        mime,
    })
}

/// One input of `cross-fade()`: `<percentage [0,100]>? && [ <image> | <color> ]`.
fn parse_fade_input(input: &mut Parser<'_>) -> Result<(FadeSource, Option<f32>)> {
    let percentage = |input: &mut Parser<'_>| -> Result<f32> {
        let fraction = input.expect_percentage()?;
        if (0.0..=1.0).contains(&fraction) {
            Ok(fraction)
        } else {
            fail()
        }
    };
    let mut fraction = input.try_parse(percentage).ok();
    let source = match input.try_parse(|input| ImageValue::parse_image(input, true)) {
        Ok(image) => FadeSource::Image(image),
        Err(_) => FadeSource::Color(Color::parse(input)?),
    };
    if fraction.is_none() {
        fraction = input.try_parse(percentage).ok();
    }
    Ok((source, fraction))
}

/// The used fraction of each `cross-fade()` input (CSS Images 4 § 2.6): inputs without a
/// percentage share what the others leave, and a total over 100% scales down.
pub fn fade_fractions(inputs: &[(FadeSource, Option<f32>)]) -> Vec<f32> {
    let given: f32 = inputs.iter().filter_map(|(_, f)| *f).sum();
    let missing = inputs.iter().filter(|(_, f)| f.is_none()).count();
    let share = if missing > 0 {
        (1.0 - given).max(0.0) / missing as f32
    } else {
        0.0
    };
    let fractions: Vec<f32> = inputs.iter().map(|(_, f)| f.unwrap_or(share)).collect();
    let total: f32 = fractions.iter().sum();
    if total > 1.0 {
        fractions.iter().map(|f| f / total).collect()
    } else {
        fractions
    }
}
