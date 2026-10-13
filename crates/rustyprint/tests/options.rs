//! Page sizes, lengths and render options as JSON.

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use common::{MM, close};
use rustyprint::{
    Error, Limits, Remote, RenderOptions, Sides, Size, page_size, parse_length, render,
};

fn invalid(result: Result<impl std::fmt::Debug, Error>, mentions: &str) {
    match result {
        Err(Error::InvalidOptions(message)) => {
            assert!(
                message.contains(mentions),
                "{message:?} should mention {mentions:?}"
            )
        }
        other => panic!("expected invalid options mentioning {mentions:?}, got {other:?}"),
    }
}

fn json(text: &str) -> RenderOptions {
    RenderOptions::from_json(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

// Page sizes and lengths.

#[test]
fn page_sizes_by_name() {
    assert_eq!(page_size("A4").unwrap(), Size::A4);
    assert_eq!(page_size("letter").unwrap(), Size::LETTER);
    let sizes = [
        ("a3", 297.0 * MM, 420.0 * MM),
        ("A5", 148.0 * MM, 210.0 * MM),
        ("LEGAL", 8.5 * 96.0, 14.0 * 96.0),
    ];
    for (name, width, height) in sizes {
        let size = page_size(name).unwrap();
        assert!(
            close(size.width, width) && close(size.height, height),
            "{name}: {size:?}"
        );
    }
}

#[test]
fn unknown_page_size_is_invalid() {
    invalid(page_size("B4"), "B4");
    invalid(page_size(""), "");
}

#[test]
fn lengths_in_every_unit() {
    let lengths = [
        ("40px", 40.0),
        ("40", 40.0),
        ("0", 0.0),
        ("2cm", 2.0 * 96.0 / 2.54),
        ("15mm", 15.0 * MM),
        ("0.5in", 48.0),
        ("12pt", 16.0),
        (" 1in ", 96.0),
        ("-5mm", -5.0 * MM),
    ];
    for (text, px) in lengths {
        let length = parse_length(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        assert!(close(length, px), "{text:?} gave {length}, expected {px}");
    }
}

#[test]
fn invalid_lengths() {
    invalid(parse_length("15xx"), "xx");
    invalid(parse_length("abc"), "abc");
    invalid(parse_length(""), "");
    invalid(parse_length("mm"), "mm");
}

// JSON options.

#[test]
fn empty_json_gives_the_defaults() {
    assert_eq!(json("{}"), RenderOptions::default());
}

#[test]
fn page_size_by_name_or_lengths() {
    assert_eq!(json(r#"{"page_size": "Letter"}"#).page_size, Size::LETTER);
    let size = json(r#"{"page_size": {"width": "100mm", "height": "6in"}}"#).page_size;
    assert!(
        close(size.width, 100.0 * MM) && close(size.height, 576.0),
        "{size:?}"
    );
}

#[test]
fn landscape_swaps_the_page_size() {
    let options = json(r#"{"page_size": "A4", "landscape": true}"#);
    assert_eq!(
        options.page_size,
        Size::new(Size::A4.height, Size::A4.width)
    );
    let options = json(r#"{"landscape": false}"#);
    assert_eq!(options.page_size, Size::A4);
}

#[test]
fn margins_follow_the_css_shorthand() {
    let margin = |text: &str| json(&format!(r#"{{"margin": "{text}"}}"#)).margin;
    let sides = |top: f32, right: f32, bottom: f32, left: f32| Sides {
        top: top * MM,
        right: right * MM,
        bottom: bottom * MM,
        left: left * MM,
    };
    assert_eq!(margin("15mm"), Sides::all(15.0 * MM));
    assert_eq!(margin("10mm 20mm"), sides(10.0, 20.0, 10.0, 20.0));
    assert_eq!(margin("1mm 2mm 3mm"), sides(1.0, 2.0, 3.0, 2.0));
    assert_eq!(margin("1mm 2mm 3mm 4mm"), sides(1.0, 2.0, 3.0, 4.0));
    assert_eq!(margin("0"), Sides::all(0.0));
}

#[test]
fn title_and_stylesheet() {
    let options = json(r#"{"title": "Invoice 42", "stylesheet": "p { color: red }"}"#);
    assert_eq!(options.title.as_deref(), Some("Invoice 42"));
    assert_eq!(options.stylesheet.as_deref(), Some("p { color: red }"));
}

#[test]
fn remote_hosts_keep_the_default_fetch_limits() {
    let options = json(r#"{"remote": {"allowed_hosts": ["cdn.example.com"], "allow_http": true}}"#);
    let expected = Remote {
        allow_http: true,
        ..Remote::allowing(["cdn.example.com"])
    };
    assert_eq!(options.assets.remote, Some(expected));
}

#[test]
fn remote_fetch_limits_can_be_tuned() {
    let options = json(
        r#"{"remote": {"allowed_hosts": ["a.example"], "timeout_ms": 2500, "max_bytes": 1000,
            "max_redirects": 0, "max_fetches": 3}}"#,
    );
    let remote = options.assets.remote.unwrap();
    assert_eq!(remote.timeout, Duration::from_millis(2500));
    assert_eq!(remote.max_bytes, 1000);
    assert_eq!(remote.max_redirects, 0);
    assert_eq!(remote.max_fetches, 3);
    assert!(!remote.allow_http);
}

#[test]
fn json_never_fetches_or_reads_files_by_default() {
    let options = json("{}");
    assert_eq!(options.assets.remote, None);
    assert_eq!(options.assets.directory, None);
    assert!(options.assets.files.is_empty());
}

#[test]
fn limits_left_out_keep_their_defaults() {
    let options = json(r#"{"limits": {"max_pages": 10, "timeout_ms": 500}}"#);
    let expected = Limits {
        max_pages: 10,
        timeout: Some(Duration::from_millis(500)),
        ..Limits::default()
    };
    assert_eq!(options.limits, expected);
}

#[test]
fn null_timeout_means_no_deadline() {
    assert_eq!(
        json(r#"{"limits": {"timeout_ms": null}}"#).limits.timeout,
        None
    );
}

#[test]
fn invalid_json_options() {
    let cases = [
        ("{", ""),
        (r#"{"pagesize": "A4"}"#, "pagesize"),
        (r#"{"assets": {}}"#, "assets"),
        (r#"{"landscape": "yes"}"#, "landscape"),
        (r#"{"page_size": "B9"}"#, "B9"),
        (
            r#"{"page_size": {"width": "0mm", "height": "10mm"}}"#,
            "page_size",
        ),
        (r#"{"page_size": {"width": "10mm"}}"#, "height"),
        (r#"{"margin": "15xx"}"#, "15xx"),
        (r#"{"margin": "-1mm"}"#, "margin"),
        (r#"{"margin": "1mm 2mm 3mm 4mm 5mm"}"#, "margin"),
        (r#"{"limits": {"max_pages": -1}}"#, "max_pages"),
        (r#"{"limits": {"max_pagez": 1}}"#, "max_pagez"),
        (r#"{"remote": {"allow_http": true}}"#, "allowed_hosts"),
    ];
    for (text, mentions) in cases {
        invalid(RenderOptions::from_json(text), mentions);
    }
}

#[test]
fn json_options_render_like_typed_options() {
    let html = "<h1>Same</h1><p>Rendered either way.</p>";
    let typed = render(html, &RenderOptions::default()).unwrap().pdf;
    let from_json = render(html, &json(r#"{"page_size": "A4", "margin": "2cm"}"#))
        .unwrap()
        .pdf;
    assert_eq!(typed, from_json);
}

// The schema.

fn schema() -> serde_json::Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schema/render-options.schema.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).expect("the schema is valid JSON")
}

fn property_names(object: &serde_json::Value) -> BTreeSet<&str> {
    object["properties"]
        .as_object()
        .expect("an object with properties")
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn schema_describes_every_option() {
    let schema = schema();
    let names = |list: &[&'static str]| list.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        property_names(&schema),
        names(&[
            "landscape",
            "limits",
            "margin",
            "page_size",
            "remote",
            "stylesheet",
            "title"
        ])
    );
    assert_eq!(
        property_names(&schema["properties"]["limits"]),
        names(&[
            "max_depth",
            "max_image_pixels",
            "max_input_bytes",
            "max_nodes",
            "max_pages",
            "timeout_ms"
        ])
    );
    assert_eq!(
        property_names(&schema["properties"]["remote"]),
        names(&[
            "allow_http",
            "allowed_hosts",
            "max_bytes",
            "max_fetches",
            "max_redirects",
            "timeout_ms"
        ])
    );
    for object in [
        &schema,
        &schema["properties"]["limits"],
        &schema["properties"]["remote"],
    ] {
        assert_eq!(object["additionalProperties"], false);
    }
}

#[test]
fn schema_examples_are_accepted() {
    let schema = schema();
    let examples = schema["examples"]
        .as_array()
        .expect("the schema has examples");
    assert!(!examples.is_empty());
    for example in examples {
        json(&example.to_string());
    }
}
