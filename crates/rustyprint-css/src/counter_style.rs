//! Counter styles (CSS Counter Styles 3): `@counter-style` descriptors, `symbols()`, and
//! turning counter values into text. The predefined styles are `@counter-style` rules in
//! the UA stylesheet.

use std::collections::HashMap;
use std::sync::Arc;

use cssparser::{ParseError, Parser, Token};

type Result<T> = std::result::Result<T, ParseError<()>>;

fn fail<T>(_input: &Parser<'_>) -> Result<T> {
    Err(ParseError::custom(()))
}

/// How a counter style turns values into text.
#[derive(Debug, Clone, PartialEq)]
pub enum System {
    Cyclic,
    Numeric,
    Alphabetic,
    Symbolic,
    Additive,
    /// The symbols stand for consecutive values from this one.
    Fixed(i32),
    /// Another style with some descriptors replaced.
    Extends(Arc<str>),
}

/// The ranges a counter style covers; `auto` depends on the system.
#[derive(Debug, Clone, PartialEq)]
pub enum CounterRange {
    Auto,
    /// Inclusive bounds, with `i64::MIN` and `i64::MAX` for `infinite`.
    Ranges(Vec<(i64, i64)>),
}

/// The descriptors of a `@counter-style` rule; `None` for those it does not set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Descriptors {
    pub system: Option<System>,
    pub negative: Option<(Arc<str>, Arc<str>)>,
    pub prefix: Option<Arc<str>>,
    pub suffix: Option<Arc<str>>,
    pub range: Option<CounterRange>,
    pub pad: Option<(u32, Arc<str>)>,
    pub fallback: Option<Arc<str>>,
    pub symbols: Option<Vec<Arc<str>>>,
    pub additive_symbols: Option<Vec<(u32, Arc<str>)>>,
    /// Parsed for validity; speech is out of scope for PDF.
    pub speak_as: Option<Arc<str>>,
    /// The URLs of image symbols, which the symbols above hold as the characters
    /// [`IMAGE_SYMBOLS`] plus their index here.
    pub images: Vec<Arc<str>>,
}

/// Image symbols are characters of a private use plane in symbol text, from this one on, so
/// counters format to text; [`CounterStyles::image`] gives the URL of each.
pub const IMAGE_SYMBOLS: u32 = 0x10_F000;

/// The character of the image symbol at `index`.
fn image_char(index: usize) -> Option<char> {
    char::from_u32(IMAGE_SYMBOLS + u32::try_from(index).ok()?).filter(|_| index < 0x0FFE)
}

/// Names that `@counter-style` cannot redefine.
const FIXED_NAMES: &[&str] = &[
    "decimal",
    "disc",
    "square",
    "circle",
    "disclosure-open",
    "disclosure-closed",
];

/// Whether `name` can name a `@counter-style` rule: a custom identifier other than `none`.
pub fn valid_rule_name(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "none" | "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default"
    )
}

