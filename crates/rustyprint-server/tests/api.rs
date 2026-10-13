//! The HTTP API, exercised through the router without a network.

use std::collections::HashMap;

use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use rustyprint_server::{Config, app};
use tower::ServiceExt;

fn config(vars: &[(&str, &str)]) -> Config {
    let vars: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Config::from_vars(|name| vars.get(name).cloned()).expect("a valid configuration")
}

async fn send(config: Config, request: Request<Body>) -> (StatusCode, Response<Body>, Vec<u8>) {
    let response = app(config).oneshot(request).await.unwrap();
    let status = response.status();
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes().to_vec();
    (status, Response::from_parts(parts, Body::empty()), bytes)
}

fn json(body: serde_json::Value) -> Request<Body> {
    Request::post("/v1/pdf")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// A multipart/form-data body with the given parts.
fn multipart(parts: &[(&str, &[u8])]) -> Request<Body> {
    const BOUNDARY: &str = "rustyprint-test-boundary";
    let mut body = Vec::new();
    for (name, data) in parts {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{name}\"\r\n\
                 Content-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    Request::post("/v1/pdf")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn error_kind(body: &[u8]) -> (String, String) {
    let value: serde_json::Value = serde_json::from_slice(body).expect("a JSON error body");
    (
        value["error"].as_str().unwrap().to_owned(),
        value["message"].as_str().unwrap().to_owned(),
    )
}

fn pdf(bytes: &[u8]) -> lopdf::Document {
    lopdf::Document::load_mem(bytes).expect("a valid PDF")
}

fn page_size(bytes: &[u8]) -> (f32, f32) {
    let doc = pdf(bytes);
    let page = doc.get_pages()[&1];
    let media: Vec<f32> = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"MediaBox")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_float().unwrap())
        .collect();
    (media[2] - media[0], media[3] - media[1])
}

fn image_count(bytes: &[u8]) -> usize {
    pdf(bytes)
        .objects
        .values()
        .filter(|object| {
            object.as_stream().is_ok_and(|stream| {
                stream
                    .dict
                    .get(b"Subtype")
                    .and_then(|s| s.as_name())
                    .is_ok_and(|name| name == b"Image")
            })
        })
        .count()
}

/// A 2×2 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd, 0xd4, 0x9a,
    0x73, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x44, 0x0c, 0x10, 0x0a, 0x00, 0x1f, 0xee, 0x03, 0xfd, 0x8b, 0x5f, 0x14, 0xd4, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

// Health and metrics.

#[tokio::test]
async fn health() {
    let request = Request::get("/healthz").body(Body::empty()).unwrap();
    let (status, _, body) = send(config(&[]), request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"ok");
}

