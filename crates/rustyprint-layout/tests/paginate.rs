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
fn a_positioned_box_split_by_pages_is_one_containing_block() {
    // Two rows fit a page, leaving 60px: the box is 300 + 300 + 120 = 720px tall.
    let row = "<div style='height: 120px; break-inside: avoid'></div>";
    let pages = pages(&format!(
        "<div style='position: relative'>\
           <div style='--id: static; position: absolute; left: 50%'>s</div>\
           <div style='--id: top; position: absolute; top: 12.5%; height: 10px'></div>\
           <div style='--id: middle; position: absolute; top: 50%; height: 10px'></div>\
           <div style='--id: bottom; position: absolute; bottom: 0; height: 20px'></div>\
           {}\
         </div>",
        row.repeat(5)
    ));
    assert_eq!(pages.len(), 3);
    assert_eq!(y_of(&pages[0], "static"), 0.0);
    assert_eq!(y_of(&pages[0], "top"), 90.0);
    assert_eq!(y_of(&pages[1], "middle"), 60.0);
    assert_eq!(y_of(&pages[2], "bottom"), 100.0);
}

#[test]
fn a_fixed_box_in_an_absolute_box_of_a_split_box_repeats_on_every_page() {
    let row = "<div style='height: 120px; break-inside: avoid'></div>";
    let pages = pages(&format!(
        "<div style='position: relative'>\
           <div style='position: absolute; top: 0'>\
             <div style='--id: fixed; position: fixed; top: 10px; height: 5px'></div>\
           </div>\
           {}\
         </div>",
        row.repeat(5)
    ));
    assert_eq!(pages.len(), 3);
    for page in &pages {
        let fixed = find(page, "fixed");
        assert!(fixed.out_of_flow.is_none(), "still a placeholder");
        assert_eq!(y_of(page, "fixed"), 10.0);
    }
}

#[test]
#[cfg(feature = "emoji")]
fn emoji_set_upright_in_vertical_text_are_painted_in_color() {
    // The color emoji font draws its glyphs from color layers, not from outlines, so text
    // that only drew the glyph would leave it blank.
    let layers = |style: &str| {
        let (document, styles) = styled(&format!("<p style='{style}'>\u{1F381}</p>"));
        let document = layout(&document, &styles, &options(), &Images::none());
        document.pages[0]
            .items
            .iter()
            .filter(|item| matches!(item, DisplayItem::PushClipPath { .. }))
            .count()
    };
    let horizontal = layers("");
    assert!(horizontal > 0);
    assert_eq!(
        layers("writing-mode: vertical-rl; text-orientation: upright"),
        horizontal
    );
}

#[test]
fn a_cell_width_wider_than_the_table_allows_is_squeezed_to_fit_as_browsers_do() {
    // The cells ask for 700px in a table capped at the 400px page. A cell's width is what its
    // column prefers, not a minimum, so the columns shrink to fit instead of running off the page.
    let pages = pages(
        "<table style='--id: t; width: 100%; max-width: 100%; border-collapse: collapse'><tr>\
         <td style='--id: a; width: 300px; padding: 0'>A</td>\
         <td style='--id: b; width: 200px; padding: 0'>B</td>\
         <td style='--id: c; width: 200px; padding: 0'>C</td>\
         </tr></table>",
    );
    let table = find(&pages[0], "t");
    assert!(
        table.rect.width <= PAGE.width + 0.01,
        "table is {} wide",
        table.rect.width
    );
    let cells: f32 = ["a", "b", "c"]
        .iter()
        .map(|id| find(&pages[0], id).rect.width)
        .sum();
    assert!(cells <= PAGE.width + 0.01, "cells take {cells}");
    // The space is still shared in proportion to what the cells asked for.
    assert!(find(&pages[0], "a").rect.width > find(&pages[0], "b").rect.width);
}

#[test]
fn a_column_gets_the_width_its_cell_asks_for_before_other_columns_grow() {
    // The first cell asks for 200px of the 400px table. The text beside it would take far
    // more than the rest, so it gets what is left once the first column has its width.
    let pages = pages(&format!(
        "<table style='width: 100%; border-collapse: collapse'><tr>\
         <td style='--id: a; width: 200px; padding: 0'>A</td>\
         <td style='--id: b; padding: 0'>{}</td></tr></table>",
        "to be or not ".repeat(20)
    ));
    let width = |id| find(&pages[0], id).rect.width;
    assert_eq!((width("a"), width("b")), (200.0, 200.0));
}

#[test]
fn a_col_width_is_still_a_minimum_when_the_table_is_narrower() {
    // Unlike a cell's width, a column's width counts towards its minimum (CSS Tables 3
    // § 3.9.1), so a 300px column keeps its width in a table styled 100px wide.
    let pages = pages(
        "<table style='width: 100px; border-collapse: collapse'>\
         <col style='width: 300px'><tr><td style='--id: a; padding: 0'>A</td></tr></table>",
    );
    assert_eq!(find(&pages[0], "a").rect.width, 300.0);
}

#[test]
fn a_cell_min_width_is_a_minimum_when_the_table_is_narrower() {
    // The cell's width may be squeezed, but not below its own min-width, which counts in
    // its border box under box-sizing: border-box.
    let pages = pages(
        "<table style='width: 100px; border-collapse: collapse'><tr>\
         <td style='--id: a; width: 200px; min-width: 200px; padding: 0'></td>\
         <td style='--id: b; width: 200px; min-width: 150px; padding: 0 5px; \
         box-sizing: border-box'></td></tr></table>",
    );
    assert_eq!(find(&pages[0], "a").rect.width, 200.0);
    assert_eq!(find(&pages[0], "b").rect.width, 150.0);
}