/// Whether authors can redefine a style: all but a few predefined ones.
pub fn redefinable(name: &str) -> bool {
    !FIXED_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

/// A `<counter-style-name>`: a custom identifier other than `none`.
fn parse_name(input: &mut Parser<'_>) -> Result<Arc<str>> {
    let name = input.expect_ident_cloned()?;
    if name.eq_ignore_ascii_case("none") {
        return fail(input);
    }
    Ok(normalize_name(&name))
}

/// Names of the predefined styles match ASCII case-insensitively; others are case-sensitive.
pub fn normalize_name(name: &str) -> Arc<str> {
    let lower = name.to_ascii_lowercase();
    if PREDEFINED.contains(&lower.as_str()) {
        Arc::from(lower)
    } else {
        Arc::from(name)
    }
}

/// A `<symbol>`: a string, an identifier or an image, which `url()` gives and which is
/// added to `images`.
fn parse_symbol(input: &mut Parser<'_>, images: &mut Vec<Arc<str>>) -> Result<Arc<str>> {
    if let Ok(url) = input.try_parse(|input| input.expect_url()) {
        let Some(symbol) = image_char(images.len()) else {
            return fail(input);
        };
        images.push(Arc::from(&*url));
        return Ok(Arc::from(symbol.to_string()));
    }
    match input.next()?.clone() {
        Token::QuotedString(s) => Ok(Arc::from(&*s)),
        Token::Ident(s) => Ok(Arc::from(&*s)),
        _ => fail(input),
    }
}

fn parse_non_negative(input: &mut Parser<'_>) -> Result<u32> {
    match *input.next()? {
        Token::Number {
            int_value: Some(v), ..
        } if v >= 0 => Ok(v as u32),
        _ => fail(input),
    }
}

impl Descriptors {
    /// Sets a descriptor from its source text. The error is the warning to report.
    pub fn set(&mut self, name: &str, value: &str) -> std::result::Result<(), String> {
        let mut input = Parser::new(value);
        let mut images = std::mem::take(&mut self.images);
        let mut parse_symbol = |input: &mut Parser<'_>| parse_symbol(input, &mut images);
        let parsed = input.parse_entirely(|input| -> Result<()> {
            match name {
                "system" => {
                    let ident = input.expect_ident_cloned()?;
                    self.system = Some(match &*ident.to_ascii_lowercase() {
                        "cyclic" => System::Cyclic,
                        "numeric" => System::Numeric,
                        "alphabetic" => System::Alphabetic,
                        "symbolic" => System::Symbolic,
                        "additive" => System::Additive,
                        "fixed" => System::Fixed(
                            input.try_parse(|input| input.expect_integer()).unwrap_or(1),
                        ),
                        "extends" => System::Extends(parse_name(input)?),
                        _ => return fail(input),
                    });
                }
                "negative" => {
                    let start = parse_symbol(input)?;
                    let end = input
                        .try_parse(parse_symbol)
                        .unwrap_or_else(|_| Arc::from(""));
                    self.negative = Some((start, end));
                }
                "prefix" => self.prefix = Some(parse_symbol(input)?),
                "suffix" => self.suffix = Some(parse_symbol(input)?),
                "range" => {
                    if input
                        .try_parse(|input| input.expect_ident_matching("auto"))
                        .is_ok()
                    {
                        self.range = Some(CounterRange::Auto);
                    } else {
                        fn bound(input: &mut Parser<'_>, infinite: i64) -> Result<i64> {
                            if input
                                .try_parse(|input| input.expect_ident_matching("infinite"))
                                .is_ok()
                            {
                                return Ok(infinite);
                            }
                            Ok(i64::from(input.expect_integer()?))
                        }
                        let ranges = input.parse_comma_separated(|input| {
                            let low = bound(input, i64::MIN)?;
                            let high = bound(input, i64::MAX)?;
                            if low > high {
                                return fail(input);
                            }
                            Ok((low, high))
                        })?;
                        self.range = Some(CounterRange::Ranges(ranges));
                    }
                }
                "pad" => {
                    let (length, symbol) = match input.try_parse(parse_non_negative) {
                        Ok(length) => (length, parse_symbol(input)?),
                        Err(_) => {
                            let symbol = parse_symbol(input)?;
                            (parse_non_negative(input)?, symbol)
                        }
                    };
                    self.pad = Some((length, symbol));
                }
                "fallback" => self.fallback = Some(parse_name(input)?),
                "symbols" => {
                    let mut symbols = vec![parse_symbol(input)?];
                    while let Ok(symbol) = input.try_parse(&mut parse_symbol) {
                        symbols.push(symbol);
                    }
                    self.symbols = Some(symbols);
                }
                "additive-symbols" => {
                    let symbols = input.parse_comma_separated(|input| {
                        match input.try_parse(parse_non_negative) {
                            Ok(weight) => Ok((weight, parse_symbol(input)?)),
                            Err(_) => {
                                let symbol = parse_symbol(input)?;
                                Ok((parse_non_negative(input)?, symbol))
                            }
                        }
                    })?;
                    // Weights must decrease.
                    if symbols.windows(2).any(|w| w[0].0 <= w[1].0) {
                        return fail(input);
                    }
                    self.additive_symbols = Some(symbols);
                }
                "speak-as" => {
                    let ident = input.expect_ident_cloned()?;
                    if ident.eq_ignore_ascii_case("none") {
                        return fail(input);
                    }
                    self.speak_as = Some(Arc::from(&*ident));
                }
                _ => return fail(input),
            }
            Ok(())
        });
        self.images = images;
        match parsed {
            Ok(()) => Ok(()),
            Err(_) if !DESCRIPTORS.contains(&name) => {
                Err(format!("unknown @counter-style descriptor `{name}`"))
            }
            Err(_) => Err(format!("invalid value for `{name}`: `{value}`")),
        }
    }

    /// Why a rule with these descriptors is invalid, if it is (§ 3.1).
    pub fn invalid(&self) -> Option<&'static str> {
        let symbols = self.symbols.as_ref().map_or(0, Vec::len);
        match &self.system {
            Some(System::Extends(_)) => (self.symbols.is_some() || self.additive_symbols.is_some())
                .then_some("`extends` does not take `symbols` or `additive-symbols`"),
            Some(System::Additive) => self
                .additive_symbols
                .is_none()
                .then_some("`additive` needs `additive-symbols`"),
            Some(System::Alphabetic | System::Numeric) if symbols < 2 => {
                Some("this system needs at least two symbols")
            }
            _ if symbols == 0 => Some("missing `symbols`"),
            _ => None,
        }
    }
}

