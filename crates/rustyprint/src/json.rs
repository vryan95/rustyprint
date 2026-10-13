//! Render options as JSON, in the format of `schema/render-options.schema.json`.

use std::time::Duration;

use serde::{Deserialize, Deserializer};

use crate::{Error, Limits, Remote, RenderOptions, Size, page_size, parse_length, parse_margin};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// A name, or `{ "width": …, "height": … }`; checked by hand for clearer errors.
    page_size: Option<serde_json::Value>,
    #[serde(default)]
    landscape: bool,
    margin: Option<String>,
    title: Option<String>,
    stylesheet: Option<String>,
    remote: Option<RemoteOptions>,
    #[serde(default)]
    limits: LimitOptions,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageLengths {
    width: String,
    height: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteOptions {
    allowed_hosts: Vec<String>,
    allow_http: Option<bool>,
    timeout_ms: Option<u64>,
    max_bytes: Option<u64>,
    max_redirects: Option<u32>,
    max_fetches: Option<usize>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitOptions {
    max_input_bytes: Option<usize>,
    max_nodes: Option<usize>,
    max_depth: Option<usize>,
    max_pages: Option<usize>,
    max_image_pixels: Option<u64>,
    /// Left out keeps the default; `null` means no deadline.
    #[serde(default, deserialize_with = "nullable")]
    timeout_ms: Option<Option<u64>>,
}

fn nullable<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<u64>>, D::Error> {
    Option::deserialize(deserializer).map(Some)
}

/// Prefixes the message of an invalid-options error with the field it is about.
fn in_field(field: &str) -> impl FnOnce(Error) -> Error + '_ {
    move |error| match error {
        Error::InvalidOptions(message) => Error::InvalidOptions(format!("{field}: {message}")),
        other => other,
    }
}

pub(crate) fn from_json(json: &str) -> Result<RenderOptions, Error> {
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let parsed: Options = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|e| Error::InvalidOptions(e.to_string()))?;
    deserializer
        .end()
        .map_err(|e| Error::InvalidOptions(e.to_string()))?;

    let mut options = RenderOptions::default();
    if let Some(size) = parsed.page_size {
        options.page_size = page_size_of(size).map_err(in_field("page_size"))?;
    }
    if parsed.landscape {
        let Size { width, height } = options.page_size;
        options.page_size = Size::new(width.max(height), width.min(height));
    }
    if let Some(margin) = parsed.margin {
        options.margin = parse_margin(&margin).map_err(in_field("margin"))?;
    }
    options.title = parsed.title;
    options.stylesheet = parsed.stylesheet;
    options.assets.remote = parsed.remote.map(|remote| {
        let default = Remote::allowing(remote.allowed_hosts);
        Remote {
            allow_http: remote.allow_http.unwrap_or(default.allow_http),
            timeout: remote
                .timeout_ms
                .map_or(default.timeout, Duration::from_millis),
            max_bytes: remote.max_bytes.unwrap_or(default.max_bytes),
            max_redirects: remote.max_redirects.unwrap_or(default.max_redirects),
            max_fetches: remote.max_fetches.unwrap_or(default.max_fetches),
            ..default
        }
    });
    let limits = parsed.limits;
    let default = Limits::default();
    options.limits = Limits {
        max_input_bytes: limits.max_input_bytes.unwrap_or(default.max_input_bytes),
        max_nodes: limits.max_nodes.unwrap_or(default.max_nodes),
        max_depth: limits.max_depth.unwrap_or(default.max_depth),
        max_pages: limits.max_pages.unwrap_or(default.max_pages),
        max_image_pixels: limits.max_image_pixels.unwrap_or(default.max_image_pixels),
        timeout: limits
            .timeout_ms
            .map_or(default.timeout, |ms| ms.map(Duration::from_millis)),
    };
    Ok(options)
}

fn page_size_of(value: serde_json::Value) -> Result<Size, Error> {
    let invalid = Error::InvalidOptions;
    match value {
        serde_json::Value::String(name) => page_size(&name),
        serde_json::Value::Object(_) => {
            let lengths: PageLengths =
                serde_json::from_value(value).map_err(|e| invalid(e.to_string()))?;
            let width = parse_length(&lengths.width).map_err(in_field("width"))?;
            let height = parse_length(&lengths.height).map_err(in_field("height"))?;
            if width <= 0.0 || height <= 0.0 {
                return Err(invalid(format!(
                    "the page must be wider and taller than 0, not {} by {}",
                    lengths.width, lengths.height
                )));
            }
            Ok(Size::new(width, height))
        }
        other => Err(invalid(format!(
            "expected a page size name or {{\"width\", \"height\"}}, not `{other}`"
        ))),
    }
}
