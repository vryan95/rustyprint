//! Value types of generated content (CSS Generated Content 3, CSS Lists 3): `content` and
//! counter styles.

use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

use super::{ImageValue, Result, fail};
use crate::counter_style::CounterStyleRef;
use crate::properties::{ComputeContext, Property};

/// One item of a `content` list.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentItem {
    String(Arc<str>),
    /// `counter(name, style)`.
    Counter {
        name: Arc<str>,
        style: CounterStyleRef,
    },
    /// `counters(name, separator, style)`.
    Counters {
        name: Arc<str>,
        separator: Arc<str>,
        style: CounterStyleRef,
    },
    OpenQuote,
    CloseQuote,
    NoOpenQuote,
    NoCloseQuote,
    Image(ImageValue),
    /// `element(name, policy)`: a running element (CSS GCPM 3 § 2.2).
    Element {
        name: Arc<str>,
        policy: ElementPolicy,
    },
    /// `target-counter(url, name, style)`: a counter's value at the element a URL points to,
    /// such as its page number (CSS GCPM 3 § 4).
    TargetCounter {
        target: Arc<str>,
        name: Arc<str>,
        style: CounterStyleRef,
    },
    /// `target-counters(url, name, separator, style)`.
    TargetCounters {
        target: Arc<str>,
        name: Arc<str>,
        separator: Arc<str>,
        style: CounterStyleRef,
    },
    /// `target-text(url, kind)`: text of the element a URL points to.
    TargetText {
        target: Arc<str>,
        kind: TargetTextKind,
    },
    /// `leader(pattern)`: the pattern repeated to fill the rest of the line (CSS GCPM 3 § 5).
    Leader(Arc<str>),
}

/// Which text of its target `target-text()` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetTextKind {
    Content,
    Before,
    After,
    FirstLetter,
}

impl TargetTextKind {
    fn as_str(self) -> &'static str {
        match self {
            TargetTextKind::Content => "content",
            TargetTextKind::Before => "before",
            TargetTextKind::After => "after",
            TargetTextKind::FirstLetter => "first-letter",
        }
    }
}

/// The URL of `target-counter()` and similar: a `<url>`, or a string, which `attr(href)`
/// gives.
fn target_url(input: &mut Parser<'_>) -> Result<Arc<str>> {
    Ok(Arc::from(&*input.expect_url_or_string()?))
}

/// Which running element `element()` takes when a page has several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementPolicy {
    /// The first assigned on the page, or the last from earlier pages.
    First,
    /// Like `first`, but only an element that starts the page counts as on it.
    Start,
    /// The last assigned on the page or before it.
    Last,
    /// None on pages that assign one; otherwise like `first`.
    FirstExcept,
}

impl ElementPolicy {
    fn as_str(self) -> &'static str {
        match self {
            ElementPolicy::First => "first",
            ElementPolicy::Start => "start",
            ElementPolicy::Last => "last",
            ElementPolicy::FirstExcept => "first-except",
        }
    }
}

/// `position`. Only `running()` affects layout so far: the other values lay out as `static`
/// until milestone 10.
#[derive(Debug, Clone, PartialEq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
    /// `running(name)`: the element leaves the flow for `element()` in margin boxes.
    Running(Arc<str>),
}

