//! Stylesheet structure: rules, nesting and at-rules.
//!
//! Declaration values are kept as source text here; properties parse them.

use std::cell::RefCell;

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, Delimiter, ParseError, ParseErrorKind, Parser,
    ParserState, QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, SourceLocation,
    StyleSheetParser, Token, parse_important,
};
use rustyprint_dom::Namespace;

use crate::custom::{CustomFunction, FunctionItem, Parameter};
use crate::media::{CustomMedia, MediaQueryList};
use crate::page::{MarginBox, PageSelector};
use crate::selector::{
    Compound, Namespaces, PseudoClass, Selector, SelectorList, Simple, parse_selector_list,
};
use crate::supports::SupportsCondition;

/// A parsed stylesheet and the problems found while parsing it.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
    /// Namespaces declared with `@namespace`.
    pub namespaces: Namespaces,
    pub warnings: Vec<Warning>,
}

/// Something in a stylesheet that was ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssRule {
    Style(StyleRule),
    Media {
        queries: MediaQueryList,
        rules: Vec<CssRule>,
    },
    CustomMedia {
        name: String,
        value: CustomMedia,
    },
    Supports {
        condition: SupportsCondition,
        rules: Vec<CssRule>,
    },
    /// `@layer name { ... }`, or an anonymous layer without a name.
    LayerBlock {
        name: Option<LayerName>,
        rules: Vec<CssRule>,
    },
    /// `@layer a, b;` declares layer order.
    LayerStatement(Vec<LayerName>),
    Import(ImportRule),
    /// `@scope (start) to (end) { ... }`. Without `start`, the scope root is the parent of
    /// the stylesheet's owner element.
    Scope {
        start: Option<SelectorList>,
        end: Option<SelectorList>,
        rules: Vec<CssRule>,
    },
    Page(PageRule),
    CounterStyle(CounterStyleRule),
    PositionTry(PositionTryRule),
    /// `@font-face { descriptors }`.
    FontFace(DescriptorRule),
    /// `@font-palette-values --name { descriptors }`, with the name in `name`.
    FontPaletteValues(DescriptorRule),
    FontFeatureValues(FontFeatureValuesRule),
    Keyframes(KeyframesRule),
    /// `@property --name { descriptors }`, with the name in `name`.
    Property(DescriptorRule),
    /// `@container conditions { rules }`.
    Container {
        conditions: Vec<crate::container::ContainerCondition>,
        rules: Vec<CssRule>,
    },
    /// `@color-profile --name { descriptors }`, or `device-cmyk`, with the name in `name`.
    ColorProfile(DescriptorRule),
    /// `@function --name(parameters) returns <type> { body }`.
    Function {
        name: String,
        function: CustomFunction,
    },
}

/// `@keyframes name { <keyframe-selector># { declarations } }`.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframesRule {
    pub name: String,
    /// Each keyframe block: its offsets from 0 to 1, and its declarations.
    pub keyframes: Vec<(Vec<f32>, Vec<Declaration>)>,
}

/// An at-rule whose body is descriptors, such as `@font-face`.
#[derive(Debug, Clone, PartialEq)]
pub struct DescriptorRule {
    pub name: String,
    pub declarations: Vec<Declaration>,
    pub line: u32,
    pub column: u32,
}

/// `@font-feature-values <family-name># { @<feature-type> { <name>: <integer>+; } }`.
#[derive(Debug, Clone, PartialEq)]
pub struct FontFeatureValuesRule {
    pub families: Vec<String>,
    /// Feature value blocks: their type, such as `styleset`, and their declarations.
    pub blocks: Vec<(String, Vec<Declaration>)>,
    /// `font-display`, the one descriptor allowed outside the blocks.
    pub declarations: Vec<Declaration>,
    pub line: u32,
    pub column: u32,
}

/// `@position-try --name { declarations }`: a set of positioning properties to try when a
/// positioned box overflows (CSS Anchor Positioning 1 § 4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct PositionTryRule {
    pub name: String,
    pub declarations: Vec<Declaration>,
}

/// `@counter-style name { descriptors }`.
#[derive(Debug, Clone, PartialEq)]
pub struct CounterStyleRule {
    pub name: String,
    pub declarations: Vec<Declaration>,
    /// 1-based line and column of the rule.
    pub line: u32,
    pub column: u32,
}

/// `@page selectors { declarations; @top-left { ... } }`.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRule {
    pub selectors: Vec<PageSelector>,
    /// Properties and descriptors of the page context.
    pub declarations: Vec<Declaration>,
    /// Margin boxes with their declarations, in source order.
    pub margin_boxes: Vec<(MarginBox, Vec<Declaration>)>,
}

/// A style rule. Its declarations apply before its nested rules.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRule {
    pub selectors: SelectorList,
    pub declarations: Vec<Declaration>,
    pub rules: Vec<CssRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// Lowercase property name, or a custom property name (`--x`) as written.
    pub name: String,
    /// The value's source text, trimmed, without `!important`.
    pub value: String,
    pub important: bool,
    /// 1-based line and column of the property name.
    pub line: u32,
    pub column: u32,
}

/// A dotted layer name, such as `framework.base`.
pub type LayerName = Vec<String>;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportRule {
    pub url: String,
    /// `None` without `layer`; `Some(None)` for an anonymous `layer`.
    pub layer: Option<Option<LayerName>>,
    pub supports: Option<SupportsCondition>,
    pub media: MediaQueryList,
}

impl Stylesheet {
    /// Parses a stylesheet. Invalid parts are dropped and reported in `warnings`.
    pub fn parse(css: &str) -> Self {
        let ctx = Context::default();
        let mut input = Parser::new(css);
        let mut parser = RuleParser {
            ctx: &ctx,
            parent: None,
            top_level: true,
            declarations: false,
        };
        let mut items = Vec::new();
        for result in StyleSheetParser::new(&mut input, &mut parser) {
            match result {
                Ok(item) => items.push(item),
                Err((error, source, location)) => ctx.warn_error(&error, source, location),
            }
        }
        let (_, rules) = split_items(items, None);
        Self {
            rules,
            namespaces: ctx.namespaces.into_inner(),
            warnings: ctx.warnings.into_inner(),
        }
    }

    /// Parses the contents of a `style` attribute.
    pub fn parse_style_attribute(css: &str) -> (Vec<Declaration>, Vec<Warning>) {
        let ctx = Context::default();
        let mut input = Parser::new(css);
        let mut parser = RuleParser {
            ctx: &ctx,
            parent: None,
            top_level: false,
            declarations: true,
        };
        let mut declarations = Vec::new();
        for result in RuleBodyParser::new(&mut input, &mut parser) {
            match result {
                Ok(Item::Declaration(declaration)) => declarations.push(declaration),
                Ok(_) => {}
                Err((error, source, location)) => ctx.warn_error(&error, source, location),
            }
        }
        (declarations, ctx.warnings.into_inner())
    }
}

