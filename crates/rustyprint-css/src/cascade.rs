//! The cascade: collecting stylesheets, matching rules and computing styles (CSS Cascade 5/6).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use rustyprint_dom::{Document, LocalName, NodeId, local_name, ns};

use crate::animate::{Keyframe, Keyframes, PropertyFrame, Timing, progress_at_zero, sample};
use crate::color_profile::ColorProfile;
use crate::container::{Candidate, ContainerCondition, ContainerSizes};
use crate::counter_style::{CounterStyles, Descriptors};
use crate::custom::{Environment, Substitution, compute_custom_properties, substitute};
use crate::font_rules::{
    FaceDescriptors, FeatureBlock, FeatureValues, FontFace, FontRules, FontRulesRef, FontSource,
    PaletteValues, SourceSpec, looks_like_font, parse_feature_values,
};
use crate::media::{CustomMedia, MediaEnvironment, MediaQueryList};
use crate::page::{
    Descriptor, MarginBox, MarginSafety, Marks, PageKey, PageOrientation, PageSelector, PageSize,
    PageStyle,
};
use crate::presentational::presentational_hints;
use crate::properties::types::{
    AnimationComposition, AnimationTimeline, BorderStyle, Direction, Display, DisplayInside,
    DisplayOutside, Easing, Float, LineHeight, OutlineStyle, Overflow, PageSide, Position,
    TryFallback, WritingMode,
};
use crate::properties::{
    ComputeContext, ComputedValues, CssWideKeyword, CustomValue, EARLY, LonghandId,
    PropertyDeclaration, parse_declaration, parse_substituted,
};
use crate::selector::{MatchContext, PseudoElement, Selector, SelectorList, Simple, Specificity};
use crate::stylesheet::{
    CssRule, Declaration, DescriptorRule, KeyframesRule, LayerName, Stylesheet, Warning,
};
use crate::syntax::Syntax;
use crate::values::{AbsoluteColor, FontMetrics};

const UA_STYLESHEET: &str = concat!(include_str!("ua.css"), include_str!("counter-styles.css"));

/// `@import` chains deeper than this are ignored.
const MAX_IMPORT_DEPTH: u32 = 16;

/// Where a stylesheet comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    UserAgent,
    User,
    /// Presentational HTML attributes, such as `bgcolor`.
    PresentationalHint,
    Author,
}

/// Options for computing styles.
pub struct StyleOptions<'a> {
    /// The page area in CSS pixels, for media queries and viewport units.
    pub page_size: (f32, f32),
    /// An extra author stylesheet applied after the document's own.
    pub stylesheet: Option<&'a str>,
    /// Loads `<link>` and `@import` stylesheets by URL.
    pub load: &'a dyn Fn(&str) -> Option<String>,
    /// Metrics of an element's first available font, for font-relative units such as `ex`
    /// and `ch`. The style passed in has its font properties computed. Without it, units
    /// use the fallback metrics CSS Values defines.
    pub font_metrics: Option<&'a dyn Fn(&ComputedValues) -> FontMetrics>,
    /// Loads binary files by URL: the fonts of `@font-face` rules and the ICC profiles of
    /// `@color-profile` rules. Without it, only `local()` fonts are found and no profiles
    /// load.
    pub load_font: Option<LoadFont<'a>>,
}

/// Loads a file's bytes by URL.
pub type LoadFont<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

/// Computed styles of a document.
#[derive(Debug, Default)]
pub struct Styles {
    elements: HashMap<NodeId, Arc<ComputedValues>>,
    pseudo_elements: HashMap<(NodeId, PseudoElement), Arc<ComputedValues>>,
    /// Styles of every kind of page the document can have.
    pages: HashMap<PageKey, Arc<PageStyle>>,
    /// Problems found in author stylesheets.
    pub warnings: Vec<StyleWarning>,
    /// The predefined counter styles and the document's `@counter-style` rules.
    pub counter_styles: CounterStyles,
    /// The styles of positioned elements with each `@position-try` rule their
    /// `position-try-fallbacks` names applied.
    position_try: HashMap<(NodeId, Arc<str>), Arc<ComputedValues>>,
    /// The document's `@font-face`, `@font-feature-values` and `@font-palette-values` rules,
    /// which every computed style also refers to.
    pub font_rules: FontRulesRef,
    /// Whether an element is a size container, whose size container queries and units need.
    pub size_containers: bool,
    /// The document's color profiles; native colors refer to them by index.
    pub color_profiles: Arc<[ColorProfile]>,
}

/// A warning with the stylesheet it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleWarning {
    /// `<style>`, a URL, `style attribute` or `request stylesheet`.
    pub source: String,
    pub warning: Warning,
}

impl Styles {
    /// The style of a page. Names that no element uses fall back to unnamed pages.
    pub fn page(&self, key: &PageKey) -> Arc<PageStyle> {
        self.pages
            .get(key)
            .or_else(|| {
                self.pages.get(&PageKey {
                    name: None,
                    ..key.clone()
                })
            })
            .cloned()
            .unwrap_or_else(|| Arc::new(default_page_style()))
    }

    /// The computed style of an element.
    pub fn get(&self, element: NodeId) -> Option<&Arc<ComputedValues>> {
        self.elements.get(&element)
    }

    /// The style of an element with the `@position-try` rule `name` applied, if it names the
    /// rule in `position-try-fallbacks` and the rule exists.
    pub fn position_try(&self, element: NodeId, name: &str) -> Option<&Arc<ComputedValues>> {
        self.position_try.get(&(element, Arc::from(name)))
    }

    /// The computed style of a pseudo-element, if any rule targets it.
    pub fn pseudo(&self, element: NodeId, pseudo: &PseudoElement) -> Option<&Arc<ComputedValues>> {
        self.pseudo_elements.get(&(element, pseudo.clone()))
    }

    /// These styles with every element and pseudo-element style replaced by `f`, which gets
    /// the element, the pseudo-element if any, and the style. Page styles stay.
    pub fn map(
        &self,
        mut f: impl FnMut(NodeId, Option<&PseudoElement>, &Arc<ComputedValues>) -> Arc<ComputedValues>,
    ) -> Styles {
        Styles {
            elements: self
                .elements
                .iter()
                .map(|(&node, style)| (node, f(node, None, style)))
                .collect(),
            pseudo_elements: self
                .pseudo_elements
                .iter()
                .map(|(key, style)| (key.clone(), f(key.0, Some(&key.1), style)))
                .collect(),
            pages: self.pages.clone(),
            warnings: self.warnings.clone(),
            counter_styles: self.counter_styles.clone(),
            font_rules: self.font_rules.clone(),
            size_containers: self.size_containers,
            color_profiles: self.color_profiles.clone(),
            position_try: self
                .position_try
                .iter()
                .map(|(key, style)| (key.clone(), f(key.0, None, style)))
                .collect(),
        }
    }
}

/// Computes the style of every element in `document`, without laid out containers: size
/// container queries are unknown and container units use the page area.
pub fn compute_styles(document: &Document, options: &StyleOptions) -> Styles {
    compute_styles_in(document, options, &ContainerSizes::new())
}

/// Computes the style of every element in `document`, with the content-box sizes of its
/// size containers as layout found them.
pub fn compute_styles_in(
    document: &Document,
    options: &StyleOptions,
    containers: &ContainerSizes,
) -> Styles {
    let mut collector = Collector::new(options);
    collector.add_ua();
    collector.add_document(document);
    if let Some(css) = options.stylesheet {
        collector.add(css, Origin::Author, None, "request stylesheet");
    }
    let rules = collector.finish();
    let mut styles = Styles {
        warnings: rules.warnings.clone(),
        counter_styles: rules.counter_styles.clone(),
        font_rules: rules.font_rules.clone(),
        ..Default::default()
    };
    let mut root_parent = ComputedValues::initial();
    root_parent.font_rules = rules.font_rules.clone();
    // Registered custom properties start at their initial values.
    if !rules.registered.is_empty() {
        let mut custom = (*root_parent.custom).clone();
        for (name, registration) in &rules.registered {
            if let Some(initial) = &registration.initial {
                custom.insert(name.clone(), initial.clone());
            }
        }
        root_parent.custom = Arc::new(custom);
    }
    let mut engine = Engine {
        document,
        rules: &rules,
        options,
        root: None,
        containers,
        ancestors: std::cell::RefCell::new(Vec::new()),
    };
    styles.color_profiles = rules.color_profiles.clone();
    crate::color_profile::with_profiles(&rules.color_profiles, || {
        engine.style_tree(document.root(), &Arc::new(root_parent), &mut styles);
        engine.style_pages(&mut styles);
    });
    styles
}

/// The style of a page without `@page` rules.
fn default_page_style() -> PageStyle {
    PageStyle {
        style: Arc::new(ComputedValues::initial()),
        margins_set: [false; 4],
        size: PageSize::Auto,
        orientation: PageOrientation::Upright,
        bleed: None,
        marks: Marks::default(),
        margin_safety: MarginSafety::None,
        margin_boxes: Vec::new(),
    }
}

/// The default `text-align` and `vertical-align` of each margin box.
fn margin_box_defaults() -> &'static [(MarginBox, Vec<PropertyDeclaration>)] {
    static DEFAULTS: OnceLock<Vec<(MarginBox, Vec<PropertyDeclaration>)>> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        use MarginBox::*;
        [
            (TopLeftCorner, "right", "middle"),
            (TopLeft, "left", "middle"),
            (TopCenter, "center", "middle"),
            (TopRight, "right", "middle"),
            (TopRightCorner, "left", "middle"),
            (RightTop, "center", "top"),
            (RightMiddle, "center", "middle"),
            (RightBottom, "center", "bottom"),
            (BottomRightCorner, "left", "middle"),
            (BottomRight, "right", "middle"),
            (BottomCenter, "center", "middle"),
            (BottomLeft, "left", "middle"),
            (BottomLeftCorner, "right", "middle"),
            (LeftBottom, "center", "bottom"),
            (LeftMiddle, "center", "middle"),
            (LeftTop, "center", "top"),
        ]
        .into_iter()
        .map(|(name, align, vertical)| {
            let mut declarations = parse_declaration("text-align", align).unwrap_or_default();
            declarations.extend(parse_declaration("vertical-align", vertical).unwrap_or_default());
            (name, declarations)
        })
        .collect()
    })
}

/// The declarations of a matching `@page` rule or margin box, ready to cascade.
fn page_entries<'r>(
    declarations: &'r [(PropertyDeclaration, bool)],
    key: &dyn Fn(bool) -> CascadeKey,
    origin: Origin,
) -> Vec<Applicable<'r>> {
    declarations
        .iter()
        .map(|(declaration, important)| Applicable {
            declaration,
            key: key(*important),
            origin,
            important: *important,
        })
        .collect()
}

/// Parsed declarations with their importance.
type Declarations = Arc<[(PropertyDeclaration, bool)]>;

/// An `@page` rule ready for matching pages.
struct CompiledPage {
    selectors: Vec<PageSelector>,
    declarations: Declarations,
    descriptors: Vec<(Descriptor, bool)>,
    margin_boxes: Vec<(MarginBox, Declarations)>,
    origin: Origin,
    layer: usize,
    order: u32,
}

// ---------------------------------------------------------------------------------------------
// Collecting rules

struct ParsedSheet {
    sheet: Stylesheet,
    origin: Origin,
    owner: Option<NodeId>,
    source: String,
}

