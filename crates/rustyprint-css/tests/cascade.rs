use std::sync::Arc;

use rustyprint_css::properties::types::{
    BorderStyle, Display, DisplayInside, DisplayOutside, FontStyle, LengthPercentage, MaybeAuto,
    TextAlign,
};
use rustyprint_css::selector::PseudoElement;
use rustyprint_css::values::AbsoluteColor;
use rustyprint_css::{ComputedValues, StyleOptions, Styles, compute_styles};
use rustyprint_dom::Document;

/// A4 with 2cm margins, in CSS pixels.
const PAGE: (f32, f32) = (642.5, 971.3);

fn style_with(html: &str, extra: Option<&str>, files: &[(&str, &str)]) -> (Document, Styles) {
    let document = Document::parse(html);
    let load = |url: &str| {
        files
            .iter()
            .find(|(name, _)| *name == url)
            .map(|(_, css)| css.to_string())
    };
    let styles = compute_styles(
        &document,
        &StyleOptions {
            page_size: PAGE,
            stylesheet: extra,
            load: &load,
            font_metrics: None,
            load_font: None,
        },
    );
    (document, styles)
}

/// The style of the element with `id`.
fn by_id(document: &Document, styles: &Styles, id: &str) -> Arc<ComputedValues> {
    let node = document
        .descendants(document.root())
        .find(|&n| document.element(n).is_some_and(|e| e.id() == Some(id)))
        .unwrap_or_else(|| panic!("no element #{id}"));
    styles.get(node).unwrap().clone()
}

fn style(html: &str, id: &str) -> Arc<ComputedValues> {
    let (document, styles) = style_with(html, None, &[]);
    by_id(&document, &styles, id)
}

fn rgb(style: &ComputedValues) -> [u8; 4] {
    style.color.to_rgba8()
}

fn margin(value: &MaybeAuto<LengthPercentage>) -> f32 {
    match value {
        MaybeAuto::Value(LengthPercentage::Length(px)) => *px,
        other => panic!("expected a length, got {other:?}"),
    }
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 128, 0, 255];

#[test]
fn ua_stylesheet_defaults() {
    let html = "<body id=body><h1 id=h1>T</h1><p id=p>x <b id=b>y</b></p><ul><li id=li>z</ul><span id=s></span>";
    let (doc, styles) = style_with(html, None, &[]);
    let p = by_id(&doc, &styles, "p");
    assert_eq!(p.display, Display::BLOCK);
    assert_eq!(margin(&p.margin_top), 16.0);
    assert_eq!(margin(&p.margin_left), 0.0);
    let h1 = by_id(&doc, &styles, "h1");
    assert_eq!(h1.font_size, 32.0);
    assert_eq!(margin(&h1.margin_top), 32.0 * 0.67);
    assert_eq!(h1.font_weight, 700.0);
    assert_eq!(by_id(&doc, &styles, "b").font_weight, 700.0);
    assert_eq!(margin(&by_id(&doc, &styles, "body").margin_left), 8.0);
    let li = by_id(&doc, &styles, "li");
    assert_eq!(
        li.display,
        Display::Normal {
            outside: DisplayOutside::Block,
            inside: DisplayInside::Flow,
            list_item: true
        }
    );
    assert_eq!(by_id(&doc, &styles, "s").display, Display::INLINE);
    let head = doc.find_html(&rustyprint_dom::local_name!("head")).unwrap();
    assert_eq!(styles.get(head).unwrap().display, Display::None);
}

#[test]
fn specificity_and_order() {
    let html = "<style>.a { color: red } p { color: blue } #x { color: green } p { color: blue }</style><p id=x class=a>";
    assert_eq!(rgb(&style(html, "x")), GREEN);
    let html = "<style>p { color: red } p { color: blue }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), BLUE);
}

#[test]
fn importance_and_style_attribute() {
    let html = "<style>#x { color: blue }</style><p id=x style='color: red'>";
    assert_eq!(rgb(&style(html, "x")), RED);
    let html = "<style>p { color: blue !important }</style><p id=x style='color: red'>";
    assert_eq!(rgb(&style(html, "x")), BLUE);
    let html = "<style>p { color: blue !important }</style><p id=x style='color: red !important'>";
    assert_eq!(rgb(&style(html, "x")), RED);
}