/// Where the top-level parser is, since `@import` and `@namespace` must come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
enum Stage {
    #[default]
    Imports,
    Namespaces,
    Body,
}

#[derive(Default)]
struct Context {
    namespaces: RefCell<Namespaces>,
    warnings: RefCell<Vec<Warning>>,
    stage: RefCell<Stage>,
}

impl Context {
    fn warn(&self, location: SourceLocation, message: String) {
        self.warnings.borrow_mut().push(Warning {
            line: location.line + 1,
            column: location.column,
            message,
        });
    }

    fn warn_error(&self, error: &ParseError<String>, source: &str, location: SourceLocation) {
        let message = match &error.kind {
            ParseErrorKind::Custom(message) => message.clone(),
            ParseErrorKind::Basic(_) => format!("ignored invalid CSS `{}`", source.trim()),
        };
        self.warn(location, message);
    }

    /// Moves to `stage`; returns false if the parser is already past it.
    fn advance(&self, stage: Stage) -> bool {
        let mut current = self.stage.borrow_mut();
        if *current > stage {
            return false;
        }
        *current = stage;
        true
    }
}

enum Item {
    Declaration(Declaration),
    Rule(CssRule),
    Nothing,
}

/// Separates leading declarations from rules. Declarations that follow a rule are wrapped in
/// nested declarations rules matching `parent`, so source order is kept.
fn split_items(
    items: Vec<Item>,
    parent: Option<&SelectorList>,
) -> (Vec<Declaration>, Vec<CssRule>) {
    let mut leading = Vec::new();
    let mut rules = Vec::new();
    let mut pending: Vec<Declaration> = Vec::new();
    let flush = |pending: &mut Vec<Declaration>, rules: &mut Vec<CssRule>| {
        if let (Some(parent), false) = (parent, pending.is_empty()) {
            rules.push(CssRule::Style(StyleRule {
                selectors: nested_declarations_selector(parent),
                declarations: std::mem::take(pending),
                rules: Vec::new(),
            }));
        }
    };
    for item in items {
        match item {
            Item::Declaration(declaration) if rules.is_empty() => leading.push(declaration),
            Item::Declaration(declaration) => pending.push(declaration),
            Item::Rule(rule) => {
                flush(&mut pending, &mut rules);
                rules.push(rule);
            }
            Item::Nothing => {}
        }
    }
    flush(&mut pending, &mut rules);
    (leading, rules)
}

/// `:is(parent)`: what a nested declarations rule matches.
fn nested_declarations_selector(parent: &SelectorList) -> SelectorList {
    single_compound(PseudoClass::Is(parent.clone()))
}

fn single_compound(pseudo: PseudoClass) -> SelectorList {
    SelectorList(vec![Selector {
        compounds: vec![Compound(vec![Simple::PseudoClass(pseudo)])],
        combinators: Vec::new(),
        pseudo_element: None,
    }])
}

/// Rules inside `@scope` are relative to `:where(:scope)`.
fn scope_parent() -> SelectorList {
    single_compound(PseudoClass::Where(single_compound(PseudoClass::Scope)))
}

struct RuleParser<'a> {
    ctx: &'a Context,
    /// Selectors of the enclosing style rule, for nesting.
    parent: Option<SelectorList>,
    top_level: bool,
    /// Whether declarations are allowed (inside style rules and nested group rules).
    declarations: bool,
}

impl RuleParser<'_> {
    /// Parses a block of rules (and declarations, if allowed) with the given nesting parent.
    fn parse_body<'i>(
        &self,
        input: &mut Parser<'i>,
        parent: Option<SelectorList>,
    ) -> (Vec<Declaration>, Vec<CssRule>) {
        let declarations = parent.is_some();
        let mut parser = RuleParser {
            ctx: self.ctx,
            parent: parent.clone(),
            top_level: false,
            declarations,
        };
        let mut items = Vec::new();
        for result in RuleBodyParser::new(input, &mut parser) {
            match result {
                Ok(item) => items.push(item),
                Err((error, source, location)) => self.ctx.warn_error(&error, source, location),
            }
        }
        split_items(items, parent.as_ref())
    }

    /// Parses a group rule's block: any declarations in it become a nested declarations rule.
    fn parse_group_block<'i>(
        &self,
        input: &mut Parser<'i>,
        parent: Option<SelectorList>,
    ) -> Vec<CssRule> {
        let (declarations, mut rules) = self.parse_body(input, parent.clone());
        if let (Some(parent), false) = (parent, declarations.is_empty()) {
            rules.insert(
                0,
                CssRule::Style(StyleRule {
                    selectors: nested_declarations_selector(&parent),
                    declarations,
                    rules: Vec::new(),
                }),
            );
        }
        rules
    }

    /// Whether a rule for `stage` is allowed here: at the top level, before later stages.
    fn stage(&self, stage: Stage) -> bool {
        self.top_level && self.ctx.advance(stage)
    }
}

fn error<T>(message: impl Into<String>) -> Result<T, ParseError<String>> {
    Err(ParseError::custom(message.into()))
}

/// Parses a layer name: identifiers joined by `.` with no whitespace.
fn parse_layer_name(input: &mut Parser<'_>) -> Result<LayerName, ParseError<String>> {
    let mut name = vec![input.expect_ident()?.to_string()];
    loop {
        let state = input.state();
        match input.next_including_whitespace() {
            Ok(Token::Delim('.')) => match input.next_including_whitespace()?.clone() {
                Token::Ident(part) => name.push(part.to_string()),
                _ => return error("invalid layer name"),
            },
            _ => {
                input.reset(&state);
                return Ok(name);
            }
        }
    }
}

pub(crate) enum Prelude {
    Charset,
    Import(ImportRule),
    Namespace(Option<String>, String),
    Media(MediaQueryList),
    CustomMedia(String, CustomMedia),
    Supports(SupportsCondition),
    Layer(Vec<LayerName>),
    Scope(Option<SelectorList>, Option<SelectorList>),
    Page(Vec<PageSelector>),
    CounterStyle(String),
    PositionTry(String),
    FontFace,
    FontPaletteValues(String),
    FontFeatureValues(Vec<String>),
    Keyframes(String),
    Property(String),
    Function(String, Vec<Parameter>, Option<crate::syntax::Syntax>),
    Container(Vec<crate::container::ContainerCondition>),
    ColorProfile(String),
    StartingStyle,
    ViewTransition,
}

