use rustyprint_css::{StyleOptions, compute_styles};
use rustyprint_dom::Document;
use rustyprint_layout::fragment::{BoxFragment, Fragment};
use rustyprint_layout::{DisplayItem, Images, PageOptions, Sides, Size, layout, layout_pages};

const PAGE: Size = Size::new(400.0, 300.0);

fn styled(body: &str) -> (Document, rustyprint_css::Styles) {
    let html = format!("<!DOCTYPE html><style>body {{ margin: 0 }}</style><body>{body}");
    let document = Document::parse(&html);
    let load = |_: &str| None;
    let styles = compute_styles(
        &document,
        &StyleOptions {
            page_size: (PAGE.width, PAGE.height),
            stylesheet: None,
            load: &load,
            font_metrics: None,
            load_font: None,
        },
    );
    (document, styles)
}

fn options() -> PageOptions {
    PageOptions {
        size: PAGE,
        margin: Sides::default(),
    }
}

/// The root's fragment on each page.
fn pages(body: &str) -> Vec<BoxFragment> {
    let (document, styles) = styled(body);
    layout_pages(
        &document,
        &styles,
        &options(),
        &rustyprint_layout::Images::none(),
    )
    .into_iter()
    .map(|page| page.root.expect("no blank pages"))
    .collect()
}

/// The fragment tagged `--id: <id>` on a page.
fn find<'a>(root: &'a BoxFragment, id: &str) -> &'a BoxFragment {
    fn walk<'a>(fragment: &'a BoxFragment, out: &mut Vec<&'a BoxFragment>) {
        out.push(fragment);
        for child in &fragment.children {
            if let Fragment::Box(child) = child {
                walk(child, out);
            }
        }
    }
    let mut all = Vec::new();
    walk(root, &mut all);
    all.into_iter()
        .find(|f| f.style.custom.get("--id").is_some_and(|v| v.trim() == id))
        .unwrap_or_else(|| panic!("no fragment `{id}`"))
}

/// The y position of the fragment tagged `--id: <id>` on a page.
fn y_of(root: &BoxFragment, id: &str) -> f32 {
    fn walk(fragment: &BoxFragment, y: f32, id: &str) -> Option<f32> {
        let y = y + fragment.rect.y;
        if fragment
            .style
            .custom
            .get("--id")
            .is_some_and(|v| v.trim() == id)
        {
            return Some(y);
        }
        fragment.children.iter().find_map(|child| match child {
            Fragment::Box(child) => walk(child, y, id),
            Fragment::Line(_) => None,
        })
    }
    walk(root, 0.0, id).unwrap_or_else(|| panic!("no fragment `{id}`"))
}

#[test]
fn margins_after_unforced_breaks_are_truncated() {
    let pages = pages(
        "<div style='height: 250px'></div>\
         <div style='--id: b; margin-top: 30px; height: 100px'></div>",
    );
    assert_eq!(pages.len(), 2);
    assert_eq!(y_of(&pages[1], "b"), 0.0);
}

#[test]
fn trailing_margins_that_overflow_the_page_are_truncated() {
    for body in [
        // The last box's margin.
        "<div style='height: 280px; margin-bottom: 40px'></div>",
        // Margins that collapse through nested boxes and the body.
        "<div style='margin-bottom: 30px'><p style='margin: 16px 0; height: 270px'></p></div>",
        // A margin collapsing through an empty box after the last one.
        "<div style='height: 290px'></div><div style='margin: 20px 0'></div>",
    ] {
        assert_eq!(pages(body).len(), 1, "{body}");
    }
}

#[test]
fn a_margin_before_the_next_box_still_breaks_when_the_box_does_not_fit() {
    let pages = pages(
        "<div style='height: 280px; margin-bottom: 40px'></div>\
         <div style='--id: b; height: 10px'></div>",
    );
    assert_eq!(pages.len(), 2);
    assert_eq!(y_of(&pages[1], "b"), 0.0);
}

#[test]
fn margins_after_forced_breaks_are_kept() {
    let pages = pages(
        "<div style='height: 50px'></div>\
         <div style='--id: b; break-before: page; margin-top: 30px; height: 10px'></div>",
    );
    assert_eq!(pages.len(), 2);
    assert_eq!(y_of(&pages[1], "b"), 30.0);
}

#[test]
fn sliced_boxes_drop_borders_at_breaks_and_cloned_ones_repeat_them() {
    for (decoration, top, bottom) in [("slice", 0.0, 0.0), ("clone", 10.0, 10.0)] {
        let pages = pages(&format!(
            "<div style='--id: a; border: 10px solid; box-decoration-break: {decoration}'>\
             <div style='height: 500px'></div></div>"
        ));
        assert_eq!(pages.len(), 2, "{decoration}");
        let first = find(&pages[0], "a");
        let second = find(&pages[1], "a");
        assert_eq!(first.border.top, 10.0);
        assert_eq!(first.border.bottom, bottom, "{decoration}");
        assert_eq!(second.border.top, top, "{decoration}");
        assert_eq!(second.border.bottom, 10.0);
        // The first fragment fills the page.
        assert_eq!(first.rect.bottom(), PAGE.height, "{decoration}");
    }
}