#[test]
fn inheritance() {
    let html = "<div style='color: red; margin: 5px; padding: 3px'><p id=x></p></div>";
    let p = style(html, "x");
    assert_eq!(rgb(&p), RED);
    assert_eq!(margin(&p.margin_left), 0.0);
    let html = "<div style='padding: 3px'><p id=x style='padding: inherit; margin-left: inherit'></p></div>";
    let p = style(html, "x");
    assert_eq!(p.padding_left, LengthPercentage::Length(3.0));
}

#[test]
fn cascade_layers() {
    let html = "<style>@layer a, b; @layer b { p { color: red } } @layer a { p { color: blue } }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), RED);
    // Unlayered styles beat layered ones, whatever their specificity.
    let html = "<style>p { color: red } @layer a { #x { color: blue } }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), RED);
    // Important declarations reverse layer order.
    let html = "<style>@layer a { p { color: red !important } } @layer b { p { color: blue !important } }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), RED);
    let html = "<style>@layer a { p { color: red !important } } p { color: blue !important }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), RED);
    // Nested layers sort before their parent's own rules.
    let html =
        "<style>@layer a { p { color: red } @layer inner { p { color: blue } } }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), RED);
}

#[test]
fn revert_and_revert_layer() {
    let html = "<style>p { display: inline } p { display: revert }</style><p id=x>";
    assert_eq!(style(html, "x").display, Display::BLOCK);
    let html = "<style>@layer base { p { color: red } } @layer top { p { color: blue; color: revert-layer } }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), RED);
}

#[test]
fn scope_proximity() {
    let html = r#"<style>
        @scope (.light) { p { color: blue } }
        @scope (.dark) { p { color: red } }
    </style>
    <div class=dark><div class=light><p id=x></p></div></div>
    <div class=light><div class=dark><p id=y></p></div></div>"#;
    // The nearer scope root wins even though the dark rule comes later.
    assert_eq!(rgb(&style(html, "x")), BLUE);
    assert_eq!(rgb(&style(html, "y")), RED);
    let html = r#"<style>@scope (.card) to (.content) { p { color: red } }</style>
        <div class=card><p id=a></p><div class=content><p id=b></p></div></div>"#;
    assert_eq!(rgb(&style(html, "a")), RED);
    assert_eq!(rgb(&style(html, "b")), [0, 0, 0, 255]);
}

#[test]
fn custom_properties() {
    let html = "<style>:root { --main: red } div { --gap: 2em } p { color: var(--main); margin-top: var(--gap) }</style><div style='font-size: 10px'><p id=x style='font-size: 20px'></p></div>";
    let p = style(html, "x");
    assert_eq!(rgb(&p), RED);
    // `em` resolves against the element using the value, not where --gap was declared.
    assert_eq!(margin(&p.margin_top), 40.0);
    // A failed substitution makes the declaration behave as `unset`: color inherits.
    let html = "<div style='color: blue'><p id=x style='color: var(--missing)'></p></div>";
    assert_eq!(rgb(&style(html, "x")), BLUE);
    let html = "<p id=x style='--c: 2px solid; border-top: var(--c) red'></p>";
    let p = style(html, "x");
    assert_eq!(
        (p.border_top_width, p.border_top_style),
        (2.0, BorderStyle::Solid)
    );
}

#[test]
fn values_compute() {
    let html = "<html style='font-size: 20px'><p id=x style='font-size: 2rem; margin-left: calc(1em + 10px); margin-right: 50%; width: calc(100% - 2em)'></p>";
    let p = style(html, "x");
    assert_eq!(p.font_size, 40.0);
    assert_eq!(margin(&p.margin_left), 50.0);
    assert_eq!(
        p.margin_right,
        MaybeAuto::Value(LengthPercentage::Percentage(0.5))
    );
    let rustyprint_css::properties::types::Size::Value(width) = &p.width else {
        panic!()
    };
    assert_eq!(width.resolve(200.0), 120.0);
    let html = "<p id=x style='font: italic bold 12px/1.5 Georgia, serif'></p>";
    let p = style(html, "x");
    assert_eq!(
        (p.font_style, p.font_weight, p.font_size),
        (FontStyle::Italic, 700.0, 12.0)
    );
    assert_eq!(p.line_height.to_px(p.font_size), 18.0);
}