impl<'i> AtRuleParser<'i> for RuleParser<'_> {
    type Prelude = Prelude;
    type AtRule = Item;
    type Error = String;

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<Prelude, ParseError<String>> {
        let name = name.to_ascii_lowercase();
        let prelude = match name.as_str() {
            "charset" => {
                input.expect_string()?;
                Prelude::Charset
            }
            "import" => {
                if !self.stage(Stage::Imports) {
                    return error("ignored @import that is not at the start of the stylesheet");
                }
                let url = input.expect_url_or_string()?.to_string();
                let layer = if input
                    .try_parse(|input| input.expect_ident_matching("layer"))
                    .is_ok()
                {
                    Some(None)
                } else if input
                    .try_parse(|input| input.expect_function_matching("layer"))
                    .is_ok()
                {
                    Some(Some(input.parse_nested_block(|input| {
                        input.parse_entirely(parse_layer_name)
                    })?))
                } else {
                    None
                };
                let supports = if input
                    .try_parse(|input| input.expect_function_matching("supports"))
                    .is_ok()
                {
                    Some(input.parse_nested_block(|input| {
                        // `supports()` takes a condition or a bare declaration.
                        if let Ok(condition) =
                            input.try_parse(|input| input.parse_entirely(SupportsCondition::parse))
                        {
                            return Ok(condition);
                        }
                        let name = input.expect_ident_cloned()?.to_ascii_lowercase();
                        input.expect_colon()?;
                        let start = input.position();
                        while input.next().is_ok() {}
                        Ok::<_, ParseError<String>>(SupportsCondition::Declaration {
                            name,
                            value: input.slice_from(start).trim().to_owned(),
                        })
                    })?)
                } else {
                    None
                };
                Prelude::Import(ImportRule {
                    url,
                    layer,
                    supports,
                    media: MediaQueryList::parse(input),
                })
            }
            "namespace" => {
                if !self.stage(Stage::Namespaces) {
                    return error("ignored @namespace after other rules");
                }
                let prefix = input
                    .try_parse(|input| input.expect_ident_cloned())
                    .ok()
                    .map(|p| p.to_string());
                let url = input.expect_url_or_string()?.to_string();
                Prelude::Namespace(prefix, url)
            }
            "media" => Prelude::Media(MediaQueryList::parse(input)),
            "custom-media" => {
                let name = input.expect_ident_cloned()?;
                if !name.starts_with("--") {
                    return error("@custom-media names must start with --");
                }
                let value = if input
                    .try_parse(|input| input.expect_ident_matching("true"))
                    .is_ok()
                {
                    CustomMedia::True
                } else if input
                    .try_parse(|input| input.expect_ident_matching("false"))
                    .is_ok()
                {
                    CustomMedia::False
                } else {
                    CustomMedia::Queries(MediaQueryList::parse(input))
                };
                Prelude::CustomMedia(name.to_string(), value)
            }
            "supports" => Prelude::Supports(
                SupportsCondition::parse(input)
                    .or_else(|_| error("invalid @supports condition"))?,
            ),
            "layer" => {
                let names = if input.is_exhausted() {
                    Vec::new()
                } else {
                    input.parse_comma_separated(parse_layer_name)?
                };
                Prelude::Layer(names)
            }
            "scope" => {
                let namespaces = self.ctx.namespaces.borrow().clone();
                let parse_list =
                    |input: &mut Parser<'i>| -> Result<SelectorList, ParseError<String>> {
                        input.expect_parenthesis_block()?;
                        input.parse_nested_block(|input| {
                            parse_selector_list(input, &namespaces, None)
                                .or_else(|_| error("invalid @scope selector"))
                        })
                    };
                let start = input.try_parse(parse_list).ok();
                let end = if input
                    .try_parse(|input| input.expect_ident_matching("to"))
                    .is_ok()
                {
                    Some(parse_list(input)?)
                } else {
                    None
                };
                Prelude::Scope(start, end)
            }
            "page" => Prelude::Page(
                PageSelector::parse_list(input).or_else(|_| error("invalid @page selector"))?,
            ),
            "position-try" => {
                let name = input
                    .expect_ident_cloned()
                    .or_else(|_| error("@position-try needs a name"))?;
                if !name.starts_with("--") || name.len() < 3 {
                    return error("@position-try names must start with --");
                }
                Prelude::PositionTry(name.to_string())
            }
            "font-face" => Prelude::FontFace,
            "font-palette-values" => {
                let name = input
                    .expect_ident_cloned()
                    .or_else(|_| error("@font-palette-values needs a name"))?;
                if !name.starts_with("--") || name.len() < 3 {
                    return error("@font-palette-values names must start with --");
                }
                Prelude::FontPaletteValues(name.to_string())
            }
            "font-feature-values" => Prelude::FontFeatureValues(
                crate::font_rules::parse_family_list(input)
                    .or_else(|_| error("invalid @font-feature-values family list"))?,
            ),
            "keyframes" => {
                let name = match input.next()?.clone() {
                    Token::Ident(name) => {
                        let lower = name.to_ascii_lowercase();
                        if matches!(
                            &*lower,
                            "none"
                                | "initial"
                                | "inherit"
                                | "unset"
                                | "revert"
                                | "revert-layer"
                                | "default"
                        ) {
                            return error(format!("@keyframes cannot be named `{name}`"));
                        }
                        name.to_string()
                    }
                    Token::QuotedString(name) => name.to_string(),
                    _ => return error("@keyframes needs a name"),
                };
                Prelude::Keyframes(name)
            }
            "property" => {
                let name = input
                    .expect_ident_cloned()
                    .or_else(|_| error("@property needs a custom property name"))?;
                if !name.starts_with("--") || name.len() < 3 {
                    return error("@property names must start with --");
                }
                Prelude::Property(name.to_string())
            }
            "function" => {
                let name = match input.next()?.clone() {
                    Token::Function(name) if name.starts_with("--") && name.len() > 2 => {
                        name.to_string()
                    }
                    _ => return error("@function needs a name such as --name()"),
                };
                let parameters = input.parse_nested_block(parse_parameters)?;
                let returns = if input
                    .try_parse(|input| input.expect_ident_matching("returns"))
                    .is_ok()
                {
                    let start = input.position();
                    while input.next().is_ok() {}
                    Some(parse_css_type(input.slice_from(start))?)
                } else {
                    None
                };
                Prelude::Function(name, parameters, returns)
            }
            "container" => Prelude::Container(
                crate::container::parse_conditions(input)
                    .or_else(|_| error("invalid @container condition"))?,
            ),
            "color-profile" => {
                let name = input
                    .expect_ident_cloned()
                    .or_else(|_| error("@color-profile needs a name"))?;
                let device_cmyk = name.eq_ignore_ascii_case("device-cmyk");
                if !device_cmyk && (!name.starts_with("--") || name.len() < 3) {
                    return error("@color-profile names must start with -- or be device-cmyk");
                }
                Prelude::ColorProfile(if device_cmyk {
                    "device-cmyk".to_owned()
                } else {
                    name.to_string()
                })
            }
            "starting-style" => Prelude::StartingStyle,
            "view-transition" => Prelude::ViewTransition,
            "counter-style" => {
                let name = input
                    .expect_ident_cloned()
                    .or_else(|_| error("@counter-style needs a name"))?;
                if !crate::counter_style::valid_rule_name(&name) {
                    return error(format!("@counter-style cannot define `{name}`"));
                }
                Prelude::CounterStyle(name.to_string())
            }
            _ => return error(format!("unknown at-rule @{name}")),
        };
        if !matches!(
            prelude,
            Prelude::Charset | Prelude::Import(_) | Prelude::Namespace(..) | Prelude::Layer(_)
        ) && self.top_level
        {
            self.ctx.advance(Stage::Body);
        }
        input.expect_exhausted()?;
        Ok(prelude)
    }

    fn rule_without_block(&mut self, prelude: Prelude, _start: &ParserState) -> Result<Item, ()> {
        Ok(match prelude {
            Prelude::Charset => Item::Nothing,
            Prelude::Import(import) => Item::Rule(CssRule::Import(import)),
            Prelude::Namespace(prefix, url) => {
                let mut namespaces = self.ctx.namespaces.borrow_mut();
                match prefix {
                    Some(prefix) => {
                        namespaces.prefixes.insert(prefix, Namespace::from(url));
                    }
                    None => namespaces.default = Some(Namespace::from(url)),
                }
                Item::Nothing
            }
            Prelude::CustomMedia(name, value) => Item::Rule(CssRule::CustomMedia { name, value }),
            Prelude::Layer(names) if !names.is_empty() => {
                Item::Rule(CssRule::LayerStatement(names))
            }
            _ => return Err(()),
        })
    }

    fn parse_block(
        &mut self,
        prelude: Prelude,
        start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Item, ParseError<String>> {
        if self.top_level {
            self.ctx.advance(Stage::Body);
        }
        let parent = self.parent.clone();
        Ok(Item::Rule(match prelude {
            Prelude::Media(queries) => CssRule::Media {
                queries,
                rules: self.parse_group_block(input, parent),
            },
            Prelude::Supports(condition) => CssRule::Supports {
                condition,
                rules: self.parse_group_block(input, parent),
            },
            Prelude::Container(conditions) => CssRule::Container {
                conditions,
                rules: self.parse_group_block(input, parent),
            },
            Prelude::Layer(names) => {
                if names.len() > 1 {
                    return error("@layer blocks take at most one name");
                }
                CssRule::LayerBlock {
                    name: names.into_iter().next(),
                    rules: self.parse_group_block(input, parent),
                }
            }
            Prelude::Scope(start, end) => CssRule::Scope {
                start,
                end,
                rules: self.parse_group_block(input, Some(scope_parent())),
            },
            Prelude::Page(selectors) => {
                let mut parser = PageBodyParser {
                    ctx: self.ctx,
                    margin_boxes: true,
                };
                let mut declarations = Vec::new();
                let mut margin_boxes = Vec::new();
                for result in RuleBodyParser::new(input, &mut parser) {
                    match result {
                        Ok(PageItem::Declaration(declaration)) => declarations.push(declaration),
                        Ok(PageItem::MarginBox(name, body)) => margin_boxes.push((name, body)),
                        Err((error, source, location)) => {
                            self.ctx.warn_error(&error, source, location)
                        }
                    }
                }
                CssRule::Page(PageRule {
                    selectors,
                    declarations,
                    margin_boxes,
                })
            }
            Prelude::PositionTry(name) => {
                let mut parser = PageBodyParser {
                    ctx: self.ctx,
                    margin_boxes: false,
                };
                let mut declarations = Vec::new();
                for result in RuleBodyParser::new(input, &mut parser) {
                    match result {
                        Ok(PageItem::Declaration(declaration)) => declarations.push(declaration),
                        Ok(PageItem::MarginBox(..)) => {}
                        Err((error, source, location)) => {
                            self.ctx.warn_error(&error, source, location)
                        }
                    }
                }
                CssRule::PositionTry(PositionTryRule { name, declarations })
            }
            Prelude::FontFace
            | Prelude::FontPaletteValues(_)
            | Prelude::Property(_)
            | Prelude::ColorProfile(_) => {
                let mut parser = PageBodyParser {
                    ctx: self.ctx,
                    margin_boxes: false,
                };
                let mut declarations = Vec::new();
                for result in RuleBodyParser::new(input, &mut parser) {
                    match result {
                        Ok(PageItem::Declaration(declaration)) => declarations.push(declaration),
                        Ok(PageItem::MarginBox(..)) => {}
                        Err((error, source, location)) => {
                            self.ctx.warn_error(&error, source, location)
                        }
                    }
                }
                let location = start.source_location();
                let rule = |name: String| DescriptorRule {
                    name,
                    declarations,
                    line: location.line + 1,
                    column: location.column,
                };
                match prelude {
                    Prelude::FontPaletteValues(name) => CssRule::FontPaletteValues(rule(name)),
                    Prelude::Property(name) => CssRule::Property(rule(name)),
                    Prelude::ColorProfile(name) => CssRule::ColorProfile(rule(name)),
                    _ => CssRule::FontFace(rule(String::new())),
                }
            }
            Prelude::FontFeatureValues(families) => {
                let mut parser = FeatureValuesParser { ctx: self.ctx };
                let (mut blocks, mut declarations) = (Vec::new(), Vec::new());
                for result in RuleBodyParser::new(input, &mut parser) {
                    match result {
                        Ok(FeatureItem::Declaration(declaration)) => declarations.push(declaration),
                        Ok(FeatureItem::Block(name, body)) => blocks.push((name, body)),
                        Err((error, source, location)) => {
                            self.ctx.warn_error(&error, source, location)
                        }
                    }
                }
                let location = start.source_location();
                CssRule::FontFeatureValues(FontFeatureValuesRule {
                    families,
                    blocks,
                    declarations,
                    line: location.line + 1,
                    column: location.column,
                })
            }
            Prelude::CounterStyle(name) => {
                let mut parser = PageBodyParser {
                    ctx: self.ctx,
                    margin_boxes: false,
                };
                let mut declarations = Vec::new();
                for result in RuleBodyParser::new(input, &mut parser) {
                    match result {
                        Ok(PageItem::Declaration(declaration)) => declarations.push(declaration),
                        Ok(PageItem::MarginBox(..)) => {}
                        Err((error, source, location)) => {
                            self.ctx.warn_error(&error, source, location)
                        }
                    }
                }
                let location = start.source_location();
                CssRule::CounterStyle(CounterStyleRule {
                    name: crate::counter_style::normalize_name(&name).to_string(),
                    declarations,
                    line: location.line + 1,
                    column: location.column,
                })
            }
            Prelude::Keyframes(name) => {
                let mut parser = KeyframesParser { ctx: self.ctx };
                let mut keyframes = Vec::new();
                for result in RuleBodyParser::new(input, &mut parser) {
                    match result {
                        // Keyframes on view timeline ranges never apply here.
                        Ok(Some(keyframe)) => keyframes.push(keyframe),
                        Ok(None) => {}
                        Err((error, source, location)) => {
                            self.ctx.warn_error(&error, source, location)
                        }
                    }
                }
                CssRule::Keyframes(KeyframesRule { name, keyframes })
            }
            Prelude::Function(name, parameters, returns) => {
                let mut parser = FunctionBodyParser { ctx: self.ctx };
                let mut body = Vec::new();
                for result in RuleBodyParser::new(input, &mut parser) {
                    match result {
                        Ok(item) => body.push(item),
                        Err((error, source, location)) => {
                            self.ctx.warn_error(&error, source, location)
                        }
                    }
                }
                CssRule::Function {
                    name,
                    function: CustomFunction {
                        parameters,
                        returns,
                        body,
                    },
                }
            }
            // Starting styles only apply when a transition starts, which never happens on
            // paper: the rules are checked and dropped.
            Prelude::StartingStyle => {
                self.parse_group_block(input, parent);
                return Ok(Item::Nothing);
            }
            // View transitions need navigation; the descriptors are checked and dropped.
            Prelude::ViewTransition => {
                let mut parser = PageBodyParser {
                    ctx: self.ctx,
                    margin_boxes: false,
                };
                for result in RuleBodyParser::new(input, &mut parser) {
                    match result {
                        Ok(PageItem::Declaration(declaration)) => {
                            if let Err(message) = check_view_transition(&declaration) {
                                self.ctx.warnings.borrow_mut().push(Warning {
                                    line: declaration.line,
                                    column: declaration.column,
                                    message,
                                });
                            }
                        }
                        Ok(PageItem::MarginBox(..)) => {}
                        Err((error, source, location)) => {
                            self.ctx.warn_error(&error, source, location)
                        }
                    }
                }
                return Ok(Item::Nothing);
            }
            _ => return error("unexpected block"),
        }))
    }
}

