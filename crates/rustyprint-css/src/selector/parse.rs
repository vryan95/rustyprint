use std::cell::Cell;
use std::collections::HashMap;

use cssparser::{ParseError, Parser, Token, parse_nth};
use rustyprint_dom::{LocalName, Namespace};

use super::{
    AttributeCase, AttributeOperation, AttributeOperator, AttributeSelector, Combinator, Compound,
    Direction, NamespaceConstraint, Nth, PseudoClass, PseudoElement, RelativeSelector, Selector,
    SelectorList, Simple,
};

/// Error returned when a selector is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid selector")]
pub struct SelectorParseError;

type Result<T> = std::result::Result<T, ParseError<()>>;

/// Namespace prefixes declared with `@namespace`.
#[derive(Debug, Clone, Default)]
pub struct Namespaces {
    pub default: Option<Namespace>,
    pub prefixes: HashMap<String, Namespace>,
}

impl SelectorList {
    /// Parses a selector list, such as the prelude of a top-level style rule.
    pub fn parse(
        text: &str,
        namespaces: &Namespaces,
    ) -> std::result::Result<Self, SelectorParseError> {
        let mut parser = Parser::new(text);
        parser
            .parse_entirely(|input| parse_selector_list(input, namespaces, None))
            .map_err(|_| SelectorParseError)
    }
}

/// Parses a selector list. With `parent`, parses a nested rule's prelude: selectors may start
/// with a combinator, `&` stands for the parent selectors, and selectors without `&` are
/// relative to the parent, as CSS Nesting defines.
pub(crate) fn parse_selector_list<'i>(
    input: &mut Parser<'i>,
    namespaces: &Namespaces,
    parent: Option<&SelectorList>,
) -> Result<SelectorList> {
    let ctx = Context {
        namespaces,
        parent,
        used_nesting: Cell::new(false),
    };
    let selectors = input.parse_comma_separated(|input| {
        ctx.used_nesting.set(false);
        let leading = match parent {
            Some(_) => parse_leading_combinator(input),
            None => None,
        };
        let selector = parse_complex(input, &ctx, true)?;
        Ok(match parent {
            Some(parent) if leading.is_some() || !ctx.used_nesting.get() => prepend(
                Compound(vec![Simple::PseudoClass(PseudoClass::Is(parent.clone()))]),
                leading,
                selector,
            ),
            _ => selector,
        })
    })?;
    Ok(SelectorList(selectors))
}

/// Joins `compound` and `selector` with `combinator` (descendant by default).
fn prepend(compound: Compound, combinator: Option<Combinator>, mut selector: Selector) -> Selector {
    selector.compounds.insert(0, compound);
    selector
        .combinators
        .insert(0, combinator.unwrap_or(Combinator::Descendant));
    selector
}

struct Context<'a> {
    namespaces: &'a Namespaces,
    parent: Option<&'a SelectorList>,
    used_nesting: Cell<bool>,
}

impl Context<'_> {
    fn namespace(&self, prefix: &str) -> Option<Namespace> {
        self.namespaces.prefixes.get(prefix).cloned()
    }

    /// The constraint for an unprefixed type or universal selector.
    fn default_namespace(&self) -> NamespaceConstraint {
        match &self.namespaces.default {
            Some(ns) => NamespaceConstraint::Specific(ns.clone()),
            None => NamespaceConstraint::Any,
        }
    }
}

fn fail<T>() -> Result<T> {
    Err(ParseError::custom(()))
}

fn parse_leading_combinator(input: &mut Parser<'_>) -> Option<Combinator> {
    input
        .try_parse(|input| -> Result<Combinator> {
            match input.next()?.clone() {
                Token::Delim('>') => Ok(Combinator::Child),
                Token::Delim('+') => Ok(Combinator::NextSibling),
                Token::Delim('~') => Ok(Combinator::SubsequentSibling),
                _ => fail(),
            }
        })
        .ok()
}

