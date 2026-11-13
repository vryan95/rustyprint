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
