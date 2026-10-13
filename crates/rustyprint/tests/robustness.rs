//! Panics, renders on several threads at once, and warming up.

mod common;

use std::thread;

use common::varied_documents;
use rustyprint::{Error, Limits, Output, RenderOptions, render, warm_up};

/// Rendering this makes `render` panic, with the `test-support` feature.
const PANIC: &str = "<p>Before</p><!--rustyprint:panic--><p>After</p>";

fn pdf(html: &str) -> Vec<u8> {
    render(html, &RenderOptions::default())
        .unwrap_or_else(|e| panic!("{html}: {e}"))
        .pdf
}

#[test]
fn types_can_be_shared_across_threads() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<RenderOptions>();
    send_sync::<Limits>();
    send_sync::<Output>();
    send_sync::<Error>();
}

#[test]
fn panics_are_returned_as_internal_errors() {
    match render(PANIC, &RenderOptions::default()) {
        Err(Error::Internal(message)) => {
            assert!(message.contains("rustyprint:panic"), "{message:?}")
        }
        Err(other) => panic!("expected an internal error, got {other}"),
        Ok(_) => panic!("expected an internal error, but the render succeeded"),
    }
}

#[test]
fn a_panic_leaves_no_state_behind() {
    // Renders on a fresh thread are the reference. The same thread then renders after a
    // panic, which must not leave anything behind in its thread-local caches.
    let reference: Vec<Vec<u8>> =
        thread::spawn(|| varied_documents().iter().map(|h| pdf(h)).collect())
            .join()
            .unwrap();
    thread::spawn(move || {
        for (html, expected) in varied_documents().iter().zip(&reference) {
            assert!(render(PANIC, &RenderOptions::default()).is_err());
            assert!(pdf(html) == *expected, "differs after a panic: {html}");
        }
    })
    .join()
    .unwrap();
}

#[test]
fn concurrent_renders_match_single_threaded_ones() {
    let documents = varied_documents();
    let reference: Vec<Vec<u8>> = documents.iter().map(|h| pdf(h)).collect();
    thread::scope(|scope| {
        for offset in 0..8 {
            let documents = &documents;
            let reference = &reference;
            scope.spawn(move || {
                // Each thread starts at a different document, so different documents
                // render at the same time.
                for i in 0..documents.len() * 2 {
                    let index = (i + offset) % documents.len();
                    assert!(
                        pdf(&documents[index]) == reference[index],
                        "differs on a thread: {}",
                        documents[index]
                    );
                }
            });
        }
    });
}

#[test]
fn warming_up_changes_nothing() {
    let html = &varied_documents()[0];
    let before = pdf(html);
    warm_up();
    warm_up();
    assert_eq!(pdf(html), before);
}
