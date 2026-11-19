//! An `<svg>` element in a document (SVG 2 § 5.1.3): a replaced element whose picture is
//! drawn from the element and its descendants.
//!
//! The subtree is written out as an SVG file and drawn as one, by `svg.rs`. The cascade has
//! already given every SVG element its computed style, from the document's stylesheets,
//! `style` attributes and presentation attributes, so each element is written with that
//! style in place of its own presentation attributes.

use std::collections::HashMap;
use std::sync::Arc;

use rustyprint_css::cascade::Styles;
use rustyprint_css::properties::types::{
    Display, FontStyle, LengthPercentage, Paint, Size, SizeValue,
};
use rustyprint_css::properties::{ComputedValues, LonghandId, Property};
use rustyprint_dom::{Document, Element, NodeData, NodeId, local_name, ns};

use crate::images::{Image, Images};
use crate::svg::Resources;

/// The properties an SVG element is written with, which are also the presentation
/// attributes and `style` declarations it is written without. All but the last two inherit.
const PROPERTIES: &[LonghandId] = &[
    LonghandId::Color,
    LonghandId::Fill,
    LonghandId::FillOpacity,
    LonghandId::Stroke,
    LonghandId::StrokeOpacity,
    LonghandId::StrokeWidth,
    LonghandId::StrokeDasharray,
    LonghandId::StrokeLinecap,
    LonghandId::StrokeLinejoin,
    LonghandId::Visibility,
    LonghandId::FontFamily,
    LonghandId::FontSize,
    LonghandId::FontWeight,
    LonghandId::FontStyle,
    LonghandId::TextAnchor,
    LonghandId::DominantBaseline,
    LonghandId::ShapeRendering,
    LonghandId::Opacity,
    LonghandId::Display,
];

/// The size of an `<svg>` element that has neither a size nor a `viewBox`, in CSS pixels.
const DEFAULT_SIZE: (f32, f32) = (300.0, 150.0);

/// The picture of the `<svg>` element `node`, whose style is `style`, or `None` with a
/// warning if it cannot be drawn.
pub(crate) fn picture(
    document: &Document,
    styles: &Styles,
    images: &Images,
    node: NodeId,
    style: &Arc<ComputedValues>,
) -> Option<Arc<Image>> {
    let mut writer = Writer {
        document,
        styles,
        images,
        out: String::new(),
        files: HashMap::new(),
        empty: false,
        initial: ComputedValues::initial(),
    };
    writer.element(node, None);
    // An element without a size, as one that only holds definitions has, draws nothing.
    if writer.empty {
        return None;
    }
    let resources = Resources {
        style: Some(style.clone()),
        files: writer.files,
    };
    images.svg_element(&writer.out, resources)
}

/// Writes an `<svg>` element and its descendants as an SVG file.
struct Writer<'a> {
    document: &'a Document,
    styles: &'a Styles,
    images: &'a Images<'a>,
    out: String,
    /// The files that `<image>` elements link to.
    files: HashMap<String, Arc<Vec<u8>>>,
    /// Whether the outermost element has no width or no height.
    empty: bool,
    /// The values of properties that do not inherit, where an element does not set them.
    initial: ComputedValues,
}