/// A `<css-type>` of a custom function: a syntax component such as `<length>`, or
/// `type(<syntax>)`.
fn parse_css_type(text: &str) -> Result<crate::syntax::Syntax, ParseError<String>> {
    let text = text.trim();
    let syntax = match text
        .get(..5)
        .filter(|start| start.eq_ignore_ascii_case("type("))
    {
        Some(_) => text[5..].strip_suffix(')').unwrap_or(""),
        None => text,
    };
    // `type()` takes a quoted syntax too.
    let syntax = syntax.trim().trim_matches(|c| c == '"' || c == '\'');
    crate::syntax::Syntax::parse(syntax)
        .ok_or_else(|| ParseError::custom(format!("invalid type `{text}` in @function")))
}

/// Parses the parameters of `@function`: `<custom-property-name> <css-type>? [ :
/// <default-value> ]?`, separated by commas.
fn parse_parameters(input: &mut Parser<'_>) -> Result<Vec<Parameter>, ParseError<String>> {
    if input.is_exhausted() {
        return Ok(Vec::new());
    }
    input.parse_comma_separated(|input| {
        let name = input.expect_ident_cloned()?;
        if !name.starts_with("--") || name.len() < 3 {
            return error("@function parameters must be custom property names");
        }
        let start = input.position();
        let mut end = start;
        let mut default = None;
        while let Ok(token) = input.next() {
            let token = token.clone();
            if token == Token::Colon {
                let from = input.position();
                while input.next().is_ok() {}
                default = Some(input.slice_from(from).trim().to_owned());
                break;
            }
            crate::custom::skip_block(input, &token);
            end = input.position();
        }
        let ty = input.slice(start..end).trim().to_owned();
        let syntax = if ty.is_empty() {
            None
        } else {
            Some(parse_css_type(&ty)?)
        };
        Ok(Parameter {
            name: name.to_string(),
            syntax,
            default,
        })
    })
}

