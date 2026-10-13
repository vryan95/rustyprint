//! HTML parsing into an arena DOM.
//!
//! [`Document::parse`] runs the HTML5 parsing algorithm with scripting disabled, so `<noscript>`
//! content is parsed as markup. The resulting tree is read-only.

pub mod budget;
pub mod forms;
mod sink;

use std::iter;

pub use html5ever::{LocalName, Namespace, Prefix, QualName, local_name, namespace_url, ns};

/// Index of a node in its [`Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

/// Whether the document triggered quirks mode, which changes some CSS behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuirksMode {
    NoQuirks,
    LimitedQuirks,
    Quirks,
}

/// A parsed HTML document.
#[derive(Debug, Clone)]
pub struct Document {
    nodes: Vec<Node>,
    quirks_mode: QuirksMode,
}

/// A node and its links to the rest of the tree.
#[derive(Debug, Clone)]
pub struct Node {
    parent: Option<NodeId>,
    previous_sibling: Option<NodeId>,
    next_sibling: Option<NodeId>,
    first_child: Option<NodeId>,
    last_child: Option<NodeId>,
    data: NodeData,
}

#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    Doctype { name: String },
    Text(String),
    Comment(String),
    Element(Element),
    ProcessingInstruction { target: String, data: String },
}

#[derive(Debug, Clone)]
pub struct Element {
    pub name: QualName,
    pub attrs: Vec<Attribute>,
    /// For `<template>`: the document fragment holding its contents, which are not children of
    /// the element.
    pub template_contents: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: QualName,
    pub value: String,
}

impl Document {
    /// Parses an HTML document.
    pub fn parse(html: &str) -> Self {
        sink::parse(html)
    }

    /// The document node.
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    pub fn quirks_mode(&self) -> QuirksMode {
        self.quirks_mode
    }

    /// How many nodes the document holds, including the document node, text and comments.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// How deeply elements nest: 1 for the root element alone, 0 without elements.
    pub fn element_depth(&self) -> usize {
        let mut deepest = 0;
        let mut stack = vec![(self.root(), 0)];
        while let Some((id, depth)) = stack.pop() {
            let depth = depth + usize::from(self.element(id).is_some());
            deepest = deepest.max(depth);
            stack.extend(self.children(id).map(|child| (child, depth)));
        }
        deepest
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    /// The node's element data, if it is an element.
    pub fn element(&self, id: NodeId) -> Option<&Element> {
        match &self.node(id).data {
            NodeData::Element(element) => Some(element),
            _ => None,
        }
    }

    /// The root element, normally `<html>`.
    pub fn document_element(&self) -> Option<NodeId> {
        self.children(self.root())
            .find(|&id| self.element(id).is_some())
    }

    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        iter::successors(self.node(id).first_child, |&child| {
            self.node(child).next_sibling
        })
    }

