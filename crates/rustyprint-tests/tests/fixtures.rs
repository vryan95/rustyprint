//! One test per row of the tracker files (`html-elements.md`, `css-*.md`).
//!
//! Each row maps to a fixture at `tests/fixtures/<tracker>/<slug>.html`. Rows with status
//! `todo` run as ignored tests; every other row must have a fixture that passes. Fixtures that
//! match no row fail the `coverage::orphan-fixtures` test.
//!
//! A fixture is an HTML document with expectations in a
//! `<script type="text/x-rustyprint-expect">` element:
//!
//! ```text
//! #t { margin-top: 10px; color: rgb(255, 0, 0) }   computed values of every matching element
//! #t::before { color: rgb(0, 128, 0) }             computed values of a pseudo-element
//! #t { layout-width: 200px; layout-lines: 2 }      the element's first box, or `none`
//! warning: unsupported property                    a warning containing this text
//! ```
//!
//! `layout-x`, `layout-y`, `layout-width` and `layout-height` measure the border box in CSS
//! pixels; `layout-lines` counts line boxes inside it. `layout-image` is the rectangle of the
//! first image the element paints, as `"x y width height"` in whole pixels from its border box
//! (or `none`); `layout-image-smooth` and `layout-image-orientation` tell how it is drawn, and
//! `layout-image-alphas` lists the opacity of every image it paints, from 0 to 255.
//! `layout-gradient` is the first gradient shading it paints, from its border box:
//! `"axial x0 y0 x1 y1"` or `"radial cx cy r0 r1 scale-y"`. `layout-paint-order` is where the
//! document paints the element's background color, as an index into everything it paints,
//! for comparing with another element's as `>#id` (painted later) or `<#id`.
//! Layout runs on one continuous page area as wide as A4, without margins.
//!
//! Page metrics come from laying the document out on A4 pages without margins:
//! `layout-page` is the page the element starts on, `layout-pages` the number of pages it
//! appears on, `layout-page-lines` its line count on each of them (such as `"3 2"`),
//! `layout-page-x` and `layout-page-y` the position of its first box on its page,
//! `layout-page-count` the number of pages in the document, blank ones included. Values of
//! each page, separated by spaces: `layout-page-sizes` in whole pixels (such as
//! `"794x1123 400x300"`), `layout-page-bleed` in pixels, `layout-page-orientation`,
//! `layout-page-marks` (`none`, `crop`, `cross` or `crop+cross`) and
//! `layout-page-margin-safety`. Margin boxes of each page, with pages separated by ` | `:
//! `layout-page-margin-text` as `name:text` and `layout-page-margin-rects` as
//! `name:x,y,width,height` in whole pixels. `layout-page-links` lists the links of each page,
//! with pages separated by ` | `, as the URI or `#n` for page `n` of this document;
//! `layout-page-link-areas` adds each link's rectangle, as `target@x,y,width,height`; and
//! `layout-page-bookmarks` the document outline as `level:title@page`, and
//! `layout-document-title` the title the PDF gets (or `none`).
//!
//! Without `warning:` lines the document must produce no warnings; with them, there must be
//! exactly one warning per line. Files next to the fixture (such as `assets/a.css`) can be
//! loaded by `<link>`, `@import` and `url()`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use libtest_mimic::{Arguments, Failed, Trial};
use rustyprint_css::StyleOptions;
use rustyprint_css::page::{MarginSafety, PageOrientation};
use rustyprint_css::selector::{MatchContext, Namespaces, SelectorList};
use rustyprint_dom::NodeId;
use rustyprint_dom::{Document, local_name, ns};
use rustyprint_layout::display::{BlendMode, ShadingKind};
use rustyprint_layout::fragment::ElementBox;
use rustyprint_layout::{
    Bookmark, Color, DisplayItem, ImageData, Images, LaidPage, Link, LinkTarget, PageOptions, Rect,
    Sides, Size, layout_pages, layout_tree, links_and_bookmarks, paint_fragment,
};
use rustyprint_tests::{TRACKERS, progress, tracker_items};

