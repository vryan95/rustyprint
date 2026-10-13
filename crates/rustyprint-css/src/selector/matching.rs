use rustyprint_dom::{
    Document, Element, LocalName, NodeData, NodeId, QuirksMode, forms, local_name, ns,
};
use unicode_bidi::{BidiClass, bidi_class};

use super::{
    AttributeCase, AttributeOperator, AttributeSelector, Combinator, Compound, Direction,
    NamespaceConstraint, Nth, PseudoClass, Selector, SelectorList, Simple,
};

/// What selectors are matched against.
#[derive(Debug, Clone, Copy)]
pub struct MatchContext<'a> {
    pub document: &'a Document,
    /// The element `:scope` matches. Without one, `:scope` matches the root element.
    pub scope: Option<NodeId>,
}

impl<'a> MatchContext<'a> {
    pub fn new(document: &'a Document) -> Self {
        Self {
            document,
            scope: None,
        }
    }

    fn element(&self, id: NodeId) -> &'a Element {
        self.document.element(id).expect("selectors match elements")
    }

    fn quirks(&self) -> bool {
        self.document.quirks_mode() == QuirksMode::Quirks
    }
}

impl SelectorList {
    /// Whether any selector in the list matches `element`, ignoring pseudo-elements.
    pub fn matches(&self, ctx: &MatchContext, element: NodeId) -> bool {
        self.0.iter().any(|selector| selector.matches(ctx, element))
    }
}

impl Selector {
    /// Whether `element` matches the selector's compounds. A selector with a pseudo-element
    /// matches when its originating element does; callers check [`Selector::pseudo_element`].
    pub fn matches(&self, ctx: &MatchContext, element: NodeId) -> bool {
        self.match_from(ctx, self.compounds.len() - 1, element, None)
    }

    /// Matches compounds `0..=index` with `compounds[index]` at `element`. With an `anchor`,
    /// the leftmost compound must be related to the anchor element by the anchor combinator,
    /// as relative selectors in `:has()` require.
    fn match_from(
        &self,
        ctx: &MatchContext,
        index: usize,
        element: NodeId,
        anchor: Option<(Combinator, NodeId)>,
    ) -> bool {
        if !self.compounds[index].matches(ctx, element) {
            return false;
        }
        if index == 0 {
            return anchor
                .is_none_or(|(combinator, subject)| related(ctx, combinator, subject, element));
        }
        let doc = ctx.document;
        let next = |candidate: NodeId| self.match_from(ctx, index - 1, candidate, anchor);
        match self.combinators[index - 1] {
            Combinator::Child => doc.parent_element(element).is_some_and(next),
            Combinator::Descendant => doc
                .ancestors(element)
                .filter(|&a| doc.element(a).is_some())
                .any(next),
            Combinator::NextSibling => doc.previous_element_sibling(element).is_some_and(next),
            Combinator::SubsequentSibling => {
                std::iter::successors(doc.previous_element_sibling(element), |&s| {
                    doc.previous_element_sibling(s)
                })
                .any(next)
            }
            Combinator::Column => cell_columns(doc, element).into_iter().any(next),
        }
    }
}

/// Whether `element` is related to `subject` by `combinator`, with `subject` on the left.
fn related(ctx: &MatchContext, combinator: Combinator, subject: NodeId, element: NodeId) -> bool {
    let doc = ctx.document;
    match combinator {
        Combinator::Child => doc.node(element).parent() == Some(subject),
        Combinator::Descendant => doc.ancestors(element).any(|a| a == subject),
        Combinator::NextSibling => doc.previous_element_sibling(element) == Some(subject),
        Combinator::SubsequentSibling => {
            std::iter::successors(doc.previous_element_sibling(element), |&s| {
                doc.previous_element_sibling(s)
            })
            .any(|s| s == subject)
        }
        Combinator::Column => cell_columns(doc, element).contains(&subject),
    }
}