const DESCRIPTORS: &[&str] = &[
    "system",
    "negative",
    "prefix",
    "suffix",
    "range",
    "pad",
    "fallback",
    "symbols",
    "additive-symbols",
    "speak-as",
];

/// The counter style of `counter()`, `counters()` and `list-style-type`: a name, or the
/// anonymous style of `symbols()`.
#[derive(Debug, Clone, PartialEq)]
pub enum CounterStyleRef {
    Name(Arc<str>),
    Symbols(Arc<SymbolsFunction>),
}

/// `symbols(<type>? <symbol>+)`.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolsFunction {
    pub system: System,
    pub symbols: Vec<Arc<str>>,
}

impl CounterStyleRef {
    pub fn decimal() -> Self {
        CounterStyleRef::Name(Arc::from("decimal"))
    }

    /// Parses a `<counter-style>`: a name or `symbols()`. `none` is a name here, for
    /// `counter(x, none)`.
    pub fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_function_matching("symbols"))
            .is_ok()
        {
            return input.parse_nested_block(|input| {
                let system = input
                    .try_parse(|input| {
                        let ident = input.expect_ident_cloned()?;
                        Ok::<_, ParseError<()>>(match &*ident.to_ascii_lowercase() {
                            "cyclic" => System::Cyclic,
                            "numeric" => System::Numeric,
                            "alphabetic" => System::Alphabetic,
                            "symbolic" => System::Symbolic,
                            "fixed" => System::Fixed(1),
                            _ => return fail(input),
                        })
                    })
                    .unwrap_or(System::Symbolic);
                let mut symbols = Vec::new();
                while let Ok(text) = input.try_parse(|input| input.expect_string_cloned()) {
                    symbols.push(Arc::from(&*text));
                }
                let needed = if matches!(system, System::Alphabetic | System::Numeric) {
                    2
                } else {
                    1
                };
                if symbols.len() < needed {
                    return fail(input);
                }
                Ok(CounterStyleRef::Symbols(Arc::new(SymbolsFunction {
                    system,
                    symbols,
                })))
            });
        }
        let name = input.expect_ident_cloned()?;
        if name.eq_ignore_ascii_case("none") {
            return Ok(CounterStyleRef::Name(Arc::from("none")));
        }
        Ok(CounterStyleRef::Name(normalize_name(&name)))
    }

    pub fn to_css(&self) -> String {
        match self {
            CounterStyleRef::Name(name) => {
                let mut out = String::new();
                let _ = cssparser::serialize_identifier(name, &mut out);
                out
            }
            CounterStyleRef::Symbols(function) => {
                let system = match function.system {
                    System::Cyclic => "cyclic ",
                    System::Numeric => "numeric ",
                    System::Alphabetic => "alphabetic ",
                    System::Fixed(_) => "fixed ",
                    _ => "",
                };
                let symbols: Vec<String> = function
                    .symbols
                    .iter()
                    .map(|s| {
                        let mut out = String::new();
                        let _ = cssparser::serialize_string(s, &mut out);
                        out
                    })
                    .collect();
                format!("symbols({system}{})", symbols.join(" "))
            }
        }
    }

    pub fn is_decimal(&self) -> bool {
        matches!(self, CounterStyleRef::Name(name) if &**name == "decimal")
    }
}

