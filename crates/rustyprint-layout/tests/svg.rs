//! `<svg>` elements in a document: sized as replaced elements, and drawn from their
//! descendants with the styles the document gives them.

use std::sync::Arc;

use rustyprint_css::{StyleOptions, compute_styles};
use rustyprint_dom::Document;
use rustyprint_layout::{
    DisplayItem, Image, ImageData, Images, PageOptions, Rect, Sides, Size, layout_tree,
    paint_fragment,
};

/// What `body` (with no body margin) paints on an 800 × 1000 page area, and the warnings.
/// `red.png` is a 10 × 10 red image and `label.ttf` a monospace font; images are drawn with
/// at most `max_pixels` pixels.
fn paint_within(body: &str, max_pixels: u64) -> (Vec<DisplayItem>, Vec<String>) {
    let html = format!("<!DOCTYPE html><style>body {{ margin: 0 }}</style><body>{body}");
    let document = Document::parse(&html);
    let load = |_: &str| None;
    let load_font = |url: &str| {
        let mono: &[u8] = include_bytes!("../../../fonts/NotoSansMono-Regular.ttf");
        (url == "label.ttf").then(|| Arc::from(mono))
    };
    let styles = compute_styles(
        &document,
        &StyleOptions {
            page_size: (800.0, 1000.0),
            media_size: (800.0, 1000.0),
            zoom: 1.0,
            stylesheet: None,
            parsed_stylesheet: None,
            load: &load,
            font_metrics: None,
            load_font: Some(&load_font),
            screen: false,
        },
    );
    let page = PageOptions {
        size: Size::new(800.0, 1000.0),
        margin: Sides::default(),
    };
    let png = {
        let mut out = std::io::Cursor::new(Vec::new());
        let red = image::RgbImage::from_pixel(10, 10, image::Rgb([255, 0, 0]));
        image::DynamicImage::from(red)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    };
    let load_file = move |url: &str| (url == "red.png").then(|| png.clone());
    let images = Images::new(&load_file).with_max_pixels(max_pixels);
    let root = layout_tree(&document, &styles, &page, &images).unwrap();
    let items = paint_fragment(&root, &images);
    (items, images.warnings())
}

fn paint(body: &str) -> Vec<DisplayItem> {
    let (items, warnings) = paint_within(body, u64::MAX);
    assert!(warnings.is_empty(), "{warnings:?}");
    items
}

/// The pictures painted, with where they are.
fn pictures(items: &[DisplayItem]) -> Vec<(Arc<Image>, Rect)> {
    items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { image, rect, .. } => Some((image.clone(), *rect)),
            _ => None,
        })
        .collect()
}

/// The only picture `body` paints.
#[track_caller]
fn picture(body: &str) -> (Arc<Image>, Rect) {
    let pictures = pictures(&paint(body));
    assert_eq!(pictures.len(), 1, "one picture");
    pictures[0].clone()
}

/// The color and opacity at a point of a picture, given in the units of its own size.
#[track_caller]
fn rgba(image: &Image, x: f32, y: f32) -> [u8; 4] {
    let (width, height) = image.natural.expect("an SVG has a size of its own");
    let column = ((x / width * image.width as f32) as u32).min(image.width - 1);
    let row = ((y / height * image.height as f32) as u32).min(image.height - 1);
    let at = (row * image.width + column) as usize;
    let ImageData::Pixels { samples, alpha, .. } = &image.data else {
        panic!("an SVG is drawn to pixels");
    };
    let alpha = alpha.as_ref().map_or(255, |alpha| alpha[at]);
    [
        samples[at * 3],
        samples[at * 3 + 1],
        samples[at * 3 + 2],
        alpha,
    ]
}

/// Whether a picture has a dark, opaque pixel in a box given in the units of its size.
fn has_ink(image: &Image, x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> bool {
    y.flat_map(|y| x.clone().map(move |x| (x, y)))
        .any(|(x, y)| {
            let [r, g, b, a] = rgba(image, x as f32 + 0.5, y as f32 + 0.5);
            a > 200 && r < 80 && g < 80 && b < 80
        })
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[track_caller]
fn assert_size(rect: Rect, width: f32, height: f32) {
    assert!(
        (rect.width - width).abs() < 0.01 && (rect.height - height).abs() < 0.01,
        "expected {width} × {height}, got {} × {}",
        rect.width,
        rect.height
    );
}

#[test]
fn an_svg_element_is_drawn_from_its_shapes() {
    let items = paint(
        "<p style='margin: 0; line-height: 20px'>before</p>\
         <svg width='200' height='120' viewBox='0 0 200 120'>\
         <rect width='200' height='120' fill='#ff0000'/>\
         <circle cx='60' cy='60' r='40' fill='#0000ff'/>\
         <text x='120' y='60' font-size='20'>12%</text></svg>",
    );
    let pictures = pictures(&items);
    assert_eq!(pictures.len(), 1);
    let (image, rect) = &pictures[0];
    assert_size(*rect, 200.0, 120.0);
    assert!(rect.y >= 20.0, "below the paragraph, at {}", rect.y);
    assert_eq!(rgba(image, 5.0, 5.0), RED);
    assert_eq!(rgba(image, 60.0, 60.0), BLUE);
    assert_eq!(rgba(image, 190.0, 110.0), RED);
    // The label is drawn in the picture, and is not text of the page.
    assert!(has_ink(image, 120..160, 40..62));
    let page_text: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text(run) => Some(run),
            _ => None,
        })
        .collect();
    assert_eq!(page_text.len(), 1, "only the paragraph is page text");
}

