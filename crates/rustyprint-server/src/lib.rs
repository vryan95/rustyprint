//! HTTP service that renders HTML and CSS to PDF.
//!
//! - `POST /v1/pdf` renders a document, sent as JSON (`{"html": ..., "options": {...}}`) or as
//!   `multipart/form-data` (an `index.html` part, an optional `options` part, and the files
//!   the document refers to, each as a part named by its path).
//! - `GET /healthz` answers `ok`.
//! - `GET /metrics` gives Prometheus metrics.
//!
//! The server's [`Config`] decides what a request may do: requests can lower the limits and
//! fetch from the hosts it allows, but never raise the limits or fetch from other hosts.

mod config;
mod metrics;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rustyprint::{Error, Limits, Remote, RenderOptions};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Semaphore;

pub use config::Config;
use metrics::Metrics;

/// At most this many warnings go into the `X-Rustyprint-Warnings` header, each cut to
/// [`WARNING_CHARS`], so the header stays a reasonable size.
const HEADER_WARNINGS: usize = 50;
const WARNING_CHARS: usize = 300;

struct AppState {
    config: Config,
    /// One permit per render that may run at once; other requests wait for one.
    renders: Arc<Semaphore>,
    metrics: Metrics,
}

/// The service's routes, for `axum::serve`.
pub fn app(config: Config) -> Router {
    let state = Arc::new(AppState {
        renders: Arc::new(Semaphore::new(config.concurrency)),
        metrics: Metrics::default(),
        config,
    });
    let body_limit = state.config.max_body_bytes;
    Router::new()
        .route("/v1/pdf", post(render_pdf))
        .route("/healthz", get(|| async { "ok" }))
        .route("/metrics", get(metrics))
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(middleware::from_fn_with_state(state.clone(), observe))
        .with_state(state)
}

/// Logs each request and counts it by status.
async fn observe(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    let status = response.status();
    state.metrics.request(status.as_u16());
    tracing::info!(
        %method,
        path,
        status = status.as_u16(),
        ms = started.elapsed().as_millis() as u64,
        "request"
    );
    response
}

async fn metrics(State(state): State<Arc<AppState>>) -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics.render(),
    )
        .into_response()
}

/// An error, answered as `{"error": kind, "message": ...}`.
struct ApiError {
    status: StatusCode,
    kind: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            kind,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    fn invalid_options(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_options", message)
    }

    fn too_large(limit: usize) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            format!("the request body is larger than {limit} bytes"),
        )
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        let message = error.to_string();
        match error {
            Error::InvalidOptions(message) => Self::invalid_options(message),
            Error::LimitExceeded { .. } => {
                Self::new(StatusCode::UNPROCESSABLE_ENTITY, "limit_exceeded", message)
            }
            Error::Timeout => Self::new(StatusCode::GATEWAY_TIMEOUT, "timeout", message),
            _ => Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({ "error": self.kind, "message": self.message }));
        (self.status, body).into_response()
    }
}

/// A render request, however it was sent.
struct RenderRequest {
    html: String,
    options: Option<Value>,
    assets: BTreeMap<String, Vec<u8>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonRequest {
    html: String,
    #[serde(default)]
    options: Option<Value>,
}

async fn render_pdf(State(state): State<Arc<AppState>>, request: Request) -> Response {
    match handle(&state, request).await {
        Ok(response) => response,
        Err(error) => {
            state.metrics.error(error.kind);
            error.into_response()
        }
    }
}

async fn handle(state: &AppState, request: Request) -> Result<Response, ApiError> {
    let request = read_request(state, request).await?;
    let mut options = options(request.options, &state.config)?;
    options.assets.files = request.assets;

    let _permit = state.renders.clone().acquire_owned().await.map_err(|_| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "shutting down",
        )
    })?;
    let _in_flight = state.metrics.render_started();
    let started = Instant::now();
    let html = request.html;
    let rendered = tokio::task::spawn_blocking(move || rustyprint::render(&html, &options))
        .await
        .map_err(|error| ApiError::from(Error::Internal(error.to_string())))?;
    state.metrics.render_finished(started.elapsed());
    let output = rendered?;

    let mut response = Response::new(Body::from(output.pdf));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert("x-rustyprint-warning-count", output.warnings.len().into());
    if !output.warnings.is_empty() {
        let shown: Vec<String> = output
            .warnings
            .iter()
            .take(HEADER_WARNINGS)
            .map(|w| w.chars().take(WARNING_CHARS).collect())
            .collect();
        let value = ascii_json(&shown);
        if let Ok(value) = HeaderValue::from_str(&value) {
            headers.insert("x-rustyprint-warnings", value);
        }
    }
    Ok(response)
}

async fn read_request(state: &AppState, request: Request) -> Result<RenderRequest, ApiError> {
    let limit = state.config.max_body_bytes;
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if content_type.starts_with("application/json") {
        let body = axum::body::to_bytes(request.into_body(), limit)
            .await
            .map_err(|_| ApiError::too_large(limit))?;
        let request: JsonRequest = serde_json::from_slice(&body)
            .map_err(|e| ApiError::bad_request(format!("invalid JSON body: {e}")))?;
        Ok(RenderRequest {
            html: request.html,
            options: request.options,
            assets: BTreeMap::new(),
        })
    } else if content_type.starts_with("multipart/form-data") {
        read_multipart(request, limit).await
    } else {
        Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "send application/json or multipart/form-data",
        ))
    }
}