/// A style rule flattened out of its stylesheet, with one complex selector.
struct CompiledRule {
    selector: Selector,
    specificity: Specificity,
    declarations: Arc<[(PropertyDeclaration, bool)]>,
    origin: Origin,
    /// Index into the layer table.
    layer: usize,
    scope: Option<Arc<Scope>>,
    /// The `@container` rules around the rule.
    container: Option<Arc<ContainerChain>>,
    order: u32,
}

/// The conditions of the `@container` rules around a style rule, outermost first: every
/// rule must have a condition that holds.
type ContainerChain = Vec<Arc<[ContainerCondition]>>;

struct Scope {
    start: Option<SelectorList>,
    end: Option<SelectorList>,
    /// The scope root when `start` is omitted.
    implicit_root: Option<NodeId>,
}

/// Rules ready for matching, indexed by the rightmost compound's ID, class or type.
struct Rules {
    rules: Vec<CompiledRule>,
    by_id: HashMap<String, Vec<usize>>,
    by_class: HashMap<String, Vec<usize>>,
    by_type: HashMap<LocalName, Vec<usize>>,
    universal: Vec<usize>,
    pages: Vec<CompiledPage>,
    /// Rank of each layer: higher ranks win for normal declarations.
    layer_ranks: Vec<u32>,
    warnings: Vec<StyleWarning>,
    counter_styles: CounterStyles,
    font_rules: FontRulesRef,
    /// The declarations of each `@position-try` rule, by name.
    position_try: HashMap<Arc<str>, Vec<PropertyDeclaration>>,
    /// The keyframes of each `@keyframes` rule, by name.
    keyframes: HashMap<Arc<str>, Keyframes>,
    /// Custom properties registered by `@property`, by name.
    registered: HashMap<String, Arc<Registration>>,
    /// Custom functions defined by `@function`, by name.
    functions: crate::custom::Functions,
    /// `@custom-media` definitions, for media queries in `if()` and custom functions.
    custom_media: HashMap<String, CustomMedia>,
    /// The color profiles that loaded, in cascade order.
    color_profiles: Arc<[ColorProfile]>,
}

/// A custom property registered by `@property`.
#[derive(Debug)]
pub(crate) struct Registration {
    pub syntax: Syntax,
    pub inherits: bool,
    /// The computed initial value; only the universal syntax may have none.
    pub initial: Option<Arc<str>>,
}

/// A valid `@counter-style` rule, with what decides which of several with the same name wins.
struct CompiledCounterStyle {
    origin: Origin,
    layer: usize,
    order: u32,
    name: Arc<str>,
    descriptors: Descriptors,
}

struct Collector<'a> {
    options: &'a StyleOptions<'a>,
    sheets: Vec<ParsedSheet>,
}

impl<'a> Collector<'a> {
    fn new(options: &'a StyleOptions<'a>) -> Self {
        Self {
            options,
            sheets: Vec::new(),
        }
    }

    fn add_ua(&mut self) {
        static UA: OnceLock<Stylesheet> = OnceLock::new();
        let sheet = UA.get_or_init(|| Stylesheet::parse(UA_STYLESHEET)).clone();
        self.sheets.push(ParsedSheet {
            sheet,
            origin: Origin::UserAgent,
            owner: None,
            source: "UA".into(),
        });
    }

    fn add(&mut self, css: &str, origin: Origin, owner: Option<NodeId>, source: &str) {
        let sheet = Stylesheet::parse(css);
        self.sheets.push(ParsedSheet {
            sheet,
            origin,
            owner,
            source: source.to_owned(),
        });
    }

    /// Adds `<style>` and `<link rel=stylesheet>` in tree order. Media attributes are kept by
    /// wrapping the sheet's rules in a media rule.
    fn add_document(&mut self, document: &Document) {
        for node in document.descendants(document.root()) {
            let Some(element) = document.element(node) else {
                continue;
            };
            if element.name.ns != ns!(html) {
                continue;
            }
            let (css, source) = if element.name.local == local_name!("style") {
                let is_css = element
                    .attr("type")
                    .is_none_or(|t| t.is_empty() || t.eq_ignore_ascii_case("text/css"));
                if !is_css {
                    continue;
                }
                (document.text_content(node), "<style>".to_owned())
            } else if element.name.local == local_name!("link") {
                let rel = element.attr("rel").unwrap_or("").to_ascii_lowercase();
                let words: Vec<&str> = rel.split_ascii_whitespace().collect();
                if !words.contains(&"stylesheet")
                    || words.contains(&"alternate")
                    || element.has_attr("disabled")
                {
                    continue;
                }
                let Some(href) = element.attr("href") else {
                    continue;
                };
                match (self.options.load)(href) {
                    Some(css) => (css, href.to_owned()),
                    None => continue,
                }
            } else {
                continue;
            };
            let mut sheet = Stylesheet::parse(&css);
            if let Some(media) = element.attr("media").filter(|m| !m.trim().is_empty()) {
                let queries = MediaQueryList::parse(&mut cssparser::Parser::new(media));
                sheet.rules = vec![CssRule::Media {
                    queries,
                    rules: std::mem::take(&mut sheet.rules),
                }];
            }
            self.sheets.push(ParsedSheet {
                sheet,
                origin: Origin::Author,
                owner: Some(node),
                source,
            });
        }
    }

    fn finish(mut self) -> Rules {
        // Resolve imports first so their custom media and layers count.
        let mut warnings = Vec::new();
        for sheet in &mut self.sheets {
            let mut seen = Vec::new();
            sheet.sheet.rules = resolve_imports(
                std::mem::take(&mut sheet.sheet.rules),
                self.options,
                0,
                &mut seen,
                &mut warnings,
                &sheet.source,
            );
        }
        let mut custom_media = HashMap::new();
        for sheet in &self.sheets {
            collect_custom_media(&sheet.sheet.rules, &mut custom_media);
        }
        let env = MediaEnvironment {
            width: self.options.page_size.0,
            height: self.options.page_size.1,
            custom_media: &custom_media,
        };
        let mut flattener = Flattener {
            env,
            rules: Vec::new(),
            pages: Vec::new(),
            fonts: FontRules::default(),
            font_faces: Vec::new(),
            counter_styles: Vec::new(),
            position_try: Vec::new(),
            keyframes: Vec::new(),
            properties: Vec::new(),
            functions: Vec::new(),
            color_profiles: Vec::new(),
            layers: LayerTable::default(),
            order: 0,
            warnings,
        };
        for sheet in &self.sheets {
            if sheet.origin != Origin::UserAgent {
                flattener
                    .warnings
                    .extend(sheet.sheet.warnings.iter().map(|w| StyleWarning {
                        source: sheet.source.clone(),
                        warning: w.clone(),
                    }));
            }
            let context = SheetContext {
                origin: sheet.origin,
                source: &sheet.source,
                namespaces: &sheet.sheet.namespaces,
                implicit_root: sheet.owner,
            };
            let root_layer = flattener.layers.root(sheet.origin);
            flattener.flatten(&sheet.sheet.rules, &context, root_layer, None, &None);
        }
        let layer_ranks = flattener.layers.ranks();
        // Later origins, layers and rules win (CSS Cascade 5 § 6.4.3).
        let mut defined = std::mem::take(&mut flattener.counter_styles);
        defined.sort_by_key(|rule| (rule.origin, layer_ranks[rule.layer], rule.order));
        let mut counter_styles = CounterStyles::default();
        for rule in defined {
            counter_styles.define(rule.name, rule.descriptors);
        }
        let mut fonts = std::mem::take(&mut flattener.fonts);
        let mut loaded = 0;
        for (face, sources, source) in std::mem::take(&mut flattener.font_faces) {
            let face = load_face(
                face,
                sources,
                self.options,
                &source,
                &mut loaded,
                &mut flattener.warnings,
            );
            fonts.faces.push(face);
        }
        let mut tries = std::mem::take(&mut flattener.position_try);
        tries.sort_by_key(|rule| (rule.origin, layer_ranks[rule.layer], rule.order));
        let position_try = tries
            .into_iter()
            .map(|rule| (rule.name, rule.declarations))
            .collect();
        // Of several `@keyframes` rules with a name, the last in cascade order wins.
        let mut animations = std::mem::take(&mut flattener.keyframes);
        animations.sort_by_key(|rule| (rule.origin, layer_ranks[rule.layer], rule.order));
        let keyframes = animations
            .into_iter()
            .map(|rule| (rule.name, rule.keyframes))
            .collect();
        // Of several valid `@property` rules for a name, the last in cascade order wins.
        let mut properties = std::mem::take(&mut flattener.properties);
        properties.sort_by_key(|rule| (rule.origin, layer_ranks[rule.layer], rule.order));
        let registered = properties
            .into_iter()
            .map(|rule| (rule.name, Arc::new(rule.registration)))
            .collect();
        // Of several `@color-profile` rules with a name, the last in cascade order wins.
        let mut profiles = std::mem::take(&mut flattener.color_profiles);
        profiles.sort_by_key(|rule| (rule.origin, layer_ranks[rule.layer], rule.order));
        let mut chosen: Vec<CompiledColorProfile> = Vec::new();
        for rule in profiles {
            chosen.retain(|r| r.name != rule.name);
            chosen.push(rule);
        }
        let mut color_profiles = Vec::new();
        for rule in chosen {
            let data = self.options.load_font.and_then(|load| load(&rule.src));
            let loaded = data
                .ok_or_else(|| "it was not found".to_owned())
                .and_then(|data| ColorProfile::load(&rule.name, data, rule.intent));
            match loaded {
                Ok(profile) => color_profiles.push(profile),
                Err(reason) => flattener.warnings.push(StyleWarning {
                    source: rule.source,
                    warning: Warning {
                        line: rule.line,
                        column: rule.column,
                        message: format!("could not load color profile `{}`: {reason}", rule.src),
                    },
                }),
            }
        }
        let mut functions = std::mem::take(&mut flattener.functions);
        functions.sort_by_key(|rule| (rule.origin, layer_ranks[rule.layer], rule.order));
        let functions = functions
            .into_iter()
            .map(|rule| (rule.name, Arc::new(rule.function)))
            .collect();
        let mut rules = Rules {
            rules: flattener.rules,
            by_id: HashMap::new(),
            by_class: HashMap::new(),
            by_type: HashMap::new(),
            universal: Vec::new(),
            pages: flattener.pages,
            layer_ranks,
            warnings: flattener.warnings,
            counter_styles,
            font_rules: FontRulesRef(Arc::new(fonts)),
            position_try,
            keyframes,
            registered,
            functions,
            custom_media,
            color_profiles: color_profiles.into(),
        };
        for (index, rule) in rules.rules.iter().enumerate() {
            let key = rule.selector.compounds.last().and_then(|compound| {
                compound
                    .0
                    .iter()
                    .find_map(|simple| match simple {
                        Simple::Id(id) => Some(Key::Id(id.clone())),
                        Simple::Class(class) => Some(Key::Class(class.clone())),
                        _ => None,
                    })
                    .or_else(|| {
                        compound.0.iter().find_map(|simple| match simple {
                            Simple::Type { lower, .. } => Some(Key::Type(lower.clone())),
                            _ => None,
                        })
                    })
            });
            match key {
                // Quirks mode matches IDs and classes case-insensitively, so index them lowercase.
                Some(Key::Id(id)) => rules
                    .by_id
                    .entry(id.to_ascii_lowercase())
                    .or_default()
                    .push(index),
                Some(Key::Class(class)) => rules
                    .by_class
                    .entry(class.to_ascii_lowercase())
                    .or_default()
                    .push(index),
                Some(Key::Type(name)) => rules.by_type.entry(name).or_default().push(index),
                None => rules.universal.push(index),
            }
        }
        rules
    }
}

