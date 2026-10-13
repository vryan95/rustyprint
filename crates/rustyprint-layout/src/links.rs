//! Links and bookmarks: the areas of `<a href>` elements, where `#fragment` links go, and the
//! document outline from headings.

use std::collections::HashMap;

use rustyprint_dom::{Document, NodeId, local_name, ns};

use crate::display::{Bookmark, Destination, Link, LinkTarget, Rect};
use crate::fragment::ElementBox;
use crate::paginate::LaidPage;

/// The links of each page, and the bookmarks of the document.
pub(crate) fn collect(document: &Document, pages: &[LaidPage]) -> (Vec<Vec<Link>>, Vec<Bookmark>) {
    // Every box on every page, in page coordinates. Margin boxes can hold links too, but
    // only the page area holds destinations.
    let mut boxes: Vec<Vec<(NodeId, Rect)>> = Vec::with_capacity(pages.len());
    // The content boxes of images with an image map, on each page.
    let mut mapped: Vec<Vec<(NodeId, Rect)>> = Vec::with_capacity(pages.len());
    let mut first: HashMap<NodeId, Destination> = HashMap::new();
    for (index, page) in pages.iter().enumerate() {
        let mut on_page = Vec::new();
        let mut images = Vec::new();
        let mut add = |node: NodeId, rect: Rect, element: ElementBox<'_>| {
            if let ElementBox::Block(block) = element
                && document
                    .element(node)
                    .is_some_and(|e| e.is_html(&local_name!("img")) && e.attr("usemap").is_some())
            {
                let content = block.content_box();
                let (dx, dy) = (content.x - block.rect.x, content.y - block.rect.y);
                images.push((
                    node,
                    Rect::new(rect.x + dx, rect.y + dy, content.width, content.height),
                ));
            }
            on_page.push((node, rect));
        };
        if let Some(root) = &page.root {
            for (node, rect, element) in root.element_boxes() {
                let rect = rect.translated(page.area.x, page.area.y);
                first.entry(node).or_insert(Destination {
                    page: index,
                    y: rect.y,
                });
                add(node, rect, element);
            }
        }
        for margin_box in &page.margin_boxes {
            for (node, rect, element) in margin_box.fragment.element_boxes() {
                add(node, rect, element);
            }
        }
        boxes.push(on_page);
        mapped.push(images);
    }

    let anchors = anchors(document);
    let base = document.base_url();
    let target = |href: &str| -> Option<LinkTarget> {
        let href = href.trim();
        if let Some(fragment) = href.strip_prefix('#') {
            let fragment = percent_decode(fragment);
            // An empty fragment and `#top` go to the top of the document.
            if fragment.is_empty()
                || (fragment.eq_ignore_ascii_case("top") && !anchors.contains_key("top"))
            {
                return Some(LinkTarget::Internal(Destination { page: 0, y: 0.0 }));
            }
            let node = anchors.get(fragment.as_str())?;
            return first.get(node).map(|d| LinkTarget::Internal(*d));
        }
        let scheme = href.split(':').next().unwrap_or("").to_ascii_lowercase();
        if href.is_empty() || scheme == "javascript" {
            return None;
        }
        Some(LinkTarget::Uri(match base {
            Some(base) => rustyprint_dom::resolve_url(base, href),
            None => href.to_owned(),
        }))
    };

    let mut link_of: HashMap<NodeId, Option<NodeId>> = HashMap::new();
    let links = boxes
        .iter()
        .zip(&mapped)
        .map(|(on_page, images)| {
            // The areas of each link on the page, in order of first appearance.
            let mut areas: Vec<(NodeId, Vec<Rect>)> = Vec::new();
            for &(node, rect) in on_page {
                if rect.width <= 0.0 || rect.height <= 0.0 {
                    continue;
                }
                let link = *link_of
                    .entry(node)
                    .or_insert_with(|| hyperlink(document, node));
                let Some(link) = link else {
                    continue;
                };
                match areas.iter_mut().find(|(n, _)| *n == link) {
                    Some((_, rects)) => rects.push(rect),
                    None => areas.push((link, vec![rect])),
                }
            }
            let mut links = Vec::new();
            for (link, rects) in areas {
                let href = document
                    .element(link)
                    .and_then(|e| e.attr("href"))
                    .unwrap_or_default();
                let Some(target) = target(href) else {
                    continue;
                };
                // Boxes of descendants mostly lie inside the link's own boxes.
                for (i, rect) in rects.iter().enumerate() {
                    let covered = rects.iter().enumerate().any(|(j, other)| {
                        j != i && contains(other, rect) && (j < i || !contains(rect, other))
                    });
                    if !covered {
                        links.push(Link {
                            rect: *rect,
                            target: target.clone(),
                        });
                    }
                }
            }
            // Image map areas come last, over the image's own link. The first area in tree
            // order wins where areas overlap, and viewers pick the last annotation, so the
            // areas go in reverse.
            for &(image, content) in images {
                for (href, rect) in map_areas(document, image, content).into_iter().rev() {
                    if let Some(target) = target(&href) {
                        links.push(Link { rect, target });
                    }
                }
            }
            links
        })
        .collect();

    let bookmarks = document
        .descendants(document.root())
        .filter_map(|node| {
            let element = document.element(node)?;
            if element.name.ns != ns!(html) {
                return None;
            }
            let level = match element.name.local {
                local_name!("h1") => 1,
                local_name!("h2") => 2,
                local_name!("h3") => 3,
                local_name!("h4") => 4,
                local_name!("h5") => 5,
                local_name!("h6") => 6,
                _ => return None,
            };
            let target = *first.get(&node)?;
            let title = document
                .text_content(node)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            (!title.is_empty()).then_some(Bookmark {
                level,
                title,
                target,
            })
        })
        .collect();
    (links, bookmarks)
}