/// Parses the body of `@function`: local custom properties, `result`, and conditional
/// group rules with the same contents.
struct FunctionBodyParser<'a> {
    ctx: &'a Context,
}

impl<'i> DeclarationParser<'i> for FunctionBodyParser<'_> {
    type Declaration = FunctionItem;
    type Error = String;

    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        declaration_start: &ParserState,
    ) -> Result<FunctionItem, ParseError<String>> {
        let declaration = parse_declaration_value(name, input, declaration_start)?;
        if declaration.name.starts_with("--") {
            Ok(FunctionItem::Local(declaration.name, declaration.value))
        } else if declaration.name == "result" {
            Ok(FunctionItem::Result(declaration.value))
        } else {
            error(format!(
                "`{}` is not allowed in @function",
                declaration.name
            ))
        }
    }
}

impl<'i> AtRuleParser<'i> for FunctionBodyParser<'_> {
    type Prelude = (String, String);
    type AtRule = FunctionItem;
    type Error = String;

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<(String, String), ParseError<String>> {
        let name = name.to_ascii_lowercase();
        if !matches!(&*name, "media" | "supports" | "container") {
            return error(format!("@{name} is not allowed in @function"));
        }
        let start = input.position();
        while input.next().is_ok() {}
        Ok((name, input.slice_from(start).trim().to_owned()))
    }

    fn parse_block(
        &mut self,
        (name, condition): (String, String),
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<FunctionItem, ParseError<String>> {
        let mut parser = FunctionBodyParser { ctx: self.ctx };
        let mut items = Vec::new();
        for result in RuleBodyParser::new(input, &mut parser) {
            match result {
                Ok(item) => items.push(item),
                Err((error, source, location)) => self.ctx.warn_error(&error, source, location),
            }
        }
        Ok(match &*name {
            "media" => FunctionItem::Media(condition, items),
            "supports" => FunctionItem::Supports(condition, items),
            _ => FunctionItem::Container(condition, items),
        })
    }
}

impl<'i> QualifiedRuleParser<'i> for FunctionBodyParser<'_> {
    type Prelude = ();
    type QualifiedRule = FunctionItem;
    type Error = String;
}

impl<'i> RuleBodyItemParser<'i, FunctionItem, String> for FunctionBodyParser<'_> {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

/// Whether `word` (lowercase) is a CSS-wide keyword or `default`, which no
/// `<custom-ident>` can be.
fn reserved_ident(word: &str) -> bool {
    matches!(
        word,
        "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default"
    )
}