enum Key {
    Id(String),
    Class(String),
    Type(LocalName),
}

/// Replaces `@import` rules with the imported rules, wrapped in their layer, media and
/// supports conditions.
fn resolve_imports(
    rules: Vec<CssRule>,
    options: &StyleOptions,
    depth: u32,
    seen: &mut Vec<String>,
    warnings: &mut Vec<StyleWarning>,
    source: &str,
) -> Vec<CssRule> {
    let mut out = Vec::new();
    for rule in rules {
        let CssRule::Import(import) = rule else {
            out.push(rule);
            continue;
        };
        if depth >= MAX_IMPORT_DEPTH || seen.contains(&import.url) {
            warnings.push(StyleWarning {
                source: source.to_owned(),
                warning: Warning {
                    line: 0,
                    column: 0,
                    message: format!("ignored recursive @import of {}", import.url),
                },
            });
            continue;
        }
        let Some(css) = (options.load)(&import.url) else {
            warnings.push(StyleWarning {
                source: source.to_owned(),
                warning: Warning {
                    line: 0,
                    column: 0,
                    message: format!("could not load @import {}", import.url),
                },
            });
            continue;
        };
        let sheet = Stylesheet::parse(&css);
        warnings.extend(sheet.warnings.iter().map(|w| StyleWarning {
            source: import.url.clone(),
            warning: w.clone(),
        }));
        seen.push(import.url.clone());
        let mut imported =
            resolve_imports(sheet.rules, options, depth + 1, seen, warnings, &import.url);
        seen.pop();
        if let Some(layer) = import.layer {
            imported = vec![CssRule::LayerBlock {
                name: layer,
                rules: imported,
            }];
        }
        if !import.media.0.is_empty() {
            imported = vec![CssRule::Media {
                queries: import.media,
                rules: imported,
            }];
        }
        if let Some(condition) = import.supports {
            imported = vec![CssRule::Supports {
                condition,
                rules: imported,
            }];
        }
        out.extend(imported);
    }
    out
}

fn collect_custom_media(rules: &[CssRule], out: &mut HashMap<String, CustomMedia>) {
    for rule in rules {
        match rule {
            CssRule::CustomMedia { name, value } => {
                out.insert(name.clone(), value.clone());
            }
            CssRule::Media { rules, .. }
            | CssRule::Supports { rules, .. }
            | CssRule::LayerBlock { rules, .. }
            | CssRule::Container { rules, .. }
            | CssRule::Scope { rules, .. } => collect_custom_media(rules, out),
            _ => {}
        }
    }
}

struct SheetContext<'a> {
    origin: Origin,
    source: &'a str,
    namespaces: &'a crate::selector::Namespaces,
    implicit_root: Option<NodeId>,
}

/// Cascade layers, in the order they are first declared.
#[derive(Default)]
struct LayerTable {
    /// Each layer: its parent and its name within the parent.
    layers: Vec<(Option<usize>, String)>,
    roots: HashMap<Origin, usize>,
    anonymous: u32,
}

impl LayerTable {
    /// The implicit outermost layer holding an origin's unlayered rules.
    fn root(&mut self, origin: Origin) -> usize {
        *self.roots.entry(origin).or_insert_with(|| {
            self.layers.push((None, String::new()));
            self.layers.len() - 1
        })
    }

    fn child(&mut self, parent: usize, name: &str) -> usize {
        if let Some(index) = self
            .layers
            .iter()
            .position(|(p, n)| *p == Some(parent) && n == name)
        {
            return index;
        }
        self.layers.push((Some(parent), name.to_owned()));
        self.layers.len() - 1
    }

    fn path(&mut self, parent: usize, name: &LayerName) -> usize {
        name.iter()
            .fold(parent, |layer, part| self.child(layer, part))
    }

    fn anonymous(&mut self, parent: usize) -> usize {
        self.anonymous += 1;
        // NUL cannot appear in layer names.
        let name = format!("\0{}", self.anonymous);
        self.child(parent, &name)
    }

    /// Ranks layers so that sublayers come before their parent's own rules and earlier
    /// declared layers before later ones.
    fn ranks(&self) -> Vec<u32> {
        let mut ranks = vec![0; self.layers.len()];
        let mut next = 0;
        let roots: Vec<usize> = (0..self.layers.len())
            .filter(|&i| self.layers[i].0.is_none())
            .collect();
        for root in roots {
            self.rank(root, &mut ranks, &mut next);
        }
        ranks
    }

    fn rank(&self, layer: usize, ranks: &mut [u32], next: &mut u32) {
        let children: Vec<usize> = (0..self.layers.len())
            .filter(|&i| self.layers[i].0 == Some(layer))
            .collect();
        for child in children {
            self.rank(child, ranks, next);
        }
        ranks[layer] = *next;
        *next += 1;
    }
}

/// A `@position-try` rule, with what decides which of several with the same name wins.
struct CompiledPositionTry {
    origin: Origin,
    layer: usize,
    order: u32,
    name: Arc<str>,
    declarations: Vec<PropertyDeclaration>,
}

/// The properties a `@position-try` rule may set (CSS Anchor Positioning 1 § 4.4).
fn allowed_in_position_try(name: &str) -> bool {
    let name = name.strip_prefix("-webkit-").unwrap_or(name);
    matches!(
        name,
        "top"
            | "right"
            | "bottom"
            | "left"
            | "inset"
            | "inset-block"
            | "inset-inline"
            | "inset-block-start"
            | "inset-block-end"
            | "inset-inline-start"
            | "inset-inline-end"
            | "margin"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "margin-block"
            | "margin-inline"
            | "margin-block-start"
            | "margin-block-end"
            | "margin-inline-start"
            | "margin-inline-end"
            | "width"
            | "height"
            | "min-width"
            | "min-height"
            | "max-width"
            | "max-height"
            | "inline-size"
            | "block-size"
            | "min-inline-size"
            | "min-block-size"
            | "max-inline-size"
            | "max-block-size"
            | "align-self"
            | "justify-self"
            | "place-self"
            | "position-anchor"
            | "position-area"
    )
}

/// An `@keyframes` rule, with what decides which of several with the same name wins.
struct CompiledKeyframes {
    origin: Origin,
    layer: usize,
    order: u32,
    name: Arc<str>,
    keyframes: Keyframes,
}

/// An `@color-profile` rule, with what decides which of several with a name wins.
struct CompiledColorProfile {
    origin: Origin,
    layer: usize,
    order: u32,
    name: String,
    src: String,
    intent: moxcms::RenderingIntent,
    source: String,
    line: u32,
    column: u32,
}

/// An `@function` rule, with what decides which of several with a name wins.
struct CompiledFunction {
    origin: Origin,
    layer: usize,
    order: u32,
    name: String,
    function: crate::custom::CustomFunction,
}

/// A valid `@property` rule, with what decides which of several for a name wins.
struct CompiledProperty {
    origin: Origin,
    layer: usize,
    order: u32,
    name: String,
    registration: Registration,
}

/// Validates an `@property` rule (CSS Properties and Values API 1 § 3).
fn register(rule: &DescriptorRule, page_size: (f32, f32)) -> Result<Registration, String> {
    let (mut syntax, mut inherits, mut initial) = (None, None, None);
    for declaration in &rule.declarations {
        let mut input = cssparser::Parser::new(&declaration.value);
        match declaration.name.as_str() {
            "syntax" => {
                let text: String = input
                    .parse_entirely(|input| {
                        Ok::<_, cssparser::ParseError<()>>(input.expect_string()?.to_string())
                    })
                    .map_err(|_| "`syntax` must be a string".to_owned())?;
                syntax =
                    Some(Syntax::parse(&text).ok_or_else(|| format!("invalid syntax `{text}`"))?);
            }
            "inherits" => {
                let value = input
                    .parse_entirely(|input| {
                        let ident = input.expect_ident()?.to_ascii_lowercase();
                        match &*ident {
                            "true" => Ok(true),
                            "false" => Ok(false),
                            _ => Err(cssparser::ParseError::<()>::custom(())),
                        }
                    })
                    .map_err(|_| "`inherits` must be `true` or `false`".to_owned())?;
                inherits = Some(value);
            }
            "initial-value" => initial = Some(declaration.value.clone()),
            other => return Err(format!("unknown descriptor `{other}`")),
        }
    }
    let syntax = syntax.ok_or("it needs `syntax`")?;
    let inherits = inherits.ok_or("it needs `inherits`")?;
    let initial = match (initial, syntax.is_universal()) {
        (None, true) => None,
        (None, false) => return Err("it needs `initial-value`".into()),
        (Some(text), universal) => {
            if !syntax.is_computationally_independent(&text) {
                return Err(format!("`initial-value` {text} depends on the element"));
            }
            let parent = ComputedValues::initial();
            let ctx = ComputeContext::initial(&parent, page_size);
            let computed = syntax
                .compute(&text, &ctx)
                .ok_or_else(|| format!("`initial-value` {text} does not match the syntax"))?;
            if universal && computed.is_empty() {
                None
            } else {
                Some(Arc::from(computed))
            }
        }
    };
    Ok(Registration {
        syntax,
        inherits,
        initial,
    })
}

/// The longhands keyframes cannot animate: those of animations and transitions themselves.
fn animatable(id: LonghandId) -> bool {
    let name = id.name();
    !name.starts_with("animation") && !name.starts_with("transition")
}

struct Flattener<'a> {
    env: MediaEnvironment<'a>,
    rules: Vec<CompiledRule>,
    pages: Vec<CompiledPage>,
    /// Feature values and palettes; faces load once every rule is found.
    fonts: FontRules,
    /// Valid `@font-face` rules, their sources and the stylesheet they came from.
    font_faces: Vec<(FontFace, Vec<SourceSpec>, String)>,
    counter_styles: Vec<CompiledCounterStyle>,
    position_try: Vec<CompiledPositionTry>,
    keyframes: Vec<CompiledKeyframes>,
    properties: Vec<CompiledProperty>,
    functions: Vec<CompiledFunction>,
    color_profiles: Vec<CompiledColorProfile>,
    layers: LayerTable,
    order: u32,
    warnings: Vec<StyleWarning>,
}

