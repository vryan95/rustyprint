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