#[test]
fn definite_heights_continue_on_the_next_page() {
    let pages = pages("<div style='--id: a; height: 700px'></div>");
    assert_eq!(pages.len(), 3);
    let heights: Vec<f32> = pages.iter().map(|p| find(p, "a").rect.height).collect();
    assert_eq!(heights, [300.0, 300.0, 100.0]);
}

#[test]
fn fixed_backgrounds_repeat_on_every_page() {
    let (document, styles) = styled(
        "<div style='height: 500px; background: url(dot.png) no-repeat fixed right bottom'></div>",
    );
    let png = {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::from(image::RgbImage::new(10, 10))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    };
    let load = move |url: &str| (url == "dot.png").then(|| png.clone());
    let images = Images::new(&load);
    let document = layout(&document, &styles, &options(), &images);
    assert_eq!(document.pages.len(), 2);
    for page in &document.pages {
        let image = page
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Image { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("an image on every page");
        assert_eq!((image.x, image.y), (390.0, 290.0));
    }
}

/// The heights of the three cells `a`, `b` and `c` beside a 240px cell that spans their rows.
fn heights_beside_a_spanning_cell(rows: [&str; 3]) -> [f32; 3] {
    let tall = "<td rowspan=3><div style='height: 240px'></div></td>";
    let pages = pages(&format!(
        "<style>td {{ padding: 0 }}</style><table style='border-spacing: 0'>\
         <tr>{}{tall}</tr><tr>{}</tr><tr>{}</tr></table>",
        rows[0], rows[1], rows[2]
    ));
    ["a", "b", "c"].map(|id| find(&pages[0], id).rect.height.round())
}

#[test]
fn rows_under_a_taller_spanning_cell_share_its_extra_height() {
    // Three rows of 20px, 40px and 20px under a 240px cell grow in proportion.
    let cell =
        |id, height| format!("<td style='--id: {id}'><div style='height: {height}px'></div></td>");
    let (a, b, c) = (cell("a", 20), cell("b", 40), cell("c", 20));
    assert_eq!(
        heights_beside_a_spanning_cell([&a, &b, &c]),
        [60.0, 120.0, 60.0]
    );
}

#[test]
fn rows_with_a_height_grow_under_a_spanning_cell_only_when_all_rows_have_one() {
    // Rows without a height take the extra before rows with one.
    let a = "<td style='--id: a; height: 20px'></td>";
    let b = "<td style='--id: b'><div style='height: 20px'></div></td>";
    let c = "<td style='--id: c'><div style='height: 20px'></div></td>";
    assert_eq!(
        heights_beside_a_spanning_cell([a, b, c]),
        [20.0, 110.0, 110.0]
    );
    // When every row has a height, they all grow in proportion.
    let b = "<td style='--id: b; height: 60px'></td>";
    let c = "<td style='--id: c; height: 40px'></td>";
    assert_eq!(
        heights_beside_a_spanning_cell([a, b, c]),
        [40.0, 120.0, 80.0]
    );
}

#[test]
fn a_row_where_another_spanning_cell_starts_takes_the_extra_height() {
    let cell = |id| format!("<td style='--id: {id}'><div style='height: 20px'></div></td>");
    let a = cell("a");
    let b = format!("{}<td rowspan=2></td>", cell("b"));
    let c = cell("c");
    assert_eq!(
        heights_beside_a_spanning_cell([&a, &b, &c]),
        [20.0, 200.0, 20.0]
    );
}

#[test]
fn an_empty_row_under_a_spanning_cell_stays_empty() {
    let pages = pages(
        "<style>td { padding: 0 }</style><table style='border-spacing: 0'>\
         <tr><td style='--id: a'><div style='height: 20px'></div></td>\
         <td rowspan=3><div style='height: 240px'></div></td></tr><tr></tr>\
         <tr><td style='--id: c'><div style='height: 20px'></div></td></tr></table>",
    );
    let height = |id| find(&pages[0], id).rect.height.round();
    assert_eq!((height("a"), height("c")), (120.0, 120.0));
}

#[test]
fn rows_without_content_leave_a_spanning_cells_height_to_the_last_one() {
    let pages = pages(
        "<style>td { padding: 0 }</style><table style='border-spacing: 0'>\
         <tr><td rowspan=2><div style='height: 240px'></div></td><td style='--id: a'></td></tr>\
         <tr><td style='--id: b'></td></tr></table>",
    );
    let height = |id| find(&pages[0], id).rect.height.round();
    assert_eq!((height("a"), height("b")), (0.0, 240.0));
}
