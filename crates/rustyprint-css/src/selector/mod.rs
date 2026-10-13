//! Selectors: parsing, specificity and matching.
//!
//! The PDF is a static snapshot, so pseudo-classes that depend on interaction, playback or browser
//! UI parse but never match (see `css-selectors.md`).

mod matching;
mod parse;

use rustyprint_dom::{LocalName, Namespace};

pub use matching::MatchContext;
pub(crate) use parse::parse_selector_list;
pub use parse::{Namespaces, SelectorParseError};

/// A comma-separated list of complex selectors.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorList(pub Vec<Selector>);

/// A complex selector: compound selectors joined by combinators.
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    /// Compounds in source order (left to right).
    pub compounds: Vec<Compound>,
    /// `combinators[i]` joins `compounds[i]` and `compounds[i + 1]`.
    pub combinators: Vec<Combinator>,
    /// The pseudo-element the selector targets, from the last compound.
    pub pseudo_element: Option<PseudoElement>,
}

/// A selector inside `:has()`, relative to the element being matched.
#[derive(Debug, Clone, PartialEq)]
pub struct RelativeSelector {
    pub combinator: Combinator,
    pub selector: Selector,
}

/// Simple selectors that must all match the same element.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compound(pub Vec<Simple>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// Whitespace.
    Descendant,
    /// `>`
    Child,
    /// `+`
    NextSibling,
    /// `~`
    SubsequentSibling,
    /// `||`: matches a table cell belonging to the column on the left.
    Column,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Simple {
    /// `*`, `ns|*`, `*|*` or `|*`.
    Universal(NamespaceConstraint),
    /// `div`, `ns|div`. Holds the name as written and lowercased: HTML elements match
    /// case-insensitively, other elements match exactly.
    Type {
        ns: NamespaceConstraint,
        name: LocalName,
        lower: LocalName,
    },
    Id(String),
    Class(String),
    Attribute(AttributeSelector),
    PseudoClass(PseudoClass),
}

/// Which namespaces a type, universal or attribute selector accepts.
#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceConstraint {
    Any,
    /// Elements or attributes with no namespace.
    None,
    Specific(Namespace),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSelector {
    pub ns: NamespaceConstraint,
    /// The name as written and lowercased: HTML elements match the lowercased name.
    pub name: LocalName,
    pub lower: LocalName,
    pub operation: Option<AttributeOperation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeOperation {
    pub operator: AttributeOperator,
    pub value: String,
    pub case: AttributeCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeOperator {
    /// `=`
    Equals,
    /// `~=`
    Includes,
    /// `|=`
    DashMatch,
    /// `^=`
    Prefix,
    /// `$=`
    Suffix,
    /// `*=`
    Substring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeCase {
    /// No flag: case-sensitive, except for the HTML attributes the HTML spec lists.
    Default,
    /// `i`
    Insensitive,
    /// `s`
    Sensitive,
}

/// `An+B`, as used by the `:nth-*()` pseudo-classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nth {
    pub a: i32,
    pub b: i32,
}

impl Nth {
    /// Whether the 1-based `index` is `An+B` for some n ≥ 0.
    pub fn matches(self, index: i32) -> bool {
        let Nth { a, b } = self;
        if a == 0 {
            return index == b;
        }
        let diff = index - b;
        diff % a == 0 && diff / a >= 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PseudoClass {
    // Tree-structural
    Root,
    Empty,
    FirstChild,
    LastChild,
    OnlyChild,
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthChild(Nth, Option<SelectorList>),
    NthLastChild(Nth, Option<SelectorList>),
    NthOfType(Nth),
    NthLastOfType(Nth),
    // Logical
    Is(SelectorList),
    Where(SelectorList),
    Not(SelectorList),
    Has(Vec<RelativeSelector>),
    // Element
    Scope,
    Defined,
    /// `:heading`, or `:heading(...)` with the levels listed.
    Heading(Option<Vec<i32>>),
    // Linguistic
    Lang(Vec<String>),
    Dir(Direction),
    // Location
    AnyLink,
    LocalLink,
    // Input
    Enabled,
    Disabled,
    ReadOnly,
    ReadWrite,
    PlaceholderShown,
    Checked,
    Required,
    Optional,
    /// Matching not implemented yet; never matches.
    Default,
    /// Matching not implemented yet; never matches.
    Indeterminate,
    /// Matching not implemented yet; never matches.
    Valid,
    /// Matching not implemented yet; never matches.
    Invalid,
    /// Matching not implemented yet; never matches.
    InRange,
    /// Matching not implemented yet; never matches.
    OutOfRange,
    // Display and resource state
    Open,
    Paused,
    Muted,
    /// A pseudo-class that never matches in a static document, such as `:hover`.
    Never(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PseudoElement {
    Before,
    After,
    Marker,
    FirstLetter,
    FirstLine,
    Placeholder,
    FileSelectorButton,
    Checkmark,
    PickerIcon,
    DetailsContent,
    /// A pseudo-element that is never generated in a static document, such as `::selection`.
    Never(&'static str),
}

/// Selector specificity `(ids, classes, types)`, compared lexicographically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Specificity(pub u32, pub u32, pub u32);

impl std::ops::Add for Specificity {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0, self.1 + other.1, self.2 + other.2)
    }
}

impl SelectorList {
    /// The largest specificity in the list, as `:is()` and `:not()` use.
    pub fn max_specificity(&self) -> Specificity {
        self.0
            .iter()
            .map(Selector::specificity)
            .max()
            .unwrap_or_default()
    }
}

impl Selector {
    pub fn specificity(&self) -> Specificity {
        let compounds = self
            .compounds
            .iter()
            .map(Compound::specificity)
            .fold(Specificity::default(), |a, b| a + b);
        let pseudo = if self.pseudo_element.is_some() {
            Specificity(0, 0, 1)
        } else {
            Specificity::default()
        };
        compounds + pseudo
    }
}

impl Compound {
    fn specificity(&self) -> Specificity {
        self.0
            .iter()
            .map(Simple::specificity)
            .fold(Specificity::default(), |a, b| a + b)
    }
}

impl Simple {
    fn specificity(&self) -> Specificity {
        match self {
            Simple::Universal(_) => Specificity::default(),
            Simple::Type { .. } => Specificity(0, 0, 1),
            Simple::Id(_) => Specificity(1, 0, 0),
            Simple::Class(_) | Simple::Attribute(_) => Specificity(0, 1, 0),
            Simple::PseudoClass(pseudo) => match pseudo {
                PseudoClass::Where(_) => Specificity::default(),
                PseudoClass::Is(list) | PseudoClass::Not(list) => list.max_specificity(),
                PseudoClass::Has(relative) => relative
                    .iter()
                    .map(|r| r.selector.specificity())
                    .max()
                    .unwrap_or_default(),
                PseudoClass::NthChild(_, Some(list)) | PseudoClass::NthLastChild(_, Some(list)) => {
                    Specificity(0, 1, 0) + list.max_specificity()
                }
                _ => Specificity(0, 1, 0),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nth_matching() {
        let odd = Nth { a: 2, b: 1 };
        assert!(odd.matches(1) && odd.matches(3) && !odd.matches(2));
        let first_three = Nth { a: -1, b: 3 };
        assert!(first_three.matches(1) && first_three.matches(3) && !first_three.matches(4));
        let fifth = Nth { a: 0, b: 5 };
        assert!(fifth.matches(5) && !fifth.matches(10));
        let from_four = Nth { a: 1, b: 4 };
        assert!(!from_four.matches(3) && from_four.matches(4) && from_four.matches(9));
    }
}
