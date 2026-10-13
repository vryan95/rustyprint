//! The checked-in C header matches the library's source.

#[path = "../examples/header.rs"]
#[allow(dead_code)]
mod header;

#[test]
fn the_header_is_up_to_date() {
    let actual = std::fs::read_to_string(header::path()).unwrap_or_default();
    assert!(
        actual.replace("\r\n", "\n") == header::generate(),
        "include/rustyprint.h is out of date: run `cargo run -p rustyprint-ffi --example header`"
    );
}