impl Flattener<'_> {
    fn flatten(
        &mut self,
        rules: &[CssRule],
        ctx: &SheetContext,
        layer: usize,
        scope: Option<&Arc<Scope>>,
        container: &Option<Arc<ContainerChain>>,
    ) {
        for rule in rules {
            match rule {
                CssRule::Style(style) => {
                    let declarations = self.compile_declarations(&style.declarations, ctx);
                    if !declarations.is_empty() {
                        let declarations: Arc<[_]> = declarations.into();
                        for selector in &style.selectors.0 {
                            self.rules.push(CompiledRule {
                                specificity: selector.specificity(),
                                selector: selector.clone(),
                                declarations: declarations.clone(),
                                origin: ctx.origin,
                                layer,
                                scope: scope.cloned(),
                                container: container.clone(),
                                order: self.order,
                            });
                        }
                        self.order += 1;
                    }
                    self.flatten(&style.rules, ctx, layer, scope, container);
                }
                CssRule::Container { conditions, rules } => {
                    let mut chain = container.as_deref().cloned().unwrap_or_default();
                    chain.push(Arc::from(conditions.as_slice()));
                    self.flatten(rules, ctx, layer, scope, &Some(Arc::new(chain)));
                }
                CssRule::Media { queries, rules } => {
                    if queries.matches(&self.env) {
                        self.flatten(rules, ctx, layer, scope, container);
                    }
                }
                CssRule::Supports { condition, rules } => {
                    let supported = condition.evaluate(ctx.namespaces, &|name, value| {
                        parse_declaration(name, value).is_ok()
                    });
                    if supported {
                        self.flatten(rules, ctx, layer, scope, container);
                    }
                }
                CssRule::LayerStatement(names) => {
                    for name in names {
                        self.layers.path(layer, name);
                    }
                }
                CssRule::LayerBlock { name, rules } => {
                    let child = match name {
                        Some(name) => self.layers.path(layer, name),
                        None => self.layers.anonymous(layer),
                    };
                    self.flatten(rules, ctx, child, scope, container);
                }
                CssRule::Scope { start, end, rules } => {
                    let scope = Arc::new(Scope {
                        start: start.clone(),
                        end: end.clone(),
                        implicit_root: ctx.implicit_root,
                    });
                    self.flatten(rules, ctx, layer, Some(&scope), container);
                }
                CssRule::Page(page) => {
                    let (descriptors, properties): (Vec<_>, Vec<_>) = page
                        .declarations
                        .iter()
                        .cloned()
                        .partition(|d| Descriptor::is_descriptor(&d.name));
                    let descriptors = descriptors
                        .iter()
                        .filter_map(|d| match Descriptor::parse(&d.name, &d.value) {
                            Some(descriptor) => Some((descriptor, d.important)),
                            None => {
                                self.warn(
                                    ctx,
                                    d,
                                    format!("invalid value for `{}`: `{}`", d.name, d.value),
                                );
                                None
                            }
                        })
                        .collect();
                    let declarations = self.compile_declarations(&properties, ctx).into();
                    let margin_boxes = page
                        .margin_boxes
                        .iter()
                        .map(|(name, body)| (*name, self.compile_declarations(body, ctx).into()))
                        .collect();
                    self.pages.push(CompiledPage {
                        selectors: page.selectors.clone(),
                        declarations,
                        descriptors,
                        margin_boxes,
                        origin: ctx.origin,
                        layer,
                        order: self.order,
                    });
                    self.order += 1;
                }
                CssRule::CounterStyle(rule) => {
                    let mut descriptors = Descriptors::default();
                    for declaration in &rule.declarations {
                        if let Err(message) = descriptors.set(&declaration.name, &declaration.value)
                        {
                            self.warn(ctx, declaration, message);
                        }
                    }
                    let reason = if ctx.origin != Origin::UserAgent
                        && !crate::counter_style::redefinable(&rule.name)
                    {
                        Some("predefined styles like this one cannot be redefined")
                    } else {
                        descriptors.invalid()
                    };
                    if let Some(reason) = reason {
                        let at = Declaration {
                            name: String::new(),
                            value: String::new(),
                            important: false,
                            line: rule.line,
                            column: rule.column,
                        };
                        self.warn(
                            ctx,
                            &at,
                            format!("ignored @counter-style {}: {reason}", rule.name),
                        );
                    } else {
                        self.counter_styles.push(CompiledCounterStyle {
                            origin: ctx.origin,
                            layer,
                            order: self.order,
                            name: Arc::from(rule.name.as_str()),
                            descriptors,
                        });
                        self.order += 1;
                    }
                }
                CssRule::FontFace(rule) => {
                    let mut descriptors = FaceDescriptors::default();
                    for declaration in &rule.declarations {
                        if let Err(message) = descriptors.set(&declaration.name, &declaration.value)
                        {
                            self.warn(ctx, declaration, message);
                        }
                    }
                    match (descriptors.family, descriptors.sources) {
                        (Some(family), Some(sources)) => {
                            let mut face = descriptors.face;
                            face.family = Arc::from(family.as_str());
                            self.font_faces.push((face, sources, ctx.source.to_owned()));
                        }
                        _ => {
                            let at = Declaration {
                                name: String::new(),
                                value: String::new(),
                                important: false,
                                line: rule.line,
                                column: rule.column,
                            };
                            self.warn(
                                ctx,
                                &at,
                                "ignored @font-face: it needs `font-family` and `src`".to_owned(),
                            );
                        }
                    }
                }
                CssRule::FontPaletteValues(rule) => {
                    let mut palette = PaletteValues::new(&rule.name);
                    for declaration in &rule.declarations {
                        if let Err(message) = palette.set(&declaration.name, &declaration.value) {
                            self.warn(ctx, declaration, message);
                        }
                    }
                    self.fonts.palettes.push(palette);
                }
                CssRule::FontFeatureValues(rule) => {
                    let mut values = FeatureValues {
                        families: rule
                            .families
                            .iter()
                            .map(|f| Arc::from(f.as_str()))
                            .collect(),
                        ..Default::default()
                    };
                    for declaration in &rule.declarations {
                        if declaration.name != "font-display" {
                            self.warn(
                                ctx,
                                declaration,
                                format!(
                                    "unknown descriptor `{}` in @font-feature-values",
                                    declaration.name
                                ),
                            );
                        }
                    }
                    for (name, declarations) in &rule.blocks {
                        let kind = FeatureBlock::from_name(name).expect("checked by the parser");
                        for declaration in declarations {
                            match parse_feature_values(&declaration.value)
                                .filter(|v| kind.accepts(v))
                            {
                                Some(numbers) => {
                                    values.values.insert(
                                        (kind, Arc::from(declaration.name.as_str())),
                                        numbers,
                                    );
                                }
                                None => self.warn(
                                    ctx,
                                    declaration,
                                    format!(
                                        "invalid value for `{}`: `{}`",
                                        declaration.name, declaration.value
                                    ),
                                ),
                            }
                        }
                    }
                    self.fonts.feature_values.push(values);
                }
                CssRule::PositionTry(rule) => {
                    let mut allowed = Vec::new();
                    for declaration in &rule.declarations {
                        if declaration.important {
                            self.warn(
                                ctx,
                                declaration,
                                "`!important` is not allowed in @position-try".to_owned(),
                            );
                        } else if !allowed_in_position_try(&declaration.name) {
                            self.warn(
                                ctx,
                                declaration,
                                format!("`{}` is not allowed in @position-try", declaration.name),
                            );
                        } else {
                            allowed.push(declaration.clone());
                        }
                    }
                    let declarations = self
                        .compile_declarations(&allowed, ctx)
                        .into_iter()
                        .map(|(declaration, _)| declaration)
                        .collect();
                    self.position_try.push(CompiledPositionTry {
                        origin: ctx.origin,
                        layer,
                        order: self.order,
                        name: Arc::from(rule.name.as_str()),
                        declarations,
                    });
                    self.order += 1;
                }
                CssRule::ColorProfile(rule) => {
                    let mut src = None;
                    let mut intent = moxcms::RenderingIntent::RelativeColorimetric;
                    for declaration in &rule.declarations {
                        let mut input = cssparser::Parser::new(&declaration.value);
                        let valid = match declaration.name.as_str() {
                            "src" => input
                                .parse_entirely(|input| {
                                    Ok::<_, cssparser::ParseError<()>>(
                                        input.expect_url_or_string()?.to_string(),
                                    )
                                })
                                .map(|url| src = Some(url))
                                .is_ok(),
                            "rendering-intent" => {
                                crate::color_profile::rendering_intent(&declaration.value)
                                    .map(|value| intent = value)
                                    .is_some()
                            }
                            // Component names are for relative colors, which profiles
                            // do not support here.
                            "components" => input
                                .parse_entirely(|input| {
                                    input.parse_comma_separated(|input| {
                                        Ok::<_, cssparser::ParseError<()>>(
                                            input.expect_ident()?.to_string(),
                                        )
                                    })
                                })
                                .is_ok(),
                            other => {
                                self.warn(
                                    ctx,
                                    declaration,
                                    format!("unknown descriptor `{other}` in @color-profile"),
                                );
                                continue;
                            }
                        };
                        if !valid {
                            self.warn(
                                ctx,
                                declaration,
                                format!(
                                    "invalid value for `{}`: `{}`",
                                    declaration.name, declaration.value
                                ),
                            );
                        }
                    }
                    match src {
                        Some(src) => {
                            self.color_profiles.push(CompiledColorProfile {
                                origin: ctx.origin,
                                layer,
                                order: self.order,
                                name: rule.name.clone(),
                                src,
                                intent,
                                source: ctx.source.to_owned(),
                                line: rule.line,
                                column: rule.column,
                            });
                            self.order += 1;
                        }
                        None => {
                            let at = Declaration {
                                name: String::new(),
                                value: String::new(),
                                important: false,
                                line: rule.line,
                                column: rule.column,
                            };
                            self.warn(
                                ctx,
                                &at,
                                format!("ignored @color-profile {}: it needs `src`", rule.name),
                            );
                        }
                    }
                }
                CssRule::Function { name, function } => {
                    self.functions.push(CompiledFunction {
                        origin: ctx.origin,
                        layer,
                        order: self.order,
                        name: name.clone(),
                        function: function.clone(),
                    });
                    self.order += 1;
                }
                CssRule::Property(rule) => {
                    match register(rule, (self.env.width, self.env.height)) {
                        Ok(registration) => {
                            self.properties.push(CompiledProperty {
                                origin: ctx.origin,
                                layer,
                                order: self.order,
                                name: rule.name.clone(),
                                registration,
                            });
                            self.order += 1;
                        }
                        Err(reason) => {
                            let at = Declaration {
                                name: String::new(),
                                value: String::new(),
                                important: false,
                                line: rule.line,
                                column: rule.column,
                            };
                            self.warn(
                                ctx,
                                &at,
                                format!("ignored @property {}: {reason}", rule.name),
                            );
                        }
                    }
                }
                CssRule::Keyframes(rule) => {
                    let keyframes = self.compile_keyframes(rule, ctx);
                    self.keyframes.push(CompiledKeyframes {
                        origin: ctx.origin,
                        layer,
                        order: self.order,
                        name: Arc::from(rule.name.as_str()),
                        keyframes,
                    });
                    self.order += 1;
                }
                CssRule::CustomMedia { .. } | CssRule::Import(_) => {}
            }
        }
    }

    /// The keyframes of an `@keyframes` rule, merged by offset with later declarations
    /// winning. Each keyframe's `animation-timing-function` and `animation-composition` apply
    /// to the interval it starts; other animation and transition properties are ignored.
    fn compile_keyframes(&mut self, rule: &KeyframesRule, ctx: &SheetContext) -> Keyframes {
        let mut frames: Vec<Keyframe> = Vec::new();
        for (offsets, declarations) in &rule.keyframes {
            let mut easing = None;
            let mut composition = None;
            let mut animated = Vec::new();
            for declaration in declarations {
                let mut input = cssparser::Parser::new(&declaration.value);
                match declaration.name.as_str() {
                    "animation-timing-function" => {
                        use crate::properties::Property;
                        easing = input.parse_entirely(Easing::parse).ok();
                        continue;
                    }
                    "animation-composition" => {
                        use crate::properties::Property;
                        composition = input.parse_entirely(AnimationComposition::parse).ok();
                        continue;
                    }
                    _ => {}
                }
                let compiled = self.compile_declarations(std::slice::from_ref(declaration), ctx);
                animated.extend(
                    compiled
                        .into_iter()
                        .map(|(declaration, _)| declaration)
                        .filter(|declaration| declaration.id().is_none_or(animatable)),
                );
            }
            for &offset in offsets {
                match frames.iter_mut().find(|frame| frame.offset == offset) {
                    Some(frame) => {
                        frame.declarations.extend(animated.iter().cloned());
                        frame.easing = easing.clone().or(frame.easing.take());
                        frame.composition = composition.or(frame.composition);
                    }
                    None => frames.push(Keyframe {
                        offset,
                        declarations: animated.clone(),
                        easing: easing.clone(),
                        composition,
                    }),
                }
            }
        }
        frames.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        Keyframes { frames }
    }

    fn warn(&mut self, ctx: &SheetContext, declaration: &Declaration, message: String) {
        if ctx.origin != Origin::UserAgent {
            self.warnings.push(StyleWarning {
                source: ctx.source.to_owned(),
                warning: Warning {
                    line: declaration.line,
                    column: declaration.column,
                    message,
                },
            });
        }
    }

    fn compile_declarations(
        &mut self,
        declarations: &[Declaration],
        ctx: &SheetContext,
    ) -> Vec<(PropertyDeclaration, bool)> {
        let mut out = Vec::new();
        for declaration in declarations {
            match parse_declaration(&declaration.name, &declaration.value) {
                Ok(parsed) => out.extend(parsed.into_iter().map(|d| (d, declaration.important))),
                Err(message) if ctx.origin != Origin::UserAgent => {
                    self.warnings.push(StyleWarning {
                        source: ctx.source.to_owned(),
                        warning: Warning {
                            line: declaration.line,
                            column: declaration.column,
                            message,
                        },
                    })
                }
                Err(_) => {}
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Matching and computing

/// Priority of a declaration; larger keys win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CascadeKey {
    /// Origin and importance (CSS Cascade 5 § 6.2).
    origin_importance: u8,
    /// Style attributes win over rules.
    attached: bool,
    layer: u32,
    specificity: Specificity,
    /// Closer scope roots win: `u32::MAX` minus the number of generations to the root.
    proximity: u32,
    order: u32,
}

fn origin_importance(origin: Origin, important: bool) -> u8 {
    match (origin, important) {
        (Origin::UserAgent, false) => 1,
        (Origin::User, false) => 2,
        (Origin::PresentationalHint, _) => 3,
        (Origin::Author, false) => 4,
        (Origin::Author, true) => 6,
        (Origin::User, true) => 7,
        (Origin::UserAgent, true) => 8,
    }
}

/// The animation origin, between normal and important author declarations.
const ANIMATIONS: u8 = 5;

/// What an animated property is: a longhand or a custom property.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Animated {
    Longhand(LonghandId),
    Custom(Arc<str>),
}

impl Animated {
    fn name(&self) -> &str {
        match self {
            Animated::Longhand(id) => id.name(),
            Animated::Custom(name) => name,
        }
    }

    /// The computed value in `style` as text.
    fn value(&self, style: &ComputedValues) -> Option<String> {
        match self {
            Animated::Longhand(id) => Some(style.serialize(*id)),
            Animated::Custom(name) => style
                .custom
                .get(&**name)
                .map(|v| v.replace("/**/", "").trim().to_owned()),
        }
    }
}

/// An element's style, the styles of its pseudo-elements, and its styles with the
/// `@position-try` rules it names applied.
type ElementStyles = (
    ComputedValues,
    Vec<(PseudoElement, ComputedValues)>,
    Vec<(Arc<str>, ComputedValues)>,
);

/// A declaration that applies to an element, with its priority.
struct Applicable<'r> {
    declaration: &'r PropertyDeclaration,
    key: CascadeKey,
    origin: Origin,
    important: bool,
}