/// Parses a complex selector. `allow_pseudo_element` is false inside functional pseudo-classes.
fn parse_complex<'i>(
    input: &mut Parser<'i>,
    ctx: &Context,
    allow_pseudo_element: bool,
) -> Result<Selector> {
    input.skip_whitespace();
    let mut compounds = Vec::new();
    let mut combinators = Vec::new();
    loop {
        let (compound, pseudo_element) = parse_compound(input, ctx, allow_pseudo_element)?;
        compounds.push(compound);
        if pseudo_element.is_some() {
            input.skip_whitespace();
            if !input.is_exhausted() {
                return fail();
            }
            return Ok(Selector {
                compounds,
                combinators,
                pseudo_element,
            });
        }
        match parse_combinator(input)? {
            Some(combinator) => {
                combinators.push(combinator);
                input.skip_whitespace();
            }
            None => {
                return Ok(Selector {
                    compounds,
                    combinators,
                    pseudo_element: None,
                });
            }
        }
    }
}

/// Parses the combinator after a compound, or returns `None` at the end of the selector.
fn parse_combinator<'i>(input: &mut Parser<'i>) -> Result<Option<Combinator>> {
    let mut saw_whitespace = false;
    loop {
        let state = input.state();
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(_) => return Ok(None),
        };
        match token {
            Token::WhiteSpace(_) => saw_whitespace = true,
            Token::Delim('>') => return Ok(Some(Combinator::Child)),
            Token::Delim('+') => return Ok(Some(Combinator::NextSibling)),
            Token::Delim('~') => return Ok(Some(Combinator::SubsequentSibling)),
            Token::Delim('|')
                if matches!(input.next_including_whitespace(), Ok(Token::Delim('|'))) =>
            {
                return Ok(Some(Combinator::Column));
            }
            _ => {
                input.reset(&state);
                return Ok(saw_whitespace.then_some(Combinator::Descendant));
            }
        }
    }
}

fn parse_compound<'i>(
    input: &mut Parser<'i>,
    ctx: &Context,
    allow_pseudo_element: bool,
) -> Result<(Compound, Option<PseudoElement>)> {
    let mut simples = Vec::new();
    if let Some(simple) = parse_type_or_universal(input, ctx)? {
        simples.push(simple);
    }
    let mut pseudo_element = None;
    loop {
        let state = input.state();
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        let simple =
            match token {
                Token::IDHash(id) if pseudo_element.is_none() => Simple::Id(id.to_string()),
                Token::Delim('.') if pseudo_element.is_none() => {
                    match input.next_including_whitespace()?.clone() {
                        Token::Ident(class) => Simple::Class(class.to_string()),
                        _ => return fail(),
                    }
                }
                Token::SquareBracketBlock if pseudo_element.is_none() => Simple::Attribute(
                    input.parse_nested_block(|input| parse_attribute(input, ctx))?,
                ),
                Token::Delim('&') if pseudo_element.is_none() => {
                    ctx.used_nesting.set(true);
                    Simple::PseudoClass(match ctx.parent {
                        Some(parent) => PseudoClass::Is(parent.clone()),
                        None => PseudoClass::Scope,
                    })
                }
                Token::Colon => match input.next_including_whitespace()?.clone() {
                    Token::Colon => {
                        if !allow_pseudo_element || pseudo_element.is_some() {
                            return fail();
                        }
                        pseudo_element = Some(parse_pseudo_element(input)?);
                        continue;
                    }
                    Token::Ident(name) if legacy_pseudo_element(&name).is_some() => {
                        if !allow_pseudo_element || pseudo_element.is_some() {
                            return fail();
                        }
                        pseudo_element = legacy_pseudo_element(&name);
                        continue;
                    }
                    Token::Ident(name) => {
                        let pseudo = pseudo_class(&name.to_ascii_lowercase())
                            .ok_or_else(|| ParseError::custom(()))?;
                        // Only user-action pseudo-classes may follow a pseudo-element.
                        if pseudo_element.is_some() && !matches!(pseudo, PseudoClass::Never(_)) {
                            return fail();
                        }
                        Simple::PseudoClass(pseudo)
                    }
                    Token::Function(name) if pseudo_element.is_none() => {
                        let name = name.to_ascii_lowercase();
                        Simple::PseudoClass(input.parse_nested_block(|input| {
                            functional_pseudo_class(input, ctx, &name)
                        })?)
                    }
                    _ => return fail(),
                },
                _ => {
                    input.reset(&state);
                    break;
                }
            };
        simples.push(simple);
    }
    if simples.is_empty() && pseudo_element.is_none() {
        return fail();
    }
    Ok((Compound(simples), pseudo_element))
}