/// A counter style with every descriptor resolved.
#[derive(Debug, Clone)]
struct Resolved {
    system: System,
    symbols: Vec<Arc<str>>,
    additive: Vec<(u32, Arc<str>)>,
    negative: (Arc<str>, Arc<str>),
    prefix: Arc<str>,
    suffix: Arc<str>,
    range: CounterRange,
    pad: (u32, Arc<str>),
    fallback: Arc<str>,
}

/// The counter styles of a document: the predefined ones and its `@counter-style` rules.
#[derive(Debug, Clone, Default)]
pub struct CounterStyles {
    rules: HashMap<Arc<str>, Descriptors>,
    /// The URLs of the rules' image symbols.
    images: Vec<Arc<str>>,
}

/// How deep fallbacks and `extends` chains go before giving up on them.
const MAX_DEPTH: usize = 16;

/// How long a representation can get before the fallback is used instead (§ 3.1.1).
const MAX_LENGTH: usize = 120;

impl CounterStyles {
    /// Adds a rule, replacing an earlier one with the same name.
    pub(crate) fn define(&mut self, name: Arc<str>, mut descriptors: Descriptors) {
        // The rule's image symbols are numbered after those of the rules before.
        let images = std::mem::take(&mut descriptors.images);
        if !images.is_empty() {
            let offset = self.images.len();
            let renumber = |symbol: &mut Arc<str>| {
                let text: String = symbol
                    .chars()
                    .map(|c| match (c as u32).checked_sub(IMAGE_SYMBOLS) {
                        Some(i) if (i as usize) < images.len() => {
                            image_char(offset + i as usize).unwrap_or('\u{FFFD}')
                        }
                        _ => c,
                    })
                    .collect();
                *symbol = Arc::from(text);
            };
            let d = &mut descriptors;
            d.symbols.iter_mut().flatten().for_each(renumber);
            d.additive_symbols
                .iter_mut()
                .flatten()
                .for_each(|(_, symbol)| renumber(symbol));
            d.prefix.iter_mut().for_each(renumber);
            d.suffix.iter_mut().for_each(renumber);
            if let Some((start, end)) = &mut d.negative {
                renumber(start);
                renumber(end);
            }
            if let Some((_, symbol)) = &mut d.pad {
                renumber(symbol);
            }
            self.images.extend(images);
        }
        self.rules.insert(name, descriptors);
    }

    /// The URL of an image symbol in formatted counter text, if `c` is one.
    pub fn image(&self, c: char) -> Option<&Arc<str>> {
        let index = (c as u32).checked_sub(IMAGE_SYMBOLS)?;
        self.images.get(index as usize)
    }

    /// Formatted counter text without its image symbols, for where only text can go.
    pub fn without_images(&self, text: &str) -> String {
        text.chars().filter(|&c| self.image(c).is_none()).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.rules.contains_key(name)
    }