#[tokio::test]
async fn metrics_count_requests_and_renders() {
    let config = config(&[]);
    let app = app(config);
    let response = app
        .clone()
        .oneshot(json(serde_json::json!({ "html": "<p>x</p>" })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    let text = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(
        text.contains("rustyprint_requests_total{status=\"200\"} 1"),
        "{text}"
    );
    assert!(text.contains("rustyprint_render_seconds_count 1"), "{text}");
    assert!(text.contains("rustyprint_renders_in_flight 0"), "{text}");
}

// Rendering.

#[tokio::test]
async fn renders_json_requests() {
    let (status, response, body) = send(
        config(&[]),
        json(serde_json::json!({ "html": "<h1>Hello</h1>" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(response.headers()["x-rustyprint-warning-count"], "0");
    assert!(body.starts_with(b"%PDF-"));
}

#[tokio::test]
async fn json_options_apply() {
    let request = json(serde_json::json!({
        "html": "<p>x</p>",
        "options": { "page_size": "Letter", "margin": "1in" }
    }));
    let (status, _, body) = send(config(&[]), request).await;
    assert_eq!(status, StatusCode::OK);
    let (width, height) = page_size(&body);
    assert!((width - 612.0).abs() < 0.5 && (height - 792.0).abs() < 0.5);
}

#[tokio::test]
async fn renders_multipart_requests_with_assets() {
    let request = multipart(&[
        ("index.html", b"<p><img src='img/logo.png' width=20></p>"),
        ("options", br#"{"page_size": "A5"}"#),
        ("img/logo.png", PNG),
    ]);
    let (status, _, body) = send(config(&[]), request).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(image_count(&body), 1);
    let (width, _) = page_size(&body);
    assert!((width - 419.5).abs() < 0.5);
}

#[tokio::test]
async fn warnings_are_in_headers() {
    let request = json(serde_json::json!({
        "html": "<style>p { colör: red; colr: blue }</style><p>x</p>"
    }));
    let (status, response, _) = send(config(&[]), request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response.headers()["x-rustyprint-warning-count"], "2");
    let header = response.headers()["x-rustyprint-warnings"]
        .to_str()
        .expect("ASCII");
    let warnings: Vec<String> = serde_json::from_str(header).unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().any(|w| w.contains("colör")), "{warnings:?}");
}

// Bad requests.

#[tokio::test]
async fn invalid_requests() {
    let cases = [
        (
            Request::post("/v1/pdf")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{not json"))
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            json(serde_json::json!({ "options": {} })),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            json(serde_json::json!({ "html": "x", "extra": 1 })),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            json(serde_json::json!({ "html": "x", "options": { "page_size": "B9" } })),
            StatusCode::BAD_REQUEST,
            "invalid_options",
        ),
        (
            multipart(&[("img/logo.png", PNG)]),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            multipart(&[("index.html", b"<p>x</p>"), ("options", b"{")]),
            StatusCode::BAD_REQUEST,
            "invalid_options",
        ),
        (
            Request::post("/v1/pdf")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("<p>x</p>"))
                .unwrap(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ),
    ];
    for (request, expected_status, expected_kind) in cases {
        let (status, _, body) = send(config(&[]), request).await;
        assert_eq!(
            status,
            expected_status,
            "{}",
            String::from_utf8_lossy(&body)
        );
        assert_eq!(error_kind(&body).0, expected_kind);
    }
}

#[tokio::test]
async fn bodies_over_the_size_limit() {
    let config = config(&[("RUSTYPRINT_MAX_BODY_BYTES", "1000")]);
    let html = "<p>x</p>".repeat(500);
    let (status, _, body) = send(config, json(serde_json::json!({ "html": html }))).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_kind(&body).0, "payload_too_large");
}

#[tokio::test]
async fn multipart_bodies_over_the_size_limit() {
    let config = config(&[("RUSTYPRINT_MAX_BODY_BYTES", "1000")]);
    let html = "<p>x</p>".repeat(500);
    let (status, _, body) = send(config, multipart(&[("index.html", html.as_bytes())])).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_kind(&body).0, "payload_too_large");
}

// Limits and the server's policy.

#[tokio::test]
async fn exceeded_limits() {
    let request = json(serde_json::json!({
        "html": "<p>Line of text.</p>".repeat(300),
        "options": { "limits": { "max_pages": 1 } }
    }));
    let (status, _, body) = send(config(&[]), request).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (kind, message) = error_kind(&body);
    assert_eq!(kind, "limit_exceeded");
    assert!(message.contains("page count"), "{message}");
}

#[tokio::test]
async fn requests_cannot_raise_the_servers_limits() {
    let config = config(&[("RUSTYPRINT_LIMITS", r#"{"max_pages": 2}"#)]);
    let request = json(serde_json::json!({
        "html": "<p>Line of text.</p>".repeat(300),
        "options": { "limits": { "max_pages": 10000 } }
    }));
    let (status, _, body) = send(config, request).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        error_kind(&body).1.contains("of 2"),
        "{}",
        error_kind(&body).1
    );
}

#[tokio::test]
async fn passed_deadline() {
    let request = json(serde_json::json!({
        "html": "<p>Line of text.</p>".repeat(2000),
        "options": { "limits": { "timeout_ms": 1 } }
    }));
    let (status, _, body) = send(config(&[]), request).await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(error_kind(&body).0, "timeout");
}

#[tokio::test]
async fn requests_may_only_use_hosts_the_server_allows() {
    let html = "<p>x</p>";
    let remote = |hosts: &[&str]| {
        json(serde_json::json!({
            "html": html,
            "options": { "remote": { "allowed_hosts": hosts } }
        }))
    };
    // Without allowed hosts, the server fetches nothing.
    let (status, _, body) = send(config(&[]), remote(&["cdn.example.com"])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (kind, message) = error_kind(&body);
    assert_eq!(kind, "invalid_options");
    assert!(message.contains("cdn.example.com"), "{message}");

    let allowing = || {
        config(&[(
            "RUSTYPRINT_ALLOWED_HOSTS",
            "*.example.com, assets.example.org",
        )])
    };
    for hosts in [
        &["cdn.example.com"][..],
        &["assets.example.org", "img.example.com"],
    ] {
        let (status, _, body) = send(allowing(), remote(hosts)).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{hosts:?}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let (status, _, _) = send(allowing(), remote(&["evil.example.net"])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_needs_the_servers_permission() {
    let request = || {
        json(serde_json::json!({
            "html": "<p>x</p>",
            "options": { "remote": { "allowed_hosts": ["cdn.example.com"], "allow_http": true } }
        }))
    };
    let (status, _, _) = send(
        config(&[("RUSTYPRINT_ALLOWED_HOSTS", "cdn.example.com")]),
        request(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let both = config(&[
        ("RUSTYPRINT_ALLOWED_HOSTS", "cdn.example.com"),
        ("RUSTYPRINT_ALLOW_HTTP", "true"),
    ]);
    let (status, _, _) = send(both, request()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn internal_errors() {
    let (status, _, body) = send(
        config(&[]),
        json(serde_json::json!({ "html": "<!--rustyprint:panic-->" })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error_kind(&body).0, "internal");
}

#[tokio::test]
async fn unknown_paths() {
    let request = Request::get("/v2/pdf").body(Body::empty()).unwrap();
    let (status, _, _) = send(config(&[]), request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// Configuration.

#[test]
fn configuration_defaults() {
    let config = config(&[]);
    assert_eq!(config.addr.to_string(), "0.0.0.0:8080");
    assert_eq!(config.max_body_bytes, 50 * 1024 * 1024);
    assert!(config.concurrency >= 1);
    assert!(config.allowed_hosts.is_empty());
    assert!(!config.allow_http);
    assert!(config.assets_dir.is_none());
    assert_eq!(config.limits, rustyprint::Limits::default());
}

#[test]
fn invalid_configuration() {
    for (name, value) in [
        ("RUSTYPRINT_ADDR", "not an address"),
        ("RUSTYPRINT_CONCURRENCY", "0"),
        ("RUSTYPRINT_MAX_BODY_BYTES", "lots"),
        ("RUSTYPRINT_ALLOW_HTTP", "maybe"),
        ("RUSTYPRINT_LIMITS", "{"),
    ] {
        let vars: HashMap<String, String> = [(name.to_owned(), value.to_owned())].into();
        let error = Config::from_vars(|n| vars.get(n).cloned()).err();
        assert!(
            error.as_deref().is_some_and(|e| e.contains(name)),
            "{name}={value}: {error:?}"
        );
    }
}