impl<'r> Applicable<'r> {
    fn copy(&self) -> Self {
        Applicable {
            declaration: self.declaration,
            key: self.key,
            origin: self.origin,
            important: self.important,
        }
    }
}

struct Engine<'a> {
    document: &'a Document,
    rules: &'a Rules,
    options: &'a StyleOptions<'a>,
    /// The root element's style, for `rem` and `rlh`.
    root: Option<Arc<ComputedValues>>,
    /// The laid out sizes of size containers.
    containers: &'a ContainerSizes,
    /// The ancestors of the element being styled, with their styles, nearest last.
    ancestors: std::cell::RefCell<Vec<(NodeId, Arc<ComputedValues>)>>,
}

impl Engine<'_> {
    /// The metrics of a style's first available font.
    fn font_metrics(&self, style: &ComputedValues) -> FontMetrics {
        match self.options.font_metrics {
            Some(metrics) => metrics(style),
            None => FontMetrics::fallback(style.font_size),
        }
    }

    /// Computes the style of every kind of page: each page name the document uses (and no
    /// name), first or not, left or right, blank or not.
    fn style_pages(&self, styles: &mut Styles) {
        let mut names: Vec<Option<Arc<str>>> = vec![None];
        for style in styles.elements.values() {
            if style.page.is_some() && !names.contains(&style.page) {
                names.push(style.page.clone());
            }
        }
        names.sort();
        let root = self
            .root
            .clone()
            .unwrap_or_else(|| Arc::new(ComputedValues::initial()));
        for name in names {
            for first in [true, false] {
                for side in [PageSide::Left, PageSide::Right] {
                    for blank in [false, true] {
                        let key = PageKey {
                            name: name.clone(),
                            first,
                            side,
                            blank,
                        };
                        let style = self.page_style(&key, &root);
                        styles.pages.insert(key, Arc::new(style));
                    }
                }
            }
        }
    }

    /// Cascades the `@page` rules that match a page (CSS Paged Media 3 § 3.4). The page
    /// context inherits from the root element; margin boxes inherit from the page context.
    fn page_style(&self, page: &PageKey, root: &ComputedValues) -> PageStyle {
        let mut applicable: Vec<Applicable> = Vec::new();
        let mut descriptors: Vec<(CascadeKey, &Descriptor)> = Vec::new();
        let mut boxes: Vec<(MarginBox, Vec<Applicable>)> = Vec::new();
        for rule in &self.rules.pages {
            let Some((a, b, c)) = rule
                .selectors
                .iter()
                .filter(|selector| selector.matches(page))
                .map(PageSelector::specificity)
                .max()
            else {
                continue;
            };
            let rank = self.rules.layer_ranks[rule.layer];
            let key = |important: bool| CascadeKey {
                origin_importance: origin_importance(rule.origin, important),
                attached: false,
                layer: if important { u32::MAX - rank } else { rank },
                specificity: Specificity(a, b, c),
                proximity: u32::MAX,
                order: rule.order,
            };
            applicable.extend(page_entries(&rule.declarations, &key, rule.origin));
            descriptors.extend(rule.descriptors.iter().map(|(d, i)| (key(*i), d)));
            for (name, declarations) in &rule.margin_boxes {
                let entries = page_entries(declarations, &key, rule.origin);
                match boxes.iter_mut().find(|(n, _)| n == name) {
                    Some((_, list)) => list.extend(entries),
                    None => boxes.push((*name, entries)),
                }
            }
        }

        let sides = [
            LonghandId::MarginTop,
            LonghandId::MarginRight,
            LonghandId::MarginBottom,
            LonghandId::MarginLeft,
        ];
        let margins_set = sides.map(|side| {
            applicable.iter().any(|a| {
                a.declaration.id().is_some_and(|id| {
                    id.to_physical(WritingMode::HorizontalTb, Direction::Ltr) == side
                })
            })
        });
        let element = self
            .document
            .document_element()
            .unwrap_or(self.document.root());
        let style = Arc::new(self.compute(element, root, applicable, (1, 1)));

        descriptors.sort_by_key(|(key, _)| *key);
        let mut out = default_page_style();
        out.margins_set = margins_set;
        for (_, descriptor) in descriptors {
            match descriptor {
                Descriptor::Size(size) => out.size = *size,
                Descriptor::Orientation(orientation) => out.orientation = *orientation,
                Descriptor::Bleed(bleed) => out.bleed = *bleed,
                Descriptor::Marks(marks) => out.marks = *marks,
                Descriptor::MarginSafety(safety) => out.margin_safety = *safety,
            }
        }
        // Margin boxes align their content by default as CSS Paged Media 3 § 5.3 lists.
        for (name, list) in &mut boxes {
            if let Some((_, declarations)) = margin_box_defaults().iter().find(|(n, _)| n == name) {
                list.extend(declarations.iter().map(|declaration| Applicable {
                    declaration,
                    key: CascadeKey {
                        origin_importance: origin_importance(Origin::UserAgent, false),
                        attached: false,
                        layer: 0,
                        specificity: Specificity::default(),
                        proximity: u32::MAX,
                        order: 0,
                    },
                    origin: Origin::UserAgent,
                    important: false,
                }));
            }
        }
        boxes.sort_by_key(|(name, _)| *name);
        out.margin_boxes = boxes
            .into_iter()
            .map(|(name, applicable)| {
                (
                    name,
                    Arc::new(self.compute(element, &style, applicable, (1, 1))),
                )
            })
            .collect();
        out.style = style;
        out
    }

    fn style_tree(&mut self, node: NodeId, parent: &Arc<ComputedValues>, styles: &mut Styles) {
        for child in self.document.children(node).collect::<Vec<_>>() {
            if self.document.element(child).is_none() {
                continue;
            }
            let (style, pseudo, tries) = self.style_element(child, parent, styles);
            let style = Arc::new(style);
            for (name, variant) in tries {
                styles.position_try.insert((child, name), Arc::new(variant));
            }
            if self.root.is_none() {
                self.root = Some(style.clone());
            }
            for (kind, pseudo_style) in pseudo {
                styles
                    .pseudo_elements
                    .insert((child, kind), Arc::new(pseudo_style));
            }
            styles.elements.insert(child, style.clone());
            styles.size_containers |= style.container_type.size.is_some();
            self.ancestors.borrow_mut().push((child, style.clone()));
            self.style_tree(child, &style, styles);
            self.ancestors.borrow_mut().pop();
        }
    }

    /// Computes an element's style and the styles of its pseudo-elements.
    fn style_element(
        &self,
        element: NodeId,
        parent: &ComputedValues,
        styles: &mut Styles,
    ) -> ElementStyles {
        rustyprint_dom::budget::check_deadline();
        let hints = self.hints(element);
        let attribute = self.style_attribute(element, styles);
        let matched = self.matching_rules(element);

        let mut applicable: Vec<Applicable> = Vec::new();
        let mut pseudo_rules: Vec<(PseudoElement, Applicable)> = Vec::new();
        // Rules of pseudo-elements in `@container` rules, which query once the element is
        // styled.
        let mut queried: Vec<(usize, usize)> = Vec::new();
        for (index, proximity) in &matched {
            let rule = &self.rules.rules[*index];
            for (declaration, important) in rule.declarations.iter() {
                let rank = self.rules.layer_ranks[rule.layer];
                let entry = Applicable {
                    declaration,
                    key: CascadeKey {
                        origin_importance: origin_importance(rule.origin, *important),
                        attached: false,
                        // Important declarations in earlier layers win.
                        layer: if *important { u32::MAX - rank } else { rank },
                        specificity: rule.specificity,
                        proximity: u32::MAX - proximity,
                        order: rule.order,
                    },
                    origin: rule.origin,
                    important: *important,
                };
                match &rule.selector.pseudo_element {
                    Some(pseudo) => {
                        if rule.container.is_some() {
                            queried.push((pseudo_rules.len(), *index));
                        }
                        pseudo_rules.push((pseudo.clone(), entry));
                    }
                    None => applicable.push(entry),
                }
            }
        }
        for declaration in &hints {
            applicable.push(Applicable {
                declaration,
                key: CascadeKey {
                    origin_importance: origin_importance(Origin::PresentationalHint, false),
                    attached: false,
                    layer: 0,
                    specificity: Specificity::default(),
                    proximity: u32::MAX,
                    order: 0,
                },
                origin: Origin::PresentationalHint,
                important: false,
            });
        }
        for (declaration, important) in &attribute {
            applicable.push(Applicable {
                declaration,
                key: CascadeKey {
                    origin_importance: origin_importance(Origin::Author, *important),
                    attached: true,
                    layer: 0,
                    specificity: Specificity::default(),
                    proximity: u32::MAX,
                    order: 0,
                },
                origin: Origin::Author,
                important: *important,
            });
        }

        let siblings = self.sibling_position(element);
        // The declarations again, for styles with `@position-try` rules applied.
        let saved: Vec<Applicable> = if self.rules.position_try.is_empty() {
            Vec::new()
        } else {
            applicable.iter().map(Applicable::copy).collect()
        };
        let mut style = self.compute_animated(element, parent, applicable, siblings);
        if self.document.document_element() == Some(element) {
            style.display = blockify(style.display);
        }

        // Pseudo-elements can query their originating element as a container.
        if !queried.is_empty() {
            let failed: Vec<usize> = queried
                .iter()
                .filter(|(_, index)| {
                    let chain = self.rules.rules[*index].container.as_ref();
                    !chain.is_none_or(|chain| self.container_holds(chain, Some((element, &style))))
                })
                .map(|(position, _)| *position)
                .collect();
            let mut position = 0;
            pseudo_rules.retain(|_| {
                position += 1;
                !failed.contains(&(position - 1))
            });
        }

        // Pseudo-elements inherit from their originating element.
        // The UA styles every `::marker`, but only list items have one unless another
        // origin targets it.
        let list_item = matches!(
            style.display,
            Display::Normal {
                list_item: true,
                ..
            }
        );
        let mut kinds: Vec<PseudoElement> = Vec::new();
        for (kind, entry) in &pseudo_rules {
            let wanted =
                *kind != PseudoElement::Marker || list_item || entry.origin != Origin::UserAgent;
            if wanted && !kinds.contains(kind) {
                kinds.push(kind.clone());
            }
        }
        let pseudo = kinds
            .into_iter()
            .filter(|kind| !matches!(kind, PseudoElement::Never(_)))
            .map(|kind| {
                let declarations: Vec<Applicable> = pseudo_rules
                    .iter()
                    .filter(|(k, _)| *k == kind)
                    .map(|(_, a)| Applicable {
                        declaration: a.declaration,
                        key: a.key,
                        origin: a.origin,
                        important: a.important,
                    })
                    .collect();
                let pseudo_style = self.compute_animated(element, &style, declarations, siblings);
                (kind, pseudo_style)
            })
            .collect();
        // Positioned boxes get a style for each `@position-try` rule they name, whose
        // declarations win over the author's normal ones (CSS Anchor Positioning 1 § 4.4).
        let mut tries = Vec::new();
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            for fallback in style.position_try_fallbacks.iter() {
                let TryFallback::Rule {
                    name: Some(name), ..
                } = fallback
                else {
                    continue;
                };
                let Some(declarations) = self.rules.position_try.get(name) else {
                    continue;
                };
                if tries.iter().any(|(n, _): &(Arc<str>, _)| n == name) {
                    continue;
                }
                let mut applicable: Vec<Applicable> = saved.iter().map(Applicable::copy).collect();
                applicable.extend(declarations.iter().map(|declaration| Applicable {
                    declaration,
                    key: CascadeKey {
                        origin_importance: origin_importance(Origin::Author, false),
                        attached: true,
                        layer: u32::MAX,
                        specificity: Specificity(u32::MAX, u32::MAX, u32::MAX),
                        proximity: u32::MAX,
                        order: u32::MAX,
                    },
                    origin: Origin::Author,
                    important: false,
                }));
                let variant = self.compute_animated(element, parent, applicable, siblings);
                tries.push((name.clone(), variant));
            }
        }
        (style, pseudo, tries)
    }

    fn hints(&self, element: NodeId) -> Vec<PropertyDeclaration> {
        presentational_hints(self.document, element)
            .into_iter()
            .flat_map(|(name, value)| parse_declaration(name, &value).unwrap_or_default())
            .collect()
    }

    fn style_attribute(
        &self,
        element: NodeId,
        styles: &mut Styles,
    ) -> Vec<(PropertyDeclaration, bool)> {
        let Some(css) = self.document.element(element).and_then(|e| e.attr("style")) else {
            return Vec::new();
        };
        let (declarations, warnings) = Stylesheet::parse_style_attribute(css);
        let source = "style attribute".to_owned();
        styles
            .warnings
            .extend(warnings.into_iter().map(|warning| StyleWarning {
                source: source.clone(),
                warning,
            }));
        let mut out = Vec::new();
        for declaration in declarations {
            match parse_declaration(&declaration.name, &declaration.value) {
                Ok(parsed) => out.extend(parsed.into_iter().map(|d| (d, declaration.important))),
                Err(message) => styles.warnings.push(StyleWarning {
                    source: source.clone(),
                    warning: Warning {
                        line: declaration.line,
                        column: declaration.column,
                        message,
                    },
                }),
            }
        }
        out
    }

    /// Rules whose selector matches `element`, with their scope proximity.
    fn matching_rules(&self, element: NodeId) -> Vec<(usize, u32)> {
        let el = self.document.element(element).expect("styling an element");
        let mut candidates: Vec<usize> = self.rules.universal.clone();
        if let Some(id) = el.id() {
            candidates.extend(
                self.rules
                    .by_id
                    .get(&id.to_ascii_lowercase())
                    .into_iter()
                    .flatten(),
            );
        }
        for class in el.classes() {
            candidates.extend(
                self.rules
                    .by_class
                    .get(&class.to_ascii_lowercase())
                    .into_iter()
                    .flatten(),
            );
        }
        let lower = LocalName::from(el.name.local.to_ascii_lowercase());
        candidates.extend(self.rules.by_type.get(&lower).into_iter().flatten());
        candidates.sort_unstable();
        candidates.dedup();

        let ctx = MatchContext::new(self.document);
        candidates
            .into_iter()
            .filter_map(|index| {
                let rule = &self.rules.rules[index];
                let matched = match &rule.scope {
                    None => rule.selector.matches(&ctx, element).then_some((index, 0)),
                    Some(scope) => self
                        .scoped_match(&rule.selector, scope, element)
                        .map(|p| (index, p)),
                }?;
                // Pseudo-elements can query their originating element, which is styled
                // later.
                let queried = rule.selector.pseudo_element.is_some()
                    || rule
                        .container
                        .as_ref()
                        .is_none_or(|chain| self.container_holds(chain, None));
                queried.then_some(matched)
            })
            .collect()
    }

    /// The candidate query containers of the element being styled, nearest first: `own`
    /// (an element whose pseudo-element is styled), then its ancestors.
    fn candidates<'s>(
        &self,
        ancestors: &'s [(NodeId, Arc<ComputedValues>)],
        own: Option<(NodeId, &'s ComputedValues)>,
    ) -> Vec<Candidate<'s>> {
        own.into_iter()
            .chain(
                ancestors
                    .iter()
                    .rev()
                    .map(|(node, style)| (*node, &**style)),
            )
            .map(|(node, style)| Candidate {
                style,
                size: self.containers.get(&node).copied(),
            })
            .collect()
    }

    /// Whether every `@container` rule in `chain` has a condition that holds.
    fn container_holds(
        &self,
        chain: &ContainerChain,
        own: Option<(NodeId, &ComputedValues)>,
    ) -> bool {
        let ancestors = self.ancestors.borrow();
        let candidates = self.candidates(&ancestors, own);
        chain
            .iter()
            .all(|conditions| crate::container::matches(conditions, &candidates, &style_query))
    }

    /// The width and height that container units refer to: those of the nearest size
    /// containers in each axis, or the page area.
    fn container_size(&self) -> (f32, f32) {
        let (mut width, mut height) = (None, None);
        for (node, style) in self.ancestors.borrow().iter().rev() {
            let (Some(kind), Some(&(w, h))) =
                (style.container_type.size, self.containers.get(node))
            else {
                continue;
            };
            let both = kind == crate::properties::types::ContainerSize::Both;
            let vertical = style.writing_mode.is_vertical();
            if width.is_none() && (both || !vertical) {
                width = Some(w);
            }
            if height.is_none() && (both || vertical) {
                height = Some(h);
            }
            if width.is_some() && height.is_some() {
                break;
            }
        }
        let (page_width, page_height) = self.options.page_size;
        (width.unwrap_or(page_width), height.unwrap_or(page_height))
    }

    /// Matches a selector inside `@scope`, returning the number of generations between the
    /// element and its nearest scope root.
    fn scoped_match(&self, selector: &Selector, scope: &Scope, element: NodeId) -> Option<u32> {
        let doc = self.document;
        let ctx = MatchContext::new(doc);
        let chain: Vec<NodeId> = std::iter::once(element)
            .chain(doc.ancestors(element).filter(|&a| doc.element(a).is_some()))
            .collect();
        for (proximity, &root) in chain.iter().enumerate() {
            let is_root = match &scope.start {
                Some(start) => start.matches(&ctx, root),
                None => {
                    let implicit = scope
                        .implicit_root
                        .and_then(|owner| doc.parent_element(owner))
                        .or_else(|| doc.document_element());
                    implicit == Some(root)
                }
            };
            if !is_root {
                continue;
            }
            let scoped = MatchContext {
                document: doc,
                scope: Some(root),
            };
            // Elements at or below a scope limit are out of scope.
            let limited = scope
                .end
                .as_ref()
                .is_some_and(|end| chain[..proximity].iter().any(|&n| end.matches(&scoped, n)));
            if !limited && selector.matches(&scoped, element) {
                return Some(proximity as u32);
            }
        }
        None
    }

    /// `(sibling-index(), sibling-count())`.
    fn sibling_position(&self, element: NodeId) -> (u32, u32) {
        let doc = self.document;
        let Some(parent) = doc.node(element).parent() else {
            return (1, 1);
        };
        let siblings: Vec<NodeId> = doc.element_children(parent).collect();
        let index = siblings.iter().position(|&s| s == element).unwrap_or(0);
        (index as u32 + 1, siblings.len() as u32)
    }

    /// Runs the cascade and computes values, with the animations the style names applied as
    /// they are at time zero.
    fn compute_animated(
        &self,
        element: NodeId,
        parent: &ComputedValues,
        applicable: Vec<Applicable>,
        siblings: (u32, u32),
    ) -> ComputedValues {
        if self.rules.keyframes.is_empty() {
            return self.compute(element, parent, applicable, siblings);
        }
        let saved: Vec<Applicable> = applicable.iter().map(Applicable::copy).collect();
        let style = self.compute(element, parent, applicable, siblings);
        let animated = self.animated_declarations(element, parent, &saved, siblings, &style);
        if animated.is_empty() {
            return style;
        }
        let mut applicable = saved;
        applicable.extend(animated.iter().map(animation_entry));
        self.compute(element, parent, applicable, siblings)
    }

    /// The values the animations of `style` give at time zero, as declarations for the
    /// animation origin. Later animations in `animation-name` override or add to earlier
    /// ones.
    fn animated_declarations(
        &self,
        element: NodeId,
        parent: &ComputedValues,
        base: &[Applicable],
        siblings: (u32, u32),
        style: &ComputedValues,
    ) -> Vec<PropertyDeclaration> {
        let animations = crate::animate::named(&style.animation_name, &self.rules.keyframes);
        if animations.is_empty() {
            return Vec::new();
        }
        fn item<T>(list: &[T], index: usize) -> &T {
            &list[index % list.len()]
        }
        let mut current: std::collections::BTreeMap<Animated, String> = Default::default();
        for (index, keyframes) in animations {
            // Scroll and view timelines never move without scrolling, and `none` is no
            // timeline: only the document timeline runs.
            if *item(&style.animation_timeline, index) != AnimationTimeline::Auto {
                continue;
            }
            let timing = Timing {
                duration: item(&style.animation_duration, index).unwrap_or(0.0),
                delay: *item(&style.animation_delay, index),
                iterations: *item(&style.animation_iteration_count, index),
                direction: *item(&style.animation_direction, index),
                fill: *item(&style.animation_fill_mode, index),
            };
            let Some(progress) = progress_at_zero(timing) else {
                continue;
            };
            let easing = item(&style.animation_timing_function, index).clone();
            let composition = *item(&style.animation_composition, index);
            // Each keyframe's style, and the properties it sets.
            let frames: Vec<(&Keyframe, ComputedValues, Vec<Animated>)> = keyframes
                .frames
                .iter()
                .map(|frame| {
                    let mut applicable: Vec<Applicable> =
                        base.iter().map(Applicable::copy).collect();
                    applicable.extend(frame.declarations.iter().map(animation_entry));
                    let frame_style = self.compute(element, parent, applicable, siblings);
                    let properties = frame
                        .declarations
                        .iter()
                        .map(|declaration| match declaration {
                            PropertyDeclaration::Custom { name, .. } => {
                                Animated::Custom(name.clone())
                            }
                            other => Animated::Longhand(
                                other
                                    .id()
                                    .expect("not a custom property")
                                    .to_physical(style.writing_mode, style.direction),
                            ),
                        })
                        .collect();
                    (frame, frame_style, properties)
                })
                .collect();
            let mut properties: Vec<Animated> = frames
                .iter()
                .flat_map(|(_, _, properties)| properties.iter().cloned())
                .collect();
            properties.sort();
            properties.dedup();
            for property in properties {
                let Some(underlying) = current
                    .get(&property)
                    .cloned()
                    .or_else(|| property.value(style))
                else {
                    continue;
                };
                let implicit = |offset: f32| PropertyFrame {
                    offset,
                    value: underlying.clone(),
                    easing: easing.clone(),
                    composition: AnimationComposition::Replace,
                };
                let mut list: Vec<PropertyFrame> = frames
                    .iter()
                    .filter(|(_, _, properties)| properties.contains(&property))
                    .filter_map(|(frame, frame_style, _)| {
                        Some(PropertyFrame {
                            offset: frame.offset,
                            value: property.value(frame_style)?,
                            easing: frame.easing.clone().unwrap_or_else(|| easing.clone()),
                            composition: frame.composition.unwrap_or(composition),
                        })
                    })
                    .collect();
                if list.first().is_none_or(|frame| frame.offset > 0.0) {
                    list.insert(0, implicit(0.0));
                }
                if list.last().is_none_or(|frame| frame.offset < 1.0) {
                    list.push(implicit(1.0));
                }
                // Registered custom properties with numeric or color syntaxes interpolate;
                // others animate discretely.
                let kind = match &property {
                    Animated::Custom(name)
                        if self
                            .rules
                            .registered
                            .get(&**name)
                            .is_some_and(|r| r.syntax.interpolates()) =>
                    {
                        ""
                    }
                    other => other.name(),
                };
                let value = sample(kind, &list, &underlying, progress);
                current.insert(property, value);
            }
        }
        current
            .into_iter()
            .filter_map(|(property, value)| match property {
                Animated::Longhand(id) => {
                    let mut parsed = parse_declaration(id.name(), &value).ok()?;
                    (parsed.len() == 1).then(|| parsed.pop().expect("one declaration"))
                }
                Animated::Custom(name) => Some(PropertyDeclaration::Custom {
                    name,
                    value: CustomValue::Value(Arc::from(value)),
                }),
            })
            .collect()
    }

    /// Runs `f` with what substitution needs for `element`: its attributes, the custom
    /// functions, and how conditions in `if()` and function bodies are tested.
    fn with_environment<T>(&self, element: NodeId, f: impl FnOnce(&Environment) -> T) -> T {
        let attr = |name: &str| {
            self.document
                .element(element)
                .and_then(|e| e.attr(name))
                .map(str::to_owned)
        };
        let media = |text: &str| {
            let queries = MediaQueryList::parse(&mut cssparser::Parser::new(text));
            queries.matches(&MediaEnvironment {
                width: self.options.page_size.0,
                height: self.options.page_size.1,
                custom_media: &self.rules.custom_media,
            })
        };
        let supports = |text: &str| {
            let mut input = cssparser::Parser::new(text);
            input
                .parse_entirely(crate::supports::SupportsCondition::parse)
                .is_ok_and(|condition| {
                    condition.evaluate(&crate::selector::Namespaces::default(), &|name, value| {
                        parse_declaration(name, value).is_ok()
                    })
                })
        };
        let container = |text: &str| {
            let mut input = cssparser::Parser::new(text);
            input
                .parse_entirely(crate::container::parse_conditions)
                .is_ok_and(|conditions| {
                    self.container_holds(&vec![Arc::from(conditions.as_slice())], None)
                })
        };
        let env = Environment::new(&attr, &self.rules.functions, &media, &supports, &container);
        f(&env)
    }

    /// The custom properties an element inherits: its parent's, with registered properties
    /// that do not inherit back at their initial values.
    fn inherited_custom(&self, parent: &ComputedValues) -> crate::custom::CustomProperties {
        let mut reset = None;
        for (name, registration) in &self.rules.registered {
            if registration.inherits {
                continue;
            }
            let current = parent.custom.get(name.as_str());
            if current == registration.initial.as_ref() {
                continue;
            }
            let map = reset.get_or_insert_with(|| (*parent.custom).clone());
            match &registration.initial {
                Some(initial) => map.insert(name.clone(), initial.clone()),
                None => map.remove(name.as_str()),
            };
        }
        reset.map_or_else(|| parent.custom.clone(), Arc::new)
    }

    /// Runs the cascade over `applicable` declarations and computes values.
    #[allow(clippy::too_many_lines)]
    fn compute(
        &self,
        element: NodeId,
        parent: &ComputedValues,
        mut applicable: Vec<Applicable>,
        siblings: (u32, u32),
    ) -> ComputedValues {
        applicable.sort_by_key(|a| a.key);
        let initial = initial_values();
        let mut style = initial.clone();
        style.font_rules = parent.font_rules.clone();

        // Custom properties first: other values substitute them.
        let mut custom = Vec::new();
        let custom_names: Vec<&str> = {
            let mut names: Vec<&str> = applicable
                .iter()
                .filter_map(|a| match a.declaration {
                    PropertyDeclaration::Custom { name, .. } => Some(&**name),
                    _ => None,
                })
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        };
        let registered = &self.rules.registered;
        // A registered property's value without a declaration: inherited or initial.
        let default_custom = |name: &str| -> Option<String> {
            match registered.get(name) {
                Some(r) if !r.inherits => r.initial.as_deref().map(str::to_owned),
                _ => parent.custom.get(name).map(|value| value.to_string()),
            }
        };
        for name in custom_names {
            let winner = cascade_winner(applicable.iter().filter(|a| {
                matches!(a.declaration, PropertyDeclaration::Custom { name: n, .. } if &**n == name)
            }));
            let value = match winner.map(|a| a.declaration) {
                Some(PropertyDeclaration::Custom {
                    value: CustomValue::Value(value),
                    ..
                }) => Some(value.to_string()),
                Some(PropertyDeclaration::Custom {
                    value: CustomValue::CssWide(keyword),
                    ..
                }) => match keyword {
                    CssWideKeyword::Initial => registered
                        .get(name)
                        .and_then(|r| r.initial.as_deref().map(str::to_owned)),
                    CssWideKeyword::Inherit => parent.custom.get(name).map(|v| v.to_string()),
                    _ => default_custom(name),
                },
                _ => default_custom(name),
            };
            custom.push((name.to_owned(), value));
        }
        let inherited = self.inherited_custom(parent);
        style.custom = self.with_environment(element, |env| {
            compute_custom_properties(&inherited, &custom, env)
        });

        let root = self.root.as_deref();
        // Font-relative units in `font-size` refer to the parent's font.
        let parent_metrics = self.font_metrics(parent);
        let root_metrics = root.map(|r| self.font_metrics(r));
        let mut ctx = ComputeContext {
            parent,
            root_font_size: root.map_or(16.0, |r| r.font_size),
            font_size: parent.font_size,
            line_height: used_line_height(parent, &parent_metrics),
            root_line_height: root
                .zip(root_metrics)
                .map_or(16.0 * 1.2, |(r, m)| used_line_height(r, &m)),
            font_metrics: parent_metrics,
            root_font_metrics: root_metrics.unwrap_or(parent_metrics),
            color: parent.color,
            viewport: self.options.page_size,
            vertical: false,
            rtl: false,
            dark: parent.color_scheme.dark(),
            zoom: parent.effective_zoom,
            siblings,
            random_seed: Some(crate::values::fnv(format!("{element:?}").as_bytes())),
            container: self.container_size(),
        };

        // Writing mode and direction decide which physical property a logical one maps to.
        for &id in EARLY {
            let winner =
                cascade_winner(applicable.iter().filter(|a| a.declaration.id() == Some(id)));
            self.apply(&mut style, id, winner, &ctx, initial, element);
            // Inherited lengths were zoomed for the parent: they also take this element's zoom.
            let inherits = winner.is_none_or(|w| {
                matches!(
                    w.declaration,
                    PropertyDeclaration::CssWide(
                        _,
                        CssWideKeyword::Inherit | CssWideKeyword::Unset
                    )
                )
            });
            if inherits && style.zoom != 1.0 {
                match id {
                    LonghandId::FontSize => style.font_size *= style.zoom,
                    LonghandId::LineHeight => {
                        if let LineHeight::Length(px) = &mut style.line_height {
                            *px *= style.zoom;
                        }
                    }
                    _ => {}
                }
            }
            match id {
                LonghandId::Zoom => {
                    style.effective_zoom = parent.effective_zoom * style.zoom;
                    ctx.zoom = style.effective_zoom;
                }
                LonghandId::WritingMode => ctx.vertical = style.writing_mode.is_vertical(),
                LonghandId::Direction => ctx.rtl = style.direction == Direction::Rtl,
                LonghandId::FontSize => {
                    ctx.font_size = style.font_size;
                    ctx.font_metrics = self.font_metrics(&style);
                    if root.is_none() {
                        ctx.root_font_size = style.font_size;
                        ctx.root_font_metrics = ctx.font_metrics;
                    }
                }
                LonghandId::LineHeight => {
                    ctx.line_height = used_line_height(&style, &ctx.font_metrics);
                    if root.is_none() {
                        ctx.root_line_height = ctx.line_height;
                    }
                }
                LonghandId::ColorScheme => ctx.dark = style.color_scheme.dark(),
                LonghandId::Color => {
                    // The initial color is `CanvasText`, which is light in a dark scheme.
                    if winner.is_none() && root.is_none() && ctx.dark {
                        style.color = AbsoluteColor::WHITE;
                    }
                    ctx.color = style.color;
                }
                _ => {}
            }
        }
        // Registered custom properties compute by their syntax, now that the element's font
        // and color are known. A value that does not match behaves as `unset`.
        if !registered.is_empty() {
            let mut computed = None;
            for (name, declared) in &custom {
                let Some(registration) = registered.get(name.as_str()) else {
                    continue;
                };
                if registration.syntax.is_universal() || declared.is_none() {
                    continue;
                }
                let value = style
                    .custom
                    .get(name.as_str())
                    .and_then(|raw| registration.syntax.compute(&raw.replace("/**/", ""), &ctx))
                    .map(Arc::<str>::from)
                    .or_else(|| {
                        if registration.inherits {
                            parent.custom.get(name.as_str()).cloned()
                        } else {
                            registration.initial.clone()
                        }
                    });
                let map = computed.get_or_insert_with(|| (*style.custom).clone());
                match value {
                    Some(value) => map.insert(name.clone(), value),
                    None => map.remove(name.as_str()),
                };
            }
            if let Some(map) = computed {
                style.custom = Arc::new(map);
            }
        }
        let (writing_mode, direction) = (style.writing_mode, style.direction);
        // A physical property's value comes from it or any logical property mapping to it:
        // the declarations by the physical property they set, each in cascade order.
        let mut by_property: Vec<(LonghandId, &Applicable)> = applicable
            .iter()
            .filter_map(|a| Some((a.declaration.id()?.to_physical(writing_mode, direction), a)))
            .collect();
        by_property.sort_by_key(|(id, _)| *id);
        for &id in LonghandId::ALL {
            if EARLY.contains(&id) || id.is_logical() {
                continue;
            }
            let start = by_property.partition_point(|(p, _)| *p < id);
            let end = by_property.partition_point(|(p, _)| *p <= id);
            let winner = cascade_winner(by_property[start..end].iter().map(|(_, a)| *a));
            // The style starts from the initial values, which reset properties keep.
            if winner.is_none() && !id.inherited() {
                continue;
            }
            match winner.map(|w| w.declaration) {
                // A winning logical declaration sets the physical property.
                Some(declaration) if declaration.id().is_some_and(|d| d != id) => {
                    let mapped = map_declaration(declaration, id);
                    self.apply_declaration(&mut style, id, &mapped, &ctx, initial, element);
                }
                _ => self.apply(&mut style, id, winner, &ctx, initial, element),
            }
        }
        fixup(&mut style, parent);
        style
    }

    fn apply(
        &self,
        style: &mut ComputedValues,
        id: LonghandId,
        winner: Option<&Applicable>,
        ctx: &ComputeContext,
        initial: &ComputedValues,
        element: NodeId,
    ) {
        match winner {
            Some(winner) => {
                self.apply_declaration(style, id, winner.declaration, ctx, initial, element)
            }
            None => default_value(style, id, ctx.parent, initial),
        }
    }

    fn apply_declaration(
        &self,
        style: &mut ComputedValues,
        id: LonghandId,
        declaration: &PropertyDeclaration,
        ctx: &ComputeContext,
        initial: &ComputedValues,
        element: NodeId,
    ) {
        match declaration {
            PropertyDeclaration::Value(value) => style.set(value, ctx),
            PropertyDeclaration::CssWide(_, keyword) => match keyword {
                CssWideKeyword::Initial => style.copy(id, initial),
                CssWideKeyword::Inherit => style.copy(id, ctx.parent),
                // `revert` and `revert-layer` were resolved by `cascade_winner`; reaching here
                // means nothing was left to revert to.
                CssWideKeyword::Unset | CssWideKeyword::Revert | CssWideKeyword::RevertLayer => {
                    default_value(style, id, ctx.parent, initial)
                }
            },
            PropertyDeclaration::Unparsed {
                id: declared,
                shorthand,
                value,
            } => {
                let substituted = self.with_environment(element, |env| {
                    substitute(
                        value,
                        &Substitution {
                            custom: &style.custom,
                            env,
                        },
                    )
                });
                // A value that fails after substitution is invalid at computed-value time and
                // behaves as `unset`.
                match substituted.and_then(|text| parse_substituted(*declared, *shorthand, &text)) {
                    // A logical property's value is mapped to the physical property.
                    Some(value) => match map_declaration(&PropertyDeclaration::Value(value), id) {
                        PropertyDeclaration::Value(value) => style.set(&value, ctx),
                        _ => unreachable!("values map to values"),
                    },
                    None => default_value(style, id, ctx.parent, initial),
                }
            }
            PropertyDeclaration::Custom { .. } => {}
        }
    }
}