/// Parses an optional `ns|name` prefix shared by type, universal and attribute selectors.
/// Returns the namespace constraint (`None` if unprefixed) and the name token.
fn parse_qualified_name<'i>(
    input: &mut Parser<'i>,
    ctx: &Context,
    allow_universal: bool,
) -> Result<Option<(Option<NamespaceConstraint>, Option<String>)>> {
    let start = input.state();
    let first = match input.next_including_whitespace() {
        Ok(token) => token.clone(),
        Err(_) => return Ok(None),
    };
    let first_name = match &first {
        Token::Ident(name) => Some(name.to_string()),
        Token::Delim('*') if allow_universal => None,
        Token::Delim('|') => {
            let local = parse_local_name(input, allow_universal)?;
            return Ok(Some((Some(NamespaceConstraint::None), local)));
        }
        _ => {
            input.reset(&start);
            return Ok(None);
        }
    };
    let after_first = input.state();
    let is_prefix = matches!(input.next_including_whitespace(), Ok(Token::Delim('|')))
        && matches!(
            input.next_including_whitespace(),
            Ok(Token::Ident(_)) | Ok(Token::Delim('*'))
        );
    input.reset(&after_first);
    if !is_prefix {
        return Ok(Some((None, first_name)));
    }
    input.next_including_whitespace()?;
    let ns = match &first_name {
        None => NamespaceConstraint::Any,
        Some(prefix) => NamespaceConstraint::Specific(
            ctx.namespace(prefix)
                .ok_or_else(|| ParseError::custom(()))?,
        ),
    };
    let local = parse_local_name(input, allow_universal)?;
    Ok(Some((Some(ns), local)))
}

fn parse_local_name<'i>(input: &mut Parser<'i>, allow_universal: bool) -> Result<Option<String>> {
    match input.next_including_whitespace()?.clone() {
        Token::Ident(name) => Ok(Some(name.to_string())),
        Token::Delim('*') if allow_universal => Ok(None),
        _ => fail(),
    }
}

fn parse_type_or_universal<'i>(input: &mut Parser<'i>, ctx: &Context) -> Result<Option<Simple>> {
    let Some((ns, name)) = parse_qualified_name(input, ctx, true)? else {
        return Ok(None);
    };
    let ns = ns.unwrap_or_else(|| ctx.default_namespace());
    Ok(Some(match name {
        None => Simple::Universal(ns),
        Some(name) => Simple::Type {
            ns,
            lower: LocalName::from(name.to_ascii_lowercase()),
            name: LocalName::from(name),
        },
    }))
}

fn parse_attribute<'i>(input: &mut Parser<'i>, ctx: &Context) -> Result<AttributeSelector> {
    input.skip_whitespace();
    let Some((ns, Some(name))) = parse_qualified_name(input, ctx, false)? else {
        return fail();
    };
    let ns = ns.unwrap_or(NamespaceConstraint::None);
    let lower = LocalName::from(name.to_ascii_lowercase());
    let name = LocalName::from(name);
    input.skip_whitespace();
    if input.is_exhausted() {
        return Ok(AttributeSelector {
            ns,
            name,
            lower,
            operation: None,
        });
    }
    let operator = match input.next()?.clone() {
        Token::Delim('=') => AttributeOperator::Equals,
        Token::IncludeMatch => AttributeOperator::Includes,
        Token::DashMatch => AttributeOperator::DashMatch,
        Token::PrefixMatch => AttributeOperator::Prefix,
        Token::SuffixMatch => AttributeOperator::Suffix,
        Token::SubstringMatch => AttributeOperator::Substring,
        _ => return fail(),
    };
    let value = input.expect_ident_or_string()?.to_string();
    let case = match input.try_parse(|input| input.expect_ident_cloned()) {
        Ok(flag) if flag.eq_ignore_ascii_case("i") => AttributeCase::Insensitive,
        Ok(flag) if flag.eq_ignore_ascii_case("s") => AttributeCase::Sensitive,
        Ok(_) => return fail(),
        Err(_) => AttributeCase::Default,
    };
    input.expect_exhausted()?;
    Ok(AttributeSelector {
        ns,
        name,
        lower,
        operation: Some(AttributeOperation {
            operator,
            value,
            case,
        }),
    })
}

