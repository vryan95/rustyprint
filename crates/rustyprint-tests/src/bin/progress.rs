//! Writes `progress.html` from the tracker files: `cargo run -p rustyprint-tests --bin progress`.

use std::path::Path;

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    rustyprint_tests::progress::write(&root)?;
    println!("wrote {}", rustyprint_tests::progress::PAGE);
    Ok(())
}