impl Property for Position {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            Position::Static => "static".into(),
            Position::Relative => "relative".into(),
            Position::Absolute => "absolute".into(),
            Position::Fixed => "fixed".into(),
            Position::Sticky => "sticky".into(),
            Position::Running(name) => format!("running({})", ident_css(name)),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        let token = input.next()?.clone();
        Ok(match token {
            Token::Ident(ident) => match &*ident.to_ascii_lowercase() {
                "static" => Position::Static,
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                "fixed" => Position::Fixed,
                "sticky" => Position::Sticky,
                _ => return fail(),
            },
            Token::Function(name) if name.eq_ignore_ascii_case("running") => input
                .parse_nested_block(|input| {
                    Ok(Position::Running(Arc::from(&*input.expect_ident_cloned()?)))
                })?,
            _ => return fail(),
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

/// The `content` property.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Normal,
    None,
    /// Content items, with alternative text after `/`.
    Items {
        items: Arc<[ContentItem]>,
        alt: Option<Arc<[ContentItem]>>,
    },
}

fn string_css(value: &str) -> String {
    let mut out = String::new();
    let _ = cssparser::serialize_string(value, &mut out);
    out
}

fn ident_css(value: &str) -> String {
    let mut out = String::new();
    let _ = cssparser::serialize_identifier(value, &mut out);
    out
}

impl ContentItem {
    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        // An image: a URL or a gradient, but not `none`.
        if let Ok(image) = input.try_parse(|input| match ImageValue::parse(input)? {
            ImageValue::None => fail(),
            image => Ok(image),
        }) {
            return Ok(ContentItem::Image(image));
        }
        let token = input.next()?.clone();
        Ok(match token {
            Token::QuotedString(text) => ContentItem::String(Arc::from(&*text)),
            Token::Ident(ident) => match &*ident.to_ascii_lowercase() {
                "open-quote" => ContentItem::OpenQuote,
                "close-quote" => ContentItem::CloseQuote,
                "no-open-quote" => ContentItem::NoOpenQuote,
                "no-close-quote" => ContentItem::NoCloseQuote,
                _ => return fail(),
            },
            Token::Function(name) => {
                let name = name.to_ascii_lowercase();
                input.parse_nested_block(|input| match name.as_str() {
                    "counter" => {
                        let counter = input.expect_ident_cloned()?;
                        let style = counter_style(input)?;
                        Ok(ContentItem::Counter {
                            name: Arc::from(&*counter),
                            style,
                        })
                    }
                    "element" => {
                        let element = input.expect_ident_cloned()?;
                        let policy = if input.try_parse(|input| input.expect_comma()).is_ok() {
                            match &*input.expect_ident()?.to_ascii_lowercase() {
                                "first" => ElementPolicy::First,
                                "start" => ElementPolicy::Start,
                                "last" => ElementPolicy::Last,
                                "first-except" => ElementPolicy::FirstExcept,
                                _ => return Err(ParseError::custom(())),
                            }
                        } else {
                            ElementPolicy::First
                        };
                        Ok(ContentItem::Element {
                            name: Arc::from(&*element),
                            policy,
                        })
                    }
                    "target-counter" => {
                        let target = target_url(input)?;
                        input.expect_comma()?;
                        let counter = input.expect_ident_cloned()?;
                        let style = counter_style(input)?;
                        Ok(ContentItem::TargetCounter {
                            target,
                            name: Arc::from(&*counter),
                            style,
                        })
                    }
                    "target-counters" => {
                        let target = target_url(input)?;
                        input.expect_comma()?;
                        let counter = input.expect_ident_cloned()?;
                        input.expect_comma()?;
                        let separator = input.expect_string_cloned()?;
                        let style = counter_style(input)?;
                        Ok(ContentItem::TargetCounters {
                            target,
                            name: Arc::from(&*counter),
                            separator: Arc::from(&*separator),
                            style,
                        })
                    }
                    "target-text" => {
                        let target = target_url(input)?;
                        let kind = if input.try_parse(|input| input.expect_comma()).is_ok() {
                            match &*input.expect_ident()?.to_ascii_lowercase() {
                                "content" => TargetTextKind::Content,
                                "before" => TargetTextKind::Before,
                                "after" => TargetTextKind::After,
                                "first-letter" => TargetTextKind::FirstLetter,
                                _ => return Err(ParseError::custom(())),
                            }
                        } else {
                            TargetTextKind::Content
                        };
                        Ok(ContentItem::TargetText { target, kind })
                    }
                    "leader" => {
                        let pattern = match input.try_parse(|input| input.expect_string_cloned()) {
                            Ok(text) => text.to_string(),
                            Err(_) => match &*input.expect_ident()?.to_ascii_lowercase() {
                                "dotted" => ". ".into(),
                                "solid" => "_".into(),
                                "space" => " ".into(),
                                _ => return Err(ParseError::custom(())),
                            },
                        };
                        Ok(ContentItem::Leader(Arc::from(pattern)))
                    }
                    "counters" => {
                        let counter = input.expect_ident_cloned()?;
                        input.expect_comma()?;
                        let separator = input.expect_string_cloned()?;
                        let style = counter_style(input)?;
                        Ok(ContentItem::Counters {
                            name: Arc::from(&*counter),
                            separator: Arc::from(&*separator),
                            style,
                        })
                    }
                    _ => Err(ParseError::custom(())),
                })?
            }
            _ => return fail(),
        })
    }

    fn css(&self) -> String {
        let style_css = |style: &CounterStyleRef| {
            if style.is_decimal() {
                String::new()
            } else {
                format!(", {}", style.to_css())
            }
        };
        match self {
            ContentItem::String(text) => string_css(text),
            ContentItem::Counter { name, style } => {
                format!("counter({}{})", ident_css(name), style_css(style))
            }
            ContentItem::Counters {
                name,
                separator,
                style,
            } => format!(
                "counters({}, {}{})",
                ident_css(name),
                string_css(separator),
                style_css(style)
            ),
            ContentItem::OpenQuote => "open-quote".into(),
            ContentItem::CloseQuote => "close-quote".into(),
            ContentItem::NoOpenQuote => "no-open-quote".into(),
            ContentItem::NoCloseQuote => "no-close-quote".into(),
            ContentItem::Element { name, policy } => match policy {
                ElementPolicy::First => format!("element({})", ident_css(name)),
                policy => format!("element({}, {})", ident_css(name), policy.as_str()),
            },
            ContentItem::Image(image) => image.to_css(),
            ContentItem::TargetCounter {
                target,
                name,
                style,
            } => format!(
                "target-counter(url({}), {}{})",
                string_css(target),
                ident_css(name),
                style_css(style)
            ),
            ContentItem::TargetCounters {
                target,
                name,
                separator,
                style,
            } => format!(
                "target-counters(url({}), {}, {}{})",
                string_css(target),
                ident_css(name),
                string_css(separator),
                style_css(style)
            ),
            ContentItem::TargetText { target, kind } => match kind {
                TargetTextKind::Content => format!("target-text(url({}))", string_css(target)),
                kind => format!(
                    "target-text(url({}), {})",
                    string_css(target),
                    kind.as_str()
                ),
            },
            ContentItem::Leader(pattern) => match &**pattern {
                ". " => "leader(dotted)".into(),
                "_" => "leader(solid)".into(),
                " " => "leader(space)".into(),
                pattern => format!("leader({})", string_css(pattern)),
            },
        }
    }
}

/// The optional `, <counter-style>` at the end of `counter()` and `counters()`.
fn counter_style(input: &mut Parser<'_>) -> Result<CounterStyleRef> {
    if input.try_parse(|input| input.expect_comma()).is_ok() {
        CounterStyleRef::parse(input)
    } else {
        Ok(CounterStyleRef::decimal())
    }
}

impl Property for Content {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            Content::Normal => "normal".into(),
            Content::None => "none".into(),
            Content::Items { items, alt } => {
                let list = |items: &[ContentItem]| {
                    items
                        .iter()
                        .map(ContentItem::css)
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                match alt {
                    Some(alt) => format!("{} / {}", list(items), list(alt)),
                    None => list(items),
                }
            }
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if let Ok(keyword) = input.try_parse(|input| {
            let ident = input.expect_ident_cloned()?.to_ascii_lowercase();
            input.expect_exhausted()?;
            match ident.as_str() {
                "normal" => Ok(Content::Normal),
                "none" => Ok(Content::None),
                _ => fail(),
            }
        }) {
            return Ok(keyword);
        }
        let mut items = vec![ContentItem::parse(input)?];
        while let Ok(item) = input.try_parse(ContentItem::parse) {
            items.push(item);
        }
        let alt = if input.try_parse(|input| input.expect_delim('/')).is_ok() {
            let mut alt = Vec::new();
            while let Ok(item) = input.try_parse(ContentItem::parse) {
                if matches!(item, ContentItem::Image(_)) {
                    return fail();
                }
                alt.push(item);
            }
            Some(alt.into())
        } else {
            None
        };
        Ok(Content::Items {
            items: items.into(),
            alt,
        })
    }

