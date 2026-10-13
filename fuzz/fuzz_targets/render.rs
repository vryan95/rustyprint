//! The whole pipeline, from HTML to PDF.
//!
//! `render` catches panics and returns them as `Error::Internal`, so that error is turned back
//! into a crash here. Tight limits keep every run short; exceeding them is not a bug.

#![no_main]

use std::time::Duration;

use libfuzzer_sys::fuzz_target;
use rustyprint::{Error, Limits, RenderOptions, render};

fuzz_target!(|html: &str| {
    let options = RenderOptions {
        limits: Limits {
            max_nodes: 20_000,
            max_depth: 128,
            max_pages: 20,
            max_image_pixels: 1_000_000,
            timeout: Some(Duration::from_secs(5)),
            ..Limits::default()
        },
        ..RenderOptions::default()
    };
    if let Err(Error::Internal(message)) = render(html, &options) {
        panic!("internal error: {message}");
    }
});