    /// Only the predefined styles, from the UA stylesheet.
    pub fn predefined() -> Self {
        let sheet = crate::stylesheet::Stylesheet::parse(include_str!("counter-styles.css"));
        let mut styles = Self::default();
        for rule in &sheet.rules {
            if let crate::stylesheet::CssRule::CounterStyle(rule) = rule {
                let mut descriptors = Descriptors::default();
                for declaration in &rule.declarations {
                    let _ = descriptors.set(&declaration.name, &declaration.value);
                }
                styles.define(Arc::from(rule.name.as_str()), descriptors);
            }
        }
        styles
    }

    /// The text of `value` in a style, as `counter()` shows it: without prefix and suffix.
    pub fn format(&self, value: i32, style: &CounterStyleRef) -> String {
        match style {
            CounterStyleRef::Name(name) if &**name == "none" => String::new(),
            _ => self.generate(value, &self.resolve_ref(style), 0),
        }
    }

    /// The text of a list marker: the prefix, the value and the suffix (CSS Lists 3 § 3.2).
    pub fn marker(&self, value: i32, style: &CounterStyleRef) -> String {
        if matches!(style, CounterStyleRef::Name(name) if &**name == "none") {
            return String::new();
        }
        let resolved = self.resolve_ref(style);
        format!(
            "{}{}{}",
            resolved.prefix,
            self.generate(value, &resolved, 0),
            resolved.suffix
        )
    }

    fn resolve_ref(&self, style: &CounterStyleRef) -> Resolved {
        match style {
            CounterStyleRef::Name(name) => self.resolve(name, 0),
            CounterStyleRef::Symbols(function) => Resolved {
                system: function.system.clone(),
                symbols: function.symbols.clone(),
                additive: Vec::new(),
                negative: (Arc::from("-"), Arc::from("")),
                prefix: Arc::from(""),
                suffix: Arc::from(" "),
                range: CounterRange::Auto,
                pad: (0, Arc::from("")),
                fallback: Arc::from("decimal"),
            },
        }
    }

    /// Resolves a style by name, following `extends`. Unknown names are `decimal`.
    fn resolve(&self, name: &str, depth: usize) -> Resolved {
        let Some(rule) = self.rules.get(name).filter(|_| depth < MAX_DEPTH) else {
            return if name == "decimal" || depth >= MAX_DEPTH {
                decimal()
            } else {
                self.resolve("decimal", depth + 1)
            };
        };
        let mut base = match &rule.system {
            Some(System::Extends(parent)) => self.resolve(parent, depth + 1),
            _ => Resolved {
                system: rule.system.clone().unwrap_or(System::Symbolic),
                symbols: rule.symbols.clone().unwrap_or_default(),
                additive: rule.additive_symbols.clone().unwrap_or_default(),
                ..defaults()
            },
        };
        if let Some(v) = &rule.negative {
            base.negative = v.clone();
        }
        if let Some(v) = &rule.prefix {
            base.prefix = v.clone();
        }
        if let Some(v) = &rule.suffix {
            base.suffix = v.clone();
        }
        if let Some(v) = &rule.range {
            base.range = v.clone();
        }
        if let Some(v) = &rule.pad {
            base.pad = v.clone();
        }
        if let Some(v) = &rule.fallback {
            base.fallback = v.clone();
        }
        base
    }

    /// The representation of `value`, with the negative sign and padding, or the fallback
    /// style's if the style cannot represent it (§ 2.1).
    fn generate(&self, value: i32, style: &Resolved, depth: usize) -> String {
        let value = i64::from(value);
        let fallback = |this: &Self| {
            if depth >= MAX_DEPTH {
                return value.to_string();
            }
            let next = this.resolve(&style.fallback, 0);
            this.generate(value as i32, &next, depth + 1)
        };
        if !in_range(style, value) {
            return fallback(self);
        }
        let negative = value < 0
            && matches!(
                style.system,
                System::Symbolic | System::Alphabetic | System::Numeric | System::Additive
            );
        let magnitude = if negative { -value } else { value };
        let Some(mut text) = represent(style, magnitude).filter(|t| t.len() <= MAX_LENGTH * 4)
        else {
            return fallback(self);
        };
        let mut length = text.chars().count();
        if negative {
            length += style.negative.0.chars().count() + style.negative.1.chars().count();
        }
        let (pad, symbol) = &style.pad;
        if (length as u32) < *pad {
            let padding = symbol.repeat((*pad as usize - length).min(MAX_LENGTH));
            text = padding + &text;
        }
        if negative {
            text = format!("{}{}{}", style.negative.0, text, style.negative.1);
        }
        text
    }
}

