//! `<svg>` elements in the PDF: shapes and text, and pixels only where shapes cannot say it.

mod common;

use common::{image_count, page_count};
use rustyprint::{Output, RenderOptions, render};

fn rendered(html: &str, options: &RenderOptions) -> Output {
    render(html, options).expect("renders")
}

/// The operators of each page's content, in order, and the text of each page.
fn pages(pdf: &[u8]) -> Vec<(Vec<String>, String)> {
    let doc = lopdf::Document::load_mem(pdf).expect("valid PDF");
    doc.get_pages()
        .into_iter()
        .map(|(number, id)| {
            let content = doc.get_page_content(id);
            let operators = lopdf::content::Content::decode(&content)
                .expect("content operators")
                .operations
                .into_iter()
                .map(|operation| operation.operator)
                .collect();
            (operators, doc.extract_text(&[number]).expect("page text"))
        })
        .collect()
}

fn count(operators: &[String], operator: &str) -> usize {
    operators.iter().filter(|found| *found == operator).count()
}

/// A chart as a charting library writes it: axes, grid lines, bars, a line and labels.
const CHART: &str = "<svg width='400' height='200'>
  <style>.axis path { fill: none; stroke: #000000 } .grid line { stroke: #aaaaaa; stroke-dasharray: 3 3 }</style>
  <defs><clipPath id='plot'><rect width='360' height='160'/></clipPath></defs>
  <g transform='translate(30 10)'>
    <g class='grid'><line x1='0' x2='360' y1='40' y2='40'/><line x1='0' x2='360' y1='80' y2='80'/></g>
    <g clip-path='url(#plot)'>
      <rect x='20' y='60' width='40' height='100' fill='#1f77b4'/>
      <rect x='100' y='20' width='40' height='140' fill='#ff7f0e'/>
      <path d='M 0 120 C 60 20 120 140 360 30' fill='none' stroke='#2ca02c' stroke-width='2'/>
    </g>
    <g class='axis'><path d='M 0 0 V 160 H 360'/>
      <text x='40' y='175' text-anchor='middle' font-size='10'>January</text>
      <text x='120' y='175' text-anchor='middle' font-size='10'>February</text>
      <text transform='rotate(-90)' x='-80' y='-12' text-anchor='middle' font-size='10'>Volume</text>
    </g>
  </g>
</svg>";

#[test]
fn a_chart_is_shapes_and_text_in_the_pdf() {
    let output = rendered(CHART, &RenderOptions::default());
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    assert_eq!(image_count(&output.pdf), 0, "no picture of the chart");
    let pages = pages(&output.pdf);
    let (operators, text) = &pages[0];
    // Two bars are filled; two grid lines, the curve and the axes are stroked, the grid
    // lines with dashes; the plot is clipped to its area.
    assert_eq!(count(operators, "f"), 2, "{operators:?}");
    assert_eq!(count(operators, "S"), 4, "{operators:?}");
    assert_eq!(count(operators, "c"), 1, "the curve stays a curve");
    assert!(count(operators, "d") >= 2, "{operators:?}");
    assert!(count(operators, "W") >= 2, "{operators:?}");
    // The labels are text that can be found and copied.
    for label in ["January", "February", "Volume"] {
        assert!(text.contains(label), "{label} in {text:?}");
    }
}

#[test]
fn a_drawing_with_a_gradient_or_a_filter_is_a_picture_in_the_pdf() {
    for paint in [
        "<linearGradient id='paint'><stop offset='0' stop-color='#ff0000'/>\
         <stop offset='1' stop-color='#0000ff'/></linearGradient>\
         <rect width='40' height='10' fill='url(#paint)'/>",
        "<filter id='paint'><feGaussianBlur stdDeviation='1'/></filter>\
         <rect width='40' height='10' fill='#ff0000' filter='url(#paint)'/>",
    ] {
        let output = rendered(
            &format!("<svg width='40' height='10'>{paint}</svg>"),
            &RenderOptions::default(),
        );
        assert!(output.warnings.is_empty(), "{:?}", output.warnings);
        assert!(image_count(&output.pdf) >= 1, "{paint}");
    }
}

/// A table of `count` rows, each with a small drawing unlike any other, holding `shape`.
fn sparklines(count: usize, shape: &str) -> String {
    let rows: String = (0..count)
        .map(|i| {
            format!(
                "<tr><td>{i}</td><td><svg width='40' height='10'>{shape}\
                 <path d='M 0 5 L {} {} L 40 5' fill='none' stroke='#1f77b4'/></svg></td></tr>",
                i % 40,
                i / 40
            )
        })
        .collect();
    format!("<table>{rows}</table>")
}

#[test]
fn thousands_of_small_drawings_are_shapes() {
    let output = rendered(&sparklines(5000, ""), &RenderOptions::default());
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    assert_eq!(image_count(&output.pdf), 0);
    let strokes: usize = pages(&output.pdf)
        .iter()
        .map(|(operators, _)| count(operators, "S"))
        .sum();
    assert_eq!(strokes, 5000);
}

#[test]
fn drawings_in_pixels_past_the_limit_are_left_blank_with_a_warning() {
    let shape = "<linearGradient id='paint'><stop offset='0' stop-color='#ff0000'/>\
                 <stop offset='1' stop-color='#0000ff'/></linearGradient>\
                 <rect width='40' height='10' fill='url(#paint)'/>";
    let html = sparklines(200, shape);
    // Each drawing is 160 × 40 pixels.
    let mut options = RenderOptions::default();
    options.limits.max_svg_pixels = 200 * 160 * 40;
    let output = rendered(&html, &options);
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    assert_eq!(image_count(&output.pdf), 200);

    options.limits.max_svg_pixels = 10 * 160 * 40;
    let output = rendered(&html, &options);
    assert_eq!(image_count(&output.pdf), 10);
    assert_eq!(output.warnings.len(), 1, "{:?}", output.warnings);
    assert!(
        output.warnings[0].contains("an `<svg>` element was left blank"),
        "{:?}",
        output.warnings
    );
    // The rows are all there.
    let last: String = pages(&output.pdf).pop().expect("a page").1;
    assert!(last.contains("199"), "{last:?}");
}

#[test]
fn a_drawing_that_does_not_fit_on_a_page_moves_to_the_next() {
    let html = format!("<p>first</p><div style='height: 850px'></div>{CHART}<p>last</p>");
    let output = rendered(&html, &RenderOptions::default());
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    assert_eq!(page_count(&output.pdf), 2);
    let pages = pages(&output.pdf);
    assert!(pages[0].1.contains("first") && !pages[0].1.contains("January"));
    assert_eq!(count(&pages[0].0, "S"), 0);
    // The whole chart is on the second page, with the text after it.
    assert_eq!(count(&pages[1].0, "S"), 4);
    assert_eq!(count(&pages[1].0, "f"), 2);
    assert!(pages[1].1.contains("January") && pages[1].1.contains("last"));
}

/// The bounding box and the content of each transparency group of a PDF.
fn groups(pdf: &[u8]) -> Vec<([f32; 4], lopdf::content::Content)> {
    let doc = lopdf::Document::load_mem(pdf).expect("valid PDF");
    let number = |object: &lopdf::Object| object.as_float().expect("a number");
    doc.objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .filter(|stream| {
            let subtype = stream.dict.get(b"Subtype").and_then(|s| s.as_name());
            subtype.is_ok_and(|name| name == b"Form")
        })
        .map(|stream| {
            let bbox = stream.dict.get(b"BBox").and_then(|b| b.as_array());
            let bbox: Vec<f32> = bbox.expect("a bounding box").iter().map(number).collect();
            let content = stream.decompressed_content().expect("content");
            let content = lopdf::content::Content::decode(&content).expect("operators");
            ([bbox[0], bbox[1], bbox[2], bbox[3]], content)
        })
        .collect()
}

#[test]
fn a_group_of_a_drawing_with_a_large_view_box_is_not_cut_off() {
    // The drawing is scaled down a thousand times, so its shapes are far from the corner
    // in their own coordinates; the group's box has to reach them there.
    let output = rendered(
        "<svg width='400' height='400' viewBox='0 0 400000 400000'><g opacity='0.5'>\
         <rect x='300000' y='300000' width='50000' height='50000' fill='#ff0000'/>\
         <rect x='320000' y='320000' width='50000' height='50000' fill='#0000ff'/></g></svg>",
        &RenderOptions::default(),
    );
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    let groups = groups(&output.pdf);
    assert_eq!(groups.len(), 1);
    let ([left, bottom, right, top], content) = &groups[0];
    // The corners of its two rectangles, in the group's coordinates, are inside its box.
    let corners: Vec<(f32, f32)> = content
        .operations
        .iter()
        .filter(|operation| matches!(&*operation.operator, "m" | "l"))
        .map(|operation| {
            let at = |index: usize| operation.operands[index].as_float().expect("a number");
            (at(0), at(1))
        })
        .collect();
    assert!(corners.len() >= 8, "{corners:?}");
    for (x, y) in corners {
        assert!(
            *left <= x && x <= *right && *bottom <= y && y <= *top,
            "({x}, {y}) in {:?}",
            groups[0].0
        );
    }
}

#[test]
fn a_shape_whose_coordinates_overflow_is_left_out_of_a_valid_pdf() {
    for (broken, warns) in [
        (
            "<rect x='1e39' width='10' height='10' fill='#0000ff'/>",
            false,
        ),
        ("<path d='M 0 0 L 1e39 0 L 5 5 Z' fill='#0000ff'/>", false),
        (
            "<g transform='scale(1e30)'><rect width='1e30' height='1e30' fill='#0000ff'/></g>",
            true,
        ),
        (
            "<path d='M 0 0 L 5 0 L 5 5 Z' fill='#0000ff' stroke='#000000' \
             stroke-width='1e30'/>",
            true,
        ),
    ] {
        let output = rendered(
            &format!(
                "<svg width='40' height='40'>{broken}\
                 <rect width='20' height='20' fill='#ff0000'/></svg>"
            ),
            &RenderOptions::default(),
        );
        let left_out = output
            .warnings
            .iter()
            .any(|warning| warning.contains("part of an SVG was left out"));
        assert_eq!(left_out, warns, "{broken}: {:?}", output.warnings);
        // Every number of the page is a number, and the shape that can be drawn is there.
        let doc = lopdf::Document::load_mem(&output.pdf).expect("valid PDF");
        let (_, page) = doc.get_pages().into_iter().next().expect("a page");
        let content = doc.get_page_content(page);
        let text = String::from_utf8_lossy(&content).to_ascii_lowercase();
        assert!(!text.contains("inf") && !text.contains("nan"), "{broken}");
        let (operators, _) = &pages(&output.pdf)[0];
        assert!(count(operators, "f") >= 1, "{broken}: {operators:?}");
    }
}

#[test]
fn dashes_too_short_to_see_are_a_solid_line() {
    let dashes = |svg: &str| {
        let output = rendered(svg, &RenderOptions::default());
        assert!(output.warnings.is_empty(), "{:?}", output.warnings);
        let (operators, _) = &pages(&output.pdf)[0];
        assert_eq!(count(operators, "S"), 1, "{operators:?}");
        count(operators, "d")
    };
    let line = |attributes: &str, dashes: &str| {
        format!(
            "<svg width='400' height='20' {attributes}>\
             <path d='M 0 10 H 400' stroke='#000000' stroke-dasharray='{dashes}'/></svg>"
        )
    };
    assert_eq!(dashes(&line("", "3 3")), 1);
    // A viewer would have to make millions of these along the line.
    assert_eq!(dashes(&line("", "0.0001")), 0);
    // What counts is the length on the page: this drawing is a thousandth of its size.
    assert_eq!(dashes(&line("viewBox='0 0 400000 20000'", "3 3")), 0);
    assert_eq!(dashes(&line("viewBox='0 0 400000 20000'", "3000 3000")), 1);
}