#[test]
fn an_svg_element_is_inline_and_sized_as_a_replaced_element() {
    let square = "<rect width='100' height='50' fill='red'/>";
    // Attributes are its size.
    let (_, rect) = picture(&format!("<svg width='120' height='80'>{square}</svg>"));
    assert_size(rect, 120.0, 80.0);
    // CSS sizes override them.
    let (_, rect) = picture(&format!(
        "<svg width='120' height='80' style='width: 60px; height: 30px'>{square}</svg>"
    ));
    assert_size(rect, 60.0, 30.0);
    let (_, rect) = picture(&format!(
        "<style>svg {{ width: 50px; height: 40px }}</style><svg width='120' height='80'>{square}</svg>"
    ));
    assert_size(rect, 50.0, 40.0);
    // One size and a `viewBox` give the other.
    let (_, rect) = picture(&format!(
        "<svg width='300' viewBox='0 0 100 50'>{square}</svg>"
    ));
    assert_size(rect, 300.0, 150.0);
    // A `viewBox` alone is a shape, which fills the width there is.
    let (_, rect) = picture(&format!(
        "<div style='width: 400px'><svg viewBox='0 0 100 50'>{square}</svg></div>"
    ));
    assert_size(rect, 400.0, 200.0);
    // Without any of them it is 300 × 150.
    let (_, rect) = picture(&format!("<svg>{square}</svg>"));
    assert_size(rect, 300.0, 150.0);
    // It sits in a line of text, and can be a block.
    let (_, rect) = picture(&format!(
        "<span style='display: inline-block; width: 30px; height: 10px'></span>\
         <svg width='20' height='10'>{square}</svg>"
    ));
    assert!(
        (rect.x - 30.0).abs() < 0.01,
        "after the span, at {}",
        rect.x
    );
    let (_, rect) = picture(&format!(
        "<span style='display: inline-block; width: 30px; height: 10px'></span>\
         <svg width='20' height='10' style='display: block'>{square}</svg>"
    ));
    assert_eq!(rect.x, 0.0);
    // Not rendered at all, its text included: no boxes, and none in its place.
    for display in ["none", "contents"] {
        let items = paint(&format!(
            "<svg width='20' height='10' style='display: {display}'>{square}<text>a</text></svg>"
        ));
        assert!(items.is_empty(), "display: {display}");
    }
    // The opacity of the element is its box's, applied once.
    let (image, _) = picture(&format!(
        "<svg width='20' height='10' style='opacity: 0.5'>{square}</svg>"
    ));
    assert_eq!(rgba(&image, 5.0, 5.0), RED);
}

#[test]
fn a_view_box_scales_the_drawing_as_preserve_aspect_ratio_says() {
    let shapes = "<rect width='100' height='100' fill='#00ff00'/>";
    // The 100 × 100 drawing is centered in the 200 × 100 box.
    let (image, rect) = picture(&format!(
        "<svg width='200' height='100' viewBox='0 0 100 100'>{shapes}</svg>"
    ));
    assert_size(rect, 100.0, 100.0);
    assert_eq!(rect.x, 50.0);
    assert_eq!(rgba(&image, 50.0, 50.0), GREEN);
    // `none` stretches it. HTML keeps the capital letters of these attribute names.
    let (_, rect) = picture(&format!(
        "<svg width='200' height='100' viewBox='0 0 100 100' preserveAspectRatio='none'>{shapes}</svg>"
    ));
    assert_size(rect, 200.0, 100.0);
    assert_eq!(rect.x, 0.0);
}