/// Pseudo-elements that may be written with a single colon for compatibility with CSS 2.
fn legacy_pseudo_element(name: &str) -> Option<PseudoElement> {
    Some(match &*name.to_ascii_lowercase() {
        "before" => PseudoElement::Before,
        "after" => PseudoElement::After,
        "first-line" => PseudoElement::FirstLine,
        "first-letter" => PseudoElement::FirstLetter,
        _ => return None,
    })
}

fn parse_pseudo_element<'i>(input: &mut Parser<'i>) -> Result<PseudoElement> {
    match input.next_including_whitespace()?.clone() {
        Token::Ident(name) => {
            let name = name.to_ascii_lowercase();
            Ok(match &*name {
                "before" => PseudoElement::Before,
                "after" => PseudoElement::After,
                "marker" => PseudoElement::Marker,
                "first-letter" => PseudoElement::FirstLetter,
                "first-line" => PseudoElement::FirstLine,
                "placeholder" => PseudoElement::Placeholder,
                "file-selector-button" => PseudoElement::FileSelectorButton,
                "checkmark" => PseudoElement::Checkmark,
                "picker-icon" => PseudoElement::PickerIcon,
                "details-content" => PseudoElement::DetailsContent,
                _ => PseudoElement::Never(
                    never_pseudo_element(&name).ok_or_else(|| ParseError::custom(()))?,
                ),
            })
        }
        Token::Function(name) => {
            let name = name.to_ascii_lowercase();
            let never =
                never_functional_pseudo_element(&name).ok_or_else(|| ParseError::custom(()))?;
            input.parse_nested_block(|input| {
                while input.next().is_ok() {}
                Ok::<_, ParseError<()>>(())
            })?;
            Ok(PseudoElement::Never(never))
        }
        _ => fail(),
    }
}

fn never_pseudo_element(name: &str) -> Option<&'static str> {
    const NAMES: &[&str] = &[
        "selection",
        "target-text",
        "spelling-error",
        "grammar-error",
        "backdrop",
        "cue",
        "cue-region",
        "scroll-marker",
        "scroll-marker-group",
        "view-transition",
    ];
    NAMES.iter().copied().find(|&n| n == name)
}

fn never_functional_pseudo_element(name: &str) -> Option<&'static str> {
    const NAMES: &[&str] = &[
        "highlight",
        "cue",
        "cue-region",
        "part",
        "slotted",
        "picker",
        "view-transition-group",
        "view-transition-image-pair",
        "view-transition-new",
        "view-transition-old",
    ];
    NAMES.iter().copied().find(|&n| n == name)
}

/// Non-functional pseudo-classes, by lowercase name.
fn pseudo_class(name: &str) -> Option<PseudoClass> {
    const NEVER: &[&str] = &[
        "hover",
        "active",
        "focus",
        "focus-visible",
        "focus-within",
        "visited",
        "target",
        "target-within",
        "target-current",
        "user-valid",
        "user-invalid",
        "autofill",
        "fullscreen",
        "modal",
        "picture-in-picture",
        "popover-open",
        "playing",
        "seeking",
        "buffering",
        "stalled",
        "volume-locked",
        "current",
        "past",
        "future",
        "host",
        "has-slotted",
        "active-view-transition",
        "xr-overlay",
    ];
    Some(match name {
        "root" => PseudoClass::Root,
        "empty" => PseudoClass::Empty,
        "first-child" => PseudoClass::FirstChild,
        "last-child" => PseudoClass::LastChild,
        "only-child" => PseudoClass::OnlyChild,
        "first-of-type" => PseudoClass::FirstOfType,
        "last-of-type" => PseudoClass::LastOfType,
        "only-of-type" => PseudoClass::OnlyOfType,
        "scope" => PseudoClass::Scope,
        "defined" => PseudoClass::Defined,
        "heading" => PseudoClass::Heading(None),
        // Nothing is ever visited, so every link is unvisited.
        "any-link" | "link" => PseudoClass::AnyLink,
        "local-link" => PseudoClass::LocalLink,
        "enabled" => PseudoClass::Enabled,
        "disabled" => PseudoClass::Disabled,
        "read-only" => PseudoClass::ReadOnly,
        "read-write" => PseudoClass::ReadWrite,
        "placeholder-shown" => PseudoClass::PlaceholderShown,
        "default" => PseudoClass::Default,
        "checked" => PseudoClass::Checked,
        "indeterminate" => PseudoClass::Indeterminate,
        "valid" => PseudoClass::Valid,
        "invalid" => PseudoClass::Invalid,
        "in-range" => PseudoClass::InRange,
        "out-of-range" => PseudoClass::OutOfRange,
        "required" => PseudoClass::Required,
        "optional" => PseudoClass::Optional,
        "open" => PseudoClass::Open,
        "paused" => PseudoClass::Paused,
        "muted" => PseudoClass::Muted,
        _ => PseudoClass::Never(NEVER.iter().copied().find(|&n| n == name)?),
    })
}

