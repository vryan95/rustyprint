//! `<svg>` elements in a document: sized as replaced elements, and drawn from their
//! descendants with the styles the document gives them.

use std::sync::Arc;

use rustyprint_css::{StyleOptions, compute_styles};
use rustyprint_dom::Document;
use rustyprint_layout::display::{LineCap, LineJoin, StrokeShape};
use rustyprint_layout::{
    Color, DisplayItem, FillRule, Image, ImageData, Images, PageOptions, Rect, Sides, Size,
    layout_tree, paint_fragment, rasterize,
};

/// What a document painted.
struct Painted {
    items: Vec<DisplayItem>,
    warnings: Vec<String>,
    /// How many pixels its `<svg>` elements were drawn to.
    svg_pixels: u64,
}

/// The limits a document is painted within.
#[derive(Clone, Copy)]
struct Within {
    /// The most pixels of an image.
    image_pixels: u64,
    /// The most pixels of all the `<svg>` elements that are drawn to pixels.
    svg_pixels: u64,
}

const UNLIMITED: Within = Within {
    image_pixels: u64::MAX,
    svg_pixels: u64::MAX,
};

/// What `body` (with no body margin) paints on an 800 × 1000 page area. `red.png` is a
/// 10 × 10 red image, as is the file whose name has the characters XML escapes, and
/// `label.ttf` is a monospace font.
fn paint_within(body: &str, within: Within) -> Painted {
    let html = format!("<!DOCTYPE html><style>body {{ margin: 0 }}</style><body>{body}");
    let document = Document::parse(&html);
    let load = |_: &str| None;
    let load_font = |url: &str| {
        let mono: &[u8] = include_bytes!("../../../fonts/NotoSansMono-Regular.ttf");
        (url == "label.ttf").then(|| mono.to_vec())
    };
    let styles = compute_styles(
        &document,
        &StyleOptions {
            page_size: (800.0, 1000.0),
            stylesheet: None,
            load: &load,
            font_metrics: None,
            load_font: Some(&load_font),
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
    let loads = std::cell::Cell::new(0);
    let load_file = |url: &str| {
        loads.set(loads.get() + 1);
        matches!(url, "red.png" | ESCAPED_NAME).then(|| png.clone())
    };
    let images = Images::new(&load_file)
        .with_max_pixels(within.image_pixels)
        .with_max_svg_pixels(within.svg_pixels);
    let root = layout_tree(&document, &styles, &page, &images).unwrap();
    let items = paint_fragment(&root, &images);
    LOADS.set(loads.get());
    Painted {
        items,
        warnings: images.warnings(),
        svg_pixels: images.svg_pixels(),
    }
}

/// A file name with the characters that XML escapes or ends a section with.
const ESCAPED_NAME: &str = "a&b<c\"]]>.png";

thread_local! {
    /// How many files the last document painted on this thread asked its loader for.
    static LOADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn paint(body: &str) -> Vec<DisplayItem> {
    let painted = paint_within(body, UNLIMITED);
    assert!(painted.warnings.is_empty(), "{:?}", painted.warnings);
    painted.items
}

/// The images painted, with where they are: the `<svg>` elements that are drawn to pixels,
/// and the images inside the ones drawn as shapes.
fn images(items: &[DisplayItem]) -> Vec<(Arc<Image>, Rect)> {
    items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { image, rect, .. } => Some((image.clone(), *rect)),
            _ => None,
        })
        .collect()
}

/// The index of the item that ends the clip or transform pushed at `start`.
fn end_of(items: &[DisplayItem], start: usize) -> usize {
    let mut depth = 0;
    for (index, item) in items.iter().enumerate().skip(start) {
        match item {
            DisplayItem::PushClip(_)
            | DisplayItem::PushClipPath { .. }
            | DisplayItem::PushClipText(_)
            | DisplayItem::PushTransform(_) => depth += 1,
            DisplayItem::PopClip => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            _ => {}
        }
    }
    panic!("a clip that never ends");
}

/// The items of each `<svg>` element painted, with its box: one drawn as shapes is clipped
/// to its box and scaled, and one drawn to pixels is an image.
fn drawings(items: &[DisplayItem]) -> Vec<(&[DisplayItem], Rect)> {
    let mut drawings = Vec::new();
    let mut index = 0;
    while index < items.len() {
        match (&items[index], items.get(index + 1)) {
            (DisplayItem::PushClip(rect), Some(DisplayItem::PushTransform(_))) => {
                let end = end_of(items, index);
                drawings.push((&items[index..=end], *rect));
                index = end;
            }
            (DisplayItem::Image { rect, .. }, _) => {
                drawings.push((&items[index..=index], *rect));
            }
            _ => {}
        }
        index += 1;
    }
    drawings
}

/// The `<svg>` elements painted, each as the pixels the engine draws its items to, with
/// the box it is in. A point of the picture is given in CSS pixels from the box's corner.
fn pictures(items: &[DisplayItem]) -> Vec<(Arc<Image>, Rect)> {
    drawings(items)
        .into_iter()
        .map(|(drawing, rect)| {
            let mut image = rasterize(drawing, rect).expect("a box with a size");
            image.natural = Some((rect.width, rect.height));
            (Arc::new(image), rect)
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

/// The runs of text painted, each with the left end of its baseline.
fn texts(items: &[DisplayItem]) -> Vec<(&str, f32, f32)> {
    items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text(run) => Some((&*run.text, run.origin.x, run.origin.y)),
            _ => None,
        })
        .collect()
}

/// The color and opacity at a point of a picture, given in the units of its own size.
#[track_caller]
fn rgba(image: &Image, x: f32, y: f32) -> [u8; 4] {
    let (width, height) = image.natural.expect("a picture has a size of its own");
    let column = ((x / width * image.width as f32) as u32).min(image.width - 1);
    let row = ((y / height * image.height as f32) as u32).min(image.height - 1);
    let at = (row * image.width + column) as usize;
    let ImageData::Pixels { samples, alpha, .. } = &image.data else {
        panic!("a picture is pixels");
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

/// A paint that shapes cannot express, though it is one color: a drawing that uses it is
/// drawn to pixels.
const GRADIENT: &str = "<defs><linearGradient id='fade'>\
    <stop offset='0' stop-color='#00ff00'/><stop offset='1' stop-color='#00ff00'/>\
    </linearGradient></defs>";

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
    assert!(has_ink(image, 120..160, 40..62));
    // It is drawn as shapes, with no pixels of its own, and its label is text like the
    // paragraph's, at its place in the drawing.
    assert!(images(&items).is_empty());
    let texts = texts(&items);
    assert_eq!(texts.len(), 2, "{texts:?}");
    assert_eq!(texts[0].0, "before");
    assert_eq!(texts[1], ("12%", 120.0, 60.0));
}

#[test]
fn shapes_are_drawn_as_they_are_stroked_and_filled() {
    let items = paint(
        "<svg width='100' height='100'>
           <path d='M 10 10 L 90 10 L 90 90' fill='none' stroke='#0000ff' stroke-width='4'
                 stroke-linecap='round' stroke-linejoin='bevel' stroke-miterlimit='3'
                 stroke-dasharray='6 2' stroke-dashoffset='1'/>
           <path d='M 0 0 H 50 V 50 H 0 Z M 10 10 H 40 V 40 H 10 Z' fill='#ff0000'
                 fill-rule='evenodd' fill-opacity='0.5'/>
           <g opacity='0.5'><rect width='10' height='10' fill='#00ff00'/>
             <rect x='5' width='10' height='10' fill='#00ff00'/></g>
         </svg>",
    );
    assert!(images(&items).is_empty());
    let stroke = items
        .iter()
        .find_map(|item| match item {
            DisplayItem::StrokePath {
                width,
                color,
                shape,
                ..
            } => Some((*width, *color, shape.clone().expect("a stated stroke"))),
            _ => None,
        })
        .expect("a stroke");
    assert_eq!(stroke.0, 4.0);
    assert_eq!(stroke.1, Color::rgb(0, 0, 255));
    assert_eq!(
        *stroke.2,
        StrokeShape {
            cap: LineCap::Round,
            join: LineJoin::Bevel,
            miter_limit: 3.0,
            dashes: vec![6.0, 2.0],
            dash_offset: 1.0,
        }
    );
    let fill = items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Path { color, rule, .. } => Some((*color, *rule)),
            _ => None,
        })
        .expect("a fill");
    assert_eq!(fill, (Color::rgba(255, 0, 0, 128), FillRule::EvenOdd));
    // Shapes that fade together are a group, so that where they overlap is no darker.
    let group = items.iter().find_map(|item| match item {
        DisplayItem::PushGroup(group) => Some(group.opacity),
        _ => None,
    });
    assert_eq!(group, Some(0.5));
    let (image, _) = &pictures(&items)[0];
    // The hole of the path with two contours shows what is under it.
    assert_eq!(rgba(image, 25.0, 25.0), CLEAR);
    let [r, g, _, a] = rgba(image, 7.0, 5.0);
    assert!(r < 140 && g > 120 && a > 120, "green over red, once");
}

#[test]
fn text_of_a_drawing_is_text() {
    let items = paint(
        "<svg width='300' height='100' style='font-family: monospace'>
           <text x='150' y='20' font-size='10' text-anchor='middle'>mid</text>
           <text x='150' y='40' font-size='10' text-anchor='end'>end</text>
           <text x='10' y='60' font-size='10'>one<tspan x='10' dy='12'>two</tspan></text>
           <g transform='translate(200 80) rotate(-90)'><text font-size='10'>up</text></g>
           <text x='10' y='90' font-size='10' fill='#ff0000'>a &amp; b &lt; c \"q\" ]]&gt;</text>
         </svg>",
    );
    assert!(images(&items).is_empty());
    let texts = texts(&items);
    let find = |text: &str| {
        *texts
            .iter()
            .find(|(found, ..)| *found == text)
            .unwrap_or_else(|| panic!("{text:?} among {texts:?}"))
    };
    // A monospace letter of 10 pixels is 6 wide.
    let near = |a: f32, b: f32| (a - b).abs() < 0.05;
    let (_, x, y) = find("mid");
    assert!(near(x, 150.0 - 9.0) && near(y, 20.0), "{x}, {y}");
    let (_, x, _) = find("end");
    assert!(near(x, 150.0 - 18.0), "{x}");
    assert_eq!(find("one"), ("one", 10.0, 60.0));
    assert_eq!(find("two"), ("two", 10.0, 72.0));
    // Turned text is text in a turned place.
    assert_eq!(find("up"), ("up", 0.0, 0.0));
    // What the markup escapes comes back as it was written.
    let (text, ..) = find("a & b < c \"q\" ]]>");
    assert_eq!(text, "a & b < c \"q\" ]]>");
    let (image, _) = &pictures(&items)[0];
    assert!(has_ink(image, 190..202, 60..82), "the turned word");
    let [r, g, b, _] = (10..80)
        .map(|x| rgba(image, x as f32, 87.0))
        .max_by_key(|[r, ..]| *r)
        .unwrap();
    assert!(r > 200 && g < 80 && b < 80, "red text");
}

#[test]
fn text_that_runs_cannot_say_is_drawn_as_outlines() {
    for (what, markup) in [
        (
            "stroked",
            "<text x='10' y='30' font-size='20' stroke='#000000'>Hi</text>",
        ),
        (
            "spaced",
            "<text x='10' y='30' font-size='20' letter-spacing='4'>Hi</text>",
        ),
        (
            "moved letter by letter",
            "<text x='10' y='30' dx='0 9' font-size='20'>Hi</text>",
        ),
        (
            "turned letter by letter",
            "<text x='10' y='30' rotate='20' font-size='20'>Hi</text>",
        ),
        (
            "underlined",
            "<text x='10' y='30' font-size='20' text-decoration='underline'>Hi</text>",
        ),
    ] {
        let items = paint(&format!("<svg width='100' height='40'>{markup}</svg>"));
        assert!(texts(&items).is_empty(), "{what}");
        assert!(images(&items).is_empty(), "{what}");
        let (image, _) = &pictures(&items)[0];
        assert!(has_ink(image, 5..70, 5..35), "{what}");
    }
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
    assert_size(rect, 200.0, 100.0);
    assert_eq!(rgba(&image, 25.0, 50.0), CLEAR);
    assert_eq!(rgba(&image, 100.0, 50.0), GREEN);
    assert_eq!(rgba(&image, 175.0, 50.0), CLEAR);
    // `none` stretches it. HTML keeps the capital letters of these attribute names.
    let (image, rect) = picture(&format!(
        "<svg width='200' height='100' viewBox='0 0 100 100' preserveAspectRatio='none'>{shapes}</svg>"
    ));
    assert_size(rect, 200.0, 100.0);
    assert_eq!(rgba(&image, 25.0, 50.0), GREEN);
    assert_eq!(rgba(&image, 175.0, 50.0), GREEN);
    // `slice` covers the box, and what falls outside it is cut off.
    let (image, rect) = picture(&format!(
        "<div style='height: 20px'></div>\
         <svg width='200' height='100' viewBox='0 0 100 100' preserveAspectRatio='xMidYMid slice'\
          style='display: block'>{shapes}</svg>"
    ));
    assert_size(rect, 200.0, 100.0);
    assert_eq!(rgba(&image, 5.0, 5.0), GREEN);
    assert_eq!(rgba(&image, 195.0, 95.0), GREEN);
    // The same holds for a drawing that is drawn to pixels.
    let (image, _) = picture(&format!(
        "<svg width='200' height='100' viewBox='0 0 100 100'>{GRADIENT}\
         <rect width='100' height='100' fill='url(#fade)'/></svg>"
    ));
    assert_eq!(rgba(&image, 25.0, 50.0), CLEAR);
    assert_eq!(rgba(&image, 100.0, 50.0), GREEN);
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
    let items = paint(&format!(
        "<svg width='150' height='50'>\
         <image href='red.png' width='50' height='50' preserveAspectRatio='none'/>\
         <image xlink:href='{data}' x='50' width='50' height='50' preserveAspectRatio='none'/>\
         <image xlink:href='red.png' x='100' width='50' height='50'/>\
         </svg>"
    ));
    // The drawing is shapes, and its images are the images themselves.
    let held = images(&items);
    assert_eq!(held.len(), 3);
    assert_eq!((held[0].0.width, held[0].0.height), (10, 10));
    assert_eq!((held[1].0.width, held[1].0.height), (1, 1));
    assert!(Arc::ptr_eq(&held[0].0, &held[2].0), "decoded once");
    let (image, _) = &pictures(&items)[0];
    assert_eq!(rgba(image, 25.0, 25.0), RED);
    assert_eq!(rgba(image, 75.0, 25.0), BLUE);
    assert_eq!(rgba(image, 125.0, 25.0), RED);

    // A file the loader does not have is not drawn, and is warned about.
    let painted = paint_within(
        "<svg width='100' height='50'><image href='/etc/hosts' width='50' height='50'/>\
         <image href='missing.png' x='50' width='50' height='50'/></svg>",
        UNLIMITED,
    );
    let (image, _) = &pictures(&painted.items)[0];
    assert_eq!(rgba(image, 25.0, 25.0), CLEAR);
    assert_eq!(
        painted.warnings,
        [
            "could not load image `/etc/hosts`",
            "could not load image `missing.png`"
        ]
    );
}

#[test]
fn a_linked_file_is_loaded_once_for_a_document() {
    let chart = |color: &str| {
        format!(
            "<svg width='50' height='50'><rect width='5' height='5' fill='{color}'/>\
             <image href='red.png' width='50' height='50'/></svg>"
        )
    };
    let items = paint(&[chart("#000001"), chart("#000002"), chart("#000003")].concat());
    assert_eq!(LOADS.get(), 1);
    let held = images(&items);
    assert_eq!(held.len(), 3);
    assert!(Arc::ptr_eq(&held[0].0, &held[2].0), "decoded once");
}

#[test]
fn names_and_text_that_xml_cannot_hold_are_left_out_and_the_rest_is_drawn() {
    // HTML reads `"x`, `a:b` and `1st` as the names of attributes, and `g"x` as the name
    // of an element, none of which an SVG file can have.
    let (image, _) = picture(
        "<svg width='100' height='50'>\
         <rect \"x='1' a:b='2' 1st='3' width='10' height='10' fill='#ff0000'/>\
         <g\"x><rect x='20' width='10' height='10' fill='#ff0000'/></g\"x>\
         <rect x='40' width='10' height='10' fill='#0000ff'/>\
         <text x='60' y='20' font-size='10'>a&#12;b&#xFFFE;c</text>\
         </svg>",
    );
    assert_eq!(rgba(&image, 5.0, 5.0), RED);
    // An element SVG does not know draws nothing, and neither does what is inside it.
    assert_eq!(rgba(&image, 25.0, 5.0), CLEAR);
    assert_eq!(rgba(&image, 45.0, 5.0), BLUE);
    assert!(has_ink(&image, 60..90, 5..25));
}

#[test]
fn escaped_characters_keep_their_meaning() {
    // In an attribute: the name of the file is asked of the loader as it was written.
    let items = paint(
        "<svg width='50' height='50'>\
         <image href='a&amp;b&lt;c&quot;]]&gt;.png' width='50' height='50'\
          data-note='&amp;amp; &lt;tag&gt; &quot; ]]&gt;'/></svg>",
    );
    assert_eq!(images(&items).len(), 1);
    let (image, _) = &pictures(&items)[0];
    assert_eq!(rgba(image, 25.0, 25.0), RED);
    // In text: see `text_of_a_drawing_is_text`.
}

#[test]
fn a_drawing_that_shapes_cannot_express_is_drawn_to_pixels() {
    // Four pixels for each CSS pixel of the box it is laid out in, however small.
    for (what, markup) in [
        (
            "a gradient",
            format!("{GRADIENT}<rect width='40' height='10' fill='url(#fade)'/>"),
        ),
        (
            "a filter",
            "<defs><filter id='soft'><feGaussianBlur stdDeviation='0.1'/></filter></defs>\
             <rect width='40' height='10' fill='#00ff00' filter='url(#soft)'/>"
                .to_owned(),
        ),
        (
            "a mask",
            "<defs><mask id='all'><rect width='40' height='10' fill='#ffffff'/></mask></defs>\
             <rect width='40' height='10' fill='#00ff00' mask='url(#all)'/>"
                .to_owned(),
        ),
        (
            "a pattern",
            "<defs><pattern id='dots' width='2' height='2' patternUnits='userSpaceOnUse'>\
             <rect width='2' height='2' fill='#00ff00'/></pattern></defs>\
             <rect width='40' height='10' fill='url(#dots)'/>"
                .to_owned(),
        ),
        (
            "a clip path of several shapes",
            "<defs><clipPath id='two'><rect width='30' height='10'/>\
             <rect x='10' width='30' height='10'/></clipPath></defs>\
             <rect width='40' height='10' fill='#00ff00' clip-path='url(#two)'/>"
                .to_owned(),
        ),
    ] {
        let painted = paint_within(
            &format!(
                "<svg width='40' height='10' style='width: 80px; height: 20px' viewBox='0 0 40 10'>{markup}</svg>"
            ),
            UNLIMITED,
        );
        assert!(
            painted.warnings.is_empty(),
            "{what}: {:?}",
            painted.warnings
        );
        let held = images(&painted.items);
        assert_eq!(held.len(), 1, "{what}");
        let (image, rect) = &held[0];
        assert_eq!((image.width, image.height), (320, 80), "{what}");
        assert_size(*rect, 80.0, 20.0);
        assert_eq!(painted.svg_pixels, 320 * 80, "{what}");
        let (image, _) = &pictures(&painted.items)[0];
        assert_eq!(rgba(image, 40.0, 10.0), GREEN, "{what}");
    }
}

#[test]
fn a_clip_path_of_one_shape_clips_shapes() {
    let items = paint(
        "<svg width='100' height='100'>
           <defs><clipPath id='left'><rect width='50' height='100'/></clipPath></defs>
           <g clip-path='url(#left)'><rect width='100' height='50' fill='#0000ff'/></g>
         </svg>",
    );
    assert!(images(&items).is_empty());
    let (image, _) = &pictures(&items)[0];
    assert_eq!(rgba(image, 25.0, 25.0), BLUE);
    assert_eq!(rgba(image, 75.0, 25.0), CLEAR);
}

/// `count` drawings of 40 × 10, no two alike, each holding `shape`.
fn sparklines(count: usize, shape: &str) -> String {
    (0..count)
        .map(|i| {
            format!(
                "<svg width='40' height='10'>{shape}\
                 <path d='M 0 5 L {} {} L 40 5' fill='none' stroke='#0000ff'/></svg> ",
                i % 40,
                i / 40
            )
        })
        .collect()
}

#[test]
fn many_small_drawings_take_no_pixels() {
    let painted = paint_within(&sparklines(5000, ""), UNLIMITED);
    assert!(painted.warnings.is_empty(), "{:?}", painted.warnings);
    assert_eq!(drawings(&painted.items).len(), 5000);
    assert!(images(&painted.items).is_empty());
    assert_eq!(painted.svg_pixels, 0);
}

#[test]
fn drawings_in_pixels_stay_within_the_documents_limit() {
    // Each is 160 × 40 pixels. There is room for a hundred of them.
    let each = 160 * 40;
    let shape = format!("{GRADIENT}<rect width='40' height='10' fill='url(#fade)'/>");
    let within = Within {
        svg_pixels: 100 * each,
        ..UNLIMITED
    };
    let painted = paint_within(&sparklines(5000, &shape), within);
    assert_eq!(images(&painted.items).len(), 100);
    assert_eq!(painted.svg_pixels, 100 * each);
    // The rest are left blank, which is said once.
    assert_eq!(painted.warnings.len(), 1, "{:?}", painted.warnings);
    assert!(
        painted.warnings[0].starts_with("an `<svg>` element was left blank"),
        "{:?}",
        painted.warnings
    );
    assert!(painted.warnings[0].contains("640000 pixels"));

    // Drawings that are the same are drawn once, and take their pixels once.
    let same = format!("<svg width='40' height='10'>{shape}</svg> ").repeat(300);
    let painted = paint_within(&same, within);
    assert!(painted.warnings.is_empty(), "{:?}", painted.warnings);
    let held = images(&painted.items);
    assert_eq!(held.len(), 300);
    assert!(held.iter().all(|(image, _)| Arc::ptr_eq(image, &held[0].0)));
    assert_eq!(painted.svg_pixels, each);
}

#[test]
fn a_drawing_in_pixels_is_drawn_within_the_pixel_limit() {
    let painted = paint_within(
        &format!(
            "<svg width='400' height='100'>{GRADIENT}\
             <rect width='400' height='100' fill='url(#fade)'/></svg>"
        ),
        Within {
            image_pixels: 10_000,
            ..UNLIMITED
        },
    );
    assert!(painted.warnings.is_empty(), "{:?}", painted.warnings);
    let (image, rect) = &images(&painted.items)[0];
    assert_size(*rect, 400.0, 100.0);
    assert!(u64::from(image.width) * u64::from(image.height) <= 10_000);
    let (image, _) = &pictures(&painted.items)[0];
    assert_eq!(rgba(image, 200.0, 50.0), GREEN);
}

/// A PNG file that says it is `side` pixels each way, and holds none.
fn png_header(side: u32) -> Vec<u8> {
    let chunk = |kind: &[u8; 4], data: &[u8]| {
        let body = [&kind[..], data].concat();
        let mut crc = u32::MAX;
        for byte in &body {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xEDB8_8320 & 0_u32.wrapping_sub(crc & 1));
            }
        }
        [
            &(data.len() as u32).to_be_bytes()[..],
            &body,
            &(!crc).to_be_bytes(),
        ]
        .concat()
    };
    let size = [side.to_be_bytes(), side.to_be_bytes()].concat();
    [
        b"\x89PNG\r\n\x1a\n".to_vec(),
        chunk(b"IHDR", &[&size[..], &[8, 2, 0, 0, 0]].concat()),
        chunk(b"IDAT", &[0x78, 0x9c]),
        chunk(b"IEND", &[]),
    ]
    .concat()
}