#[test]
fn the_document_styles_the_shapes() {
    let (image, _) = picture(
        "<style>
           .chart path { fill: none; stroke: #0000ff; stroke-width: 10px }
           .chart .slice { fill: #00ff00 }
           .legend rect { fill: #ff0000 }
         </style>
         <div class='chart'><svg width='100' height='100'>
           <rect class='slice' width='40' height='40' fill='#ff0000'/>
           <path d='M 0 70 H 100'/>
           <g class='legend' fill='#0000ff'><rect x='60' width='40' height='40'/></g>
           <g fill='#0000ff'><rect x='60' y='80' width='40' height='20'/></g>
         </svg></div>",
    );
    // A rule overrides the element's own attribute.
    assert_eq!(rgba(&image, 20.0, 20.0), GREEN);
    // A path with no attributes is stroked, not filled, as the rule says.
    assert_eq!(rgba(&image, 50.0, 70.0), BLUE);
    assert_eq!(rgba(&image, 50.0, 60.0), CLEAR);
    // A rule on an element overrides what it would inherit from an attribute.
    assert_eq!(rgba(&image, 80.0, 20.0), RED);
    // An attribute is inherited where no rule says otherwise.
    assert_eq!(rgba(&image, 80.0, 90.0), BLUE);
}

#[test]
fn style_attributes_and_inherited_color_style_the_shapes() {
    let (image, _) = picture(
        "<div style='color: #00ff00'><svg width='100' height='100'>
           <rect width='40' height='40' fill='#ff0000' style='fill: #0000ff'/>
           <rect x='60' width='40' height='40' fill='currentColor'/>
           <rect y='60' width='40' height='40' style='fill: rgb(255 0 0 / 50%)'/>
           <line x1='60' y1='80' x2='100' y2='80' stroke='#ff0000' stroke-width='20'
                 style='stroke-dasharray: 10, 20'/>
         </svg></div>",
    );
    assert_eq!(rgba(&image, 20.0, 20.0), BLUE);
    assert_eq!(rgba(&image, 80.0, 20.0), GREEN);
    let [r, _, _, a] = rgba(&image, 20.0, 80.0);
    assert!(r > 250 && (120..136).contains(&a), "half-transparent red");
    // A dash of 10, then a gap of 20.
    assert_eq!(rgba(&image, 65.0, 80.0), RED);
    assert_eq!(rgba(&image, 80.0, 80.0), CLEAR);
}

#[test]
fn hidden_shapes_are_not_drawn() {
    let (image, _) = picture(
        "<style>.off { display: none } .unseen { visibility: hidden } .faded { opacity: 0 }</style>
         <svg width='100' height='100'>
           <rect class='off' width='20' height='20' fill='#ff0000'/>
           <g class='unseen'><rect x='20' width='20' height='20' fill='#ff0000'/></g>
           <rect class='faded' x='40' width='20' height='20' fill='#ff0000' opacity='1'/>
           <rect x='60' width='20' height='20' fill='#ff0000' style='display: none'/>
           <rect x='80' width='20' height='20' fill='#ff0000'/>
         </svg>",
    );
    for x in [10.0, 30.0, 50.0, 70.0] {
        assert_eq!(rgba(&image, x, 10.0), CLEAR, "at {x}");
    }
    assert_eq!(rgba(&image, 90.0, 10.0), RED);
}

#[test]
fn the_document_styles_the_text() {
    // Where the ink of a label is, from the left edge, in tenths of the width.
    let ink = |style: &str| {
        let (image, _) = picture(&format!(
            "<style>{style}</style><svg width='200' height='40'>\
             <text x='100' y='30' font-size='10'>HHHH</text></svg>"
        ));
        (0..10)
            .map(|tenth| has_ink(&image, tenth * 20..tenth * 20 + 20, 0..40))
            .collect::<Vec<bool>>()
    };
    let start = ink("");
    assert!(!start[4] && start[5], "from the middle: {start:?}");
    // `text-anchor` from a rule moves it, and `font-size` from a rule overrides the
    // attribute.
    let end = ink("text { text-anchor: end }");
    assert!(end[4] && !end[5], "up to the middle: {end:?}");
    let large = ink("svg text { text-anchor: end; font-size: 40px }");
    assert!(large[1] && !large[5], "wider: {large:?}");
    assert!(!end[1]);
    let hidden = ink("text { display: none }");
    assert_eq!(hidden, [false; 10]);
}

#[test]
fn text_is_drawn_in_the_fonts_of_the_page() {
    // How far the ink of a line of narrow letters reaches, in CSS pixels.
    let reach = |style: &str| {
        let (image, _) = picture(&format!(
            "<style>{style}</style><svg width='300' height='40'>\
             <text x='0' y='30' font-size='20'>iiiiiiiiii</text></svg>"
        ));
        (0..300)
            .rev()
            .find(|&x| has_ink(&image, x..x + 1, 0..40))
            .expect("text is drawn")
    };
    // The letters of a monospace font are all as wide as each other.
    let (serif, mono) = (
        reach("text { font-family: serif }"),
        reach("text { font-family: monospace }"),
    );
    assert!(mono > serif * 3 / 2, "{mono} against {serif}");
    // Without a family of its own, the text is in the font the page gives its element.
    assert_eq!(reach("body { font-family: monospace }"), mono);
    assert_eq!(reach(""), serif);
    // A font the document brings is used as it is for page text.
    let own = reach(
        "@font-face { font-family: Label; src: url(label.ttf) } text { font-family: Label, serif }",
    );
    assert_eq!(own, mono);
}

#[test]
fn an_inner_svg_is_drawn_in_its_place() {
    let (image, rect) = picture(
        "<svg width='100' height='100'>
           <rect width='100' height='100' fill='#00ff00'/>
           <svg x='50' y='50' width='40' height='40' viewBox='0 0 10 10'>
             <rect width='10' height='10' fill='#ff0000'/>
           </svg>
         </svg>",
    );
    assert_size(rect, 100.0, 100.0);
    assert_eq!(rgba(&image, 25.0, 25.0), GREEN);
    assert_eq!(rgba(&image, 70.0, 70.0), RED);
    assert_eq!(rgba(&image, 95.0, 95.0), GREEN);
}

#[test]
fn references_and_mixed_case_names_are_kept() {
    let (image, _) = picture(
        "<svg width='100' height='100'>
           <defs>
             <clipPath id='left'><rect width='50' height='100'/></clipPath>
             <linearGradient id='fade' gradientUnits='userSpaceOnUse' x1='0' x2='0' y1='50' y2='100'>
               <stop offset='0' stop-color='#ff0000'/><stop offset='1' stop-color='#ff0000'/>
             </linearGradient>
           </defs>
           <rect width='100' height='50' fill='#0000ff' clip-path='url(#left)'/>
           <rect y='50' width='100' height='50' style='fill: url(#fade)'/>
         </svg>",
    );
    assert_eq!(rgba(&image, 25.0, 25.0), BLUE);
    assert_eq!(rgba(&image, 75.0, 25.0), CLEAR);
    assert_eq!(rgba(&image, 75.0, 75.0), RED);
}

#[test]
fn an_image_in_an_svg_element_loads_through_the_documents_loader() {
    // A 1 × 1 blue PNG.
    let data = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNgYPgPAAEDAQAIicLsAAAAAElFTkSuQmCC";
    let (image, _) = picture(&format!(
        "<svg width='100' height='50'>\
         <image href='red.png' width='50' height='50' preserveAspectRatio='none'/>\
         <image xlink:href='{data}' x='50' width='50' height='50' preserveAspectRatio='none'/>\
         </svg>"
    ));
    assert_eq!(rgba(&image, 25.0, 25.0), RED);
    assert_eq!(rgba(&image, 75.0, 25.0), BLUE);

    // A file the loader does not have is not drawn, and is warned about.
    let (items, warnings) = paint_within(
        "<svg width='100' height='50'><image href='/etc/hosts' width='50' height='50'/>\
         <image href='missing.png' x='50' width='50' height='50'/></svg>",
        u64::MAX,
    );
    let (image, _) = &pictures(&items)[0];
    assert_eq!(rgba(image, 25.0, 25.0), CLEAR);
    assert_eq!(
        warnings,
        [
            "could not load image `/etc/hosts`",
            "could not load image `missing.png`"
        ]
    );
}

#[test]
fn an_svg_element_is_drawn_within_the_pixel_limit() {
    let (items, warnings) = paint_within(
        "<svg width='400' height='100'><rect width='400' height='100' fill='#ff0000'/></svg>",
        10_000,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    let (image, rect) = &pictures(&items)[0];
    assert_size(*rect, 400.0, 100.0);
    assert!(u64::from(image.width) * u64::from(image.height) <= 10_000);
    assert_eq!(rgba(image, 200.0, 50.0), RED);
}

#[test]
fn an_svg_element_that_cannot_be_drawn_warns() {
    let (items, warnings) = paint_within(
        // HTML reads `"x` as the name of an attribute, which no SVG file can have.
        "<svg width='100' height='50'><rect \"x='1' width='10' height='10'/></svg><p>after</p>",
        u64::MAX,
    );
    assert!(pictures(&items).is_empty());
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].starts_with("could not draw an `<svg>` element"));
    // The page goes on after it.
    assert!(
        items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text(_)))
    );
}

#[test]
fn an_svg_element_without_a_size_draws_nothing() {
    // Definitions kept in an element of no size, for others to use.
    let items = paint(
        "<svg width='0' height='0'><defs><rect id='shape' width='10' height='10'/></defs></svg>",
    );
    assert!(pictures(&items).is_empty());
}