fn functional_pseudo_class<'i>(
    input: &mut Parser<'i>,
    ctx: &Context,
    name: &str,
) -> Result<PseudoClass> {
    Ok(match name {
        "nth-child" | "nth-last-child" => {
            let (a, b) = parse_nth(input)?;
            let of = if input
                .try_parse(|input| input.expect_ident_matching("of"))
                .is_ok()
            {
                Some(SelectorList(input.parse_comma_separated(|input| {
                    parse_complex(input, ctx, false)
                })?))
            } else {
                None
            };
            if name == "nth-child" {
                PseudoClass::NthChild(Nth { a, b }, of)
            } else {
                PseudoClass::NthLastChild(Nth { a, b }, of)
            }
        }
        "nth-of-type" => {
            let (a, b) = parse_nth(input)?;
            PseudoClass::NthOfType(Nth { a, b })
        }
        "nth-last-of-type" => {
            let (a, b) = parse_nth(input)?;
            PseudoClass::NthLastOfType(Nth { a, b })
        }
        // `:is()` and `:where()` take forgiving selector lists: invalid selectors are dropped.
        "is" | "matches" => PseudoClass::Is(SelectorList(
            input.parse_comma_separated_ignoring_errors(|input| parse_complex(input, ctx, false)),
        )),
        "where" => PseudoClass::Where(SelectorList(
            input.parse_comma_separated_ignoring_errors(|input| parse_complex(input, ctx, false)),
        )),
        "not" => PseudoClass::Not(SelectorList(
            input.parse_comma_separated(|input| parse_complex(input, ctx, false))?,
        )),
        "has" => {
            let relative = input.parse_comma_separated(|input| {
                let combinator = parse_leading_combinator(input).unwrap_or(Combinator::Descendant);
                let selector = parse_complex(input, ctx, false)?;
                Ok(RelativeSelector {
                    combinator,
                    selector,
                })
            })?;
            // `:has()` cannot be nested.
            if relative.iter().any(|r| contains_has(&r.selector)) {
                return fail();
            }
            PseudoClass::Has(relative)
        }
        "lang" => PseudoClass::Lang(input.parse_comma_separated(|input| {
            Ok::<_, ParseError<()>>(input.expect_ident_or_string()?.to_string())
        })?),
        "dir" => {
            let dir = input.expect_ident_cloned()?;
            if dir.eq_ignore_ascii_case("ltr") {
                PseudoClass::Dir(Direction::Ltr)
            } else if dir.eq_ignore_ascii_case("rtl") {
                PseudoClass::Dir(Direction::Rtl)
            } else {
                return fail();
            }
        }
        "heading" => {
            PseudoClass::Heading(Some(input.parse_comma_separated(|input| {
                Ok::<_, ParseError<()>>(input.expect_integer()?)
            })?))
        }
        "host" | "host-context" | "current" | "state" | "active-view-transition-type" => {
            while input.next().is_ok() {}
            PseudoClass::Never(match name {
                "host" => "host",
                "host-context" => "host-context",
                "current" => "current",
                "state" => "state",
                _ => "active-view-transition-type",
            })
        }
        _ => return fail(),
    })
}

