use rustyprint_css::{StyleOptions, compute_styles};
use rustyprint_dom::Document;
use rustyprint_layout::{
    DisplayItem, Images, PageOptions, Point, Rect, Sides, Size, layout_tree, paint_fragment,
};

/// Paints `body` (with no body margin) on an 800 × 1000 page area. `image.png` is a 10 × 10
/// opaque image.
fn paint(body: &str) -> Vec<DisplayItem> {
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
    let png = {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::from(image::RgbImage::new(10, 10))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    };
    let load_file = move |url: &str| (url == "image.png").then(|| png.clone());
    let images = Images::new(&load_file);
    paint_fragment(&root, &images)
}

fn clips(items: &[DisplayItem]) -> Vec<Rect> {
    items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::PushClip(rect) => Some(*rect),
            _ => None,
        })
        .collect()
}

#[track_caller]
fn assert_close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.01, "expected {b}, got {a}");
}

#[test]
fn overflow_clip_margin_extends_the_clip() {
    let items = paint(
        "<div style='width: 100px; height: 50px; padding: 5px; overflow: clip; \
         overflow-clip-margin: content-box 3px'></div>",
    );
    let clip = clips(&items)[0];
    // The content box (5px in) grown by 3px.
    assert_close(clip.x, 2.0);
    assert_close(clip.y, 2.0);
    assert_close(clip.width, 106.0);
    assert_close(clip.height, 56.0);
}

#[test]
fn hidden_overflow_ignores_the_clip_margin() {
    let items = paint(
        "<div style='width: 100px; height: 50px; overflow: hidden; \
         overflow-clip-margin: 20px'></div>",
    );
    let clip = clips(&items)[0];
    assert_close(clip.x, 0.0);
    assert_close(clip.width, 100.0);
}

#[test]
fn rounded_overflow_clips_to_a_path() {
    let items = paint(
        "<div style='width: 100px; height: 50px; overflow: hidden; border-radius: 10px'></div>",
    );
    assert!(
        items
            .iter()
            .any(|item| matches!(item, DisplayItem::PushClipPath { .. }))
    );
}

#[test]
fn outlines_surround_the_border_box() {
    let items = paint(
        "<div style='margin: 50px; width: 100px; height: 20px; \
         outline: 4px solid red; outline-offset: 6px'></div>",
    );
    let corners = items
        .iter()
        .filter_map(DisplayItem::bounds)
        .flat_map(|r| [Point::new(r.x, r.y), Point::new(r.right(), r.bottom())]);
    let bounds = Rect::enclosing(corners).unwrap();
    // The border box at (50, 50) grown by the offset and the width.
    assert_close(bounds.x, 40.0);
    assert_close(bounds.y, 40.0);
    assert_close(bounds.width, 120.0);
    assert_close(bounds.height, 40.0);
}

#[test]
fn background_images_tile_from_the_padding_box() {
    let items = paint(
        "<div style='width: 30px; height: 10px; border: 5px solid transparent; \
         background: url(image.png) repeat-x'></div>",
    );
    let tiles: Vec<Rect> = items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    // From the padding box at x = 5, repeated across the border box (0 to 40).
    let xs: Vec<f32> = tiles.iter().map(|t| t.x).collect();
    assert_eq!(xs, [-5.0, 5.0, 15.0, 25.0, 35.0]);
    assert!(tiles.iter().all(|t| t.y == 5.0 && t.width == 10.0));
}

#[test]
fn background_color_follows_background_clip() {
    let items = paint(
        "<div style='width: 30px; height: 10px; padding: 4px; background: red content-box'></div>",
    );
    let rect = items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert_eq!(rect, Rect::new(4.0, 4.0, 30.0, 10.0));
}
