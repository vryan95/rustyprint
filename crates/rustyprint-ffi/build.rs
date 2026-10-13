//! Tells the tests which target they run on, for the `cc` crate to build the C smoke test.

fn main() {
    for var in ["TARGET", "HOST"] {
        let value = std::env::var(var).unwrap();
        println!("cargo:rustc-env=RUSTYPRINT_FFI_{var}={value}");
    }
}