/// An A4 page without margins, in CSS pixels: what media queries and viewport units see.
const PAGE: (f32, f32) = (210.0 * 96.0 / 25.4, 297.0 * 96.0 / 25.4);

fn main() {
    let args = Arguments::from_args();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = root.join("tests/fixtures");
    let mut trials = Vec::new();
    let mut expected_files = BTreeSet::new();
    for (tracker, _) in TRACKERS {
        let text =
            fs::read_to_string(root.join(format!("{tracker}.md"))).expect("tracker file exists");
        for item in tracker_items(&text) {
            let slug = slug(tracker, &item.section, &item.name);
            let path = fixtures.join(tracker).join(format!("{slug}.html"));
            expected_files.insert(path.clone());
            let ignored = item.status == "todo";
            let trial = Trial::test(format!("{tracker}::{slug}"), move || run_fixture(&path))
                .with_ignored_flag(ignored);
            trials.push(trial);
        }
    }
    trials.push(Trial::test("coverage::orphan-fixtures", move || {
        orphans(&fixtures, &expected_files)
    }));
    let plan_root = root.clone();
    trials.push(Trial::test("coverage::milestone-status", move || {
        milestone_status(&plan_root)
    }));
    trials.push(Trial::test("coverage::progress-page", move || {
        progress_page(&root)
    }));
    libtest_mimic::run(&args, trials).exit();
}

/// The fixture file name for a tracker row.
fn slug(tracker: &str, section: &str, name: &str) -> String {
    let words = |s: &str| s.to_ascii_lowercase().replace(' ', "-");
    let slug = match tracker {
        "css-properties" if name == "--*" => "custom-properties".to_owned(),
        "css-selectors" => {
            let (base, function) = match name.strip_suffix("()") {
                Some(base) => (base, true),
                None => (name, false),
            };
            let base = if let Some(rest) = base.strip_prefix("::") {
                format!("pseudo-element-{rest}")
            } else if let Some(rest) = base.strip_prefix(':') {
                format!("pseudo-class-{rest}")
            } else {
                words(base)
            };
            if function { format!("{base}-fn") } else { base }
        }
        "css-at-rules" => {
            let bare = name.trim_start_matches('@');
            match section {
                "At-rules" => bare.to_owned(),
                "Page margin boxes" => format!("page-margin-{bare}"),
                "Feature value blocks" => format!("font-feature-values-{bare}"),
                "Media types" => format!("media-type-{bare}"),
                "Features" => format!("media-feature-{bare}"),
                descriptors => format!("{}-{bare}", descriptors.trim_start_matches('@')),
            }
        }
        "css-values" => {
            if section == "CSS-wide keywords" {
                format!("keyword-{name}")
            } else if let Some(ty) = name.strip_prefix('<') {
                format!("type-{}", ty.trim_end_matches('>'))
            } else if let Some(function) = name.strip_suffix("()") {
                format!("function-{function}")
            } else {
                format!("unit-{name}")
            }
        }
        _ => name.to_owned(),
    };
    slug.to_ascii_lowercase()
}

/// Fixtures that no tracker row expects.
fn orphans(fixtures: &Path, expected: &BTreeSet<PathBuf>) -> Result<(), Failed> {
    let mut orphans = Vec::new();
    for (tracker, _) in TRACKERS {
        let Ok(entries) = fs::read_dir(fixtures.join(tracker)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "html") && !expected.contains(&path) {
                orphans.push(path.display().to_string());
            }
        }
    }
    if orphans.is_empty() {
        Ok(())
    } else {
        Err(format!("fixtures without a tracker row:\n{}", orphans.join("\n")).into())
    }
}

/// Checks that the milestone statuses in `plan.md` agree with the trackers.
fn milestone_status(root: &Path) -> Result<(), Failed> {
    let problems = progress::milestone_problems(root).map_err(|e| e.to_string())?;
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("milestone statuses in plan.md:\n{}", problems.join("\n")).into())
    }
}

