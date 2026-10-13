//! Resource limits and the render deadline.

mod common;

use std::time::{Duration, Instant};

use common::{image_count, page_count, paragraphs, png};
use rustyprint::{Error, Limit, Limits, RenderOptions, render};

fn with_limits(limits: Limits) -> RenderOptions {
    RenderOptions {
        limits,
        ..RenderOptions::default()
    }
}

fn exceeds(result: Result<rustyprint::Output, Error>, expected: Limit, expected_max: u64) {
    match result {
        Err(Error::LimitExceeded { limit, max }) => {
            assert_eq!((limit, max), (expected, expected_max))
        }
        Err(other) => panic!("expected the {expected} limit, got {other}"),
        Ok(_) => panic!("expected the {expected} limit, but the render succeeded"),
    }
}

#[test]
fn default_limits() {
    let limits = Limits::default();
    assert_eq!(limits.max_input_bytes, 20 * 1024 * 1024);
    assert_eq!(limits.max_nodes, 500_000);
    assert_eq!(limits.max_depth, 512);
    assert_eq!(limits.max_pages, 2_000);
    assert_eq!(limits.max_image_pixels, 100_000_000);
    assert_eq!(limits.timeout, Some(Duration::from_secs(60)));
}

#[test]
fn input_size() {
    let html = "<p>A short document.</p>";
    let limits = |max| Limits {
        max_input_bytes: max,
        ..Limits::default()
    };
    let size = html.len();
    exceeds(
        render(html, &with_limits(limits(size - 1))),
        Limit::InputSize,
        size as u64 - 1,
    );
    render(html, &with_limits(limits(size))).expect("a document at the limit renders");
}

#[test]
fn node_count() {
    let html = "<span>x</span>".repeat(1_000);
    let limits = Limits {
        max_nodes: 500,
        ..Limits::default()
    };
    exceeds(render(&html, &with_limits(limits)), Limit::Nodes, 500);
    render(&html, &RenderOptions::default()).expect("renders within the default limits");
}

#[test]
fn nesting_depth() {
    // Shallow enough not to overflow the stack today; tests/stack.rs covers deep documents.
    let html = format!("{}deep", "<div>".repeat(20));
    let limits = Limits {
        max_depth: 10,
        ..Limits::default()
    };
    exceeds(render(&html, &with_limits(limits)), Limit::Depth, 10);
    render(&html, &RenderOptions::default()).expect("renders within the default limits");
}

#[test]
fn page_count_limit() {
    let html = paragraphs(200);
    let pages = page_count(&render(&html, &RenderOptions::default()).unwrap().pdf);
    assert!(
        pages > 3,
        "the document should span several pages, got {pages}"
    );
    let limits = |max: usize| Limits {
        max_pages: max,
        ..Limits::default()
    };
    exceeds(
        render(&html, &with_limits(limits(pages - 1))),
        Limit::Pages,
        (pages - 1) as u64,
    );
    render(&html, &with_limits(limits(pages))).expect("a document at the limit renders");
}

#[test]
fn oversized_images_are_skipped_with_a_warning() {
    let html = "<p>Logo: <img src='big.png'></p>";
    let mut options = RenderOptions::default();
    options.assets.files.insert("big.png".into(), png(20, 20));

    options.limits.max_image_pixels = 400;
    let output = render(html, &options).expect("an image at the limit renders");
    assert_eq!(image_count(&output.pdf), 1);
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);

    options.limits.max_image_pixels = 399;
    let output = render(html, &options).expect("an oversized image does not fail the render");
    assert_eq!(image_count(&output.pdf), 0);
    assert!(
        output.warnings.iter().any(|w| w.contains("big.png")),
        "{:?}",
        output.warnings
    );
}

#[test]
fn deadline_stops_a_long_render() {
    // Far more than 100 ms of work, most of it in layout, so a deadline that is only
    // checked between pipeline stages would overrun by seconds.
    let html = paragraphs(5_000);
    let timeout = Duration::from_millis(100);
    let options = with_limits(Limits {
        timeout: Some(timeout),
        ..Limits::default()
    });
    let start = Instant::now();
    let result = render(&html, &options);
    let elapsed = start.elapsed();
    assert!(
        matches!(result, Err(Error::Timeout)),
        "{:?}",
        result.map(|o| o.pdf.len())
    );
    assert!(
        elapsed < timeout + Duration::from_secs(1),
        "stopped {elapsed:?} after starting, for a {timeout:?} deadline"
    );
}

#[test]
fn no_deadline() {
    let options = with_limits(Limits {
        timeout: None,
        ..Limits::default()
    });
    render("<p>No hurry.</p>", &options).expect("renders without a deadline");
}

#[test]
fn limit_errors_name_the_limit() {
    let message = Error::LimitExceeded {
        limit: Limit::Pages,
        max: 2,
    }
    .to_string();
    assert!(
        message.contains("page count") && message.contains('2'),
        "{message}"
    );
    assert!(Error::Timeout.to_string().contains("deadline"));
}