    /// Ancestors, starting with the parent.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        iter::successors(self.node(id).parent, |&node| self.node(node).parent)
    }

    /// The node and its descendants in tree order.
    pub fn descendants(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        iter::successors(Some(id), move |&node| {
            if let Some(child) = self.node(node).first_child {
                return Some(child);
            }
            iter::once(node)
                .chain(self.ancestors(node))
                .take_while(|&n| n != id)
                .find_map(|n| self.node(n).next_sibling)
        })
    }

    /// Element children only.
    pub fn element_children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.children(id)
            .filter(|&child| self.element(child).is_some())
    }

    pub fn previous_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        iter::successors(self.node(id).previous_sibling, |&n| {
            self.node(n).previous_sibling
        })
        .find(|&n| self.element(n).is_some())
    }

    pub fn next_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        iter::successors(self.node(id).next_sibling, |&n| self.node(n).next_sibling)
            .find(|&n| self.element(n).is_some())
    }

    /// The parent, if it is an element.
    pub fn parent_element(&self, id: NodeId) -> Option<NodeId> {
        self.node(id)
            .parent
            .filter(|&parent| self.element(parent).is_some())
    }

    /// The concatenated text of the node and its descendants.
    pub fn text_content(&self, id: NodeId) -> String {
        self.descendants(id)
            .filter_map(|n| match &self.node(n).data {
                NodeData::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The first element in tree order with the given local name in the HTML namespace.
    pub fn find_html(&self, local: &LocalName) -> Option<NodeId> {
        self.descendants(self.root())
            .find(|&n| self.element(n).is_some_and(|e| e.is_html(local)))
    }

    /// The document's base URL: the `href` of its first `<base>` element that has one.
    pub fn base_url(&self) -> Option<&str> {
        self.descendants(self.root()).find_map(|n| {
            self.element(n)
                .filter(|e| e.is_html(&local_name!("base")))
                .and_then(|e| e.attr("href"))
        })
    }

    /// `url` resolved against the document's base URL, if it has one.
    pub fn resolve_url(&self, url: &str) -> String {
        match self.base_url() {
            Some(base) => resolve_url(base, url),
            None => url.to_owned(),
        }
    }
}

/// Resolves `url` against `base`. Absolute URLs, `data:` URLs and fragments, which refer to
/// the document itself, stay as they are; `.` and `..` segments are removed.
pub fn resolve_url(base: &str, url: &str) -> String {
    let url = url.trim();
    let base = base.trim();
    if url.is_empty() || url.starts_with('#') || scheme(url).is_some() {
        return url.to_owned();
    }
    // The base's scheme and authority, such as `https://example.com`, and its path.
    let (origin, path) = match scheme(base) {
        Some(scheme) => {
            let rest = &base[scheme.len() + 1..];
            match rest.strip_prefix("//") {
                Some(after) => {
                    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
                    (&base[..scheme.len() + 3 + end], &after[end..])
                }
                None => (&base[..scheme.len() + 1], rest),
            }
        }
        None => ("", base),
    };
    if let Some(network) = url.strip_prefix("//") {
        return match scheme(base) {
            Some(scheme) => format!("{scheme}://{network}"),
            None => url.to_owned(),
        };
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    let joined = if url.starts_with('/') {
        url.to_owned()
    } else {
        // The directory of the base path, then the URL.
        let directory = match path.rfind('/') {
            Some(slash) => &path[..=slash],
            None if origin.contains("//") => "/",
            None => "",
        };
        format!("{directory}{url}")
    };
    format!("{origin}{}", remove_dot_segments(&joined))
}

/// The scheme of an absolute URL, such as `https` or `data`.
fn scheme(url: &str) -> Option<&str> {
    let colon = url.find(':')?;
    let scheme = &url[..colon];
    let valid = scheme.len() > 1
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some(scheme)
}

/// Removes `.` and `..` segments from a path, keeping its query and fragment.
fn remove_dot_segments(path: &str) -> String {
    let end = path.find(['?', '#']).unwrap_or(path.len());
    let (path, rest) = path.split_at(end);
    let absolute = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    let parts: Vec<&str> = path.split('/').collect();
    for (index, segment) in parts.iter().enumerate() {
        let last = index == parts.len() - 1;
        match *segment {
            "." => {
                if last {
                    segments.push("");
                }
            }
            ".." => {
                if segments.len() > usize::from(absolute) {
                    segments.pop();
                }
                if last {
                    segments.push("");
                }
            }
            segment => segments.push(segment),
        }
    }
    format!("{}{rest}", segments.join("/"))
}

impl Node {
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn previous_sibling(&self) -> Option<NodeId> {
        self.previous_sibling
    }

    pub fn next_sibling(&self) -> Option<NodeId> {
        self.next_sibling
    }

    pub fn first_child(&self) -> Option<NodeId> {
        self.first_child
    }

    pub fn last_child(&self) -> Option<NodeId> {
        self.last_child
    }

    pub fn data(&self) -> &NodeData {
        &self.data
    }
}

impl Element {
    /// Whether this is an HTML element with the given local name.
    pub fn is_html(&self, local: &LocalName) -> bool {
        self.name.ns == ns!(html) && self.name.local == *local
    }

    /// The value of the attribute with this local name and no namespace.
    pub fn attr(&self, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name.ns == ns!() && &*a.name.local == local)
            .map(|a| a.value.as_str())
    }

    pub fn has_attr(&self, local: &str) -> bool {
        self.attr(local).is_some()
    }

    pub fn id(&self) -> Option<&str> {
        self.attr("id")
    }

    /// The classes in the `class` attribute.
    pub fn classes(&self) -> impl Iterator<Item = &str> {
        self.attr("class").unwrap_or("").split_ascii_whitespace()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_urls() {
        assert_eq!(resolve_url("assets/", "a.png"), "assets/a.png");
        assert_eq!(resolve_url("assets/page.html", "a.png"), "assets/a.png");
        assert_eq!(resolve_url("assets/sub/", "../a.png"), "assets/a.png");
        assert_eq!(resolve_url("assets/", "#top"), "#top");
        assert_eq!(resolve_url("assets/", "data:,x"), "data:,x");
        assert_eq!(
            resolve_url("https://example.com/docs/guide.html?x", "img/a.png"),
            "https://example.com/docs/img/a.png"
        );
        assert_eq!(
            resolve_url("https://example.com", "a.png"),
            "https://example.com/a.png"
        );
        assert_eq!(
            resolve_url("https://example.com/docs/", "/root.css"),
            "https://example.com/root.css"
        );
        assert_eq!(
            resolve_url("https://example.com/a/", "//cdn.example.com/x.js"),
            "https://cdn.example.com/x.js"
        );
        assert_eq!(
            resolve_url("https://example.com/a/b/", "../../../c"),
            "https://example.com/c"
        );
        let document = Document::parse(
            "<base target=_top><base href='https://example.com/docs/'><base href='ignored/'>",
        );
        assert_eq!(document.base_url(), Some("https://example.com/docs/"));
        assert_eq!(
            document.resolve_url("a.html"),
            "https://example.com/docs/a.html"
        );
    }

    fn names(doc: &Document, ids: impl Iterator<Item = NodeId>) -> Vec<String> {
        ids.filter_map(|id| doc.element(id))
            .map(|e| e.name.local.to_string())
            .collect()
    }

    #[test]
    fn builds_implied_structure() {
        let doc = Document::parse("<title>T</title><p>Hello");
        let html = doc.document_element().unwrap();
        assert_eq!(names(&doc, doc.children(html)), ["head", "body"]);
        let p = doc.find_html(&local_name!("p")).unwrap();
        assert_eq!(doc.text_content(p), "Hello");
        assert_eq!(doc.quirks_mode(), QuirksMode::Quirks);
    }

    #[test]
    fn doctype_selects_no_quirks_mode() {
        let doc = Document::parse("<!DOCTYPE html><p>x");
        assert_eq!(doc.quirks_mode(), QuirksMode::NoQuirks);
    }

    #[test]
    fn noscript_content_is_markup() {
        let doc = Document::parse("<body><noscript><p>Fallback</p></noscript>");
        let noscript = doc.find_html(&local_name!("noscript")).unwrap();
        assert_eq!(names(&doc, doc.children(noscript)), ["p"]);
    }

    #[test]
    fn misnested_tags_are_repaired() {
        let doc = Document::parse("<p><b>bold<i>both</b>italic</i></p>");
        let p = doc.find_html(&local_name!("p")).unwrap();
        assert_eq!(doc.text_content(p), "boldbothitalic");
        assert_eq!(names(&doc, doc.descendants(p)), ["p", "b", "i", "i"]);
    }

    #[test]
    fn attributes_and_classes() {
        let doc = Document::parse(r#"<div id=main class=" a  b " data-x="1">"#);
        let div = doc
            .element(doc.find_html(&local_name!("div")).unwrap())
            .unwrap();
        assert_eq!(div.id(), Some("main"));
        assert_eq!(div.classes().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(div.attr("data-x"), Some("1"));
        assert!(!div.has_attr("title"));
    }

    #[test]
    fn descendants_stay_in_subtree() {
        let doc = Document::parse("<ul><li>a<li>b</ul><p>after");
        let ul = doc.find_html(&local_name!("ul")).unwrap();
        assert_eq!(names(&doc, doc.descendants(ul)), ["ul", "li", "li"]);
    }

    #[test]
    fn sibling_and_parent_navigation() {
        let doc = Document::parse("<ul><li>a</li> text <li>b</li></ul>");
        let ul = doc.find_html(&local_name!("ul")).unwrap();
        let items: Vec<_> = doc.element_children(ul).collect();
        assert_eq!(items.len(), 2);
        assert_eq!(doc.next_element_sibling(items[0]), Some(items[1]));
        assert_eq!(doc.previous_element_sibling(items[1]), Some(items[0]));
        assert_eq!(doc.parent_element(items[0]), Some(ul));
    }

    #[test]
    fn template_contents_are_not_children() {
        let doc = Document::parse("<template><p>inside</p></template>");
        let template = doc.find_html(&local_name!("template")).unwrap();
        assert_eq!(doc.children(template).count(), 0);
        let contents = doc.element(template).unwrap().template_contents.unwrap();
        assert_eq!(names(&doc, doc.children(contents)), ["p"]);
    }
}
