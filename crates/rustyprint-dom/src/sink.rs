//! html5ever tree builder output into the arena.

use std::borrow::Cow;
use std::cell::{Ref, RefCell};

use html5ever::interface::tree_builder::{self, ElementFlags, NodeOrText, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{ParseOpts, QualName, parse_document};

use crate::{Attribute, Document, Element, Node, NodeData, NodeId, QuirksMode};

pub(crate) fn parse(html: &str) -> Document {
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            scripting_enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let sink = Sink(RefCell::new(Document {
        nodes: vec![Node::new(NodeData::Document)],
        quirks_mode: QuirksMode::NoQuirks,
    }));
    parse_document(sink, opts).one(html)
}

struct Sink(RefCell<Document>);

impl Node {
    fn new(data: NodeData) -> Self {
        Self {
            parent: None,
            previous_sibling: None,
            next_sibling: None,
            first_child: None,
            last_child: None,
            data,
        }
    }
}

impl Document {
    fn push(&mut self, data: NodeData) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("fewer than 2^32 nodes"));
        self.nodes.push(Node::new(data));
        id
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    fn detach(&mut self, id: NodeId) {
        let node = self.node_mut(id);
        let parent = node.parent.take();
        let previous = node.previous_sibling.take();
        let next = node.next_sibling.take();
        match next {
            Some(next) => self.node_mut(next).previous_sibling = previous,
            None => {
                if let Some(parent) = parent {
                    self.node_mut(parent).last_child = previous;
                }
            }
        }
        match previous {
            Some(previous) => self.node_mut(previous).next_sibling = next,
            None => {
                if let Some(parent) = parent {
                    self.node_mut(parent).first_child = next;
                }
            }
        }
    }

    fn append(&mut self, parent: NodeId, child: NodeId) {
        self.detach(child);
        let last = self.node(parent).last_child;
        {
            let node = self.node_mut(child);
            node.parent = Some(parent);
            node.previous_sibling = last;
        }
        match last {
            Some(last) => self.node_mut(last).next_sibling = Some(child),
            None => self.node_mut(parent).first_child = Some(child),
        }
        self.node_mut(parent).last_child = Some(child);
    }

    fn insert_before(&mut self, sibling: NodeId, child: NodeId) {
        self.detach(child);
        let parent = self.node(sibling).parent;
        let previous = self.node(sibling).previous_sibling;
        {
            let node = self.node_mut(child);
            node.parent = parent;
            node.previous_sibling = previous;
            node.next_sibling = Some(sibling);
        }
        self.node_mut(sibling).previous_sibling = Some(child);
        match previous {
            Some(previous) => self.node_mut(previous).next_sibling = Some(child),
            None => {
                if let Some(parent) = parent {
                    self.node_mut(parent).first_child = Some(child);
                }
            }
        }
    }

    /// Appends text to `previous` if it is a text node; otherwise returns the text as a new node.
    fn merge_text(&mut self, previous: Option<NodeId>, text: &str) -> Option<NodeId> {
        if let Some(previous) = previous
            && let NodeData::Text(existing) = &mut self.node_mut(previous).data
        {
            existing.push_str(text);
            return None;
        }
        Some(self.push(NodeData::Text(text.to_owned())))
    }
}

impl Sink {
    fn doc(&self) -> std::cell::RefMut<'_, Document> {
        self.0.borrow_mut()
    }
}

impl TreeSink for Sink {
    type Handle = NodeId;
    type Output = Document;
    type ElemName<'a> = Ref<'a, QualName>;

    fn finish(self) -> Document {
        self.0.into_inner()
    }

    // Parse errors are recovered from as the HTML spec requires; there is nothing to report.
    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> NodeId {
        NodeId(0)
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> Ref<'a, QualName> {
        Ref::map(self.0.borrow(), |doc| {
            &doc.element(*target).expect("handle is an element").name
        })
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<html5ever::Attribute>,
        flags: ElementFlags,
    ) -> NodeId {
        let mut doc = self.doc();
        let template_contents = flags.template.then(|| doc.push(NodeData::Document));
        let attrs = attrs
            .into_iter()
            .map(|a| Attribute {
                name: a.name,
                value: a.value.to_string(),
            })
            .collect();
        doc.push(NodeData::Element(Element {
            name,
            attrs,
            template_contents,
        }))
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.doc().push(NodeData::Comment(text.to_string()))
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> NodeId {
        self.doc().push(NodeData::ProcessingInstruction {
            target: target.to_string(),
            data: data.to_string(),
        })
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        let mut doc = self.doc();
        let child = match child {
            NodeOrText::AppendNode(node) => Some(node),
            NodeOrText::AppendText(text) => {
                let last = doc.node(*parent).last_child;
                doc.merge_text(last, &text)
            }
        };
        if let Some(child) = child {
            doc.append(*parent, child);
        }
    }

    fn append_before_sibling(&self, sibling: &NodeId, child: NodeOrText<NodeId>) {
        let mut doc = self.doc();
        let child = match child {
            NodeOrText::AppendNode(node) => Some(node),
            NodeOrText::AppendText(text) => {
                let previous = doc.node(*sibling).previous_sibling;
                doc.merge_text(previous, &text)
            }
        };
        if let Some(child) = child {
            doc.insert_before(*sibling, child);
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        let has_parent = self.0.borrow().node(*element).parent.is_some();
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        let mut doc = self.doc();
        let doctype = doc.push(NodeData::Doctype {
            name: name.to_string(),
        });
        doc.append(NodeId(0), doctype);
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        self.0
            .borrow()
            .element(*target)
            .and_then(|e| e.template_contents)
            .expect("handle is a template element")
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, mode: tree_builder::QuirksMode) {
        self.doc().quirks_mode = match mode {
            tree_builder::QuirksMode::NoQuirks => QuirksMode::NoQuirks,
            tree_builder::QuirksMode::LimitedQuirks => QuirksMode::LimitedQuirks,
            tree_builder::QuirksMode::Quirks => QuirksMode::Quirks,
        };
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<html5ever::Attribute>) {
        let mut doc = self.doc();
        let NodeData::Element(element) = &mut doc.node_mut(*target).data else {
            panic!("handle is an element");
        };
        for attr in attrs {
            if !element.attrs.iter().any(|a| a.name == attr.name) {
                element.attrs.push(Attribute {
                    name: attr.name,
                    value: attr.value.to_string(),
                });
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        self.doc().detach(*target);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        let mut doc = self.doc();
        while let Some(child) = doc.node(*node).first_child {
            doc.append(*new_parent, child);
        }
    }
}