/// Checks a descriptor of `@view-transition`: `navigation: auto | none` and
/// `types: none | <custom-ident>+`.
fn check_view_transition(declaration: &Declaration) -> Result<(), String> {
    let mut input = Parser::new(&declaration.value);
    let valid = match declaration.name.as_str() {
        "navigation" => input
            .parse_entirely(|input| {
                let ident = input.expect_ident()?.to_ascii_lowercase();
                match &*ident {
                    "auto" | "none" => Ok(()),
                    _ => Err(ParseError::<()>::custom(())),
                }
            })
            .is_ok(),
        "types" => input
            .parse_entirely(|input| {
                let first = input.expect_ident_cloned()?.to_ascii_lowercase();
                if first == "none" {
                    return Ok(());
                }
                let mut word = Some(first);
                while let Some(w) = word.take() {
                    if reserved_ident(&w) || w == "none" || w.starts_with("-ua-") {
                        return Err(ParseError::custom(()));
                    }
                    word = input
                        .try_parse(|input| input.expect_ident_cloned())
                        .ok()
                        .map(|w| w.to_ascii_lowercase());
                }
                Ok::<_, ParseError<()>>(())
            })
            .is_ok(),
        other => return Err(format!("unknown descriptor `{other}` in @view-transition")),
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid value for `{}`: `{}`",
            declaration.name, declaration.value
        ))
    }
}

/// A keyframe block: its offsets and declarations.
type Keyframe = (Vec<f32>, Vec<Declaration>);

/// Parses the keyframe blocks of `@keyframes`. A block for a view timeline range, such as
/// `entry 20%`, parses to `None`.
struct KeyframesParser<'a> {
    ctx: &'a Context,
}

impl<'i> QualifiedRuleParser<'i> for KeyframesParser<'_> {
    type Prelude = Option<Vec<f32>>;
    type QualifiedRule = Option<Keyframe>;
    type Error = String;

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> Result<Option<Vec<f32>>, ParseError<String>> {
        let start = input.position();
        let selectors = input.parse_comma_separated(|input| {
            Ok::<_, ParseError<()>>(match input.next()?.clone() {
                Token::Ident(word) => match &*word.to_ascii_lowercase() {
                    "from" => Some(0.0),
                    "to" => Some(1.0),
                    "cover" | "contain" | "entry" | "exit" | "entry-crossing" | "exit-crossing" => {
                        input.expect_percentage()?;
                        None
                    }
                    _ => return Err(ParseError::custom(())),
                },
                Token::Percentage { unit_value, .. } if (0.0..=1.0).contains(&unit_value) => {
                    Some(unit_value)
                }
                _ => return Err(ParseError::custom(())),
            })
        });
        match selectors {
            // A selector list with a timeline range applies to view timelines only.
            Ok(offsets) => Ok(offsets.into_iter().collect()),
            Err(_) => {
                while input.next().is_ok() {}
                error(format!(
                    "ignored keyframe with invalid selector `{}`",
                    input.slice_from(start).trim()
                ))
            }
        }
    }

    fn parse_block(
        &mut self,
        offsets: Option<Vec<f32>>,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Option<Keyframe>, ParseError<String>> {
        let mut parser = PageBodyParser {
            ctx: self.ctx,
            margin_boxes: false,
        };
        let mut declarations = Vec::new();
        for result in RuleBodyParser::new(input, &mut parser) {
            match result {
                // `!important` declarations in keyframes are ignored.
                Ok(PageItem::Declaration(declaration)) if !declaration.important => {
                    declarations.push(declaration)
                }
                Ok(_) => {}
                Err((error, source, location)) => self.ctx.warn_error(&error, source, location),
            }
        }
        Ok(offsets.map(|offsets| (offsets, declarations)))
    }
}

impl<'i> AtRuleParser<'i> for KeyframesParser<'_> {
    type Prelude = ();
    type AtRule = Option<Keyframe>;
    type Error = String;
}

impl<'i> DeclarationParser<'i> for KeyframesParser<'_> {
    type Declaration = Option<Keyframe>;
    type Error = String;
}

impl<'i> RuleBodyItemParser<'i, Option<Keyframe>, String> for KeyframesParser<'_> {
    fn parse_declarations(&self) -> bool {
        false
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

impl<'i> QualifiedRuleParser<'i> for RuleParser<'_> {
    type Prelude = SelectorList;
    type QualifiedRule = Item;
    type Error = String;

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> Result<SelectorList, ParseError<String>> {
        if self.top_level {
            self.ctx.advance(Stage::Body);
        }
        let start = input.position();
        let namespaces = self.ctx.namespaces.borrow().clone();
        parse_selector_list(input, &namespaces, self.parent.as_ref()).or_else(|_| {
            while input.next().is_ok() {}
            error(format!(
                "ignored rule with invalid selector `{}`",
                input.slice_from(start).trim()
            ))
        })
    }

    fn parse_block(
        &mut self,
        selectors: SelectorList,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Item, ParseError<String>> {
        let (declarations, rules) = self.parse_body(input, Some(selectors.clone()));
        Ok(Item::Rule(CssRule::Style(StyleRule {
            selectors,
            declarations,
            rules,
        })))
    }
}

impl<'i> DeclarationParser<'i> for RuleParser<'_> {
    type Declaration = Item;
    type Error = String;

    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        declaration_start: &ParserState,
    ) -> Result<Item, ParseError<String>> {
        parse_declaration_value(name, input, declaration_start).map(Item::Declaration)
    }
}

/// Parses a declaration's value as source text, with `!important` split off.
fn parse_declaration_value<'i>(
    name: CowRcStr<'i>,
    input: &mut Parser<'i>,
    declaration_start: &ParserState,
) -> Result<Declaration, ParseError<String>> {
    let location = declaration_start.source_location();
    let custom = name.starts_with("--");
    let start = input.position();
    input.parse_until_before(Delimiter::Bang, |input| {
        while input.next().is_ok() {}
        Ok::<_, ParseError<String>>(())
    })?;
    let value = input.slice_from(start).trim().to_owned();
    let important = input.try_parse(parse_important).is_ok();
    input.expect_exhausted()?;
    if value.is_empty() && !custom {
        return error(format!("ignored `{name}` with an empty value"));
    }
    let name = if custom {
        name.to_string()
    } else {
        name.to_ascii_lowercase()
    };
    Ok(Declaration {
        name,
        value,
        important,
        line: location.line + 1,
        column: location.column,
    })
}

enum PageItem {
    Declaration(Declaration),
    MarginBox(MarginBox, Vec<Declaration>),
}

