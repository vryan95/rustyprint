use rustyprint_css::{StyleOptions, compute_styles};
use rustyprint_dom::Document;
use rustyprint_layout::{PageOptions, Rect, Sides, Size, layout_tree};

/// Lays out `body` (with no body margin) on an 800 × 1000 page area and returns the border
/// box of each element with an `id`.
fn boxes(body: &str) -> Vec<(String, Rect, usize)> {
    let html = format!("<!DOCTYPE html><style>body {{ margin: 0 }}</style><body>{body}");
    let document = Document::parse(&html);
    let load = |_: &str| None;
    let styles = compute_styles(
        &document,
        &StyleOptions {
            page_size: (800.0, 1000.0),
            stylesheet: None,
            load: &load,
            font_metrics: None,
            load_font: None,
        },
    );
    let page = PageOptions {
        size: Size::new(800.0, 1000.0),
        margin: Sides::default(),
    };
    let root = layout_tree(
        &document,
        &styles,
        &page,
        &rustyprint_layout::Images::none(),
    )
    .unwrap();
    root.element_boxes()
        .into_iter()
        .filter_map(|(node, rect, fragment)| {
            let id = document.element(node)?.id()?.to_owned();
            Some((id, rect, fragment.line_count()))
        })
        .collect()
}

fn rect(body: &str, id: &str) -> Rect {
    boxes(body)
        .into_iter()
        .find(|(i, ..)| i == id)
        .unwrap_or_else(|| panic!("no #{id}"))
        .1
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[track_caller]
fn assert_rect(r: Rect, x: f32, y: f32, width: f32, height: f32) {
    assert!(
        close(r.x, x) && close(r.y, y) && close(r.width, width) && close(r.height, height),
        "expected ({x}, {y}, {width}, {height}), got ({}, {}, {}, {})",
        r.x,
        r.y,
        r.width,
        r.height
    );
}

#[test]
fn auto_width_fills_the_containing_block() {
    let r = rect(
        "<div id=t style='padding: 10px; border: 5px solid; height: 20px'></div>",
        "t",
    );
    assert_rect(r, 0.0, 0.0, 800.0, 50.0);
}

#[test]
fn auto_margins_center() {
    assert_rect(
        rect(
            "<div id=t style='width: 200px; height: 1px; margin: 0 auto'></div>",
            "t",
        ),
        300.0,
        0.0,
        200.0,
        1.0,
    );
    assert_rect(
        rect(
            "<div id=t style='width: 200px; height: 1px; margin-left: auto'></div>",
            "t",
        ),
        600.0,
        0.0,
        200.0,
        1.0,
    );
}

#[test]
fn over_constrained_margins_ignore_the_end_margin() {
    assert_rect(
        rect(
            "<div id=t style='width: 200px; height: 1px; margin: 0 10px'></div>",
            "t",
        ),
        10.0,
        0.0,
        200.0,
        1.0,
    );
    let rtl = rect(
        "<div dir=rtl><div id=t style='width: 200px; height: 1px; margin: 0 10px'></div></div>",
        "t",
    );
    assert_rect(rtl, 590.0, 0.0, 200.0, 1.0);
}

#[test]
fn box_sizing_and_percentages() {
    let r = rect(
        "<div id=t style='box-sizing: border-box; width: 25%; padding: 20px; border: 5px solid; height: 100px'></div>",
        "t",
    );
    assert_rect(r, 0.0, 0.0, 200.0, 100.0);
    let r = rect(
        "<div id=t style='width: 50%; padding: 0 10%; height: 1px'></div>",
        "t",
    );
    assert_rect(r, 0.0, 0.0, 560.0, 1.0);
}

#[test]
fn min_and_max_widths() {
    assert!(close(
        rect("<div id=t style='width: 50%; max-width: 300px'></div>", "t").width,
        300.0
    ));
    assert!(close(
        rect(
            "<div id=t style='width: 100px; min-width: 500px'></div>",
            "t"
        )
        .width,
        500.0
    ));
    // min-width wins over max-width.
    assert!(close(
        rect(
            "<div id=t style='max-width: 100px; min-width: 200px'></div>",
            "t"
        )
        .width,
        200.0
    ));
}

#[test]
fn heights() {
    let body = "<div style='height: 200px'><div id=t style='height: 50%'></div></div><div><div id=u style='height: 50%'></div></div>";
    assert!(close(rect(body, "t").height, 100.0));
    // A percentage of an auto height behaves as auto.
    assert!(close(rect(body, "u").height, 0.0));
    let r = rect(
        "<div id=t style='height: 10px; min-height: 30px; max-height: 20px'></div>",
        "t",
    );
    assert!(close(r.height, 30.0));
}

#[test]
fn sibling_margins_collapse() {
    let body = "<div id=a style='height: 10px; margin-bottom: 30px'></div><div id=b style='height: 10px; margin-top: 20px'></div>";
    assert!(close(rect(body, "b").y, 40.0));
    let negative = "<div id=a style='height: 10px; margin-bottom: 20px'></div><div id=b style='height: 10px; margin-top: -5px'></div>";
    assert!(close(rect(negative, "b").y, 25.0));
}

#[test]
fn parent_and_child_margins_collapse() {
    let body = "<div id=p style='margin-top: 10px'><div id=c style='margin-top: 25px; height: 10px'></div></div>";
    assert_rect(rect(body, "p"), 0.0, 25.0, 800.0, 10.0);
    assert_rect(rect(body, "c"), 0.0, 25.0, 800.0, 10.0);
    // A border separates them.
    let body = "<div id=p style='margin-top: 10px; border-top: 1px solid'><div id=c style='margin-top: 25px; height: 10px'></div></div>";
    assert!(close(rect(body, "c").y, 36.0));
    // So does a new block formatting context.
    let body = "<div id=p style='margin-top: 10px; display: flow-root'><div id=c style='margin-top: 25px; height: 10px'></div></div>";
    assert!(close(rect(body, "p").y, 10.0));
    assert!(close(rect(body, "c").y, 35.0));
    // Bottom margins escape through an auto-height parent.
    let body = "<div id=p><div style='height: 10px; margin-bottom: 40px'></div></div><div id=n style='height: 1px'></div>";
    assert!(close(rect(body, "p").height, 10.0));
    assert!(close(rect(body, "n").y, 50.0));
}

#[test]
fn empty_blocks_collapse_through() {
    let body = "<div id=a style='height: 10px; margin-bottom: 10px'></div><div style='margin: 15px 0'></div><div id=b style='height: 10px'></div>";
    assert!(close(rect(body, "b").y, 25.0));
}

#[test]
fn text_wraps_into_lines() {
    let text = "word ".repeat(40);
    let all = boxes(&format!(
        "<p id=wide style='margin: 0'>{text}</p><p id=narrow style='width: 100px; margin: 0'>{text}</p>"
    ));
    let lines = |id: &str| all.iter().find(|(i, ..)| i == id).unwrap().2;
    assert!(lines("wide") >= 2);
    assert!(lines("narrow") > lines("wide") * 3);
}

#[test]
fn inline_blocks_shrink_to_fit() {
    let body = "<p style='margin: 0'><span id=t style='display: inline-block; padding: 0 5px'>short</span></p>";
    let r = rect(body, "t");
    assert!(r.width > 20.0 && r.width < 80.0, "{r:?}");
    let body = "<p style='margin: 0'><span id=t style='display: inline-block'>a long enough text to wrap inside the narrow box</span></p>";
    let narrow = format!("<div style='width: 120px'>{body}</div>");
    assert!(rect(&narrow, "t").width <= 120.0);
}

#[test]
fn hidden_boxes_take_no_space() {
    let body =
        "<div style='display: none; height: 50px'></div><div id=t style='height: 1px'></div>";
    assert!(close(rect(body, "t").y, 0.0));
    let body =
        "<div style='visibility: hidden; height: 50px'></div><div id=t style='height: 1px'></div>";
    assert!(close(rect(body, "t").y, 50.0));
}

#[test]
fn anonymous_blocks_have_no_borders() {
    // Inline content beside a block goes in an anonymous block, which has no border.
    let r = rect("<div><span id=a>t</span><div>x</div></div>", "a");
    assert!(close(r.x, 0.0) && close(r.y, 0.0), "{r:?}");
}