/// The value of a `span`, `colspan` or `rowspan` attribute: its leading digits.
fn span_attribute(element: &Element, name: &str) -> Option<u32> {
    let value = element.attr(name)?.trim_start();
    let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The `col` elements, or `colgroup` elements without any, of the columns a table cell is
/// in (HTML § 4.9.12), which `||` relates it to (Selectors 4 § 16.1).
fn cell_columns(doc: &Document, cell: NodeId) -> Vec<NodeId> {
    let html = |node: NodeId, names: &[LocalName]| {
        doc.element(node)
            .is_some_and(|e| e.name.ns == ns!(html) && names.contains(&e.name.local))
    };
    if !html(cell, &[local_name!("td"), local_name!("th")]) {
        return Vec::new();
    }
    let Some(row) = doc
        .parent_element(cell)
        .filter(|&r| html(r, &[local_name!("tr")]))
    else {
        return Vec::new();
    };
    let Some(section) = doc.parent_element(row) else {
        return Vec::new();
    };
    let table = if html(section, &[local_name!("table")]) {
        section
    } else if html(
        section,
        &[
            local_name!("thead"),
            local_name!("tbody"),
            local_name!("tfoot"),
        ],
    ) && let Some(table) = doc
        .parent_element(section)
        .filter(|&t| html(t, &[local_name!("table")]))
    {
        table
    } else {
        return Vec::new();
    };

    // The cell's columns, skipping slots that row spans above cover.
    let mut covered: Vec<u32> = Vec::new();
    let mut found = None;
    let rows: Vec<NodeId> = doc
        .element_children(section)
        .filter(|&r| html(r, &[local_name!("tr")]))
        .collect();
    'rows: for (index, &r) in rows.iter().enumerate() {
        let left = (rows.len() - index) as u32;
        let mut column = 0;
        for c in doc.element_children(r) {
            if !html(c, &[local_name!("td"), local_name!("th")]) {
                continue;
            }
            while covered.get(column).is_some_and(|&n| n > 0) {
                column += 1;
            }
            let element = doc.element(c).expect("element");
            let colspan = span_attribute(element, "colspan")
                .unwrap_or(1)
                .clamp(1, 1000) as usize;
            let rowspan = match span_attribute(element, "rowspan") {
                Some(0) => left,
                Some(n) => n.min(left),
                None => 1,
            };
            if c == cell {
                found = Some(column..column + colspan);
                break 'rows;
            }
            if covered.len() < column + colspan {
                covered.resize(column + colspan, 0);
            }
            for n in &mut covered[column..column + colspan] {
                *n = rowspan;
            }
            column += colspan;
        }
        for n in &mut covered {
            *n = n.saturating_sub(1);
        }
    }
    let Some(span) = found else {
        return Vec::new();
    };

    // The column elements, by column.
    let mut columns = Vec::new();
    for group in doc.element_children(table) {
        if !html(group, &[local_name!("colgroup")]) {
            continue;
        }
        let cols: Vec<NodeId> = doc
            .element_children(group)
            .filter(|&c| html(c, &[local_name!("col")]))
            .collect();
        let span_of = |node: NodeId| {
            let element = doc.element(node).expect("element");
            span_attribute(element, "span").unwrap_or(1).clamp(1, 1000) as usize
        };
        if cols.is_empty() {
            columns.extend(std::iter::repeat_n(group, span_of(group)));
        } else {
            for col in cols {
                columns.extend(std::iter::repeat_n(col, span_of(col)));
            }
        }
    }
    let mut out: Vec<NodeId> = columns
        .get(span.start..span.end.min(columns.len()))
        .unwrap_or_default()
        .to_vec();
    out.dedup();
    out
}

impl Compound {
    fn matches(&self, ctx: &MatchContext, element: NodeId) -> bool {
        self.0.iter().all(|simple| simple.matches(ctx, element))
    }
}