/// A `data:` URL of a file, with every byte escaped.
fn data_url(kind: &str, bytes: &[u8]) -> String {
    let escaped: String = bytes.iter().map(|byte| format!("%{byte:02X}")).collect();
    format!("data:{kind},{escaped}")
}

#[test]
fn an_image_over_the_limit_in_a_data_url_is_not_decoded() {
    // A few bytes that say they are 3.6 billion pixels.
    let bomb = data_url("image/png", &png_header(60_000));
    let inner = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'>\
         <image href='{bomb}' width='10' height='10'/></svg>"
    );
    let nested = data_url("image/svg+xml", inner.as_bytes());
    let limit = Within {
        image_pixels: 1_000_000,
        ..UNLIMITED
    };
    for (what, held, pixels) in [
        ("shapes", bomb.clone(), ""),
        ("pixels", bomb.clone(), GRADIENT),
        ("an SVG in the drawing", nested, ""),
    ] {
        let painted = paint_within(
            &format!(
                "<svg width='100' height='50'>{pixels}\
                 <image href=\"{held}\" width='50' height='50'/>\
                 <rect x='50' width='50' height='50' fill='#0000ff'/></svg>"
            ),
            limit,
        );
        assert_eq!(
            painted.warnings,
            ["an image in an SVG is 60000×60000 pixels, more than the limit of 1000000"],
            "{what}"
        );
        // The rest of the drawing is there.
        let (image, _) = &pictures(&painted.items)[0];
        assert_eq!(rgba(image, 25.0, 25.0), CLEAR, "{what}");
        assert_eq!(rgba(image, 75.0, 25.0), BLUE, "{what}");
    }
}

