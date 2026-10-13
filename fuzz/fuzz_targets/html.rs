//! The HTML parser and the DOM it builds.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rustyprint_dom::Document;

fuzz_target!(|html: &str| {
    let document = Document::parse(html);
    let _ = document.element_depth();
    let _ = document.base_url();
    for node in document.descendants(document.root()) {
        let _ = document.text_content(node);
    }
});