/// Evaluates a container `style()` query against a container's custom properties; tests of
/// other properties are unknown.
fn style_query(text: &str, container: &ComputedValues) -> Option<bool> {
    let functions = crate::custom::Functions::new();
    let no_attr = |_: &str| None;
    let never = |_: &str| false;
    let env = Environment::new(&no_attr, &functions, &never, &never, &never);
    let ctx = Substitution {
        custom: &container.custom,
        env: &env,
    };
    let mut input = cssparser::Parser::new(text);
    input
        .parse_entirely(|input| {
            crate::custom::boolean(input, &|input| {
                crate::custom::style_feature(input, &container.custom, &ctx)
            })
        })
        .ok()
}

/// A declaration of the animation origin.
fn animation_entry(declaration: &PropertyDeclaration) -> Applicable<'_> {
    Applicable {
        declaration,
        key: CascadeKey {
            origin_importance: ANIMATIONS,
            attached: false,
            layer: 0,
            specificity: Specificity::default(),
            proximity: u32::MAX,
            order: 0,
        },
        origin: Origin::Author,
        important: false,
    }
}

/// The highest-priority declaration among `candidates`, in cascade order, following `revert`
/// (back to the previous origin) and `revert-layer` (back to the previous layer).
fn cascade_winner<'a, 'r: 'a>(
    candidates: impl DoubleEndedIterator<Item = &'a Applicable<'r>>,
) -> Option<&'a Applicable<'r>> {
    let mut below_origin: Option<Origin> = None;
    let mut below_layer: Option<(u8, bool, u32)> = None;
    for candidate in candidates.rev() {
        if below_origin.is_some_and(|origin| origin_group(candidate.origin) >= origin_group(origin))
        {
            continue;
        }
        if let Some((oi, attached, layer)) = below_layer
            && candidate.key.origin_importance == oi
            && candidate.key.attached == attached
            && candidate.key.layer >= layer
        {
            continue;
        }
        match candidate.declaration {
            PropertyDeclaration::CssWide(_, CssWideKeyword::Revert) => {
                below_origin = Some(candidate.origin)
            }
            PropertyDeclaration::CssWide(_, CssWideKeyword::RevertLayer) => {
                below_layer = Some((
                    candidate.key.origin_importance,
                    candidate.key.attached,
                    candidate.key.layer,
                ));
            }
            _ => return Some(candidate),
        }
    }
    None
}

