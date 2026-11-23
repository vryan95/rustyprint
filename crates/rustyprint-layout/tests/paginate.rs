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

#[test]
fn a_page_holds_what_fits_its_height_rounded_up_to_a_whole_pixel() {
    // Browsers lay a page out in a whole number of pixels: 300px of boxes fit a page area
    // of 299.5px, and 301px of them do not.
    let boxes = |second: f32| {
        pages(&format!(
            "<style>@page {{ size: 400px 299.5px; margin: 0 }} div {{ break-inside: avoid }}</style>\
             <div style='height: 150px'></div><div style='height: {second}px'></div>"
        ))
        .len()
    };
    assert_eq!(boxes(150.0), 1);
    assert_eq!(boxes(151.0), 2);
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

#[test]
fn a_column_prefers_the_width_of_its_cells_that_have_one() {
    // The header cells ask for 50, 50 and 100px in a 400px table. The long text below the
    // second one has no width of its own, so it wraps in the column rather than widening it,
    // and the columns share the spare 200px in proportion to their widths.
    let pages = pages(&format!(
        "<table style='width: 100%; border-collapse: collapse'><tr>\
         <td style='--id: a; width: 50px; padding: 0'>A</td>\
         <td style='--id: b; width: 50px; padding: 0'>B</td>\
         <td style='--id: c; width: 100px; padding: 0'>C</td>\
         </tr><tr><td style='padding: 0'>1</td><td style='padding: 0'>{}</td>\
         <td style='padding: 0'></td></tr></table>",
        "to be or not ".repeat(20)
    ));
    let width = |id| find(&pages[0], id).rect.width;
    assert_eq!((width("a"), width("b"), width("c")), (100.0, 100.0, 200.0));
}

/// The widths of the cells tagged `a`, `b` and `c` in a 400px table with these cells.
fn column_widths(cells: &str) -> [f32; 3] {
    let pages = pages(&format!(
        "<table style='width: 100%; border-collapse: collapse'><tr>{cells}</tr></table>"
    ));
    ["a", "b", "c"].map(|id| find(&pages[0], id).rect.width)
}

fn assert_widths(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 0.01,
            "{actual} is not {expected}"
        );
    }
}

#[test]
fn percentage_columns_share_spare_width_in_proportion_to_their_percentages() {
    // The percentages come to half of the table. The first column's content keeps it at
    // 100px rather than its 40px, which leaves 140px to share 1 : 2 : 2.
    let widths = column_widths(
        "<td style='--id: a; width: 10%; padding: 0'><div style='width: 100px'></div></td>\
         <td style='--id: b; width: 20%; padding: 0'></td>\
         <td style='--id: c; width: 20%; padding: 0'></td>",
    );
    assert_widths(widths, [128.0, 136.0, 136.0]);
}

#[test]
fn percentages_past_a_hundred_are_cut_from_the_last_column_back() {
    let widths = column_widths(
        "<td style='--id: a; width: 50%; padding: 0'></td>\
         <td style='--id: b; width: 30%; padding: 0'></td>\
         <td style='--id: c; width: 300%; padding: 0'></td>",
    );
    assert_widths(widths, [200.0, 120.0, 80.0]);
}

#[test]
fn percentage_columns_short_of_width_each_get_the_same_part_of_what_they_lack() {
    // The last column's content takes 300px of the 400px, so the first two, which ask for
    // 120px each, share the remaining 100px.
    let widths = column_widths(
        "<td style='--id: a; width: 30%; padding: 0'></td>\
         <td style='--id: b; width: 30%; padding: 0'></td>\
         <td style='--id: c; width: 40%; padding: 0'><div style='width: 300px'></div></td>",
    );
    assert_widths(widths, [50.0, 50.0, 300.0]);
}