    fn compute(specified: &Self, ctx: &ComputeContext) -> Self {
        let compute = |items: &Arc<[ContentItem]>| -> Arc<[ContentItem]> {
            items
                .iter()
                .map(|item| match item {
                    ContentItem::Image(image) => ContentItem::Image(image.compute(ctx)),
                    other => other.clone(),
                })
                .collect()
        };
        match specified {
            Content::Items { items, alt } => Content::Items {
                items: compute(items),
                alt: alt.as_ref().map(compute),
            },
            other => other.clone(),
        }
    }
}

/// One counter named by `counter-reset`, `counter-set` or `counter-increment`.
#[derive(Debug, Clone, PartialEq)]
pub struct CounterChange {
    pub name: Arc<str>,
    /// The value, or `None` for `reversed(name)` without one: the counter then starts from
    /// the number of items it counts.
    pub value: Option<i32>,
    /// `reversed(name)` in `counter-reset`.
    pub reversed: bool,
}

/// A `<counter-name>`: a custom identifier other than `none`.
fn counter_name(input: &mut Parser<'_>) -> Result<Arc<str>> {
    let name = input.expect_ident_cloned()?;
    if name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("default") {
        return fail();
    }
    Ok(Arc::from(&*name))
}

/// `none | [ <counter-name> <integer>? ]+`, with `reversed(<counter-name>)` when
/// `reversible`. Counters without a value get `default`.
fn parse_counter_changes(
    input: &mut Parser<'_>,
    default: i32,
    reversible: bool,
) -> Result<Arc<[CounterChange]>> {
    if input
        .try_parse(|input| input.expect_ident_matching("none"))
        .is_ok()
    {
        return Ok(Arc::from([]));
    }
    let mut changes = Vec::new();
    loop {
        let reversed = reversible
            && input
                .try_parse(|input| input.expect_function_matching("reversed"))
                .is_ok();
        let name = if reversed {
            input.parse_nested_block(counter_name)?
        } else if changes.is_empty() {
            counter_name(input)?
        } else {
            match input.try_parse(counter_name) {
                Ok(name) => name,
                Err(_) => break,
            }
        };
        let value = input
            .try_parse(|input| super::parse_integer(input, i32::MIN))
            .ok();
        changes.push(CounterChange {
            name,
            value: value.or((!reversed).then_some(default)),
            reversed,
        });
    }
    Ok(changes.into())
}

fn counter_changes_css(changes: &[CounterChange]) -> String {
    if changes.is_empty() {
        return "none".into();
    }
    changes
        .iter()
        .map(|change| {
            let name = if change.reversed {
                format!("reversed({})", ident_css(&change.name))
            } else {
                ident_css(&change.name)
            };
            match change.value {
                Some(value) => format!("{name} {value}"),
                None => name,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

macro_rules! counter_property {
    ($(#[$meta:meta])* $name:ident, $default:expr, $reversible:expr) => {
        $(#[$meta])*
        pub struct $name;

        impl Property for $name {
            type Specified = Arc<[CounterChange]>;
            type Computed = Arc<[CounterChange]>;

            fn to_css(computed: &Self::Computed) -> String {
                counter_changes_css(computed)
            }

            fn parse(input: &mut Parser<'_>) -> Result<Self::Specified> {
                parse_counter_changes(input, $default, $reversible)
            }

            fn compute(specified: &Self::Specified, _ctx: &ComputeContext) -> Self::Computed {
                specified.clone()
            }
        }
    };
}

counter_property!(
    /// `counter-reset`: creates counters, starting at 0 by default.
    CounterReset,
    0,
    true
);
counter_property!(
    /// `counter-increment`: adds to counters, 1 by default.
    CounterIncrement,
    1,
    false
);
counter_property!(
    /// `counter-set`: sets counters, to 0 by default.
    CounterSet,
    0,
    false
);

/// The `quotes` property. `match-parent` computes to the parent's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Quotes {
    /// Quotes for the content language.
    Auto,
    None,
    /// Open and close quotes for each nesting level.
    Pairs(Arc<[(Arc<str>, Arc<str>)]>),
}

/// The specified `quotes`: a computed value or `match-parent`.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecifiedQuotes {
    Value(Quotes),
    MatchParent,
}

impl Property for Quotes {
    type Specified = SpecifiedQuotes;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            Quotes::Auto => "auto".into(),
            Quotes::None => "none".into(),
            Quotes::Pairs(pairs) => pairs
                .iter()
                .map(|(open, close)| format!("{} {}", string_css(open), string_css(close)))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<SpecifiedQuotes> {
        if let Ok(ident) = input.try_parse(|input| input.expect_ident_cloned()) {
            return Ok(match &*ident.to_ascii_lowercase() {
                "auto" => SpecifiedQuotes::Value(Quotes::Auto),
                "none" => SpecifiedQuotes::Value(Quotes::None),
                "match-parent" => SpecifiedQuotes::MatchParent,
                _ => return fail(),
            });
        }
        let mut pairs = Vec::new();
        while let Ok(open) = input.try_parse(|input| input.expect_string_cloned()) {
            let close = input.expect_string_cloned()?;
            pairs.push((Arc::from(&*open), Arc::from(&*close)));
        }
        if pairs.is_empty() {
            return fail();
        }
        Ok(SpecifiedQuotes::Value(Quotes::Pairs(pairs.into())))
    }

    fn compute(specified: &SpecifiedQuotes, ctx: &ComputeContext) -> Self {
        match specified {
            SpecifiedQuotes::Value(quotes) => quotes.clone(),
            SpecifiedQuotes::MatchParent => ctx.parent.quotes.clone(),
        }
    }
}

impl Quotes {
    /// The open and close quotes for nesting `depth` (0 for the outermost), in `lang` for
    /// `auto`. Levels deeper than the list repeat its last pair.
    pub fn pair(&self, depth: usize, lang: Option<&str>) -> Option<(Arc<str>, Arc<str>)> {
        match self {
            Quotes::None => None,
            Quotes::Pairs(pairs) => pairs.get(depth.min(pairs.len() - 1)).cloned(),
            Quotes::Auto => {
                let pairs = auto_quotes(lang.unwrap_or(""));
                let (open, close) = pairs[depth.min(pairs.len() - 1)];
                Some((Arc::from(open), Arc::from(close)))
            }
        }
    }
}

/// Quotation marks by language, from the CLDR delimiters.
fn auto_quotes(lang: &str) -> &'static [(&'static str, &'static str)] {
    let primary = lang
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match primary.as_str() {
        "de" | "cs" | "sk" | "sl" | "lt" | "is" | "bg" => &[("„", "“"), ("‚", "‘")],
        "da" => &[("»", "«"), ("›", "‹")],
        "nl" | "en" | "" => &[("“", "”"), ("‘", "’")],
        "fr" => &[("«\u{202F}", "\u{202F}»"), ("«\u{202F}", "\u{202F}»")],
        "es" | "it" | "pt" | "ca" | "el" | "ru" | "uk" | "be" | "fa" | "ar" | "eu" => {
            &[("«", "»"), ("“", "”")]
        }
        "nb" | "nn" | "no" => &[("«", "»"), ("‘", "’")],
        "sv" | "fi" => &[("”", "”"), ("’", "’")],
        "pl" | "ro" | "hr" | "hu" => &[("„", "”"), ("«", "»")],
        "ja" => &[("「", "」"), ("『", "』")],
        "zh" if ["hant", "tw", "hk", "mo"]
            .iter()
            .any(|tag| lang.to_ascii_lowercase().contains(tag)) =>
        {
            &[("「", "」"), ("『", "』")]
        }
        "zh" => &[("“", "”"), ("‘", "’")],
        "ko" => &[("“", "”"), ("‘", "’")],
        "he" => &[("”", "”"), ("’", "’")],
        _ => &[("“", "”"), ("‘", "’")],
    }
}

/// `list-style-type`: a counter style, a string, or `none`.
#[derive(Debug, Clone, PartialEq)]
pub enum ListStyleType {
    None,
    /// A counter style, such as `decimal`, `disc` or `symbols()`.
    Style(CounterStyleRef),
    /// A string used as the marker as is.
    String(Arc<str>),
}

impl Property for ListStyleType {
    type Specified = Self;
    type Computed = Self;

    fn to_css(computed: &Self) -> String {
        match computed {
            ListStyleType::None => "none".into(),
            ListStyleType::Style(style) => style.to_css(),
            ListStyleType::String(text) => string_css(text),
        }
    }

    fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if let Ok(text) = input.try_parse(|input| input.expect_string_cloned()) {
            return Ok(ListStyleType::String(Arc::from(&*text)));
        }
        if input
            .try_parse(|input| input.expect_ident_matching("default"))
            .is_ok()
        {
            return fail();
        }
        Ok(match CounterStyleRef::parse(input)? {
            CounterStyleRef::Name(name) if &*name == "none" => ListStyleType::None,
            style => ListStyleType::Style(style),
        })
    }

    fn compute(specified: &Self, _ctx: &ComputeContext) -> Self {
        specified.clone()
    }
}

keywords!(
    /// `list-style-position`.
    ListStylePosition { Outside = "outside", Inside = "inside" }
);

/// `list-style-image`.
pub struct ListStyleImage;

impl Property for ListStyleImage {
    type Specified = ImageValue;
    type Computed = ImageValue;

    fn to_css(computed: &ImageValue) -> String {
        computed.to_css()
    }

    fn parse(input: &mut Parser<'_>) -> Result<ImageValue> {
        ImageValue::parse(input)
    }

    fn compute(specified: &ImageValue, ctx: &ComputeContext) -> ImageValue {
        specified.compute(ctx)
    }
}