/// For `revert`, presentational hints belong to the author origin.
fn origin_group(origin: Origin) -> Origin {
    match origin {
        Origin::PresentationalHint => Origin::Author,
        other => other,
    }
}

/// Inherits or resets a property that has no winning declaration.
fn default_value(
    style: &mut ComputedValues,
    id: LonghandId,
    parent: &ComputedValues,
    initial: &ComputedValues,
) {
    if id.inherited() {
        style.copy(id, parent);
    } else {
        style.copy(id, initial);
    }
}

/// Rewrites a logical declaration so it sets the physical property `id`.
fn map_declaration(declaration: &PropertyDeclaration, id: LonghandId) -> PropertyDeclaration {
    match declaration {
        PropertyDeclaration::Value(value) => PropertyDeclaration::Value(value.retarget(id)),
        PropertyDeclaration::CssWide(_, keyword) => PropertyDeclaration::CssWide(id, *keyword),
        other => other.clone(),
    }
}

fn initial_values() -> &'static ComputedValues {
    static INITIAL: OnceLock<ComputedValues> = OnceLock::new();
    INITIAL.get_or_init(ComputedValues::initial)
}

/// Computed-value rules that depend on several properties.
/// The line height in pixels, with `normal` from the font's metrics.
fn used_line_height(style: &ComputedValues, metrics: &FontMetrics) -> f32 {
    match style.line_height {
        crate::properties::types::LineHeight::Normal => metrics.line_height,
        ref other => other.to_px(style.font_size),
    }
}