async fn read_multipart(request: Request, limit: usize) -> Result<RenderRequest, ApiError> {
    let mut multipart = Multipart::from_request(request, &())
        .await
        .map_err(|e| ApiError::bad_request(e.body_text()))?;
    let mut html = None;
    let mut options = None;
    let mut assets = BTreeMap::new();
    loop {
        let field = multipart
            .next_field()
            .await
            .map_err(|e| multipart_error(&e, limit))?;
        let Some(field) = field else {
            break;
        };
        let name = field
            .name()
            .or(field.file_name())
            .unwrap_or_default()
            .to_owned();
        let data = field
            .bytes()
            .await
            .map_err(|e| multipart_error(&e, limit))?;
        match name.as_str() {
            "index.html" => {
                let text = String::from_utf8(data.to_vec())
                    .map_err(|_| ApiError::bad_request("`index.html` is not UTF-8"))?;
                html = Some(text);
            }
            "options" => {
                let value = serde_json::from_slice(&data)
                    .map_err(|e| ApiError::invalid_options(format!("options: {e}")))?;
                options = Some(value);
            }
            "" => return Err(ApiError::bad_request("a part has no name")),
            _ => {
                assets.insert(name, data.to_vec());
            }
        }
    }
    let html = html.ok_or_else(|| ApiError::bad_request("no `index.html` part"))?;
    Ok(RenderRequest {
        html,
        options,
        assets,
    })
}

fn multipart_error(error: &axum::extract::multipart::MultipartError, limit: usize) -> ApiError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::too_large(limit)
    } else {
        ApiError::bad_request(error.body_text())
    }
}

/// The request's options under the server's policy: limits the request leaves out are the
/// server's, none may be higher than the server's, and remote hosts must be ones the server
/// allows.
fn options(request: Option<Value>, config: &Config) -> Result<RenderOptions, ApiError> {
    let mut value = match request {
        None | Some(Value::Null) => json!({}),
        Some(value @ Value::Object(_)) => value,
        Some(_) => return Err(ApiError::invalid_options("options must be a JSON object")),
    };
    // The limits the request leaves out are the server's.
    let server = limits_json(&config.limits);
    let object = value.as_object_mut().expect("an object, checked above");
    match object.entry("limits").or_insert_with(|| json!({})) {
        Value::Object(limits) => {
            for (key, limit) in server {
                limits.entry(key).or_insert(limit);
            }
        }
        _ => return Err(ApiError::invalid_options("limits must be a JSON object")),
    }
    let mut options = RenderOptions::from_json(&value.to_string())?;
    options.limits = lowest(&options.limits, &config.limits);
    if let Some(remote) = &mut options.assets.remote {
        allowed(remote, config)?;
    }
    options.assets.directory = config.assets_dir.clone();
    Ok(options)
}

fn limits_json(limits: &Limits) -> serde_json::Map<String, Value> {
    let value = json!({
        "max_input_bytes": limits.max_input_bytes,
        "max_nodes": limits.max_nodes,
        "max_depth": limits.max_depth,
        "max_pages": limits.max_pages,
        "max_image_pixels": limits.max_image_pixels,
        "timeout_ms": limits.timeout.map(|t| t.as_millis() as u64),
    });
    match value {
        Value::Object(map) => map,
        _ => unreachable!("built as an object"),
    }
}

/// Each limit at the lower of the two; no deadline is the highest.
fn lowest(request: &Limits, server: &Limits) -> Limits {
    let timeout = match (request.timeout, server.timeout) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    Limits {
        max_input_bytes: request.max_input_bytes.min(server.max_input_bytes),
        max_nodes: request.max_nodes.min(server.max_nodes),
        max_depth: request.max_depth.min(server.max_depth),
        max_pages: request.max_pages.min(server.max_pages),
        max_image_pixels: request.max_image_pixels.min(server.max_image_pixels),
        timeout,
    }
}

/// Checks that every host the request may fetch from is one the server allows, and keeps
/// its fetch limits within the server's.
fn allowed(remote: &mut Remote, config: &Config) -> Result<(), ApiError> {
    let server = Remote::allowing(config.allowed_hosts.iter().cloned());
    for host in &remote.allowed_hosts {
        let listed = config
            .allowed_hosts
            .iter()
            .any(|h| h.eq_ignore_ascii_case(host));
        if !listed && (host.starts_with("*.") || !server.allows_host(host)) {
            return Err(ApiError::invalid_options(format!(
                "this server does not fetch from `{host}`"
            )));
        }
    }
    if remote.allow_http && !config.allow_http {
        return Err(ApiError::invalid_options(
            "this server does not fetch plain http URLs",
        ));
    }
    remote.timeout = remote.timeout.min(server.timeout);
    remote.max_bytes = remote.max_bytes.min(server.max_bytes);
    remote.max_redirects = remote.max_redirects.min(server.max_redirects);
    remote.max_fetches = remote.max_fetches.min(server.max_fetches);
    Ok(())
}

/// `value` as JSON with every character outside ASCII escaped, as a header value needs.
fn ascii_json(value: &impl serde::Serialize) -> String {
    let text = serde_json::to_string(value).expect("strings serialize");
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn escapes_everything_outside_ascii() {
        let text = ascii_json(&vec!["colör 😀"]);
        assert!(text.is_ascii());
        assert_eq!(text, r#"["col\u00f6r \ud83d\ude00"]"#);
        let back: Vec<String> = serde_json::from_str(&text).unwrap();
        assert_eq!(back, ["colör 😀"]);
    }

    #[test]
    fn the_lower_limit_wins_and_no_deadline_is_highest() {
        let server = Limits {
            max_pages: 10,
            timeout: None,
            ..Limits::default()
        };
        let request = Limits {
            max_pages: 100,
            timeout: Some(Duration::from_secs(5)),
            ..Limits::default()
        };
        let limits = lowest(&request, &server);
        assert_eq!(limits.max_pages, 10);
        assert_eq!(limits.timeout, Some(Duration::from_secs(5)));
    }
}
