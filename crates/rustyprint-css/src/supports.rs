//! `@supports` conditions.

use cssparser::{ParseError, Parser, Token};

use crate::selector::{Namespaces, SelectorList};

type Result<T> = std::result::Result<T, ParseError<()>>;

#[derive(Debug, Clone, PartialEq)]
pub enum SupportsCondition {
    Not(Box<SupportsCondition>),
    And(Vec<SupportsCondition>),
    Or(Vec<SupportsCondition>),
    /// `(property: value)`
    Declaration {
        name: String,
        value: String,
    },
    /// `selector(...)`, with the selector source.
    Selector(String),
    /// `font-tech(...)`
    FontTech(String),
    /// `font-format(...)`
    FontFormat(String),
    /// Syntax this engine does not recognise (`<general-enclosed>`); false.
    Unknown,
}

impl SupportsCondition {
    pub fn parse(input: &mut Parser<'_>) -> Result<Self> {
        if input
            .try_parse(|input| input.expect_ident_matching("not"))
            .is_ok()
        {
            return Ok(Self::Not(Box::new(parse_in_parens(input)?)));
        }
        let first = parse_in_parens(input)?;
        let operator = match input.try_parse(|input| input.expect_ident_cloned()) {
            Ok(op) if op.eq_ignore_ascii_case("and") => "and",
            Ok(op) if op.eq_ignore_ascii_case("or") => "or",
            Ok(_) => return Err(ParseError::custom(())),
            Err(_) => return Ok(first),
        };
        let mut conditions = vec![first, parse_in_parens(input)?];
        while input
            .try_parse(|input| input.expect_ident_matching(operator))
            .is_ok()
        {
            conditions.push(parse_in_parens(input)?);
        }
        Ok(if operator == "and" {
            Self::And(conditions)
        } else {
            Self::Or(conditions)
        })
    }

    /// Evaluates the condition. `declaration` reports whether a property accepts a value.
    pub fn evaluate(
        &self,
        namespaces: &Namespaces,
        declaration: &dyn Fn(&str, &str) -> bool,
    ) -> bool {
        match self {
            Self::Not(inner) => !inner.evaluate(namespaces, declaration),
            Self::And(all) => all.iter().all(|c| c.evaluate(namespaces, declaration)),
            Self::Or(any) => any.iter().any(|c| c.evaluate(namespaces, declaration)),
            Self::Declaration { name, value } => declaration(name, value),
            Self::Selector(source) => {
                SelectorList::parse(source, namespaces).is_ok_and(|list| list.0.len() == 1)
            }
            // Fonts are parsed with `ttf-parser`, which reads OpenType layout tables.
            Self::FontTech(tech) => tech == "features-opentype",
            Self::FontFormat(format) => {
                matches!(format.as_str(), "truetype" | "opentype" | "collection")
            }
            Self::Unknown => false,
        }
    }
}

fn rest(input: &mut Parser<'_>) -> String {
    let start = input.position();
    while input.next().is_ok() {}
    input.slice_from(start).trim().to_owned()
}

fn parse_in_parens(input: &mut Parser<'_>) -> Result<SupportsCondition> {
    match input.next()?.clone() {
        Token::ParenthesisBlock => input.parse_nested_block(|input| {
            if let Ok(condition) =
                input.try_parse(|input| input.parse_entirely(SupportsCondition::parse))
            {
                return Ok(condition);
            }
            let declaration = input.try_parse(|input| -> Result<SupportsCondition> {
                let name = input.expect_ident_cloned()?;
                input.expect_colon()?;
                let value = rest(input);
                let name = if name.starts_with("--") {
                    name.to_string()
                } else {
                    name.to_ascii_lowercase()
                };
                Ok(SupportsCondition::Declaration { name, value })
            });
            Ok(declaration.unwrap_or_else(|_| {
                rest(input);
                SupportsCondition::Unknown
            }))
        }),
        Token::Function(name) => {
            let name = name.to_ascii_lowercase();
            input.parse_nested_block(|input| {
                let argument = rest(input);
                Ok(match name.as_str() {
                    "selector" => SupportsCondition::Selector(argument),
                    "font-tech" => SupportsCondition::FontTech(argument.to_ascii_lowercase()),
                    "font-format" => SupportsCondition::FontFormat(argument.to_ascii_lowercase()),
                    _ => SupportsCondition::Unknown,
                })
            })
        }
        _ => Err(ParseError::custom(())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supports(text: &str) -> bool {
        let mut input = Parser::new(text);
        let condition = input.parse_entirely(SupportsCondition::parse).unwrap();
        condition.evaluate(&Namespaces::default(), &|name, value| {
            name == "color" && value == "red"
        })
    }

    #[test]
    fn declarations_and_logic() {
        assert!(supports("(color: red)"));
        assert!(!supports("(color: blue)"));
        assert!(supports("not (color: blue)"));
        assert!(supports("(color: red) and (not (color: blue))"));
        assert!(supports("(color: blue) or (color: red)"));
        assert!(!supports("(color: red) and (made-up: 1)"));
    }

    #[test]
    fn functions() {
        assert!(supports("selector(a > b)"));
        assert!(!supports("selector(a >)"));
        assert!(!supports("selector(a, b)"));
        assert!(supports("font-format(opentype)"));
        assert!(!supports("font-format(woff2)"));
        assert!(!supports("whatever(1)"));
        assert!(!supports("(not a declaration)"));
    }

    #[test]
    fn mixing_operators_is_invalid() {
        let mut input = Parser::new("(a: 1) and (b: 2) or (c: 3)");
        assert!(input.parse_entirely(SupportsCondition::parse).is_err());
    }
}