impl Simple {
    fn matches(&self, ctx: &MatchContext, id: NodeId) -> bool {
        let element = ctx.element(id);
        match self {
            Simple::Universal(ns) => namespace_matches(ns, &element.name.ns),
            Simple::Type { ns, name, lower } => {
                let local = if element.name.ns == ns!(html) {
                    lower
                } else {
                    name
                };
                namespace_matches(ns, &element.name.ns) && element.name.local == *local
            }
            Simple::Id(wanted) => element
                .id()
                .is_some_and(|id| equals(id, wanted, ctx.quirks())),
            Simple::Class(wanted) => element
                .classes()
                .any(|class| equals(class, wanted, ctx.quirks())),
            Simple::Attribute(selector) => attribute_matches(selector, element),
            Simple::PseudoClass(pseudo) => pseudo.matches(ctx, id),
        }
    }
}

fn equals(a: &str, b: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn namespace_matches(
    constraint: &NamespaceConstraint,
    namespace: &rustyprint_dom::Namespace,
) -> bool {
    match constraint {
        NamespaceConstraint::Any => true,
        NamespaceConstraint::None => *namespace == ns!(),
        NamespaceConstraint::Specific(wanted) => namespace == wanted,
    }
}

/// HTML attributes whose values selectors compare case-insensitively (HTML § 4.16.2).
const CASE_INSENSITIVE_ATTRIBUTES: &[&str] = &[
    "accept",
    "accept-charset",
    "align",
    "alink",
    "axis",
    "bgcolor",
    "charset",
    "checked",
    "clear",
    "codetype",
    "color",
    "compact",
    "declare",
    "defer",
    "dir",
    "direction",
    "disabled",
    "enctype",
    "face",
    "frame",
    "hreflang",
    "http-equiv",
    "lang",
    "language",
    "link",
    "media",
    "method",
    "multiple",
    "nohref",
    "noresize",
    "noshade",
    "nowrap",
    "readonly",
    "rel",
    "rev",
    "rules",
    "scope",
    "scrolling",
    "selected",
    "shape",
    "target",
    "text",
    "type",
    "valign",
    "valuetype",
    "vlink",
];

fn attribute_matches(selector: &AttributeSelector, element: &Element) -> bool {
    let is_html = element.name.ns == ns!(html);
    let name = if is_html {
        &selector.lower
    } else {
        &selector.name
    };
    element
        .attrs
        .iter()
        .filter(|attr| attr.name.local == *name && namespace_matches(&selector.ns, &attr.name.ns))
        .any(|attr| {
            let Some(operation) = &selector.operation else {
                return true;
            };
            let insensitive = match operation.case {
                AttributeCase::Insensitive => true,
                AttributeCase::Sensitive => false,
                AttributeCase::Default => {
                    is_html
                        && attr.name.ns == ns!()
                        && CASE_INSENSITIVE_ATTRIBUTES.contains(&&*attr.name.local)
                }
            };
            let (value, wanted) = if insensitive {
                (
                    attr.value.to_ascii_lowercase(),
                    operation.value.to_ascii_lowercase(),
                )
            } else {
                (attr.value.clone(), operation.value.clone())
            };
            match operation.operator {
                AttributeOperator::Equals => value == wanted,
                AttributeOperator::Includes => {
                    !wanted.is_empty() && value.split_ascii_whitespace().any(|word| word == wanted)
                }
                AttributeOperator::DashMatch => {
                    value == wanted
                        || value
                            .strip_prefix(&wanted)
                            .is_some_and(|rest| rest.starts_with('-'))
                }
                AttributeOperator::Prefix => !wanted.is_empty() && value.starts_with(&wanted),
                AttributeOperator::Suffix => !wanted.is_empty() && value.ends_with(&wanted),
                AttributeOperator::Substring => !wanted.is_empty() && value.contains(&wanted),
            }
        })
}

impl PseudoClass {
    fn matches(&self, ctx: &MatchContext, id: NodeId) -> bool {
        let doc = ctx.document;
        let element = ctx.element(id);
        let is_html = |names: &[LocalName]| {
            element.name.ns == ns!(html) && names.contains(&element.name.local)
        };
        match self {
            PseudoClass::Root => doc.document_element() == Some(id),
            PseudoClass::Empty => doc.children(id).all(|child| match doc.node(child).data() {
                NodeData::Element(_) => false,
                NodeData::Text(text) => text.is_empty(),
                _ => true,
            }),
            PseudoClass::FirstChild => doc.previous_element_sibling(id).is_none(),
            PseudoClass::LastChild => doc.next_element_sibling(id).is_none(),
            PseudoClass::OnlyChild => {
                doc.previous_element_sibling(id).is_none() && doc.next_element_sibling(id).is_none()
            }
            PseudoClass::FirstOfType => !siblings_before(ctx, id).any(|s| same_type(ctx, s, id)),
            PseudoClass::LastOfType => !siblings_after(ctx, id).any(|s| same_type(ctx, s, id)),
            PseudoClass::OnlyOfType => {
                !siblings_before(ctx, id).any(|s| same_type(ctx, s, id))
                    && !siblings_after(ctx, id).any(|s| same_type(ctx, s, id))
            }
            PseudoClass::NthChild(nth, of) => {
                nth_index(ctx, id, *nth, of.as_ref(), siblings_before(ctx, id))
            }
            PseudoClass::NthLastChild(nth, of) => {
                nth_index(ctx, id, *nth, of.as_ref(), siblings_after(ctx, id))
            }
            PseudoClass::NthOfType(nth) => {
                let index = 1 + siblings_before(ctx, id)
                    .filter(|&s| same_type(ctx, s, id))
                    .count();
                nth.matches(index as i32)
            }
            PseudoClass::NthLastOfType(nth) => {
                let index = 1 + siblings_after(ctx, id)
                    .filter(|&s| same_type(ctx, s, id))
                    .count();
                nth.matches(index as i32)
            }
            PseudoClass::Is(list) | PseudoClass::Where(list) => list.matches(ctx, id),
            PseudoClass::Not(list) => !list.matches(ctx, id),
            PseudoClass::Has(relative) => relative.iter().any(|r| {
                has_candidates(ctx, id, r.combinator)
                    .into_iter()
                    .any(|candidate| {
                        r.selector.match_from(
                            ctx,
                            r.selector.compounds.len() - 1,
                            candidate,
                            Some((r.combinator, id)),
                        )
                    })
            }),
            PseudoClass::Scope => match ctx.scope {
                Some(scope) => scope == id,
                None => doc.document_element() == Some(id),
            },
            // Without scripting no custom element is ever defined.
            PseudoClass::Defined => {
                !(element.name.ns == ns!(html) && element.name.local.contains('-'))
            }
            PseudoClass::Heading(levels) => heading_level(element)
                .is_some_and(|level| levels.as_ref().is_none_or(|l| l.contains(&level))),
            PseudoClass::Lang(ranges) => {
                let tag = language(ctx, id);
                ranges.iter().any(|range| lang_matches(&tag, range))
            }
            PseudoClass::Dir(direction) => directionality(ctx, id) == *direction,
            PseudoClass::AnyLink => is_link(element),
            PseudoClass::LocalLink => {
                is_link(element) && element.attr("href").is_some_and(|h| h.starts_with('#'))
            }
            PseudoClass::Enabled => can_be_disabled(element) && !is_disabled(ctx, id),
            PseudoClass::Disabled => can_be_disabled(element) && is_disabled(ctx, id),
            PseudoClass::ReadWrite => is_read_write(ctx, id),
            PseudoClass::ReadOnly => !is_read_write(ctx, id),
            PseudoClass::PlaceholderShown => {
                let empty = if is_html(&[local_name!("textarea")]) {
                    doc.text_content(id).is_empty()
                } else {
                    is_text_input(element) && element.attr("value").is_none_or(str::is_empty)
                };
                element.has_attr("placeholder") && empty
            }
            PseudoClass::Checked => {
                forms::is_checked(doc, id)
                    || (is_html(&[local_name!("option")]) && forms::is_selected(doc, id))
            }
            PseudoClass::Required | PseudoClass::Optional => {
                is_html(&[
                    local_name!("input"),
                    local_name!("select"),
                    local_name!("textarea"),
                ]) && element.has_attr("required") == matches!(self, PseudoClass::Required)
            }
            PseudoClass::Open => {
                is_html(&[local_name!("details"), local_name!("dialog")])
                    && element.has_attr("open")
            }
            // Media never plays.
            PseudoClass::Paused => is_html(&[local_name!("audio"), local_name!("video")]),
            PseudoClass::Muted => {
                is_html(&[local_name!("audio"), local_name!("video")]) && element.has_attr("muted")
            }
            PseudoClass::Default => {
                let checkable = forms::input_type(doc, id).is_some_and(|t| {
                    matches!(t, forms::InputType::Checkbox | forms::InputType::Radio)
                });
                (checkable && element.has_attr("checked"))
                    || (is_html(&[local_name!("option")]) && element.has_attr("selected"))
                    || forms::is_default_button(doc, id)
            }
            PseudoClass::Indeterminate => forms::is_indeterminate(doc, id),
            PseudoClass::Valid | PseudoClass::Invalid => {
                let valid = if is_html(&[local_name!("form"), local_name!("fieldset")]) {
                    !forms::contains_invalid(doc, id)
                } else if forms::is_validated(doc, id) {
                    forms::is_valid(doc, id)
                } else {
                    return false;
                };
                valid == matches!(self, PseudoClass::Valid)
            }
            PseudoClass::InRange => forms::out_of_range(doc, id) == Some(false),
            PseudoClass::OutOfRange => forms::out_of_range(doc, id) == Some(true),
            PseudoClass::Never(_) => false,
        }
    }
}

fn siblings_before<'a>(ctx: &MatchContext<'a>, id: NodeId) -> impl Iterator<Item = NodeId> + 'a {
    let doc = ctx.document;
    std::iter::successors(doc.previous_element_sibling(id), move |&s| {
        doc.previous_element_sibling(s)
    })
}