impl Writer<'_> {
    /// Writes an element inside the element whose style is `parent`, or the outermost
    /// `<svg>` without one.
    fn element(&mut self, node: NodeId, parent: Option<&ComputedValues>) {
        let Some(element) = self.document.element(node) else {
            return;
        };
        // Content of other namespaces, such as HTML in a `<foreignObject>`, is not drawn.
        if element.name.ns != ns!(svg) || element.name.local == local_name!("script") {
            return;
        }
        let name = &*element.name.local;
        let style = self.styles.get(node).cloned();
        self.out.push('<');
        self.out.push_str(name);
        if parent.is_none() {
            self.out.push_str(
                r#" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#,
            );
            self.size(element, style.as_deref());
        }
        let mut declarations = String::new();
        for attribute in &element.attrs {
            let local = &*attribute.name.local;
            let prefix = match attribute.name.ns {
                ns!() => "",
                ns!(xlink) => "xlink:",
                ns!(xml) => "xml:",
                // Namespace declarations are written on the outermost element.
                _ => continue,
            };
            let sized = parent.is_none() && matches!(local, "width" | "height");
            if prefix.is_empty() && (local == "xmlns" || sized) {
                continue;
            }
            if prefix.is_empty() && style.is_some() {
                if is_written(local) {
                    continue;
                }
                if local == "style" {
                    // The outermost element's other declarations are for its box.
                    if parent.is_some() {
                        declarations = other_declarations(&attribute.value);
                    }
                    continue;
                }
            }
            if local == "href" && matches!(name, "image" | "feImage") {
                self.link(&attribute.value);
            }
            self.attribute(&format!("{prefix}{local}"), &attribute.value);
        }
        if let Some(style) = &style {
            for &id in PROPERTIES {
                let Some(own) = value(style, id) else {
                    continue;
                };
                let inherited = match parent {
                    // The outermost element says everything, since the file starts from
                    // SVG's initial values and not from the document's. Its opacity is
                    // its box's, which the page applies.
                    None if id == LonghandId::Opacity => continue,
                    None => None,
                    Some(parent) if id.inherited() => value(parent, id),
                    Some(_) => value(&self.initial, id),
                };
                // A baseline is written wherever it is set: it is not inherited in SVG 1.1.
                let baseline = id == LonghandId::DominantBaseline && own != "auto";
                if inherited.as_ref() != Some(&own) || baseline {
                    declarations.push_str(id.name());
                    declarations.push(':');
                    declarations.push_str(&own);
                    declarations.push(';');
                }
            }
        }
        if !declarations.is_empty() {
            self.attribute("style", &declarations);
        }
        self.out.push('>');
        for child in self.document.children(node) {
            match self.document.node(child).data() {
                NodeData::Text(text) => escape(&mut self.out, text),
                NodeData::Element(_) => self.element(child, style.as_deref().or(parent)),
                _ => {}
            }
        }
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push('>');
    }

    /// Writes the size of the outermost element. One with a `viewBox` is written without a
    /// size: it is drawn as that shape and placed in its box as `preserveAspectRatio`
    /// says. Without a `viewBox` nothing scales, so it is drawn at the size CSS gives its
    /// box, when that is a length, or else its attributes do.
    fn size(&mut self, element: &Element, style: Option<&ComputedValues>) {
        if element.has_attr("viewBox") {
            return;
        }
        let zoom = style.map_or(1.0, |style| style.effective_zoom);
        let length = |size: Option<&SizeValueOf>, attribute: &str, default: f32| match size {
            Some(Size::Value(LengthPercentage::Length(px))) => px / zoom,
            _ => element
                .attr(attribute)
                .and_then(|value| value.trim().trim_end_matches("px").parse::<f32>().ok())
                .filter(|length| length.is_finite() && *length >= 0.0)
                .unwrap_or(default),
        };
        let width = length(style.map(|style| &style.width), "width", DEFAULT_SIZE.0);
        let height = length(style.map(|style| &style.height), "height", DEFAULT_SIZE.1);
        self.empty = width <= 0.0 || height <= 0.0;
        self.attribute("width", &width.to_string());
        self.attribute("height", &height.to_string());
    }

    /// Loads the file an `<image>` links to through the document's loader. A data URL
    /// holds its own file.
    fn link(&mut self, url: &str) {
        let url = url.trim();
        let data = url
            .get(..5)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"));
        if !data
            && !self.files.contains_key(url)
            && let Some(bytes) = self.images.file(url)
        {
            self.files.insert(url.to_owned(), Arc::new(bytes));
        }
    }

    fn attribute(&mut self, name: &str, value: &str) {
        self.out.push(' ');
        self.out.push_str(name);
        self.out.push_str("=\"");
        escape(&mut self.out, value);
        self.out.push('"');
    }
}