fn defaults() -> Resolved {
    Resolved {
        system: System::Symbolic,
        symbols: Vec::new(),
        additive: Vec::new(),
        negative: (Arc::from("-"), Arc::from("")),
        prefix: Arc::from(""),
        suffix: Arc::from(". "),
        range: CounterRange::Auto,
        pad: (0, Arc::from("")),
        fallback: Arc::from("decimal"),
    }
}

/// `decimal`, for when the UA stylesheet's definition is not reachable.
fn decimal() -> Resolved {
    Resolved {
        system: System::Numeric,
        symbols: ('0'..='9').map(|c| Arc::from(c.to_string())).collect(),
        ..defaults()
    }
}

fn in_range(style: &Resolved, value: i64) -> bool {
    match &style.range {
        CounterRange::Ranges(ranges) => ranges.iter().any(|&(lo, hi)| lo <= value && value <= hi),
        CounterRange::Auto => match style.system {
            System::Alphabetic | System::Symbolic => value >= 1,
            System::Additive => value >= 0,
            _ => true,
        },
    }
}

/// The counter algorithm of each system (§ 3.1), for a value the style covers.
fn represent(style: &Resolved, value: i64) -> Option<String> {
    let symbols = &style.symbols;
    let n = symbols.len() as i64;
    match style.system {
        System::Cyclic if n > 0 => Some(symbols[(value - 1).rem_euclid(n) as usize].to_string()),
        System::Fixed(first) if n > 0 => {
            let index = value - i64::from(first);
            (0..n)
                .contains(&index)
                .then(|| symbols[index as usize].to_string())
        }
        System::Symbolic if n > 0 && value >= 1 => {
            let symbol = &symbols[((value - 1) % n) as usize];
            let count = ((value - 1) / n + 1) as usize;
            (count <= MAX_LENGTH).then(|| symbol.repeat(count))
        }
        System::Alphabetic if n >= 2 && value >= 1 => {
            let mut digits = Vec::new();
            let mut v = value;
            while v > 0 {
                v -= 1;
                digits.push(&symbols[(v % n) as usize]);
                v /= n;
            }
            Some(digits.iter().rev().map(|s| &***s).collect())
        }
        System::Numeric if n >= 2 => {
            if value == 0 {
                return Some(symbols[0].to_string());
            }
            let mut digits = Vec::new();
            let mut v = value;
            while v > 0 {
                digits.push(&symbols[(v % n) as usize]);
                v /= n;
            }
            Some(digits.iter().rev().map(|s| &***s).collect())
        }
        System::Additive => {
            let additive = &style.additive;
            if value == 0 {
                return additive
                    .iter()
                    .find(|(weight, _)| *weight == 0)
                    .map(|(_, s)| s.to_string());
            }
            let mut v = value;
            let mut out = String::new();
            for (weight, symbol) in additive {
                let weight = i64::from(*weight);
                if weight == 0 || weight > v {
                    continue;
                }
                let count = v / weight;
                if count as usize > MAX_LENGTH {
                    return None;
                }
                out.push_str(&symbol.repeat(count as usize));
                v -= count * weight;
                if v == 0 {
                    return Some(out);
                }
            }
            None
        }
        _ => None,
    }
}