/// Parses the body of an `@page` rule, or of a margin box inside one.
struct PageBodyParser<'a> {
    ctx: &'a Context,
    /// Whether margin boxes are allowed: in `@page`, but not inside a margin box.
    margin_boxes: bool,
}

impl<'i> DeclarationParser<'i> for PageBodyParser<'_> {
    type Declaration = PageItem;
    type Error = String;

    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        declaration_start: &ParserState,
    ) -> Result<PageItem, ParseError<String>> {
        parse_declaration_value(name, input, declaration_start).map(PageItem::Declaration)
    }
}

impl<'i> AtRuleParser<'i> for PageBodyParser<'_> {
    type Prelude = MarginBox;
    type AtRule = PageItem;
    type Error = String;

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<MarginBox, ParseError<String>> {
        let lower = name.to_ascii_lowercase();
        match MarginBox::from_name(&lower) {
            Some(margin_box) if self.margin_boxes => {
                input.expect_exhausted()?;
                Ok(margin_box)
            }
            Some(_) => error(format!("@{lower} is not allowed inside a margin box")),
            None => error(format!("unknown at-rule @{lower} in @page")),
        }
    }

    fn parse_block(
        &mut self,
        margin_box: MarginBox,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<PageItem, ParseError<String>> {
        let mut parser = PageBodyParser {
            ctx: self.ctx,
            margin_boxes: false,
        };
        let mut declarations = Vec::new();
        for result in RuleBodyParser::new(input, &mut parser) {
            match result {
                Ok(PageItem::Declaration(declaration)) => declarations.push(declaration),
                Ok(PageItem::MarginBox(..)) => {}
                Err((error, source, location)) => self.ctx.warn_error(&error, source, location),
            }
        }
        Ok(PageItem::MarginBox(margin_box, declarations))
    }
}

impl<'i> QualifiedRuleParser<'i> for PageBodyParser<'_> {
    type Prelude = ();
    type QualifiedRule = PageItem;
    type Error = String;
}

impl<'i> RuleBodyItemParser<'i, PageItem, String> for PageBodyParser<'_> {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

/// An item of an `@font-feature-values` body.
enum FeatureItem {
    Declaration(Declaration),
    /// A feature value block, such as `@styleset { ... }`, by type.
    Block(String, Vec<Declaration>),
}

/// Parses the body of an `@font-feature-values` rule, or of a feature value block in it.
struct FeatureValuesParser<'a> {
    ctx: &'a Context,
}

impl<'i> DeclarationParser<'i> for FeatureValuesParser<'_> {
    type Declaration = FeatureItem;
    type Error = String;

    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        declaration_start: &ParserState,
    ) -> Result<FeatureItem, ParseError<String>> {
        parse_declaration_value(name, input, declaration_start).map(FeatureItem::Declaration)
    }
}

impl<'i> AtRuleParser<'i> for FeatureValuesParser<'_> {
    type Prelude = String;
    type AtRule = FeatureItem;
    type Error = String;

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<String, ParseError<String>> {
        let lower = name.to_ascii_lowercase();
        if crate::font_rules::FeatureBlock::from_name(&lower).is_none() {
            return error(format!("unknown at-rule @{lower} in @font-feature-values"));
        }
        input.expect_exhausted()?;
        Ok(lower)
    }

    fn parse_block(
        &mut self,
        name: String,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<FeatureItem, ParseError<String>> {
        let mut declarations = Vec::new();
        let mut parser = FeatureValuesParser { ctx: self.ctx };
        for result in RuleBodyParser::new(input, &mut parser) {
            match result {
                Ok(FeatureItem::Declaration(declaration)) => declarations.push(declaration),
                Ok(FeatureItem::Block(..)) => {}
                Err((error, source, location)) => self.ctx.warn_error(&error, source, location),
            }
        }
        Ok(FeatureItem::Block(name, declarations))
    }
}

impl<'i> QualifiedRuleParser<'i> for FeatureValuesParser<'_> {
    type Prelude = ();
    type QualifiedRule = FeatureItem;
    type Error = String;
}

impl<'i> RuleBodyItemParser<'i, FeatureItem, String> for FeatureValuesParser<'_> {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