fn fixup(style: &mut ComputedValues, parent: &ComputedValues) {
    // A border with style `none` or `hidden` has zero width.
    let none = |s: BorderStyle| matches!(s, BorderStyle::None | BorderStyle::Hidden);
    if none(style.border_top_style) {
        style.border_top_width = 0.0;
    }
    if none(style.border_right_style) {
        style.border_right_width = 0.0;
    }
    if none(style.border_bottom_style) {
        style.border_bottom_width = 0.0;
    }
    if none(style.border_left_style) {
        style.border_left_width = 0.0;
    }
    if none(style.column_rule_style) {
        style.column_rule_width = 0.0;
    }
    if style.outline_style == OutlineStyle::None {
        style.outline_width = 0.0;
    }
    // `visible` and `clip` cannot pair with a scrollable value (CSS Overflow 3 § 3.1).
    let scrollable =
        |o: Overflow| matches!(o, Overflow::Hidden | Overflow::Scroll | Overflow::Auto);
    if scrollable(style.overflow_x) || scrollable(style.overflow_y) {
        let adjust = |o: Overflow| match o {
            Overflow::Visible => Overflow::Auto,
            Overflow::Clip => Overflow::Hidden,
            other => other,
        };
        style.overflow_x = adjust(style.overflow_x);
        style.overflow_y = adjust(style.overflow_y);
    }
    // Absolutely positioned boxes do not float, and both are block-level (CSS 2.1 § 9.7).
    if matches!(style.position, Position::Absolute | Position::Fixed) {
        style.float = Float::None;
        style.display = blockify(style.display);
    } else if style.float != Float::None {
        style.display = blockify(style.display);
    }
    // The children of flex and grid containers are their items, which are block-level
    // (CSS Flexbox 1 § 4, CSS Grid 1 § 6).
    if matches!(
        parent.display,
        Display::Normal {
            inside: DisplayInside::Flex | DisplayInside::Grid,
            ..
        }
    ) {
        style.display = blockify(style.display);
    }
}

/// Blockifies a display value (CSS Display 3 § 2.7), as the root element requires.
fn blockify(display: Display) -> Display {
    match display {
        Display::Normal {
            outside: DisplayOutside::Inline | DisplayOutside::RunIn,
            inside,
            list_item,
        } => {
            let inside = if inside == DisplayInside::Ruby {
                DisplayInside::Flow
            } else {
                inside
            };
            Display::Normal {
                outside: DisplayOutside::Block,
                inside,
                list_item,
            }
        }
        Display::Internal(_) => Display::BLOCK,
        other => other,
    }
}

/// The most `@font-face` files a document loads.
const MAX_FONT_FILES: usize = 64;

/// Loads the sources of an `@font-face` rule up to the first file that loads: `local()`
/// names are kept for whatever selects fonts to resolve.
fn load_face(
    mut face: FontFace,
    sources: Vec<SourceSpec>,
    options: &StyleOptions,
    stylesheet: &str,
    loaded: &mut usize,
    warnings: &mut Vec<StyleWarning>,
) -> FontFace {
    for source in sources {
        match source {
            SourceSpec::Local(name) => face.sources.push(FontSource::Local(Arc::from(name))),
            SourceSpec::Url(url) => {
                let data = options.load_font.and_then(|load| {
                    if *loaded >= MAX_FONT_FILES {
                        return None;
                    }
                    *loaded += 1;
                    load(&url)
                });
                match data.filter(|data| looks_like_font(data)) {
                    Some(data) => {
                        face.sources.push(FontSource::Data(Arc::from(data)));
                        break;
                    }
                    None => warnings.push(StyleWarning {
                        source: stylesheet.to_owned(),
                        warning: Warning {
                            line: 0,
                            column: 0,
                            message: format!("could not load font `{url}`"),
                        },
                    }),
                }
            }
        }
    }
    face
}