fn contains_has(selector: &Selector) -> bool {
    selector
        .compounds
        .iter()
        .flat_map(|compound| &compound.0)
        .any(|simple| matches!(simple, Simple::PseudoClass(PseudoClass::Has(_))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::Specificity;

    fn parse(text: &str) -> SelectorList {
        SelectorList::parse(text, &Namespaces::default())
            .unwrap_or_else(|_| panic!("{text:?} should parse"))
    }

    fn specificity(text: &str) -> Specificity {
        parse(text).max_specificity()
    }

    #[test]
    fn parses_valid_selectors() {
        for text in [
            "*",
            "div",
            "div.a#b[c]",
            "a > b + c ~ d e",
            "a>b",
            "[href^='https' i]",
            "[data-x|=en]",
            "ul li:nth-child(2n+1 of .x)",
            ":is(h1, h2):not(.a, .b)",
            "section:has(> img, + p)",
            "p::first-line",
            "p:before",
            "a::before:hover",
            ":lang(en, \"*-CH\")",
            ":dir(rtl)",
            ":heading(1, 2)",
            "col || td",
            "*|*",
            "|div",
            "::part(label)",
            ":where(.ok, !invalid)",
        ] {
            parse(text);
        }
    }

    #[test]
    fn rejects_invalid_selectors() {
        for text in [
            "",
            "a >",
            "> a",
            ".",
            "::before p",
            "p::before::after",
            ":unknown",
            ":not(.ok, !invalid)",
            ":has(:has(a))",
            "ns|div",
            "a::before.x",
        ] {
            assert!(
                SelectorList::parse(text, &Namespaces::default()).is_err(),
                "{text:?} should fail"
            );
        }
    }

    #[test]
    fn specificity_follows_selectors_4() {
        assert_eq!(specificity("*"), Specificity(0, 0, 0));
        assert_eq!(specificity("li"), Specificity(0, 0, 1));
        assert_eq!(specificity("ul li"), Specificity(0, 0, 2));
        assert_eq!(specificity("ul ol+li"), Specificity(0, 0, 3));
        assert_eq!(specificity("h1 + *[rel=up]"), Specificity(0, 1, 1));
        assert_eq!(specificity("ul ol li.red"), Specificity(0, 1, 3));
        assert_eq!(specificity("li.red.level"), Specificity(0, 2, 1));
        assert_eq!(specificity("#x34y"), Specificity(1, 0, 0));
        assert_eq!(specificity("#s12:not(FOO)"), Specificity(1, 0, 1));
        assert_eq!(specificity(":is(em, #foo)"), Specificity(1, 0, 0));
        assert_eq!(specificity(":where(em, #foo)"), Specificity(0, 0, 0));
        assert_eq!(specificity(".foo :is(.bar, #baz)"), Specificity(1, 1, 0));
        assert_eq!(
            specificity(":nth-child(2n of .a, #b)"),
            Specificity(1, 1, 0)
        );
        assert_eq!(specificity("p::before"), Specificity(0, 0, 2));
    }

    #[test]
    fn selector_list_uses_max_specificity_per_selector() {
        let list = parse("a, #b");
        assert_eq!(list.0[0].specificity(), Specificity(0, 0, 1));
        assert_eq!(list.0[1].specificity(), Specificity(1, 0, 0));
    }

    #[test]
    fn namespace_prefixes_resolve() {
        let mut namespaces = Namespaces::default();
        namespaces
            .prefixes
            .insert("svg".into(), Namespace::from("http://www.w3.org/2000/svg"));
        assert!(SelectorList::parse("svg|rect", &namespaces).is_ok());
        assert!(SelectorList::parse("[svg|href]", &namespaces).is_ok());
        assert!(SelectorList::parse("foo|rect", &namespaces).is_err());
    }

    #[test]
    fn nested_selectors_resolve_against_parent() {
        let parent = parse(".card");
        let nested = |text: &str| {
            let mut parser = Parser::new(text);
            parser
                .parse_entirely(|input| {
                    parse_selector_list(input, &Namespaces::default(), Some(&parent))
                })
                .unwrap()
        };
        // `.title` means `.card .title`.
        let implicit = nested(".title");
        assert_eq!(implicit.0[0].combinators, [Combinator::Descendant]);
        // `> p` means `.card > p`.
        assert_eq!(nested("> p").0[0].combinators, [Combinator::Child]);
        // `&:hover` stays one compound.
        assert_eq!(nested("&.active").0[0].compounds.len(), 1);
        // Specificity counts the parent through `:is()`.
        assert_eq!(nested("&.active").max_specificity(), Specificity(0, 2, 0));
    }
}