/// The computed value of `width` and `height`.
type SizeValueOf = <SizeValue as Property>::Computed;

/// Whether the property called `name` is written from the computed style.
fn is_written(name: &str) -> bool {
    PROPERTIES.iter().any(|id| id.name() == name)
}

/// The declarations of a `style` attribute for properties that are not written from the
/// computed style, such as `clip-path`, which the SVG keeps as they are.
fn other_declarations(css: &str) -> String {
    let mut out = String::new();
    // Split at the semicolons outside brackets and strings, as in `url(data:a;b)`.
    let (mut depth, mut quote, mut start) = (0_u32, None, 0);
    let mut keep = |declaration: &str| {
        let name = declaration.split(':').next().unwrap_or("").trim();
        if !name.is_empty() && !is_written(&name.to_ascii_lowercase()) {
            out.push_str(declaration.trim());
            out.push(';');
        }
    };
    for (at, c) in css.char_indices() {
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (c, Some(open)) if c == open => quote = None,
            ('(', None) => depth += 1,
            (')', None) => depth = depth.saturating_sub(1),
            (';', None) if depth == 0 => {
                keep(&css[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    keep(&css[start..]);
    out
}

/// A computed value as SVG reads it, or `None` for one that says nothing in SVG, such as
/// any `display` but `none`.
fn value(style: &ComputedValues, id: LonghandId) -> Option<String> {
    let hex = |[r, g, b, _]: [u8; 4]| format!("#{r:02x}{g:02x}{b:02x}");
    let paint = |paint: &Paint| match paint {
        Paint::None => "none".to_owned(),
        Paint::Color(color) => hex(color.resolve(&style.color).to_rgba8()),
        Paint::ContextFill => "context-fill".to_owned(),
        Paint::ContextStroke => "context-stroke".to_owned(),
        Paint::Url(url, fallback) => match fallback {
            Some(color) => format!("url({url}) {}", hex(color.resolve(&style.color).to_rgba8())),
            None => format!("url({url})"),
        },
    };
    // A paint's own transparency, which SVG states apart from the color.
    let alpha = |paint: &Paint| match paint {
        Paint::Color(color) => color.resolve(&style.color).alpha,
        _ => 1.0,
    };
    Some(match id {
        LonghandId::Color => hex(style.color.to_rgba8()),
        LonghandId::Fill => paint(&style.fill),
        LonghandId::Stroke => paint(&style.stroke),
        LonghandId::FillOpacity => (style.fill_opacity * alpha(&style.fill)).to_string(),
        LonghandId::StrokeOpacity => (style.stroke_opacity * alpha(&style.stroke)).to_string(),
        LonghandId::StrokeWidth => style.stroke_width.to_string(),
        LonghandId::StrokeDasharray if style.stroke_dasharray.is_empty() => "none".to_owned(),
        LonghandId::StrokeDasharray => {
            let dashes: Vec<String> = style.stroke_dasharray.iter().map(f32::to_string).collect();
            dashes.join(" ")
        }
        LonghandId::FontSize => format!("{}px", style.font_size),
        LonghandId::FontWeight => style.font_weight.round().to_string(),
        LonghandId::FontStyle => match style.font_style {
            FontStyle::Normal => "normal",
            FontStyle::Italic => "italic",
            FontStyle::Oblique(_) => "oblique",
        }
        .to_owned(),
        LonghandId::Opacity => style.opacity.to_string(),
        LonghandId::Display if style.display == Display::None => "none".to_owned(),
        LonghandId::Display => return None,
        _ => style.serialize(id),
    })
}

/// Writes text as XML character data or an attribute value.
fn escape(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Characters XML does not allow.
            c if c < ' ' && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
}