/// The links of the image map an `<img usemap>` uses, in tree order: each `<area href>`'s
/// `href` and the rectangle around its shape, in the coordinates of the image's content box
/// `content` (HTML § 4.8.14).
fn map_areas(document: &Document, image: NodeId, content: Rect) -> Vec<(String, Rect)> {
    let Some(name) = document
        .element(image)
        .and_then(|e| e.attr("usemap"))
        .and_then(|usemap| usemap.trim().strip_prefix('#'))
        .filter(|name| !name.is_empty())
    else {
        return Vec::new();
    };
    let Some(map) = document.descendants(document.root()).find(|&node| {
        document
            .element(node)
            .is_some_and(|e| e.is_html(&local_name!("map")) && (e.attr("name") == Some(name)))
    }) else {
        return Vec::new();
    };
    document
        .descendants(map)
        .filter_map(|node| {
            let area = document.element(node)?;
            if !area.is_html(&local_name!("area")) {
                return None;
            }
            let href = area.attr("href")?;
            let coords = parse_coords(area.attr("coords").unwrap_or(""));
            let shape = area
                .attr("shape")
                .unwrap_or("rect")
                .trim()
                .to_ascii_lowercase();
            let (x0, y0, x1, y1) = match &*shape {
                "default" => (0.0, 0.0, content.width, content.height),
                "circle" | "circ" => {
                    let [x, y, r] = <[f32; 3]>::try_from(coords.get(..3)?).ok()?;
                    if r <= 0.0 {
                        return None;
                    }
                    (x - r, y - r, x + r, y + r)
                }
                "poly" | "polygon" => {
                    let points = coords.as_chunks::<2>().0;
                    if points.len() < 3 {
                        return None;
                    }
                    let xs = points.iter().map(|[x, _]| *x);
                    let ys = points.iter().map(|[_, y]| *y);
                    (
                        xs.clone().fold(f32::INFINITY, f32::min),
                        ys.clone().fold(f32::INFINITY, f32::min),
                        xs.fold(f32::NEG_INFINITY, f32::max),
                        ys.fold(f32::NEG_INFINITY, f32::max),
                    )
                }
                "rect" | "rectangle" => {
                    let [a, b, c, d] = <[f32; 4]>::try_from(coords.get(..4)?).ok()?;
                    (a.min(c), b.min(d), a.max(c), b.max(d))
                }
                _ => return None,
            };
            // Clipped to the image.
            let (x0, y0) = (x0.max(0.0), y0.max(0.0));
            let (x1, y1) = (x1.min(content.width), y1.min(content.height));
            (x1 > x0 && y1 > y0).then(|| {
                let rect = Rect::new(content.x + x0, content.y + y0, x1 - x0, y1 - y0);
                (href.to_owned(), rect)
            })
        })
        .collect()
}

/// The numbers of a `coords` attribute: HTML's rules for a list of floating-point numbers,
/// which split at commas, spaces and other separators and read each number's leading digits.
fn parse_coords(value: &str) -> Vec<f32> {
    value
        .split(|c: char| c == ',' || c == ';' || c.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let end = part
                .char_indices()
                .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || (i == 0 && c == '-')))
                .map_or(part.len(), |(i, _)| i);
            part[..end].parse().unwrap_or(0.0)
        })
        .collect()
}

/// The page, from one, that each element in the page areas starts on.
pub(crate) fn first_pages(pages: &[LaidPage]) -> HashMap<NodeId, usize> {
    let mut first = HashMap::new();
    for (index, page) in pages.iter().enumerate() {
        if let Some(root) = &page.root {
            for (node, ..) in root.element_boxes() {
                first.entry(node).or_insert(index + 1);
            }
        }
    }
    first
}

/// The `<a>` or `<area>` element with an `href` that a node is inside, if any.
fn hyperlink(document: &Document, node: NodeId) -> Option<NodeId> {
    std::iter::once(node)
        .chain(document.ancestors(node))
        .find(|&n| {
            document.element(n).is_some_and(|e| {
                e.name.ns == ns!(html)
                    && matches!(e.name.local, local_name!("a") | local_name!("area"))
                    && e.attr("href").is_some()
            })
        })
}

/// The elements `#fragment` links can go to: the first element with each `id`, then
/// `<a name>` (HTML § 7.4.6.4).
pub(crate) fn anchors(document: &Document) -> HashMap<String, NodeId> {
    let mut anchors = HashMap::new();
    let elements: Vec<_> = document
        .descendants(document.root())
        .filter_map(|n| document.element(n).map(|e| (n, e)))
        .collect();
    for &(node, element) in &elements {
        if let Some(id) = element.attr("id").filter(|id| !id.is_empty()) {
            anchors.entry(id.to_owned()).or_insert(node);
        }
    }
    for &(node, element) in &elements {
        if element.is_html(&local_name!("a"))
            && let Some(name) = element.attr("name").filter(|name| !name.is_empty())
        {
            anchors.entry(name.to_owned()).or_insert(node);
        }
    }
    anchors
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    const SLACK: f32 = 0.01;
    inner.x >= outer.x - SLACK
        && inner.y >= outer.y - SLACK
        && inner.right() <= outer.right() + SLACK
        && inner.bottom() <= outer.bottom() + SLACK
}

/// Decodes `%XX` escapes as UTF-8, leaving invalid ones as they are.
pub(crate) fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_fragments() {
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a%2"), "a%2");
    }
}