#[test]
fn an_image_that_shrinks_with_its_cell_does_not_hold_its_column_open() {
    // The image asks for 300px and `max-width: 100%` lets it shrink with its cell, so its
    // column can go down to nothing. The other column needs 300px of the 400px table.
    let (document, styles) = styled(
        "<table style='width: 100%; border-collapse: collapse'><tr>\
         <td style='--id: a; padding: 0'><img src='dot.png' width='300' height='50' \
         style='--id: i; display: block; max-width: 100%'></td>\
         <td style='--id: b; padding: 0'><div style='width: 300px'></div></td></tr></table>",
    );
    let png = {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::from(image::RgbImage::new(10, 10))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    };
    let load = move |url: &str| (url == "dot.png").then(|| png.clone());
    let pages = layout_pages(&document, &styles, &options(), &Images::new(&load));
    let root = pages[0].root.as_ref().expect("a page");
    let size = |id| {
        let rect = find(root, id).rect;
        (rect.width, rect.height)
    };
    assert_eq!(size("a").0, 100.0);
    assert_eq!(size("b").0, 300.0);
    assert_eq!(size("i"), (100.0, 50.0));
}

#[test]
fn a_cell_taller_than_its_content_by_its_own_height_still_centres_it() {
    // The cell is 100px tall because of its height, its content 20px: `vertical-align:
    // middle` puts the content 40px down.
    let pages = pages(
        "<table style='border-spacing: 0'><tr>\
         <td style='height: 100px; padding: 0; vertical-align: middle'>\
         <div style='--id: c; width: 50px; height: 20px'></div></td></tr></table>",
    );
    let content = find(&pages[0], "c");
    assert!(
        (content.rect.y - 40.0).abs() < 0.5,
        "the content is at {}",
        content.rect.y
    );
}

/// The bottom of the fragment tagged `--id: <id>` on a page.
fn bottom_of(root: &BoxFragment, id: &str) -> f32 {
    y_of(root, id) + find(root, id).rect.height
}

/// A box of 10px borders and padding around `content`, tagged `a`.
fn widget(content: &str) -> String {
    format!(
        "<div style='--id: a; border: 10px solid; padding: 10px; background: #eee'>{content}</div>"
    )
}

fn table_rows(count: usize) -> String {
    let row = "<tr><td style='padding: 0; height: 50px'></td></tr>";
    format!(
        "<table style='border-spacing: 0'>{}</table>",
        row.repeat(count)
    )
}

#[test]
fn a_box_whose_content_breaks_fills_the_rest_of_the_page() {
    // Browsers paint a box's background and sides down to the end of every page it
    // continues from, wherever its content breaks: between rows, at a forced break, or
    // between boxes in a flex column or a table cell.
    let forced = "<div style='height: 100px'></div><div style='break-before: page'></div>";
    for content in [
        forced.to_owned(),
        table_rows(9),
        format!(
            "<div style='display: flex; flex-direction: column'>{}</div>",
            table_rows(9)
        ),
    ] {
        let pages = pages(&widget(&content));
        assert_eq!(pages.len(), 2, "{content}");
        assert_eq!(bottom_of(&pages[0], "a"), PAGE.height, "{content}");
        assert_eq!(find(&pages[1], "a").border.top, 0.0, "{content}");
        assert_eq!(find(&pages[1], "a").border.bottom, 10.0, "{content}");
    }
    let in_cell = pages(&format!(
        "<table style='border-spacing: 0'><tr><td style='padding: 0'>{}</td></tr></table>",
        widget(&table_rows(9))
    ));
    assert_eq!(in_cell.len(), 2);
    assert_eq!(bottom_of(&in_cell[0], "a"), PAGE.height);
    assert_eq!(y_of(&in_cell[1], "a"), 0.0);
}