/// Checks that `progress.html` matches the trackers.
fn progress_page(root: &Path) -> Result<(), Failed> {
    let expected = progress::page(root).map_err(|e| e.to_string())?;
    let actual = fs::read_to_string(root.join(progress::PAGE)).unwrap_or_default();
    if actual.replace("\r\n", "\n") == expected {
        Ok(())
    } else {
        Err(format!(
            "{} is out of date: run `cargo run -p rustyprint-tests --bin progress`",
            progress::PAGE
        )
        .into())
    }
}

fn run_fixture(path: &Path) -> Result<(), Failed> {
    let html =
        fs::read_to_string(path).map_err(|_| format!("missing fixture {}", path.display()))?;
    let document = Document::parse(&html);
    let expectations = expectations(&document);
    if expectations.trim().is_empty() {
        return Err(
            "fixture has no <script type=\"text/x-rustyprint-expect\"> expectations".into(),
        );
    }
    let dir = path
        .parent()
        .expect("fixtures live in a directory")
        .to_owned();
    // Relative URLs resolve against the document's `<base>`.
    let base = document.base_url().map(str::to_owned);
    let path = |url: &str| match &base {
        Some(base) => dir.join(rustyprint_dom::resolve_url(base, url)),
        None => dir.join(url),
    };
    let load = |url: &str| fs::read_to_string(path(url)).ok();
    let load_font = |url: &str| fs::read(path(url)).ok();
    let page = PageOptions {
        size: Size::new(PAGE.0, PAGE.1),
        margin: Sides::default(),
    };
    let load_file = |url: &str| fs::read(path(url)).ok();
    let images = Images::new(&load_file);
    let styles = rustyprint_layout::style(
        &document,
        &StyleOptions {
            page_size: PAGE,
            stylesheet: None,
            load: &load,
            font_metrics: Some(&rustyprint_layout::font_metrics),
            load_font: Some(&load_font),
        },
        &page,
        &images,
    );

    let (blocks, warnings) = parse_expectations(&expectations)?;
    let tree = layout_tree(&document, &styles, &page, &images);
    let pages = layout_pages(&document, &styles, &page, &images);
    rustyprint_layout::picture::register_elements(&document, &pages, &images);
    let links = links_and_bookmarks(&document, &pages);
    if let Some(root) = &tree {
        // Paints everything once, so missing images warn.
        paint_fragment(root, &images);
    }
    let boxes = tree
        .as_ref()
        .map(|root| root.element_boxes())
        .unwrap_or_default();
    let painted = tree
        .as_ref()
        .map(|root| paint_fragment(root, &images))
        .unwrap_or_default();
    let mut failures = Vec::new();
    let ctx = MatchContext::new(&document);
    for block in &blocks {
        let selectors = SelectorList::parse(&block.selector, &Namespaces::default())
            .map_err(|_| format!("invalid expectation selector `{}`", block.selector))?;
        let pseudo = selectors.0[0].pseudo_element.clone();
        let elements: Vec<_> = document
            .descendants(document.root())
            .filter(|&n| document.element(n).is_some() && selectors.matches(&ctx, n))
            .collect();
        if elements.is_empty() {
            failures.push(format!("`{}` matches no element", block.selector));
            continue;
        }
        for element in elements {
            let style = match &pseudo {
                Some(pseudo) => styles.pseudo(element, pseudo),
                None => styles.get(element),
            };
            let Some(style) = style else {
                failures.push(format!("`{}` has no computed style", block.selector));
                continue;
            };
            for (property, expected) in &block.declarations {
                let metric = property
                    .strip_prefix("layout-")
                    .or_else(|| (property == "paint-items").then_some(property.as_str()));
                if metric == Some("document-title") {
                    let wanted = expected.trim().trim_matches('"');
                    let actual = rustyprint_layout::document_title(&document);
                    if actual.as_deref().unwrap_or("none") != wanted {
                        failures.push(format!(
                            "{} {{ {property} }}: expected `{wanted}`, got `{actual:?}`",
                            block.selector
                        ));
                    }
                    continue;
                }
                if let Some(metric) = metric.filter(|m| m.starts_with("page")) {
                    if let Err(failure) = check_pages(&pages, &links, element, metric, expected) {
                        failures.push(format!("{} {{ {property} }}: {failure}", block.selector));
                    }
                    continue;
                }
                if let Some(metric) = metric {
                    if let Err(failure) = check_layout(
                        &document, &boxes, &images, &painted, element, metric, expected,
                    ) {
                        failures.push(format!("{} {{ {property} }}: {failure}", block.selector));
                    }
                    continue;
                }
                match style.get_property(property) {
                    None => failures.push(format!("`{property}` is not a supported property")),
                    Some(actual) if normalize(&actual) != normalize(expected) => {
                        failures.push(format!(
                            "{} {{ {property} }}: expected `{expected}`, got `{actual}`",
                            block.selector
                        ))
                    }
                    Some(_) => {}
                }
            }
        }
    }

    let actual: Vec<String> = styles
        .warnings
        .iter()
        .map(|w| format!("{}:{}: {}", w.source, w.warning.line, w.warning.message))
        .chain(images.warnings())
        .collect();
    for wanted in &warnings {
        if !actual.iter().any(|a| a.contains(wanted.as_str())) {
            failures.push(format!("expected a warning containing `{wanted}`"));
        }
    }
    if actual.len() != warnings.len() {
        failures.push(format!(
            "expected {} warning(s), got {}:\n  {}",
            warnings.len(),
            actual.len(),
            actual.join("\n  ")
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n").into())
    }
}

/// Checks a `layout-*` expectation against the element's first box: `x`, `y`, `width` and
/// `height` of the border box (page area coordinates, CSS pixels), `lines`, the number of
/// line boxes inside it, or `text`, the text it paints in visual order with lines separated
/// by `|` (a quoted string), or `paint`, tokens of the display items it paints in order
/// (see [`paint_token`]). `none` expects no box.
///
/// Numbers may be prefixed with `<`, `>` or `=`, and may name another element as `#id` to
/// compare with the same metric of its box.
fn check_layout(
    document: &Document,
    boxes: &[(NodeId, Rect, ElementBox<'_>)],
    images: &Images,
    painted: &[DisplayItem],
    element: NodeId,
    metric: &str,
    expected: &str,
) -> Result<(), String> {
    let found = boxes.iter().find(|(node, ..)| *node == element);
    let Some((_, rect, fragment)) = found else {
        return if expected == "none" {
            Ok(())
        } else {
            Err("the element has no box".into())
        };
    };
    if expected == "none" && !metric.starts_with("image") && metric != "gradient" {
        return Err("expected no box".into());
    }
    if metric == "text" {
        let wanted = expected.trim().trim_matches('"');
        let actual = fragment.text();
        return if actual == wanted {
            Ok(())
        } else {
            Err(format!("expected text `{wanted}`, got `{actual}`"))
        };
    }
    if metric == "paint" {
        let wanted = expected.trim().trim_matches('"');
        let ElementBox::Block(block) = fragment else {
            return Err("inline boxes paint with their block".into());
        };
        let tokens: Vec<String> = paint_fragment(block, images)
            .iter()
            .map(|item| paint_token(item, block.rect.x, block.rect.y))
            .collect();
        return if paint_matches(&tokens, wanted) {
            Ok(())
        } else {
            Err(format!(
                "expected paint items `{wanted}` in order, got `{}`",
                tokens.join(" ")
            ))
        };
    }
    if metric == "mask" {
        let ElementBox::Block(block) = fragment else {
            return Err("inline boxes paint with their block".into());
        };
        return check_mask(&paint_fragment(block, images), block.rect, expected);
    }
    if metric == "gradient" {
        let wanted = expected.trim().trim_matches('"');
        let ElementBox::Block(block) = fragment else {
            return Err("an inline box paints no gradient".into());
        };
        let (dx, dy) = (block.rect.x, block.rect.y);
        let actual = paint_fragment(block, images)
            .into_iter()
            .find_map(|item| match item {
                DisplayItem::Gradient { shading, .. } => Some(match shading.kind {
                    ShadingKind::Axial { start, end } => format!(
                        "axial {} {} {} {}",
                        (start.x - dx).round(),
                        (start.y - dy).round(),
                        (end.x - dx).round(),
                        (end.y - dy).round()
                    ),
                    ShadingKind::Radial {
                        center,
                        r0,
                        r1,
                        scale_y,
                    } => format!(
                        "radial {} {} {} {} {:.2}",
                        (center.x - dx).round(),
                        (center.y - dy).round(),
                        r0.round(),
                        r1.round(),
                        scale_y
                    ),
                }),
                _ => None,
            })
            .unwrap_or_else(|| "none".to_owned());
        return if actual == wanted {
            Ok(())
        } else {
            Err(format!("expected `{wanted}`, got `{actual}`"))
        };
    }
    if metric.starts_with("image") {
        let wanted = expected.trim().trim_matches('"');
        let ElementBox::Block(block) = fragment else {
            return Err("an inline box paints no image".into());
        };
        let painted = paint_fragment(block, images);
        if metric == "image-alphas" {
            let alphas: Vec<String> = painted
                .iter()
                .filter_map(|item| match item {
                    DisplayItem::Image { alpha, .. } => Some(alpha.to_string()),
                    _ => None,
                })
                .collect();
            let actual = if alphas.is_empty() {
                "none".to_owned()
            } else {
                alphas.join(" ")
            };
            return if actual == wanted {
                Ok(())
            } else {
                Err(format!("expected `{wanted}`, got `{actual}`"))
            };
        }
        let image = painted.into_iter().find_map(|item| match item {
            DisplayItem::Image {
                rect,
                smooth,
                orientation,
                ..
            } => Some((rect, smooth, orientation)),
            _ => None,
        });
        let actual = match (metric, image) {
            (_, None) => "none".to_owned(),
            ("image", Some((r, ..))) => format!(
                "{} {} {} {}",
                (r.x - block.rect.x).round(),
                (r.y - block.rect.y).round(),
                r.width.round(),
                r.height.round()
            ),
            ("image-smooth", Some((_, smooth, _))) => smooth.to_string(),
            ("image-orientation", Some((.., orientation))) => orientation.to_string(),
            (other, _) => return Err(format!("unknown layout metric `{other}`")),
        };
        return if actual == wanted {
            Ok(())
        } else {
            Err(format!("expected `{wanted}`, got `{actual}`"))
        };
    }
    let measure = |rect: &Rect, fragment: &ElementBox<'_>| -> Result<f32, String> {
        Ok(match metric {
            "x" => rect.x,
            "y" => rect.y,
            "width" => rect.width,
            "height" => rect.height,
            "lines" => fragment.line_count() as f32,
            // Display items the box paints; inline boxes paint with their block.
            "paint-items" => match fragment {
                ElementBox::Block(block) => paint_fragment(block, images).len() as f32,
                ElementBox::Inline(_) => 0.0,
            },
            // The first rectangle filled over exactly the border box: its background.
            "paint-order" => painted
                .iter()
                .position(|item| {
                    matches!(item, DisplayItem::Rect { rect: r, .. }
                        if (r.x - rect.x).abs() < 0.01
                            && (r.y - rect.y).abs() < 0.01
                            && (r.width - rect.width).abs() < 0.01
                            && (r.height - rect.height).abs() < 0.01)
                })
                .ok_or("the element paints no background")? as f32,
            other => return Err(format!("unknown layout metric `{other}`")),
        })
    };
    let actual = measure(rect, fragment)?;
    let expected = expected.trim();
    let (op, operand) = match expected.chars().next() {
        Some(c @ ('<' | '>' | '=')) => (c, expected[1..].trim()),
        _ => ('=', expected),
    };
    let wanted = if let Some(id) = operand.strip_prefix('#') {
        let other = boxes
            .iter()
            .find(|(node, ..)| document.element(*node).and_then(|e| e.id()) == Some(id))
            .ok_or_else(|| format!("no box for `#{id}`"))?;
        measure(&other.1, &other.2)?
    } else {
        operand
            .trim_end_matches("px")
            .trim()
            .parse()
            .map_err(|_| format!("invalid number `{operand}`"))?
    };
    let ok = match op {
        '<' => actual < wanted - 0.01,
        '>' => actual > wanted + 0.01,
        _ => (actual - wanted).abs() <= 0.01,
    };
    if !ok {
        Err(format!("expected `{expected}` ({wanted}), got `{actual}`"))
    } else {
        Ok(())
    }
}

/// A display item as a token for `layout-paint`: its kind, with its color for filled and
/// stroked items (`#rrggbb`, or `#rrggbbaa` when not opaque), its opacity, blend mode and
/// mask for groups, and its matrix for transforms, with the translation relative to the
/// box at `(dx, dy)`. Pops of clips and transforms are `pop`, of groups `/group`.
fn paint_token(item: &DisplayItem, dx: f32, dy: f32) -> String {
    let hex = |c: Color| {
        if c.a == 255 {
            format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
        }
    };
    let number = |v: f32| {
        let v = (v * 1000.0).round() / 1000.0;
        if v == 0.0 {
            "0".to_owned()
        } else {
            v.to_string()
        }
    };
    match item {
        DisplayItem::Text(run) => format!("text({})", hex(run.color)),
        DisplayItem::Rect { color, .. } => format!("rect({})", hex(*color)),
        DisplayItem::Polygon { color, .. } => format!("polygon({})", hex(*color)),
        DisplayItem::Polyline { color, .. } => format!("polyline({})", hex(*color)),
        DisplayItem::Line { color, .. } => format!("line({})", hex(*color)),
        DisplayItem::Path { color, .. } => format!("path({})", hex(*color)),
        DisplayItem::StrokePath { color, .. } => format!("stroke({})", hex(*color)),
        DisplayItem::Image { .. } => "image".to_owned(),
        DisplayItem::Gradient { .. } => "gradient".to_owned(),
        DisplayItem::PushClip(_) => "clip".to_owned(),
        DisplayItem::PushClipText(_) => "clip-text".to_owned(),
        DisplayItem::PushClipPath { .. } => "clip-path".to_owned(),
        DisplayItem::PushTransform([a, b, c, d, e, f]) => {
            // The same transformation about the box's own corner.
            let e = a * dx + c * dy + e - dx;
            let f = b * dx + d * dy + f - dy;
            let values: Vec<String> = [*a, *b, *c, *d, e, f].into_iter().map(number).collect();
            format!("transform({})", values.join(","))
        }
        DisplayItem::PopClip => "pop".to_owned(),
        DisplayItem::PushGroup(group) => {
            let mut parts = Vec::new();
            if group.opacity < 1.0 {
                parts.push(number(group.opacity));
            }
            if group.blend != BlendMode::Normal {
                parts.push(format!("{:?}", group.blend).to_ascii_lowercase());
            }
            if group.mask.is_some() {
                parts.push("mask".to_owned());
            }
            if parts.is_empty() {
                "group".to_owned()
            } else {
                format!("group({})", parts.join(","))
            }
        }
        DisplayItem::PopGroup => "/group".to_owned(),
        DisplayItem::Backdrop(_) => "backdrop".to_owned(),
    }
}

/// Checks `layout-mask`: comma-separated `x y value` triples, each the mask value from 0 to
/// 255 (within 10) of the box's first masked group at `(x, y)` from its border box's corner.
fn check_mask(painted: &[DisplayItem], rect: Rect, expected: &str) -> Result<(), String> {
    let mask = painted
        .iter()
        .find_map(|item| match item {
            DisplayItem::PushGroup(group) => group.mask.as_ref(),
            _ => None,
        })
        .ok_or("the box paints no mask")?;
    let ImageData::Pixels { samples, gray, .. } = &mask.image.data else {
        return Err("the mask is not a raster".into());
    };
    let (width, height) = (mask.image.width, mask.image.height);
    for triple in expected.trim().trim_matches('"').split(',') {
        let numbers: Vec<f32> = triple
            .split_whitespace()
            .map(|n| n.parse().map_err(|_| format!("invalid number `{n}`")))
            .collect::<Result<_, _>>()?;
        let [x, y, wanted] = numbers[..] else {
            return Err(format!("expected `x y value`, got `{triple}`"));
        };
        let px = ((rect.x + x - mask.rect.x) / mask.rect.width * width as f32).floor();
        let py = ((rect.y + y - mask.rect.y) / mask.rect.height * height as f32).floor();
        let actual = if px < 0.0 || py < 0.0 || px >= width as f32 || py >= height as f32 {
            0.0
        } else {
            let index = (py as u32 * width + px as u32) as usize;
            f32::from(samples[if *gray { index } else { index * 3 }])
        };
        if (actual - wanted).abs() > 10.0 {
            return Err(format!("expected mask {wanted} at {x} {y}, got {actual}"));
        }
    }
    Ok(())
}

/// Whether the wanted tokens appear among `tokens` in order. A wanted token without
/// parentheses matches its kind whatever the details; `!kind` asserts that no item of that
/// kind is painted at all, and `kind*n` that exactly `n` are.
fn paint_matches(tokens: &[String], wanted: &str) -> bool {
    let mut rest = tokens.iter();
    for want in wanted.split_whitespace() {
        if let Some((kind, count)) = want.split_once('*') {
            let found = tokens
                .iter()
                .filter(|t| t.split('(').next() == Some(kind))
                .count();
            if count.parse() != Ok(found) {
                return false;
            }
            continue;
        }
        if let Some(kind) = want.strip_prefix('!') {
            if tokens
                .iter()
                .any(|t| t == kind || t.split('(').next() == Some(kind))
            {
                return false;
            }
            continue;
        }
        let matches = |token: &String| {
            if want.contains('(') {
                token == want
            } else {
                token.split('(').next() == Some(want)
            }
        };
        if !rest.any(matches) {
            return false;
        }
    }
    true
}

/// Checks a page metric: `page`, `pages`, `page-lines`, `page-x`, `page-y`, `page-count`
/// or `page-sizes`.
fn check_pages(
    pages: &[LaidPage],
    (links, bookmarks): &(Vec<Vec<Link>>, Vec<Bookmark>),
    element: NodeId,
    metric: &str,
    expected: &str,
) -> Result<(), String> {
    // The page number, line count and page position of the element's first box on each
    // page it is on.
    let mut found: Vec<(usize, usize, Rect)> = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let Some(root) = &page.root else {
            continue;
        };
        if let Some((_, rect, fragment)) = root
            .element_boxes()
            .into_iter()
            .find(|(node, ..)| *node == element)
        {
            let rect = rect.translated(page.area.x, page.area.y);
            found.push((index + 1, fragment.line_count(), rect));
        }
    }
    let number = |value: f32| format!("{}", (value * 100.0).round() / 100.0);
    // A value for every page, separated by spaces.
    let each =
        |value: &dyn Fn(&LaidPage) -> String| pages.iter().map(value).collect::<Vec<_>>().join(" ");
    let actual = match metric {
        "page" => found
            .first()
            .map_or("none".into(), |(page, ..)| page.to_string()),
        "pages" => found.len().to_string(),
        "page-lines" => found
            .iter()
            .map(|(_, lines, _)| lines.to_string())
            .collect::<Vec<_>>()
            .join(" "),
        "page-x" => found
            .first()
            .map_or("none".into(), |(.., rect)| number(rect.x)),
        "page-y" => found
            .first()
            .map_or("none".into(), |(.., rect)| number(rect.y)),
        "page-count" => pages.len().to_string(),
        "page-sizes" => {
            each(&|page| format!("{}x{}", page.size.width.round(), page.size.height.round()))
        }
        "page-bleed" => each(&|page| number(page.style.used_bleed())),
        "page-orientation" => each(&|page| {
            match page.style.orientation {
                PageOrientation::Upright => "upright",
                PageOrientation::RotateLeft => "rotate-left",
                PageOrientation::RotateRight => "rotate-right",
            }
            .to_owned()
        }),
        "page-marks" => each(&|page| {
            match (page.style.marks.crop, page.style.marks.cross) {
                (false, false) => "none",
                (true, false) => "crop",
                (false, true) => "cross",
                (true, true) => "crop+cross",
            }
            .to_owned()
        }),
        "page-margin-text" => pages
            .iter()
            .map(|page| {
                page.margin_boxes
                    .iter()
                    .map(|b| format!("{}:{}", b.name.name(), b.fragment.text()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join(" | "),
        "page-link-areas" => links
            .iter()
            .map(|page| {
                page.iter()
                    .map(|link| {
                        let target = match &link.target {
                            LinkTarget::Uri(uri) => uri.clone(),
                            LinkTarget::Internal(d) => format!("#{}", d.page + 1),
                        };
                        let r = link.rect;
                        format!(
                            "{target}@{},{},{},{}",
                            r.x.round(),
                            r.y.round(),
                            r.width.round(),
                            r.height.round()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join(" | "),
        "page-links" => links
            .iter()
            .map(|page| {
                page.iter()
                    .map(|link| match &link.target {
                        LinkTarget::Uri(uri) => uri.clone(),
                        LinkTarget::Internal(d) => format!("#{}", d.page + 1),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join(" | "),
        "page-bookmarks" => bookmarks
            .iter()
            .map(|b| format!("{}:{}@{}", b.level, b.title, b.target.page + 1))
            .collect::<Vec<_>>()
            .join(", "),
        "page-margin-rects" => pages
            .iter()
            .map(|page| {
                page.margin_boxes
                    .iter()
                    .map(|b| {
                        let r = b.fragment.rect;
                        format!(
                            "{}:{},{},{},{}",
                            b.name.name(),
                            r.x.round(),
                            r.y.round(),
                            r.width.round(),
                            r.height.round()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join(" | "),
        "page-margin-safety" => each(&|page| {
            match page.style.margin_safety {
                MarginSafety::None => "none",
                MarginSafety::Clamp => "clamp",
                MarginSafety::Add => "add",
            }
            .to_owned()
        }),
        other => return Err(format!("unknown page metric `{other}`")),
    };
    let expected = expected.trim().trim_end_matches("px");
    let wanted = expected.trim().trim_matches('"');
    if actual == wanted {
        Ok(())
    } else {
        Err(format!("expected `{wanted}`, got `{actual}`"))
    }
}

/// The text of the fixture's expectation scripts.
fn expectations(document: &Document) -> String {
    document
        .descendants(document.root())
        .filter(|&n| {
            document.element(n).is_some_and(|e| {
                e.name.ns == ns!(html)
                    && e.name.local == local_name!("script")
                    && e.attr("type") == Some("text/x-rustyprint-expect")
            })
        })
        .map(|n| document.text_content(n))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Block {
    selector: String,
    declarations: Vec<(String, String)>,
}

/// Parses `selector { property: value; ... }` blocks and `warning:` lines.
fn parse_expectations(text: &str) -> Result<(Vec<Block>, Vec<String>), Failed> {
    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        if let Some(line) = rest.strip_prefix("warning:") {
            let end = line.find('\n').unwrap_or(line.len());
            warnings.push(line[..end].trim().to_owned());
            rest = &line[end..];
            continue;
        }
        let open = rest.find('{').ok_or("expectation without `{`")?;
        let close = rest[open..].find('}').ok_or("expectation without `}`")? + open;
        let selector = rest[..open].trim().to_owned();
        let declarations = rest[open + 1..close]
            .split(';')
            .filter(|d| !d.trim().is_empty())
            .map(|d| {
                let (name, value) = d
                    .split_once(':')
                    .ok_or_else(|| format!("invalid expectation `{}`", d.trim()))?;
                Ok((name.trim().to_owned(), value.trim().to_owned()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        blocks.push(Block {
            selector,
            declarations,
        });
        rest = &rest[close + 1..];
    }
    Ok((blocks, warnings))
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
