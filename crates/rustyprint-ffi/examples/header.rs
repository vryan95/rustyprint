//! Writes the C header, `include/rustyprint.h`, from the library's source with cbindgen:
//! `cargo run -p rustyprint-ffi --example header`. A test checks that it is up to date.

use std::path::{Path, PathBuf};

/// Where the header is written.
pub fn path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("include/rustyprint.h")
}

/// The header for the library's current source.
pub fn generate() -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = cbindgen::Config::from_file(dir.join("cbindgen.toml")).unwrap();
    let mut header = Vec::new();
    cbindgen::Builder::new()
        .with_config(config)
        .with_src(dir.join("src/lib.rs"))
        .generate()
        .expect("cbindgen reads the source")
        .write(&mut header);
    String::from_utf8(header).unwrap()
}

fn main() {
    std::fs::write(path(), generate()).unwrap();
    println!("wrote {}", path().display());
}
