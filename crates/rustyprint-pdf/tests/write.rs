use rustyprint_layout::display::Document;
use rustyprint_layout::{Color, DisplayItem, Font, Page, Point, Size, TextRun};

fn page(lines: &[&str]) -> Page {
    let items = lines
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let origin = Point::new(96.0, 96.0 + 24.0 * i as f32);
            DisplayItem::Text(TextRun::new(
                Font::bundled(),
                16.0,
                Color::BLACK,
                origin,
                *text,
            ))
        })
        .collect();
    Page::new(Size::A4, items)
}

fn document(pages: Vec<Page>) -> Document {
    Document {
        title: Some("Test".to_owned()),
        pages,
        bookmarks: Vec::new(),
        color_profiles: Vec::new(),
        device_cmyk: None,
    }
}

fn parse(pdf: &[u8]) -> lopdf::Document {
    lopdf::Document::load_mem(pdf).expect("output parses as PDF")
}

#[test]
fn writes_one_page_per_page() {
    let pdf =
        rustyprint_pdf::write(&document(vec![page(&["one"]), page(&["two"]), page(&[])])).unwrap();
    assert_eq!(parse(&pdf).get_pages().len(), 3);
}

#[test]
fn page_size_is_in_points() {
    let pdf = rustyprint_pdf::write(&document(vec![page(&[])])).unwrap();
    let doc = parse(&pdf);
    let page_id = doc.get_pages()[&1];
    let media_box = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"MediaBox")
        .unwrap()
        .as_array()
        .unwrap();
    let size: Vec<f32> = media_box.iter().map(|v| v.as_float().unwrap()).collect();
    assert_eq!(size[0..2], [0.0, 0.0]);
    assert!((size[2] - 595.28).abs() < 0.01, "width {}", size[2]);
    assert!((size[3] - 841.89).abs() < 0.01, "height {}", size[3]);
}

#[test]
fn text_is_extractable() {
    let pdf = rustyprint_pdf::write(&document(vec![page(&["Hello office", "Ελληνικά"])])).unwrap();
    let text = parse(&pdf).extract_text(&[1]).unwrap();
    assert!(text.contains("Hello office"), "extracted {text:?}");
    assert!(text.contains("Ελληνικά"), "extracted {text:?}");
}

#[test]
fn fonts_are_subset_and_shared() {
    let pdf = rustyprint_pdf::write(&document(vec![page(&["Hello"]), page(&["World"])])).unwrap();
    // The full font is about 600 KB.
    assert!(pdf.len() < 20_000, "PDF is {} bytes", pdf.len());
    let doc = parse(&pdf);
    let fonts = doc.objects.values().filter(|object| {
        object.as_dict().is_ok_and(|dict| {
            dict.get(b"Subtype")
                .is_ok_and(|s| s.as_name().is_ok_and(|n| n == b"Type0"))
        })
    });
    assert_eq!(fonts.count(), 1);
}

#[test]
fn output_is_deterministic() {
    let doc = document(vec![page(&["Same input,", "same bytes."])]);
    assert_eq!(
        rustyprint_pdf::write(&doc).unwrap(),
        rustyprint_pdf::write(&doc).unwrap()
    );
}

#[test]
fn native_colors_keep_their_color_space() {
    use rustyprint_layout::Rect;
    use rustyprint_layout::display::{ColorProfile, DEVICE_CMYK, NativeColor};
    let native = |space: u32, components: [u16; 4]| Color {
        native: Some(NativeColor { space, components }),
        ..Color::rgb(0, 0, 0)
    };
    let items = vec![
        DisplayItem::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            color: native(0, [0, 65535, 0, 0]),
        },
        DisplayItem::Rect {
            rect: Rect::new(10.0, 0.0, 10.0, 10.0),
            color: native(DEVICE_CMYK, [0, 0, 0, 65535]),
        },
        DisplayItem::Rect {
            rect: Rect::new(20.0, 0.0, 10.0, 10.0),
            color: Color::rgb(255, 0, 0),
        },
    ];
    let mut document = document(vec![Page::new(Size::A4, items)]);
    let profile = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/css-at-rules/assets/cmyk.icc"),
    )
    .unwrap();
    document.color_profiles = vec![ColorProfile {
        data: profile.into(),
        components: 4,
    }];
    let pdf = parse(&rustyprint_pdf::write(&document).unwrap());
    let (_, page) = pdf.get_pages().into_iter().next().unwrap();
    let content = String::from_utf8_lossy(&pdf.get_page_content(page)).into_owned();
    assert!(content.contains("/Cs0 cs"), "{content}");
    assert!(content.contains("0 1 0 0 scn"), "{content}");
    assert!(content.contains("0 0 0 1 k"), "{content}");
    assert!(content.contains("1 0 0 rg"), "{content}");
    // The profile is embedded as an ICC-based color space with four components.
    let icc = pdf
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .find(|stream| stream.dict.get(b"N").is_ok())
        .expect("an ICC profile stream");
    assert_eq!(icc.dict.get(b"N").unwrap().as_i64().unwrap(), 4);
}
