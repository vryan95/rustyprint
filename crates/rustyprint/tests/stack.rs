//! Deeply nested documents, rendered from threads with small stacks.
//!
//! A stack overflow aborts the whole process instead of failing one test, so these tests have
//! a file of their own. Hosts call `render` from threads whose stack they choose: about 1 MB
//! for the Windows main thread, .NET and Ruby threads, 2 MB for tokio's blocking threads.
//! Every document within the default limits must render from any of them.

use std::thread;

use rustyprint::{Limits, RenderOptions, render};

/// Smaller than the stack of any host the bindings run in.
const SMALL_STACK: usize = 512 * 1024;

fn renders_on_a_small_stack(html: String) {
    thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(move || {
            render(&html, &RenderOptions::default()).expect("renders within the default limits")
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn nested_blocks() {
    renders_on_a_small_stack(format!("{}deep", "<div>".repeat(200)));
}

#[test]
fn nested_tables() {
    // Email-style layout: every table adds four levels (table, tbody, tr, td).
    let open = "<table><tr><td>".repeat(30);
    let close = "</td></tr></table>".repeat(30);
    renders_on_a_small_stack(format!("{open}cell{close}"));
}

#[test]
fn nested_inline_and_list_content() {
    let lists = "<ul><li>".repeat(60);
    let spans = "<span style='font-weight: bold'><em>".repeat(60);
    renders_on_a_small_stack(format!("{lists}{spans}text"));
}

#[test]
fn deepest_document_within_the_default_limits() {
    // `html` and `body` take two of the levels.
    let depth = Limits::default().max_depth - 2;
    renders_on_a_small_stack(format!("{}deep", "<div>".repeat(depth)));
}