/// The names of the predefined counter styles, which the UA stylesheet defines.
pub const PREDEFINED: &[&str] = &[
    "decimal",
    "decimal-leading-zero",
    "arabic-indic",
    "armenian",
    "upper-armenian",
    "lower-armenian",
    "bengali",
    "cambodian",
    "khmer",
    "cjk-decimal",
    "devanagari",
    "georgian",
    "gujarati",
    "gurmukhi",
    "hebrew",
    "kannada",
    "lao",
    "malayalam",
    "mongolian",
    "myanmar",
    "oriya",
    "persian",
    "lower-roman",
    "upper-roman",
    "tamil",
    "telugu",
    "thai",
    "tibetan",
    "lower-alpha",
    "lower-latin",
    "upper-alpha",
    "upper-latin",
    "lower-greek",
    "hiragana",
    "hiragana-iroha",
    "katakana",
    "katakana-iroha",
    "cjk-earthly-branch",
    "cjk-heavenly-stem",
    "disc",
    "circle",
    "square",
    "disclosure-open",
    "disclosure-closed",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn styles(css: &[(&str, &[(&str, &str)])]) -> CounterStyles {
        let mut styles = CounterStyles::default();
        for (name, descriptors) in css {
            let mut d = Descriptors::default();
            for (k, v) in *descriptors {
                d.set(k, v).unwrap();
            }
            assert_eq!(d.invalid(), None, "{name}");
            styles.define(Arc::from(*name), d);
        }
        styles
    }

    fn named(name: &str) -> CounterStyleRef {
        CounterStyleRef::Name(Arc::from(name))
    }

    #[test]
    fn systems() {
        let s = styles(&[
            (
                "decimal",
                &[
                    ("system", "numeric"),
                    ("symbols", "'0' '1' '2' '3' '4' '5' '6' '7' '8' '9'"),
                ],
            ),
            ("c", &[("system", "cyclic"), ("symbols", "a b c")]),
            ("f", &[("system", "fixed 3"), ("symbols", "x y")]),
            ("s", &[("system", "symbolic"), ("symbols", "'*' '†'")]),
            ("a", &[("system", "alphabetic"), ("symbols", "a b")]),
            ("n", &[("system", "numeric"), ("symbols", "'0' '1'")]),
            (
                "r",
                &[
                    ("system", "additive"),
                    ("additive-symbols", "10 x, 5 v, 4 iv, 1 i"),
                    ("range", "1 39"),
                ],
            ),
            (
                "e",
                &[
                    ("system", "extends n"),
                    ("prefix", "'<'"),
                    ("suffix", "'>'"),
                    ("pad", "4 '0'"),
                ],
            ),
            (
                "neg",
                &[("system", "extends decimal"), ("negative", "'(' ')'")],
            ),
        ]);
        let f = |v: i32, name: &str| s.format(v, &named(name));
        assert_eq!([f(1, "c"), f(4, "c"), f(0, "c")], ["a", "a", "c"]);
        assert_eq!([f(3, "f"), f(4, "f"), f(5, "f")], ["x", "y", "5"]);
        assert_eq!([f(1, "s"), f(3, "s"), f(4, "s")], ["*", "**", "††"]);
        assert_eq!([f(1, "a"), f(3, "a"), f(6, "a")], ["a", "aa", "bb"]);
        assert_eq!([f(5, "n"), f(0, "n"), f(-2, "n")], ["101", "0", "-10"]);
        assert_eq!([f(14, "r"), f(40, "r"), f(0, "r")], ["xiv", "40", "0"]);
        assert_eq!(s.marker(5, &named("e")), "<0101>");
        assert_eq!(f(-3, "neg"), "(3)");
        assert_eq!(f(7, "unknown"), "7");
        assert_eq!(f(7, "none"), "");
    }

    #[test]
    fn invalid_rules() {
        let mut d = Descriptors::default();
        d.set("system", "alphabetic").unwrap();
        d.set("symbols", "a").unwrap();
        assert!(d.invalid().is_some());
        assert!(d.set("additive-symbols", "1 a, 5 b").is_err());
        assert!(d.set("range", "5 1").is_err());
        assert!(d.set("colour", "red").unwrap_err().contains("unknown"));
        assert!(!redefinable("Decimal"));
        assert!(!valid_rule_name("None"));
        assert!(valid_rule_name("my-style"));
    }
}