#[test]
fn an_svg_element_without_a_size_draws_nothing() {
    // Definitions kept in an element of no size, for others to use.
    let items = paint(
        "<svg width='0' height='0'><defs><rect id='shape' width='10' height='10'/></defs></svg>",
    );
    assert!(drawings(&items).is_empty());
}

#[test]
fn an_svg_element_is_drawn_where_its_box_is_laid_out() {
    let chart = "<svg width='40' height='20'><rect width='40' height='20' fill='#ff0000'/>\
                 <text x='2' y='15' font-size='10'>7</text></svg>";
    // In a table cell.
    let items = paint(&format!(
        "<table style='border-collapse: collapse'><tr>\
         <td style='width: 100px; padding: 0'></td>\
         <td style='padding: 5px'>{chart}</td></tr></table>"
    ));
    let (image, rect) = &pictures(&items)[0];
    assert_eq!((rect.x, rect.y), (105.0, 5.0));
    assert_eq!(rgba(image, 30.0, 10.0), RED);
    // In a line of text, on its baseline.
    let items = paint(&format!(
        "<p style='margin: 0; font: 20px/40px monospace'>ab{chart}cd</p>"
    ));
    let (_, rect) = &pictures(&items)[0];
    let texts = texts(&items);
    let (_, _, baseline) = texts[0];
    assert_eq!(rect.x, 24.0);
    assert!(
        (rect.bottom() - baseline).abs() < 0.01,
        "{rect:?} on {baseline}"
    );
    // Its own text is placed in the drawing, which is placed on the page.
    assert_eq!(
        texts.iter().find(|(text, ..)| *text == "7"),
        Some(&("7", 2.0, 15.0))
    );
    // The line goes on after it.
    assert_eq!(texts[2].1, 64.0, "{texts:?}");
}