#[test]
fn logical_properties() {
    let html = "<p id=x dir=rtl style='margin-inline-start: 5px; padding-block-end: 2px'></p>";
    let p = style(html, "x");
    assert_eq!(margin(&p.margin_right), 5.0);
    assert_eq!(margin(&p.margin_left), 0.0);
    assert_eq!(p.padding_bottom, LengthPercentage::Length(2.0));
    // A later physical declaration wins over an earlier logical one, and the reverse.
    let html = "<p id=x style='margin-inline-start: 5px; margin-left: 7px'></p>";
    assert_eq!(margin(&style(html, "x").margin_left), 7.0);
    let html = "<p id=x style='margin-left: 7px; margin-inline-start: 5px'></p>";
    assert_eq!(margin(&style(html, "x").margin_left), 5.0);
}

#[test]
fn borders() {
    let html = "<p id=x style='border: 2px dashed red; border-bottom-style: none'></p>";
    let p = style(html, "x");
    assert_eq!(p.border_top_width, 2.0);
    assert_eq!(p.border_top_style, BorderStyle::Dashed);
    assert_eq!(p.border_bottom_width, 0.0);
    // Initial style is `none`, so a width alone computes to 0.
    assert_eq!(
        style("<p id=x style='border-width: 4px'>", "x").border_left_width,
        0.0
    );
}

#[test]
fn presentational_hints_lose_to_author_styles() {
    let html = "<p id=x align=center></p><p id=y align=center style='text-align: left'></p>";
    let (doc, styles) = style_with(html, None, &[]);
    assert_eq!(by_id(&doc, &styles, "x").text_align, TextAlign::Center);
    assert_eq!(by_id(&doc, &styles, "y").text_align, TextAlign::Left);
    let body = style("<body id=b bgcolor=red>", "b");
    assert_eq!(
        body.background_color
            .resolve(&AbsoluteColor::BLACK)
            .to_rgba8(),
        RED
    );
}

#[test]
fn media_and_supports() {
    let html = "<style>@media screen { p { color: red } } @media print and (min-width: 10cm) { p { color: blue } }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), BLUE);
    let html = "<style>@supports (color: red) { p { color: red } } @supports (made-up: 1) { p { color: blue } }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), RED);
    let html = "<style>@custom-media --wide (width > 100px); @media (--wide) { p { color: red } }</style><p id=x>";
    assert_eq!(rgb(&style(html, "x")), RED);
}

#[test]
fn imports_links_and_request_stylesheet() {
    let files = [
        ("base.css", "p { color: red } @import 'ignored.css';"),
        (
            "theme.css",
            "@import url(base.css) layer(base); p { margin-top: 3px }",
        ),
    ];
    let html = "<link rel=stylesheet href=theme.css><link rel=stylesheet href=theme.css media=screen><p id=x>";
    let (doc, styles) = style_with(html, None, &files);
    let p = by_id(&doc, &styles, "x");
    assert_eq!(rgb(&p), RED);
    assert_eq!(margin(&p.margin_top), 3.0);
    let (doc, styles) = style_with(
        "<style>p { color: red }</style><p id=x>",
        Some("p { color: blue }"),
        &[],
    );
    assert_eq!(rgb(&by_id(&doc, &styles, "x")), BLUE);
}

#[test]
fn warnings_point_at_the_problem() {
    let (_, styles) = style_with(
        "<style>\np {\n  colour: red;\n  width: -1px;\n}\n</style>",
        None,
        &[],
    );
    let messages: Vec<String> = styles
        .warnings
        .iter()
        .map(|w| format!("{}:{} {}", w.source, w.warning.line, w.warning.message))
        .collect();
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(
        messages[0].starts_with("<style>:3 unsupported property `colour`"),
        "{messages:?}"
    );
    assert!(
        messages[1].contains("invalid value for `width`"),
        "{messages:?}"
    );
}

#[test]
fn pseudo_elements() {
    let html = "<style>p::before { color: red; margin-left: 1em } p { font-size: 10px; color: blue }</style><p id=x>";
    let (doc, styles) = style_with(html, None, &[]);
    let node = doc.find_html(&rustyprint_dom::local_name!("p")).unwrap();
    let before = styles.pseudo(node, &PseudoElement::Before).unwrap();
    assert_eq!(rgb(before), RED);
    assert_eq!(margin(&before.margin_left), 10.0);
    assert!(styles.pseudo(node, &PseudoElement::After).is_none());
}

#[test]
fn root_is_blockified() {
    let html = "<style>html { display: inline }</style>";
    let (doc, styles) = style_with(html, None, &[]);
    assert_eq!(
        styles.get(doc.document_element().unwrap()).unwrap().display,
        Display::BLOCK
    );
}