fn siblings_after<'a>(ctx: &MatchContext<'a>, id: NodeId) -> impl Iterator<Item = NodeId> + 'a {
    let doc = ctx.document;
    std::iter::successors(doc.next_element_sibling(id), move |&s| {
        doc.next_element_sibling(s)
    })
}

fn same_type(ctx: &MatchContext, a: NodeId, b: NodeId) -> bool {
    ctx.element(a).name == ctx.element(b).name
}

/// Matches `:nth-child()` or `:nth-last-child()`, counting `siblings` (before or after).
fn nth_index(
    ctx: &MatchContext,
    id: NodeId,
    nth: Nth,
    of: Option<&SelectorList>,
    siblings: impl Iterator<Item = NodeId>,
) -> bool {
    match of {
        Some(list) => {
            list.matches(ctx, id)
                && nth.matches(1 + siblings.filter(|&s| list.matches(ctx, s)).count() as i32)
        }
        None => nth.matches(1 + siblings.count() as i32),
    }
}

/// Elements that could satisfy a `:has()` argument starting with `combinator`.
fn has_candidates(ctx: &MatchContext, subject: NodeId, combinator: Combinator) -> Vec<NodeId> {
    let doc = ctx.document;
    let elements = |root: NodeId| doc.descendants(root).filter(|&n| doc.element(n).is_some());
    match combinator {
        Combinator::Child | Combinator::Descendant => elements(subject).skip(1).collect(),
        Combinator::NextSibling | Combinator::SubsequentSibling => {
            siblings_after(ctx, subject).flat_map(elements).collect()
        }
        // The cells of the column's table.
        Combinator::Column => doc
            .ancestors(subject)
            .find(|&a| {
                doc.element(a)
                    .is_some_and(|e| e.is_html(&local_name!("table")))
            })
            .map(|table| {
                elements(table)
                    .filter(|&n| {
                        doc.element(n).is_some_and(|e| {
                            e.is_html(&local_name!("td")) || e.is_html(&local_name!("th"))
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn heading_level(element: &Element) -> Option<i32> {
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
    Some(level)
}

/// The element's language from the nearest `lang` or `xml:lang` attribute, or empty if unknown.
fn language(ctx: &MatchContext, id: NodeId) -> String {
    std::iter::once(id)
        .chain(ctx.document.ancestors(id))
        .filter_map(|n| ctx.document.element(n))
        .find_map(|e| {
            e.attrs
                .iter()
                .find(|a| {
                    a.name.local == local_name!("lang")
                        && (a.name.ns == ns!() || a.name.ns == ns!(xml))
                })
                .map(|a| a.value.clone())
        })
        .unwrap_or_default()
}

/// RFC 4647 extended filtering with an implicit wildcard, as `:lang()` uses.
fn lang_matches(tag: &str, range: &str) -> bool {
    if tag.is_empty() {
        return range.is_empty();
    }
    let range: Vec<&str> = range.split('-').collect();
    let tag: Vec<&str> = tag.split('-').collect();
    if range[0] != "*" && !range[0].eq_ignore_ascii_case(tag[0]) {
        return false;
    }
    let (mut r, mut t) = (1, 1);
    while r < range.len() {
        if range[r] == "*" {
            r += 1;
        } else if t >= tag.len() {
            return false;
        } else if range[r].eq_ignore_ascii_case(tag[t]) {
            r += 1;
            t += 1;
        } else if tag[t].len() == 1 {
            return false;
        } else {
            t += 1;
        }
    }
    true
}

/// The element's directionality (HTML § 3.2.6.4).
fn directionality(ctx: &MatchContext, id: NodeId) -> Direction {
    let element = ctx.element(id);
    let dir = element.attr("dir").map(str::to_ascii_lowercase);
    match dir.as_deref() {
        Some("ltr") => Direction::Ltr,
        Some("rtl") => Direction::Rtl,
        Some("auto") => auto_direction(ctx, id).unwrap_or(Direction::Ltr),
        _ if element.name.ns == ns!(html) && element.name.local == local_name!("bdi") => {
            auto_direction(ctx, id).unwrap_or(Direction::Ltr)
        }
        _ => match ctx.document.parent_element(id) {
            Some(parent) => directionality(ctx, parent),
            None => Direction::Ltr,
        },
    }
}

/// The direction of the first strong character in the element's text, skipping descendants
/// that set their own direction.
fn auto_direction(ctx: &MatchContext, id: NodeId) -> Option<Direction> {
    let doc = ctx.document;
    for child in doc.children(id) {
        let found = match doc.node(child).data() {
            NodeData::Text(text) => text.chars().find_map(|c| match bidi_class(c) {
                BidiClass::L => Some(Direction::Ltr),
                BidiClass::R | BidiClass::AL => Some(Direction::Rtl),
                _ => None,
            }),
            NodeData::Element(element) => {
                let skip = element.has_attr("dir")
                    || (element.name.ns == ns!(html)
                        && matches!(
                            element.name.local,
                            local_name!("bdi")
                                | local_name!("script")
                                | local_name!("style")
                                | local_name!("textarea")
                        ));
                if skip {
                    None
                } else {
                    auto_direction(ctx, child)
                }
            }
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn is_link(element: &Element) -> bool {
    element.name.ns == ns!(html)
        && matches!(
            element.name.local,
            local_name!("a") | local_name!("area") | local_name!("link")
        )
        && element.has_attr("href")
}

/// Whether an `<input>` takes typed text, so that `readonly` and `placeholder` apply.
fn is_text_input(element: &Element) -> bool {
    element.is_html(&local_name!("input")) && forms::InputType::of(element).is_textual()
}

fn can_be_disabled(element: &Element) -> bool {
    element.name.ns == ns!(html)
        && matches!(
            element.name.local,
            local_name!("button")
                | local_name!("input")
                | local_name!("select")
                | local_name!("textarea")
                | local_name!("optgroup")
                | local_name!("option")
                | local_name!("fieldset")
        )
}

/// Whether a form control is disabled (HTML § 4.10.18.5).
fn is_disabled(ctx: &MatchContext, id: NodeId) -> bool {
    forms::is_disabled(ctx.document, id)
}

fn is_read_write(ctx: &MatchContext, id: NodeId) -> bool {
    let element = ctx.element(id);
    if is_text_input(element) || element.is_html(&local_name!("textarea")) {
        return !element.has_attr("readonly") && !is_disabled(ctx, id);
    }
    // Editing hosts and their descendants.
    std::iter::once(id)
        .chain(ctx.document.ancestors(id))
        .filter_map(|n| ctx.document.element(n))
        .find_map(|e| e.attr("contenteditable"))
        .is_some_and(|value| !value.eq_ignore_ascii_case("false"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::Namespaces;

    /// IDs of elements matching `selector`, read from their `id` attributes.
    fn select(html: &str, selector: &str) -> Vec<String> {
        let doc = Document::parse(html);
        let list = SelectorList::parse(selector, &Namespaces::default()).unwrap();
        let ctx = MatchContext::new(&doc);
        doc.descendants(doc.root())
            .filter(|&n| doc.element(n).is_some() && list.matches(&ctx, n))
            .map(|n| doc.element(n).unwrap().id().unwrap_or("?").to_owned())
            .collect()
    }

    #[test]
    fn combinators() {
        let html =
            r#"<div id=a><p id=b><span id=c></span></p><p id=d></p><span id=e></span></div>"#;
        assert_eq!(select(html, "div span"), ["c", "e"]);
        assert_eq!(select(html, "div > span"), ["e"]);
        assert_eq!(select(html, "p + p"), ["d"]);
        assert_eq!(select(html, "#b ~ *"), ["d", "e"]);
        assert_eq!(select(html, "div p span"), ["c"]);
    }

    #[test]
    fn backtracks_through_descendants() {
        let html = r#"<div class=x><section><div><p id=p></p></div></section></div>"#;
        assert_eq!(select(html, ".x > section p"), ["p"]);
        assert_eq!(select(html, ".x > div p"), Vec::<String>::new());
    }

    #[test]
    fn type_selectors_ignore_case_for_html() {
        assert_eq!(select("<p id=p>", "P"), ["p"]);
    }

    #[test]
    fn attributes() {
        let html =
            r#"<a id=a href="https://x.org/doc.pdf" lang="en-US" class="btn primary" type=TEXT>"#;
        assert_eq!(select(html, "[href]"), ["a"]);
        assert_eq!(select(html, "[href^=https][href$='.pdf'][href*=x]"), ["a"]);
        assert_eq!(select(html, "[class~=primary]"), ["a"]);
        assert_eq!(select(html, "[lang|=en]"), ["a"]);
        assert_eq!(select(html, "[lang|=e]"), Vec::<String>::new());
        assert_eq!(
            select(html, "[type=text]"),
            ["a"],
            "type is case-insensitive in HTML"
        );
        assert_eq!(select(html, "[type=text s]"), Vec::<String>::new());
        assert_eq!(select(html, "[class='BTN PRIMARY' i]"), ["a"]);
        assert_eq!(select(html, "[HREF]"), ["a"]);
    }

    #[test]
    fn structural() {
        let html = r#"<ul id=u><li id=1></li><li id=2 class=x></li><li id=3></li><li id=4 class=x></li><b id=5></b></ul>"#;
        assert_eq!(select(html, "li:first-child"), ["1"]);
        assert_eq!(select(html, "li:last-of-type"), ["4"]);
        assert_eq!(select(html, "ul > :only-of-type"), ["5"]);
        assert_eq!(select(html, "li:nth-child(odd)"), ["1", "3"]);
        assert_eq!(select(html, "li:nth-last-child(2)"), ["4"]);
        assert_eq!(select(html, ":nth-child(2 of .x)"), ["4"]);
        assert_eq!(select(html, "li:nth-of-type(-n+2)"), ["1", "2"]);
        assert_eq!(select(html, "li:empty:not(.x)"), ["1", "3"]);
    }

    #[test]
    fn logical_and_has() {
        let html = r#"<section id=a><h2 id=h></h2><p id=p></p></section><section id=b><p id=q></p></section>"#;
        assert_eq!(select(html, "section:has(h2)"), ["a"]);
        assert_eq!(select(html, "section:has(> p)"), ["a", "b"]);
        assert_eq!(select(html, "h2:has(+ p)"), ["h"]);
        assert_eq!(select(html, "section:not(:has(h2))"), ["b"]);
        assert_eq!(select(html, ":is(h2, p):where(#p, #q)"), ["p", "q"]);
        assert_eq!(select(html, "section:has(h2 + p)"), ["a"]);
    }

    #[test]
    fn root_scope_and_links() {
        let html =
            r##"<html id=r><a id=a href="#top"></a><a id=b href="/x"></a><a id=c></a></html>"##;
        assert_eq!(select(html, ":root"), ["r"]);
        assert_eq!(select(html, ":scope"), ["r"]);
        assert_eq!(select(html, ":any-link"), ["a", "b"]);
        assert_eq!(select(html, ":link"), ["a", "b"]);
        assert_eq!(select(html, ":local-link"), ["a"]);
        assert_eq!(
            select(html, ":visited, :hover, :focus"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn language_and_direction() {
        let html = r#"<div id=a lang=de-CH><p id=b></p></div><div id=c dir=rtl><p id=d></p><p id=e dir=auto>abc</p></div><p id=f dir=auto>שלום</p>"#;
        assert_eq!(select(html, ":lang(de)"), ["a", "b"]);
        assert_eq!(select(html, ":lang('*-CH')"), ["a", "b"]);
        assert_eq!(select(html, ":lang(fr)"), Vec::<String>::new());
        assert_eq!(select(html, "div :dir(rtl), p:dir(rtl)"), ["d", "f"]);
        assert_eq!(select(html, "p:dir(ltr)"), ["b", "e"]);
    }

    #[test]
    fn form_states() {
        let html = r#"
            <input id=a><input id=b disabled><input id=c readonly><input id=d type=checkbox checked>
            <fieldset id=f disabled><legend id=l><input id=g></legend><input id=h></fieldset>
            <textarea id=t placeholder=x></textarea><select id=s required><option id=o selected></select>
            <details id=dt open></details><video id=v muted></video>"#;
        assert_eq!(select(html, "input:disabled"), ["b", "h"]);
        assert_eq!(select(html, "input:enabled"), ["a", "c", "d", "g"]);
        assert_eq!(select(html, ":read-write"), ["a", "g", "t"]);
        assert_eq!(select(html, ":checked"), ["d", "o"]);
        assert_eq!(select(html, ":required"), ["s"]);
        assert_eq!(select(html, ":placeholder-shown"), ["t"]);
        assert_eq!(select(html, ":open"), ["dt"]);
        assert_eq!(select(html, ":paused:muted"), ["v"]);
    }

    #[test]
    fn defined_and_headings() {
        let html = r#"<h1 id=a></h1><h3 id=b></h3><my-widget id=c></my-widget>"#;
        assert_eq!(select(html, ":heading"), ["a", "b"]);
        assert_eq!(select(html, ":heading(3)"), ["b"]);
        assert_eq!(select(html, "body :not(:defined)"), ["c"]);
    }

    #[test]
    fn quirks_mode_ignores_class_case() {
        assert_eq!(select("<p id=p class=Foo>", ".foo"), ["p"]);
        assert_eq!(
            select("<!DOCTYPE html><p id=p class=Foo>", ".foo"),
            Vec::<String>::new()
        );
    }
}