impl<'i> RuleBodyItemParser<'i, Item, String> for RuleParser<'_> {
    fn parse_declarations(&self) -> bool {
        self.declarations
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::Specificity;

    fn style(rule: &CssRule) -> &StyleRule {
        match rule {
            CssRule::Style(style) => style,
            other => panic!("expected a style rule, got {other:?}"),
        }
    }

    fn declarations(rule: &CssRule) -> Vec<(String, String, bool)> {
        style(rule)
            .declarations
            .iter()
            .map(|d| (d.name.clone(), d.value.clone(), d.important))
            .collect()
    }

    #[test]
    fn style_rules_and_declarations() {
        let sheet =
            Stylesheet::parse("p, .a { COLOR: red !important; margin : 0 auto ; --My-Var: { x } }");
        assert!(sheet.warnings.is_empty(), "{:?}", sheet.warnings);
        assert_eq!(style(&sheet.rules[0]).selectors.0.len(), 2);
        assert_eq!(
            declarations(&sheet.rules[0]),
            [
                ("color".into(), "red".into(), true),
                ("margin".into(), "0 auto".into(), false),
                ("--My-Var".into(), "{ x }".into(), false)
            ]
        );
    }

    #[test]
    fn invalid_rules_are_dropped_with_warnings() {
        let sheet = Stylesheet::parse(
            "p { color: red } a:unknown { color: blue } q { width: ; height: 1px } @foo bar;",
        );
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(
            declarations(&sheet.rules[1]),
            [("height".into(), "1px".into(), false)]
        );
        let messages: Vec<_> = sheet.warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].contains("invalid selector `a:unknown`"));
        assert!(messages[1].contains("empty value"));
        assert!(messages[2].contains("unknown at-rule @foo"));
        assert_eq!(sheet.warnings[0].line, 1);
    }

    #[test]
    fn nesting() {
        let sheet = Stylesheet::parse(
            ".card { color: red; .title { color: blue } & > p { margin: 0 } color: green; }",
        );
        let card = style(&sheet.rules[0]);
        assert_eq!(card.declarations.len(), 1);
        assert_eq!(card.rules.len(), 3);
        let title = style(&card.rules[0]);
        assert_eq!(title.selectors.max_specificity(), Specificity(0, 2, 0));
        // The declaration after the nested rules keeps its place in a nested declarations rule.
        assert_eq!(
            declarations(&card.rules[2]),
            [("color".into(), "green".into(), false)]
        );
    }

    #[test]
    fn nested_group_rules() {
        let sheet = Stylesheet::parse(".a { @media print { color: red; .b { color: blue } } }");
        let a = style(&sheet.rules[0]);
        let CssRule::Media { rules, .. } = &a.rules[0] else {
            panic!()
        };
        assert_eq!(
            declarations(&rules[0]),
            [("color".into(), "red".into(), false)]
        );
        assert_eq!(
            style(&rules[1]).selectors.max_specificity(),
            Specificity(0, 2, 0)
        );
    }

    #[test]
    fn at_rules() {
        let sheet = Stylesheet::parse(
            r#"@charset "utf-8";
            @layer base, theme;
            @import url(a.css) layer(base) supports(display: grid) print;
            @namespace svg url(http://www.w3.org/2000/svg);
            @media print { p { color: red } }
            @custom-media --wide (min-width: 600px);
            @supports (display: grid) { p { color: red } }
            @layer theme { p { color: red } }
            @layer { p { color: red } }
            @scope (.card) to (.content) { img { border: 0 } }
            svg|rect { fill: red }"#,
        );
        assert!(sheet.warnings.is_empty(), "{:?}", sheet.warnings);
        assert!(matches!(&sheet.rules[0], CssRule::LayerStatement(names) if names.len() == 2));
        let CssRule::Import(import) = &sheet.rules[1] else {
            panic!()
        };
        assert_eq!(import.url, "a.css");
        assert_eq!(import.layer, Some(Some(vec!["base".into()])));
        assert!(import.supports.is_some());
        assert_eq!(import.media.0.len(), 1);
        assert!(sheet.namespaces.prefixes.contains_key("svg"));
        assert!(matches!(&sheet.rules[2], CssRule::Media { .. }));
        assert!(matches!(&sheet.rules[3], CssRule::CustomMedia { .. }));
        assert!(matches!(&sheet.rules[4], CssRule::Supports { .. }));
        assert!(matches!(
            &sheet.rules[5],
            CssRule::LayerBlock { name: Some(_), .. }
        ));
        assert!(matches!(
            &sheet.rules[6],
            CssRule::LayerBlock { name: None, .. }
        ));
        let CssRule::Scope { start, end, rules } = &sheet.rules[7] else {
            panic!()
        };
        assert!(start.is_some() && end.is_some());
        // Scoped selectors add no specificity for the implicit `:scope`.
        assert_eq!(
            style(&rules[0]).selectors.max_specificity(),
            Specificity(0, 0, 1)
        );
        assert!(matches!(&sheet.rules[8], CssRule::Style(_)));
    }

    #[test]
    fn misplaced_imports_and_namespaces_are_ignored() {
        let sheet = Stylesheet::parse("p {} @import 'a.css'; @namespace x url(y);");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.warnings.len(), 2, "{:?}", sheet.warnings);
        assert!(sheet.namespaces.prefixes.is_empty());
    }

    #[test]
    fn page_rules() {
        let sheet = Stylesheet::parse(
            "@page :first { margin: 1cm; size: A5; @top-center { content: 'x' } @bogus {} }              @media print { @page chapter { margin: 0 } }",
        );
        assert_eq!(sheet.warnings.len(), 1, "{:?}", sheet.warnings);
        let CssRule::Page(page) = &sheet.rules[0] else {
            panic!("expected a page rule");
        };
        assert_eq!(page.declarations.len(), 2);
        assert_eq!(page.margin_boxes.len(), 1);
        assert_eq!(page.margin_boxes[0].0, MarginBox::TopCenter);
        assert!(
            matches!(&sheet.rules[1], CssRule::Media { rules, .. } if matches!(rules[0], CssRule::Page(_)))
        );
    }

    #[test]
    fn font_rules_parse() {
        let sheet = Stylesheet::parse(
            "@font-face { font-family: x; src: url(x.woff2) }              @font-palette-values --p { base-palette: 1 }              @font-feature-values Foo, \"Bar Baz\" { @styleset { nice: 1 2; } font-display: swap }",
        );
        assert!(sheet.warnings.is_empty(), "{:?}", sheet.warnings);
        assert!(matches!(&sheet.rules[0], CssRule::FontFace(rule) if rule.declarations.len() == 2));
        assert!(matches!(&sheet.rules[1], CssRule::FontPaletteValues(rule) if rule.name == "--p"));
        let CssRule::FontFeatureValues(values) = &sheet.rules[2] else {
            panic!("expected @font-feature-values");
        };
        assert_eq!(values.families, ["Foo", "Bar Baz"]);
        assert_eq!(values.blocks[0].0, "styleset");
        assert_eq!(values.declarations[0].name, "font-display");
    }

    #[test]
    fn keyframes_and_animation_rules() {
        let sheet = Stylesheet::parse(
            "@keyframes spin { from, 50% { color: red; width: 1px !important } entry 10% { color: blue } to { color: green } }\
             @keyframes 'quoted' { 0% { color: red } }\
             @starting-style { p { color: red } }\
             @view-transition { navigation: auto; types: slide fade }\
             @view-transition { navigation: sideways }",
        );
        let messages: Vec<_> = sheet.warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains("navigation"));
        let CssRule::Keyframes(spin) = &sheet.rules[0] else {
            panic!("expected @keyframes");
        };
        assert_eq!(spin.name, "spin");
        assert_eq!(spin.keyframes.len(), 2);
        assert_eq!(spin.keyframes[0].0, [0.0, 0.5]);
        assert_eq!(spin.keyframes[0].1.len(), 1);
        assert!(matches!(&sheet.rules[1], CssRule::Keyframes(k) if k.name == "quoted"));
        assert_eq!(sheet.rules.len(), 2);
    }

    #[test]
    fn function_rules() {
        let sheet = Stylesheet::parse(
            "@function --scale(--x <length>: 1px, --by type(<number> | auto), --any) returns <length> {\
               --twice: calc(var(--x) * 2); result: var(--twice); color: red;\
               @media print { result: 0px } @layer x { } }\
             @function nope() { }",
        );
        let messages: Vec<_> = sheet.warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        let CssRule::Function { name, function } = &sheet.rules[0] else {
            panic!("expected @function");
        };
        assert_eq!(name, "--scale");
        assert_eq!(function.parameters.len(), 3);
        assert_eq!(function.parameters[0].default.as_deref(), Some("1px"));
        assert!(function.parameters[1].syntax.is_some());
        assert!(function.parameters[2].syntax.is_none());
        assert!(function.returns.is_some());
        assert_eq!(function.body.len(), 3);
        assert!(
            matches!(&function.body[2], FunctionItem::Media(condition, items)
            if condition == "print" && items.len() == 1)
        );
    }

    #[test]
    fn style_attribute() {
        let (declarations, warnings) =
            Stylesheet::parse_style_attribute("color: red; ; width: 10px !important");
        assert!(warnings.is_empty());
        assert_eq!(declarations.len(), 2);
        assert!(declarations[1].important);
    }
}