#[test]
fn a_hidden_svg_element_draws_nothing_but_what_is_made_visible() {
    let chart = "<rect width='20' height='20' fill='#ff0000'/>\
                 <rect class='shown' x='20' width='20' height='20' fill='#0000ff'/>\
                 <text x='0' y='35' font-size='10'>hidden</text>";
    let items = paint(&format!(
        "<svg width='40' height='40' style='visibility: hidden'>{chart}</svg>"
    ));
    assert!(texts(&items).is_empty());
    for (image, _) in pictures(&items) {
        assert_eq!(rgba(&image, 10.0, 10.0), CLEAR);
    }
    // `visibility` is inherited, so a shape inside what is hidden can be visible again.
    let (image, _) = picture(&format!(
        "<style>.shown {{ visibility: visible }}</style>\
         <svg width='40' height='40'><g style='visibility: hidden'>{chart}</g></svg>"
    ));
    assert_eq!(rgba(&image, 10.0, 10.0), CLEAR);
    assert_eq!(rgba(&image, 30.0, 10.0), BLUE);
    assert!(!has_ink(&image, 0..40, 22..40), "hidden text");
    // And so can a shape inside an element that is itself hidden, whose box is not drawn.
    let items = paint(&format!(
        "<style>.shown {{ visibility: visible }}</style>\
         <svg width='40' height='40' style='visibility: hidden; background: #00ff00'>{chart}</svg>"
    ));
    assert!(texts(&items).is_empty());
    assert!(
        !items
            .iter()
            .any(|item| matches!(item, DisplayItem::Rect { .. })),
        "the background of a hidden box"
    );
    let (image, _) = &pictures(&items)[0];
    assert_eq!(rgba(image, 10.0, 10.0), CLEAR);
    assert_eq!(rgba(image, 30.0, 10.0), BLUE);
}

