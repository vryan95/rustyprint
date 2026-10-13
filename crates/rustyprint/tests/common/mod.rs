//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::io::Cursor;

/// CSS pixels per millimetre.
pub const MM: f32 = 96.0 / 25.4;

pub fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

pub fn page_count(pdf: &[u8]) -> usize {
    lopdf::Document::load_mem(pdf)
        .expect("valid PDF")
        .get_pages()
        .len()
}

/// How many image XObjects the PDF holds.
pub fn image_count(pdf: &[u8]) -> usize {
    let doc = lopdf::Document::load_mem(pdf).expect("valid PDF");
    doc.objects
        .values()
        .filter(|object| {
            object.as_stream().is_ok_and(|stream| {
                stream
                    .dict
                    .get(b"Subtype")
                    .and_then(|s| s.as_name())
                    .is_ok_and(|name| name == b"Image")
            })
        })
        .count()
}

/// `count` paragraphs of filler text.
pub fn paragraphs(count: usize) -> String {
    (0..count)
        .map(|i| {
            format!(
                "<p>Paragraph {i}. The quick brown fox jumps over the lazy dog, and then \
                 it jumps back again, because the dog never minded.</p>"
            )
        })
        .collect()
}

/// A solid PNG of `width` by `height` pixels.
pub fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([200, 30, 30]));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("PNG encodes");
    bytes
}

/// Documents that between them exercise most of the engine, including the parts with
/// per-thread caches: calc, grid, writing modes, filters and rasterised effects.
pub fn varied_documents() -> Vec<String> {
    vec![
        "<h1>Title</h1><p>Hello <b>bold</b> and <i>italic</i> world.</p>".to_owned(),
        "<div style='display: grid; grid-template-columns: repeat(3, 1fr); gap: calc(1em + 2px)'>\
         <div>a</div><div>b</div><div>c</div><div>d</div></div>"
            .to_owned(),
        "<p style='writing-mode: vertical-rl; height: 5cm'>縦書きの文章です。</p>\
         <ol><li>one</li><li>two</li></ol>"
            .to_owned(),
        "<div style='filter: blur(2px); background: linear-gradient(red, blue); \
         width: 4cm; height: 2cm; box-shadow: 0 0 8px black'>Effects</div>"
            .to_owned(),
        format!(
            "<table border=1>{}</table>",
            "<tr><td>cell</td><td>cell</td></tr>".repeat(80)
        ),
    ]
}