#[test]
fn a_cloned_box_whose_content_breaks_closes_at_the_end_of_the_page() {
    let pages = pages(&format!(
        "<div style='--id: a; border: 10px solid; box-decoration-break: clone'>{}</div>",
        table_rows(9)
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(bottom_of(&pages[0], "a"), PAGE.height);
    for page in &pages {
        assert_eq!(
            (find(page, "a").border.top, find(page, "a").border.bottom),
            (10.0, 10.0)
        );
    }
}

#[test]
fn the_end_of_a_box_that_does_not_fit_goes_to_the_next_page() {
    // 20px of border and padding above content of 270px leave no room for the 20px below
    // it, which go to the next page on their own, without stretching the box.
    for (style, content) in [
        ("", "<div style='height: 270px'></div>"),
        ("height: 270px", ""),
        ("", "<div style='height: 255px; margin-bottom: 30px'></div>"),
    ] {
        let pages = pages(&format!(
            "<div style='--id: a; border: 10px solid; padding: 10px; {style}'>{content}</div>"
        ));
        assert_eq!(pages.len(), 2, "{style}{content}");
        let (first, last) = (find(&pages[0], "a"), find(&pages[1], "a"));
        let bottom = if content.contains("margin") {
            300.0
        } else {
            290.0
        };
        assert_eq!(first.rect.height, bottom, "{style}{content}");
        assert_eq!((first.border.bottom, first.padding.bottom), (0.0, 0.0));
        assert_eq!(last.rect.height, 20.0, "{style}{content}");
        assert_eq!((last.border.top, last.border.bottom), (0.0, 10.0));
    }
}

#[test]
fn content_that_can_break_earlier_goes_along_with_the_end_of_its_box() {
    // Fourteen blocks or rows of 20px fill the page below the top of the box, and the last
    // goes to the next page with the bottom border and padding, as in browsers.
    let row = "<tr><td style='--id: last; padding: 0; height: 20px'></td></tr>";
    for content in [
        "<div style='height: 20px'></div>".repeat(13)
            + "<div style='--id: last; height: 20px'></div>",
        format!(
            "<div style='border: 5px solid; padding: 5px'><table style='border-spacing: 0'>{}{row}</table></div>",
            "<tr><td style='padding: 0; height: 20px'></td></tr>".repeat(12)
        ),
    ] {
        let pages = pages(&widget(&content));
        assert_eq!(pages.len(), 2, "{content}");
        assert_eq!(bottom_of(&pages[0], "a"), PAGE.height, "{content}");
        assert!(y_of(&pages[1], "last") < 20.0, "{content}");
        assert_eq!(find(&pages[1], "a").border.bottom, 10.0, "{content}");
    }
}

#[test]
fn a_box_that_continues_breaks_before_its_end_rather_than_inside_a_child() {
    // On a page the box continues on, browsers move only lines and boxes of its own along
    // with its end: the rows of a table inside it stay.
    let pages = pages(&widget(&table_rows(11)));
    assert_eq!(pages.len(), 3);
    // Five rows on the first page, and six fill the second.
    assert_eq!(bottom_of(&pages[1], "a"), PAGE.height);
    let end = find(&pages[2], "a");
    assert_eq!((end.rect.height, end.border.bottom), (20.0, 10.0));
}

#[test]
fn a_box_below_others_moves_whole_when_only_its_end_does_not_fit() {
    let pages = pages(&format!(
        "<div style='height: 100px'></div>{}",
        widget("<div style='height: 170px'></div>")
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(y_of(&pages[1], "a"), 0.0);
    assert_eq!(find(&pages[1], "a").rect.height, 210.0);
}

#[test]
fn the_corners_and_shadows_of_a_sliced_box_stop_where_pages_break_it() {
    let (document, styles) = styled(
        "<div style='height: 500px; border-radius: 20px; background: rgb(0, 0, 255); \
         box-shadow: 0 0 0 5px rgb(0, 255, 0); margin: 0 20px'></div>",
    );
    let document = layout(&document, &styles, &options(), &Images::none());
    assert_eq!(document.pages.len(), 2);
    // The y of every point on a curve of the blue background, on each page.
    let curves = |items: &[DisplayItem]| -> Vec<f32> {
        items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Path { path, color, .. } if (color.r, color.b) == (0, 255) => {
                    Some(path.segments.iter().filter_map(|segment| match segment {
                        rustyprint_layout::PathSegment::CubicTo(_, _, end) => Some(end.y),
                        _ => None,
                    }))
                }
                _ => None,
            })
            .flatten()
            .collect()
    };
    let first = curves(&document.pages[0].items);
    let second = curves(&document.pages[1].items);
    assert!(
        !first.is_empty() && first.iter().all(|&y| y <= 20.0),
        "{first:?}"
    );
    assert!(
        !second.is_empty() && second.iter().all(|&y| y >= 180.0),
        "{second:?}"
    );
    // The shadow is cut off at the bottom of the first page.
    let clipped = document.pages[0].items.iter().any(|item| {
        matches!(item, DisplayItem::PushClip(rect) if rect.bottom() == PAGE.height && rect.y < 0.0)
    });
    assert!(clipped);
}