/// A chart with a label that is seen, and a part that is not: a shape at the left and a
/// word below it, in a group with `attributes`.
fn chart_with_unseen_part(rules: &str, attributes: &str) -> String {
    format!(
        "<style>{rules}</style>
         <svg width='100' height='60' style='font-family: monospace'>
           <text x='40' y='15' font-size='10'>seen</text>
           <g {attributes}>
             <path d='M0 0h20v20h-20z' fill='#ff0000'/>
             <text x='0' y='40' font-size='10'>unseen</text>
           </g>
         </svg>"
    )
}

#[test]
fn what_is_not_seen_is_not_drawn_and_is_not_text() {
    for (what, rules, attributes) in [
        ("a style attribute", "", "style='opacity: 0'"),
        ("a class rule", ".axis { opacity: 0 }", "class='axis'"),
        ("an attribute", "", "opacity='0'"),
        (
            "inside a group that is seen",
            "",
            "opacity='0.5'><g opacity='0'",
        ),
        ("hidden", ".axis { visibility: hidden }", "class='axis'"),
        ("not displayed", ".axis { display: none }", "class='axis'"),
        (
            "paints of no opacity",
            ".axis { fill-opacity: 0; stroke-opacity: 0 }",
            "class='axis'",
        ),
    ] {
        // The end of the `<svg>` closes a group left open.
        let items = paint(&chart_with_unseen_part(rules, attributes));
        let found: Vec<&str> = texts(&items).iter().map(|(text, ..)| *text).collect();
        assert_eq!(found, ["seen"], "{what}");
        let (image, _) = &pictures(&items)[0];
        assert_eq!(rgba(image, 10.0, 10.0), CLEAR, "{what}");
        assert!(!has_ink(image, 0..40, 30..45), "{what}: the unseen word");
        assert!(has_ink(image, 40..65, 5..17), "{what}: the label");
    }
}

#[test]
fn a_group_that_is_partly_seen_is_drawn() {
    let items = paint(&chart_with_unseen_part("", "style='opacity: 0.5'"));
    let found: Vec<&str> = texts(&items).iter().map(|(text, ..)| *text).collect();
    assert_eq!(found, ["seen", "unseen"]);
    let (image, _) = &pictures(&items)[0];
    let [r, g, b, a] = rgba(image, 10.0, 10.0);
    assert!(r > 200 && g < 80 && b < 80, "a red shape");
    assert!((120..=135).contains(&a), "half seen: {a}");
}
