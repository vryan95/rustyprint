//! The CSS parser, for stylesheets and `style` attributes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rustyprint_css::stylesheet::Stylesheet;

fuzz_target!(|css: &str| {
    let _ = Stylesheet::parse(css);
    let _ = Stylesheet::parse_style_attribute(css);
});
