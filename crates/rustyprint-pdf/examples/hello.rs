//! Writes a sample PDF: `cargo run -p rustyprint-pdf --example hello [-- out.pdf]`

use rustyprint_layout::display::Document;
use rustyprint_layout::{Color, DisplayItem, Font, Page, Point, Size, TextRun};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "hello.pdf".to_owned());
    let font = Font::bundled();
    let lines = [
        (32.0, Color::BLACK, "Hello, rustyprint!"),
        (16.0, Color::BLACK, "Ligatures: office, fluffy, affine"),
        (16.0, Color::BLACK, "Kerning: AVATAR, Type, WAVE"),
        (16.0, Color::BLACK, "Combining marks: cafe\u{301}, n\u{303}"),
        (
            16.0,
            Color::BLACK,
            "Scripts: Ελληνικά, Кириллица, Tiếng Việt",
        ),
        (
            16.0,
            Color::rgb(0xc0, 0x39, 0x2b),
            "Color: this line is red.",
        ),
    ];
    let mut y = 96.0;
    let items = lines
        .into_iter()
        .map(|(size, color, text)| {
            y += size * 1.5;
            DisplayItem::Text(TextRun::new(
                font.clone(),
                size,
                color,
                Point::new(96.0, y),
                text,
            ))
        })
        .collect();
    let document = Document {
        title: Some("Hello".to_owned()),
        pages: vec![Page::new(Size::A4, items)],
        bookmarks: Vec::new(),
        color_profiles: Vec::new(),
        device_cmyk: None,
    };
    std::fs::write(&path, rustyprint_pdf::write(&document)?)?;
    println!("wrote {path}");
    Ok(())
}