#[test]
fn a_forced_break_inside_a_table_cell_moves_the_rest_of_the_cell_to_the_next_page() {
    // The row fits on the first page; the break inside its cell still ends the page, and
    // what follows the break is kept, on the next page.
    let pages = pages(
        "<table><tr><td>\
         <div style='--id: a; height: 10px'></div>\
         <div style='break-after: page'></div>\
         <div style='--id: b; height: 10px'></div>\
         </td></tr></table>\
         <div style='--id: c; height: 10px'></div>",
    );
    assert_eq!(pages.len(), 2);
    find(&pages[0], "a");
    find(&pages[1], "b");
    find(&pages[1], "c");
}

#[test]
fn a_forced_break_nested_in_a_box_that_avoids_breaks_still_breaks() {
    // The box that avoids breaking inside starts below other content, so it gets no page to
    // break across, but a forced break inside it, one box down, still ends the page.
    let pages = pages(
        "<div style='height: 50px'></div>\
         <div style='break-inside: avoid'><div>\
         <div style='--id: a; height: 10px'></div>\
         <div style='--id: b; break-before: page; height: 10px'></div>\
         </div></div>",
    );
    assert_eq!(pages.len(), 2);
    find(&pages[0], "a");
    assert_eq!(y_of(&pages[1], "b"), 0.0);
}

#[test]
fn a_forced_break_inside_a_table_cell_resumes_on_the_side_it_asks_for() {
    // The first page is a right page, so `break-after: right` leaves the second blank.
    let (document, styles) = styled(
        "<table><tr><td>\
         <div style='--id: a; height: 10px'></div>\
         <div style='break-after: right'></div>\
         <div style='--id: b; height: 10px'></div>\
         </td></tr></table>",
    );
    let pages = layout_pages(&document, &styles, &options(), &Images::none());
    assert_eq!(pages.len(), 3);
    find(pages[0].root.as_ref().unwrap(), "a");
    assert!(pages[1].root.is_none());
    find(pages[2].root.as_ref().unwrap(), "b");
}

#[test]
fn a_forced_page_break_inside_a_table_cell_in_columns_ends_the_page() {
    let pages = pages(
        "<div style='columns: 2'><table><tr><td>\
         <div style='--id: a; height: 10px'></div>\
         <div style='break-after: page'></div>\
         <div style='--id: b; height: 10px'></div>\
         </td></tr></table></div>",
    );
    assert_eq!(pages.len(), 2);
    find(&pages[0], "a");
    find(&pages[1], "b");
}

#[test]
fn a_forced_break_inside_a_later_row_breaks_that_row() {
    // The break is in the second row: the first stays whole on the first page, and only the
    // second row continues on the next.
    let pages = pages(
        "<table style='border-spacing: 0'>\
         <tr><td style='--id: r1; padding: 0'><div style='height: 10px'></div></td></tr>\
         <tr><td style='--id: r2; padding: 0'>\
         <div style='--id: a; height: 10px'></div>\
         <div style='break-after: page'></div>\
         <div style='--id: b; height: 10px'></div>\
         </td></tr>\
         <tr><td style='--id: r3; padding: 0'><div style='height: 10px'></div></td></tr>\
         </table>",
    );
    assert_eq!(pages.len(), 2);
    assert_eq!(find(&pages[0], "r1").rect.height, 10.0);
    assert_eq!(y_of(&pages[0], "a"), 10.0);
    assert_eq!(y_of(&pages[1], "b"), 0.0);
    assert_eq!(y_of(&pages[1], "r3"), 10.0);
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
fn anchors_placed_in_an_inner_split_box_are_where_they_are_placed() {
    // The inner box's anchor is 400px down it, on the second page; the outer box's box is
    // placed below that anchor, not below where the anchor's element is in the flow.
    let row = "<div style='height: 120px; break-inside: avoid'></div>";
    let pages = pages(&format!(
        "<div style='position: relative'>\
           <div style='--id: below; position: absolute; top: anchor(--x bottom); height: 10px'></div>\
           <div style='position: relative'>\
             <div style='position: absolute; top: 400px; height: 10px; anchor-name: --x'></div>\
             {}\
           </div>\
         </div>",
        row.repeat(5)
    ));
    assert_eq!(pages.len(), 3);
    assert_eq!(y_of(&pages[1], "below"), 110.0);
}

#[test]
fn a_split_box_that_contains_fixed_boxes_places_those_in_its_absolute_boxes() {
    // The transformed box is the fixed box's containing block: 400px down it is the second
    // page, 100px down.
    let row = "<div style='height: 120px; break-inside: avoid'></div>";
    let pages = pages(&format!(
        "<div style='position: relative; transform: translateX(0)'>\
           <div style='position: absolute; top: 0'>\
             <div style='--id: fixed; position: fixed; top: 400px; height: 5px'></div>\
           </div>\
           {}\
         </div>",
        row.repeat(5)
    ));
    assert_eq!(pages.len(), 3);
    assert_eq!(y_of(&pages[1], "fixed"), 100.0);
    assert!(find(&pages[1], "fixed").out_of_flow.is_none());
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
