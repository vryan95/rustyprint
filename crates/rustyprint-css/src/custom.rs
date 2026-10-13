//! Custom properties and arbitrary substitution: `var()`, `env()`, `attr()`, `if()` and
//! custom functions defined by `@function`.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cssparser::{ParseError, Parser, Token, serialize_string};

use crate::syntax::Syntax;

/// Computed custom properties of an element: names to substituted values.
pub type CustomProperties = Arc<HashMap<String, Arc<str>>>;

/// A reference that substitution resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference<'a> {
    Var(&'a str),
    Env(&'a str),
}

/// A parameter of a custom function.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    /// The type arguments must match; `None` accepts anything.
    pub syntax: Option<Syntax>,
    pub default: Option<String>,
}

/// An item of a custom function's body.
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionItem {
    /// A local custom property.
    Local(String, String),
    /// The `result` descriptor.
    Result(String),
    /// Items that apply when a media condition holds.
    Media(String, Vec<FunctionItem>),
    /// Items that apply when a supports condition holds.
    Supports(String, Vec<FunctionItem>),
    /// Items that apply when a container query holds, which `@container` tests against the
    /// element's containers.
    Container(String, Vec<FunctionItem>),
}

/// A custom function defined by `@function` (CSS Mixins 1 § 2).
#[derive(Debug, Clone, PartialEq)]
pub struct CustomFunction {
    pub parameters: Vec<Parameter>,
    /// The type the result must match; `None` accepts anything.
    pub returns: Option<Syntax>,
    pub body: Vec<FunctionItem>,
}

/// Custom functions by name, such as `--double`.
pub type Functions = HashMap<String, Arc<CustomFunction>>;

/// How deeply custom functions may call each other.
const MAX_CALL_DEPTH: u32 = 32;

/// What substitution needs besides custom properties: the element's attributes, custom
/// functions, and how to test the conditions of `if()` and of function bodies.
pub struct Environment<'a> {
    /// Attribute values of the element being styled, for `attr()`.
    pub attr: &'a dyn Fn(&str) -> Option<String>,
    pub functions: &'a Functions,
    /// Whether a media query list holds, such as the prelude of `@media`.
    pub media: &'a dyn Fn(&str) -> bool,
    /// Whether a supports condition holds, such as the prelude of `@supports`.
    pub supports: &'a dyn Fn(&str) -> bool,
    /// Whether a container query holds for the element.
    pub container: &'a dyn Fn(&str) -> bool,
    depth: Cell<u32>,
}

impl<'a> Environment<'a> {
    pub fn new(
        attr: &'a dyn Fn(&str) -> Option<String>,
        functions: &'a Functions,
        media: &'a dyn Fn(&str) -> bool,
        supports: &'a dyn Fn(&str) -> bool,
        container: &'a dyn Fn(&str) -> bool,
    ) -> Self {
        Self {
            attr,
            functions,
            media,
            supports,
            container,
            depth: Cell::new(0),
        }
    }
}

/// What substitution looks references up in.
pub struct Substitution<'a> {
    pub custom: &'a HashMap<String, Arc<str>>,
    pub env: &'a Environment<'a>,
}

/// Whether `text` contains `var()`, `env()`, `attr()`, `if()` or a custom function.
pub fn has_references(text: &str) -> bool {
    let mut input = Parser::new(text);
    has_references_in(&mut input)
}

fn has_references_in(input: &mut Parser<'_>) -> bool {
    while let Ok(token) = input.next_including_whitespace_and_comments() {
        match token.clone() {
            Token::Function(name) if is_substitution_function(&name) => {
                // Nested blocks must be consumed entirely.
                while input.next_including_whitespace_and_comments().is_ok() {}
                return true;
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock
                if input
                    .parse_nested_block(|input| Ok::<_, ParseError<()>>(has_references_in(input)))
                    .unwrap_or(false) =>
            {
                while input.next_including_whitespace_and_comments().is_ok() {}
                return true;
            }
            _ => {}
        }
    }
    false
}

fn is_substitution_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("var")
        || name.eq_ignore_ascii_case("env")
        || name.eq_ignore_ascii_case("attr")
        || name.eq_ignore_ascii_case("if")
        || name.starts_with("--")
}

/// Values of environment variables for paged output: no safe-area or keyboard insets.
fn environment(name: &str) -> Option<&'static str> {
    match name {
        "safe-area-inset-top"
        | "safe-area-inset-right"
        | "safe-area-inset-bottom"
        | "safe-area-inset-left"
        | "safe-area-max-inset-top"
        | "safe-area-max-inset-right"
        | "safe-area-max-inset-bottom"
        | "safe-area-max-inset-left"
        | "keyboard-inset-top"
        | "keyboard-inset-right"
        | "keyboard-inset-bottom"
        | "keyboard-inset-left"
        | "keyboard-inset-width"
        | "keyboard-inset-height" => Some("0px"),
        "preferred-text-scale" => Some("1"),
        _ => None,
    }
}

/// Replaces every `var()`, `env()`, `attr()`, `if()` and custom function call in `text`.
/// Returns `None` when a reference cannot be resolved and has no fallback, which makes the
/// declaration invalid at computed-value time.
pub fn substitute(text: &str, ctx: &Substitution) -> Option<String> {
    let mut input = Parser::new(text);
    let mut out = String::with_capacity(text.len());
    substitute_into(&mut input, &mut out, ctx).ok()?;
    Some(out)
}

fn substitute_into<'i>(
    input: &mut Parser<'i>,
    out: &mut String,
    ctx: &Substitution,
) -> Result<(), ParseError<()>> {
    let mut copied = input.position();
    loop {
        let start = input.position();
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        let closing = match &token {
            Token::Function(name) if is_substitution_function(name) => {
                out.push_str(input.slice(copied..start));
                let name = if name.starts_with("--") {
                    name.to_string()
                } else {
                    name.to_ascii_lowercase()
                };
                let replacement =
                    input.parse_nested_block(|input| resolve_function(input, &name, ctx))?;
                // Empty comments keep the replacement's tokens from merging with neighbours.
                out.push_str("/**/");
                out.push_str(&replacement);
                out.push_str("/**/");
                copied = input.position();
                continue;
            }
            Token::Function(_) | Token::ParenthesisBlock => ')',
            Token::SquareBracketBlock => ']',
            Token::CurlyBracketBlock => '}',
            _ => continue,
        };
        out.push_str(input.slice(copied..input.position()));
        input.parse_nested_block(|input| substitute_into(input, out, ctx))?;
        out.push(closing);
        copied = input.position();
    }
    out.push_str(input.slice_from(copied));
    Ok(())
}

/// Resolves the arguments of a substitution function into replacement text.
fn resolve_function<'i>(
    input: &mut Parser<'i>,
    name: &str,
    ctx: &Substitution,
) -> Result<String, ParseError<()>> {
    let fail = || ParseError::custom(());
    let value = match name {
        "var" => {
            let variable = input.expect_ident_cloned()?;
            if !variable.starts_with("--") {
                return Err(fail());
            }
            ctx.custom.get(&*variable).map(|v| v.to_string())
        }
        "env" => {
            let variable = input.expect_ident_cloned()?;
            // Indices select segments of multi-segment variables; none exist here.
            while input.try_parse(|input| input.expect_integer()).is_ok() {}
            environment(&variable).map(str::to_owned)
        }
        "if" => return resolve_if(input, ctx),
        function if function.starts_with("--") => return call(input, function, ctx),
        _ => {
            let attribute = input.expect_ident_cloned()?.to_string();
            let kind = input
                .try_parse(|input| -> Result<String, ParseError<()>> {
                    match input.next()?.clone() {
                        Token::Ident(ident) => Ok(ident.to_ascii_lowercase()),
                        Token::Delim('%') => Ok("%".to_owned()),
                        Token::Function(f) if f.eq_ignore_ascii_case("type") => {
                            input.parse_nested_block(|input| {
                                while input.next().is_ok() {}
                                Ok::<_, ParseError<()>>(())
                            })?;
                            Ok("type".to_owned())
                        }
                        _ => Err(fail()),
                    }
                })
                .ok();
            (ctx.env.attr)(&attribute).and_then(|raw| match kind.as_deref() {
                None | Some("string") => {
                    let mut quoted = String::new();
                    serialize_string(&raw, &mut quoted).ok()?;
                    Some(quoted)
                }
                Some("raw-string" | "type") => Some(raw),
                // The legacy `url` type, which print stylesheets use for `target-counter()`.
                Some("url") => {
                    let mut url = String::from("url(");
                    serialize_string(&raw, &mut url).ok()?;
                    url.push(')');
                    Some(url)
                }
                Some(unit) => {
                    let number: f64 = raw.trim().parse().ok()?;
                    Some(format!("{number}{unit}"))
                }
            })
        }
    };
    if let Some(value) = value {
        // Skip the unused fallback.
        while input.next().is_ok() {}
        return Ok(value);
    }
    if input.try_parse(|input| input.expect_comma()).is_ok() {
        let mut fallback = String::new();
        substitute_into(input, &mut fallback, ctx)?;
        return Ok(fallback.trim().to_owned());
    }
    Err(fail())
}

// ---------------------------------------------------------------------------------------------
// `if()`

/// `if( [ <if-condition> : <declaration-value>? ; ]* )`: the value of the first branch whose
/// condition holds, or nothing (CSS Values 5 § 9).
fn resolve_if(input: &mut Parser<'_>, ctx: &Substitution) -> Result<String, ParseError<()>> {
    let mut chosen: Option<String> = None;
    while !input.is_exhausted() {
        let branch = input.parse_until_after(cssparser::Delimiter::Semicolon, |input| {
            let start = input.position();
            let mut before = input.position();
            loop {
                match input.next()?.clone() {
                    Token::Colon => break,
                    token => {
                        skip_block(input, &token);
                        before = input.position();
                    }
                }
            }
            let condition = input.slice(start..before).trim().to_owned();
            let value_start = input.position();
            while input.next().is_ok() {}
            let value = input.slice_from(value_start).trim().to_owned();
            Ok::<_, ParseError<()>>((condition, value))
        })?;
        if chosen.is_none() && condition_holds(&branch.0, ctx)? {
            chosen = Some(branch.1);
        }
    }
    match chosen {
        Some(value) => substitute(&value, ctx).ok_or_else(|| ParseError::custom(())),
        None => Ok(String::new()),
    }
}

/// Consumes the contents of the block `token` opens, if it opens one, so the position is
/// after its end.
pub(crate) fn skip_block(input: &mut Parser<'_>, token: &Token<'_>) {
    if matches!(
        token,
        Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock
    ) {
        let _ = input.parse_nested_block(|input| {
            while input.next().is_ok() {}
            Ok::<_, ParseError<()>>(())
        });
    }
}

/// Evaluates an `<if-condition>`: `else`, or a boolean combination of `media()`,
/// `supports()` and `style()` tests.
fn condition_holds(text: &str, ctx: &Substitution) -> Result<bool, ParseError<()>> {
    let mut input = Parser::new(text);
    input.parse_entirely(|input| {
        if input
            .try_parse(|input| input.expect_ident_matching("else"))
            .is_ok()
        {
            return Ok(true);
        }
        boolean(input, &|input| test(input, ctx))
    })
}

/// Parses a boolean expression of `test`s: `not`, `and` and `or` over parenthesized
/// expressions or tests, with one kind of operator per level.
pub(crate) fn boolean(
    input: &mut Parser<'_>,
    test: &dyn Fn(&mut Parser<'_>) -> Result<bool, ParseError<()>>,
) -> Result<bool, ParseError<()>> {
    let in_parens = |input: &mut Parser<'_>| -> Result<bool, ParseError<()>> {
        if input
            .try_parse(|input| input.expect_parenthesis_block())
            .is_ok()
        {
            return input.parse_nested_block(|input| boolean(input, test));
        }
        test(input)
    };
    if input
        .try_parse(|input| input.expect_ident_matching("not"))
        .is_ok()
    {
        return Ok(!in_parens(input)?);
    }
    let mut value = in_parens(input)?;
    let operator = match input.try_parse(|input| input.expect_ident_cloned()) {
        Ok(op) if op.eq_ignore_ascii_case("and") => "and",
        Ok(op) if op.eq_ignore_ascii_case("or") => "or",
        Ok(_) => return Err(ParseError::custom(())),
        Err(_) => return Ok(value),
    };
    loop {
        let next = in_parens(input)?;
        value = if operator == "and" {
            value && next
        } else {
            value || next
        };
        if input
            .try_parse(|input| input.expect_ident_matching(operator))
            .is_err()
        {
            return Ok(value);
        }
    }
}

/// One `<if-test>`: `media()`, `supports()` or `style()`.
fn test(input: &mut Parser<'_>, ctx: &Substitution) -> Result<bool, ParseError<()>> {
    let name = input.expect_function()?.to_ascii_lowercase();
    input.parse_nested_block(|input| {
        let start = input.position();
        match &*name {
            "media" => {
                while input.next().is_ok() {}
                Ok((ctx.env.media)(&format!(
                    "({})",
                    input.slice_from(start).trim()
                )))
            }
            "supports" => {
                while input.next().is_ok() {}
                Ok((ctx.env.supports)(&format!(
                    "({})",
                    input.slice_from(start).trim()
                )))
            }
            "style" => boolean(input, &|input| style_feature(input, ctx.custom, ctx)),
            _ => Err(ParseError::custom(())),
        }
    })
}

/// A style feature against custom properties: `--name` (it has a value) or
/// `--name: value` (it has this value).
pub(crate) fn style_feature(
    input: &mut Parser<'_>,
    custom: &HashMap<String, Arc<str>>,
    ctx: &Substitution,
) -> Result<bool, ParseError<()>> {
    let name = input.expect_ident_cloned()?;
    if !name.starts_with("--") {
        return Err(ParseError::custom(()));
    }
    let actual = custom.get(&*name).map(|v| normalize(v));
    if input.is_exhausted() {
        return Ok(actual.is_some_and(|v| !v.is_empty()));
    }
    input.expect_colon()?;
    let start = input.position();
    while input.next().is_ok() {}
    let wanted = input.slice_from(start);
    let wanted = substitute(wanted, ctx).map(|v| normalize(&v));
    Ok(actual.is_some() && actual == wanted)
}

/// Value text compared by tokens: whitespace runs as one space, without substitution marks.
pub(crate) fn normalize(text: &str) -> String {
    text.replace("/**/", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------------------------
// Custom functions

/// Calls the custom function `name` with the arguments in `input`.
fn call(input: &mut Parser<'_>, name: &str, ctx: &Substitution) -> Result<String, ParseError<()>> {
    let fail = || ParseError::custom(());
    let function = ctx.env.functions.get(name).ok_or_else(fail)?;
    let depth = ctx.env.depth.get();
    if depth >= MAX_CALL_DEPTH {
        return Err(fail());
    }
    // Arguments are separated by commas; one wrapped in `{}` may contain commas.
    let mut arguments: Vec<Option<String>> = Vec::new();
    if !input.is_exhausted() {
        arguments = input.parse_comma_separated(|input| {
            let start = input.position();
            if let Ok(inner) = input.try_parse(|input| {
                input.expect_curly_bracket_block()?;
                let inner = input.parse_nested_block(|input| {
                    let start = input.position();
                    while input.next().is_ok() {}
                    Ok::<_, ParseError<()>>(input.slice_from(start).to_owned())
                })?;
                input.expect_exhausted()?;
                Ok::<_, ParseError<()>>(inner)
            }) {
                return Ok(substitute(&inner, ctx).map(|v| v.trim().to_owned()));
            }
            while input.next().is_ok() {}
            let text = input.slice_from(start);
            Ok::<_, ParseError<()>>(substitute(text, ctx).map(|v| v.trim().to_owned()))
        })?;
    }
    if arguments.len() > function.parameters.len() {
        return Err(fail());
    }
    ctx.env.depth.set(depth + 1);
    let result = evaluate(function, arguments, ctx);
    ctx.env.depth.set(depth);
    result.ok_or_else(fail)
}

/// Evaluates a function body with its arguments bound, returning its result.
fn evaluate(
    function: &CustomFunction,
    arguments: Vec<Option<String>>,
    ctx: &Substitution,
) -> Option<String> {
    // Function bodies see the caller's custom properties, shadowed by parameters and locals.
    let mut scope: HashMap<String, Arc<str>> = ctx.custom.clone();
    let mut arguments = arguments.into_iter();
    for parameter in &function.parameters {
        let value = match arguments.next() {
            Some(argument) => argument,
            None => parameter.default.as_ref().and_then(|default| {
                let inner = Substitution {
                    custom: &scope,
                    env: ctx.env,
                };
                substitute(default, &inner).map(|v| v.trim().to_owned())
            }),
        };
        let value = value.filter(|value| {
            parameter
                .syntax
                .as_ref()
                .is_none_or(|syntax| syntax.matches(value))
        });
        match value {
            Some(value) => {
                scope.insert(parameter.name.clone(), Arc::from(value));
            }
            None => {
                scope.remove(&parameter.name);
            }
        }
    }
    let mut result = None;
    run(&function.body, &mut scope, &mut result, ctx.env);
    let inner = Substitution {
        custom: &scope,
        env: ctx.env,
    };
    let value = substitute(&result?, &inner)?.trim().to_owned();
    function
        .returns
        .as_ref()
        .is_none_or(|syntax| syntax.matches(&value))
        .then_some(value)
}

/// Runs body items in order: locals are set as they come, the last `result` wins, and
/// conditional groups run when their condition holds.
fn run(
    items: &[FunctionItem],
    scope: &mut HashMap<String, Arc<str>>,
    result: &mut Option<String>,
    env: &Environment,
) {
    for item in items {
        match item {
            FunctionItem::Local(name, value) => {
                let substituted = {
                    let inner = Substitution { custom: scope, env };
                    substitute(value, &inner)
                };
                match substituted {
                    Some(value) => {
                        scope.insert(name.clone(), Arc::from(value.trim()));
                    }
                    None => {
                        scope.remove(name);
                    }
                }
            }
            FunctionItem::Result(value) => *result = Some(value.clone()),
            FunctionItem::Media(condition, items) => {
                if (env.media)(condition) {
                    run(items, scope, result, env);
                }
            }
            FunctionItem::Supports(condition, items) => {
                if (env.supports)(condition) {
                    run(items, scope, result, env);
                }
            }
            FunctionItem::Container(condition, items) => {
                if (env.container)(condition) {
                    run(items, scope, result, env);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Computing custom properties

/// Computes an element's custom properties from the inherited ones and its declarations,
/// resolving references between them. Values in a reference cycle, or that reference a
/// missing property without a fallback, become invalid and are removed.
pub fn compute_custom_properties(
    inherited: &CustomProperties,
    declared: &[(String, Option<String>)],
    env: &Environment,
) -> CustomProperties {
    if declared.is_empty() {
        return inherited.clone();
    }
    let mut raw: HashMap<String, Arc<str>> = (**inherited).clone();
    let mut pending = Vec::new();
    for (name, value) in declared {
        match value {
            // `None` is the guaranteed-invalid value (from `initial`).
            None => {
                raw.remove(name);
            }
            Some(value) => {
                raw.insert(name.clone(), Arc::from(value.as_str()));
                if has_references(value) {
                    pending.push(name.clone());
                }
            }
        }
    }
    let mut resolved = raw.clone();
    let mut state = Resolver {
        raw: &raw,
        resolved: &mut resolved,
        visiting: HashSet::new(),
        done: HashSet::new(),
        env,
    };
    for name in pending {
        state.resolve(&name);
    }
    Arc::new(resolved)
}

struct Resolver<'a> {
    raw: &'a HashMap<String, Arc<str>>,
    resolved: &'a mut HashMap<String, Arc<str>>,
    visiting: HashSet<String>,
    done: HashSet<String>,
    env: &'a Environment<'a>,
}

impl Resolver<'_> {
    /// Resolves `name`; returns false if it is invalid (missing, cyclic or unresolvable).
    fn resolve(&mut self, name: &str) -> bool {
        if self.done.contains(name) {
            return self.resolved.contains_key(name);
        }
        if self.visiting.contains(name) {
            return false;
        }
        let Some(raw) = self.raw.get(name).cloned() else {
            return false;
        };
        if !has_references(&raw) {
            self.done.insert(name.to_owned());
            return true;
        }
        self.visiting.insert(name.to_owned());
        // Resolve dependencies first; cycles leave them unresolved and invalid.
        for dependency in references(&raw, self.env.functions) {
            if !self.resolve(&dependency) {
                self.resolved.remove(&dependency);
            }
        }
        let in_cycle = !self.visiting.remove(name);
        let ctx = Substitution {
            custom: self.resolved,
            env: self.env,
        };
        match substitute(&raw, &ctx).filter(|_| !in_cycle) {
            Some(value) => {
                self.resolved
                    .insert(name.to_owned(), Arc::from(value.as_str()));
            }
            None => {
                self.resolved.remove(name);
            }
        }
        self.done.insert(name.to_owned());
        self.resolved.contains_key(name)
    }
}

/// Custom property names that `text` depends on: those `var()` references, those `style()`
/// tests in `if()` read, and those the bodies of the custom functions it calls reference
/// beyond their own parameters and locals.
fn references(text: &str, functions: &Functions) -> Vec<String> {
    fn collect(
        input: &mut Parser<'_>,
        names: &mut Vec<String>,
        functions: &Functions,
        seen: &mut Vec<String>,
    ) {
        while let Ok(token) = input.next_including_whitespace_and_comments() {
            match token.clone() {
                Token::Function(name) if name.eq_ignore_ascii_case("var") => {
                    let _ = input.parse_nested_block(|input| {
                        if let Ok(variable) = input.expect_ident_cloned() {
                            names.push(variable.to_string());
                        }
                        collect(input, names, functions, seen);
                        Ok::<_, ParseError<()>>(())
                    });
                }
                Token::Function(name) if name.eq_ignore_ascii_case("style") => {
                    let _ = input.parse_nested_block(|input| {
                        while let Ok(token) = input.next_including_whitespace_and_comments() {
                            match token.clone() {
                                Token::Ident(ident) if ident.starts_with("--") => {
                                    names.push(ident.to_string())
                                }
                                Token::Function(_) | Token::ParenthesisBlock => {
                                    let _ = input.parse_nested_block(|input| {
                                        collect_style(input, names);
                                        Ok::<_, ParseError<()>>(())
                                    });
                                }
                                _ => {}
                            }
                        }
                        Ok::<_, ParseError<()>>(())
                    });
                }
                Token::Function(name) if name.starts_with("--") => {
                    if let Some(function) = functions.get(&*name)
                        && !seen.iter().any(|s| *s == *name)
                    {
                        seen.push(name.to_string());
                        let own: Vec<&str> = function
                            .parameters
                            .iter()
                            .map(|p| p.name.as_str())
                            .chain(locals(&function.body))
                            .collect();
                        let mut body = Vec::new();
                        for text in texts(&function.body).chain(
                            function
                                .parameters
                                .iter()
                                .filter_map(|p| p.default.as_deref()),
                        ) {
                            collect(&mut Parser::new(text), &mut body, functions, seen);
                        }
                        names.extend(body.into_iter().filter(|n| !own.contains(&n.as_str())));
                        seen.pop();
                    }
                    let _ = input.parse_nested_block(|input| {
                        collect(input, names, functions, seen);
                        Ok::<_, ParseError<()>>(())
                    });
                }
                Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock => {
                    let _ = input.parse_nested_block(|input| {
                        collect(input, names, functions, seen);
                        Ok::<_, ParseError<()>>(())
                    });
                }
                _ => {}
            }
        }
    }
    fn collect_style(input: &mut Parser<'_>, names: &mut Vec<String>) {
        while let Ok(token) = input.next_including_whitespace_and_comments() {
            match token.clone() {
                Token::Ident(ident) if ident.starts_with("--") => names.push(ident.to_string()),
                Token::ParenthesisBlock => {
                    let _ = input.parse_nested_block(|input| {
                        collect_style(input, names);
                        Ok::<_, ParseError<()>>(())
                    });
                }
                _ => {}
            }
        }
    }
    let mut names = Vec::new();
    collect(
        &mut Parser::new(text),
        &mut names,
        functions,
        &mut Vec::new(),
    );
    names
}

/// The names of a function body's locals.
fn locals(items: &[FunctionItem]) -> impl Iterator<Item = &str> {
    items
        .iter()
        .flat_map(|item| -> Box<dyn Iterator<Item = &str>> {
            match item {
                FunctionItem::Local(name, _) => Box::new(std::iter::once(name.as_str())),
                FunctionItem::Result(_) => Box::new(std::iter::empty()),
                FunctionItem::Media(_, items)
                | FunctionItem::Supports(_, items)
                | FunctionItem::Container(_, items) => Box::new(locals(items)),
            }
        })
}

/// The value texts of a function body's locals and results.
fn texts(items: &[FunctionItem]) -> impl Iterator<Item = &str> {
    items
        .iter()
        .flat_map(|item| -> Box<dyn Iterator<Item = &str>> {
            match item {
                FunctionItem::Local(_, value) | FunctionItem::Result(value) => {
                    Box::new(std::iter::once(value.as_str()))
                }
                FunctionItem::Media(_, items)
                | FunctionItem::Supports(_, items)
                | FunctionItem::Container(_, items) => Box::new(texts(items)),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_attr(_: &str) -> Option<String> {
        None
    }

    fn never(_: &str) -> bool {
        false
    }

    fn wide(text: &str) -> bool {
        text.contains("min-width")
    }

    fn sub_with(text: &str, vars: &[(&str, &str)], functions: &Functions) -> Option<String> {
        let custom: HashMap<String, Arc<str>> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), Arc::from(*v)))
            .collect();
        let attr = |name: &str| (name == "data-size").then(|| "12".to_owned());
        let supports = |text: &str| text.contains("display");
        let env = Environment::new(&attr, functions, &wide, &supports, &never);
        substitute(
            text,
            &Substitution {
                custom: &custom,
                env: &env,
            },
        )
        .map(|s| normalize(&s))
    }

    fn sub(text: &str, vars: &[(&str, &str)]) -> Option<String> {
        sub_with(text, vars, &Functions::new())
    }

    #[test]
    fn substitutes_variables() {
        assert_eq!(sub("var(--a)", &[("--a", "red")]).unwrap(), "red");
        assert_eq!(
            sub("1px solid var(--c)", &[("--c", "blue")]).unwrap(),
            "1px solid blue"
        );
        assert_eq!(
            sub("calc(var(--x) * 2)", &[("--x", "3px")]).unwrap(),
            "calc(3px * 2)"
        );
        assert_eq!(sub("var(--missing, 4px)", &[]).unwrap(), "4px");
        assert_eq!(
            sub("var(--missing, var(--b))", &[("--b", "x")]).unwrap(),
            "x"
        );
        assert_eq!(sub("var(--missing,)", &[]).unwrap(), "");
        assert!(sub("var(--missing)", &[]).is_none());
        assert!(sub("var(not-custom)", &[]).is_none());
    }

    #[test]
    fn substitution_keeps_tokens_apart() {
        let custom = HashMap::from([("--n".to_owned(), Arc::from("10"))]);
        let functions = Functions::new();
        let env = Environment::new(&no_attr, &functions, &never, &never, &never);
        let out = substitute(
            "var(--n)px",
            &Substitution {
                custom: &custom,
                env: &env,
            },
        )
        .unwrap();
        assert_eq!(out, "/**/10/**/px");
    }

    #[test]
    fn env_and_attr() {
        assert_eq!(sub("env(safe-area-inset-top)", &[]).unwrap(), "0px");
        assert_eq!(sub("env(unknown, 3px)", &[]).unwrap(), "3px");
        assert_eq!(sub("attr(data-size px)", &[]).unwrap(), "12px");
        assert_eq!(sub("attr(data-size)", &[]).unwrap(), "\"12\"");
        assert_eq!(sub("attr(data-size url)", &[]).unwrap(), "url(\"12\")");
        assert_eq!(sub("attr(title, 'none')", &[]).unwrap(), "'none'");
    }

    #[test]
    fn conditionals() {
        let vars = [("--mode", "dark"), ("--size", " 2 ")];
        let s = |text: &str| sub(text, &vars).unwrap();
        assert_eq!(s("if(style(--mode: dark): black; else: white)"), "black");
        assert_eq!(s("if(style(--mode: light): black; else: white)"), "white");
        assert_eq!(s("if(style(--size: 2): yes)"), "yes");
        assert_eq!(s("if(style(--missing): yes; else: no)"), "no");
        assert_eq!(s("if(media(min-width: 1px): a; else: b)"), "a");
        assert_eq!(s("if(not media(width < 1px): a; else: b)"), "a");
        assert_eq!(
            s("if(supports(display: grid) and style(--mode): a; else: b)"),
            "a"
        );
        assert_eq!(s("if((style(--mode: x)) or (style(--size: 2)): a)"), "a");
        assert_eq!(s("if(style(--mode: x): a)"), "");
        assert_eq!(s("1px if(else: var(--size)) 3px"), "1px 2 3px");
    }

    #[test]
    fn custom_functions() {
        let mut functions = Functions::new();
        functions.insert(
            "--double".into(),
            Arc::new(CustomFunction {
                parameters: vec![Parameter {
                    name: "--x".into(),
                    syntax: Syntax::parse("<length>"),
                    default: Some("1px".into()),
                }],
                returns: None,
                body: vec![
                    FunctionItem::Local("--twice".into(), "calc(var(--x) * 2)".into()),
                    FunctionItem::Result("var(--twice)".into()),
                    FunctionItem::Media(
                        "(min-width: 1px)".into(),
                        vec![FunctionItem::Result("wide var(--twice)".into())],
                    ),
                ],
            }),
        );
        functions.insert(
            "--loop".into(),
            Arc::new(CustomFunction {
                parameters: Vec::new(),
                returns: None,
                body: vec![FunctionItem::Result("--loop()".into())],
            }),
        );
        let f = |text: &str| sub_with(text, &[("--outer", "5px")], &functions);
        assert_eq!(f("--double(3px)").unwrap(), "wide calc(3px * 2)");
        assert_eq!(f("--double()").unwrap(), "wide calc(1px * 2)");
        assert_eq!(f("--double(var(--outer))").unwrap(), "wide calc(5px * 2)");
        assert!(f("--double(red)").is_none());
        assert!(f("--double(1px, 2px)").is_none());
        assert!(f("--unknown()").is_none());
        assert!(f("--loop()").is_none());
    }

    #[test]
    fn detects_references() {
        assert!(has_references("1px var(--a)"));
        assert!(has_references("calc(1px + ENV(x))"));
        assert!(has_references("if(else: 1px)"));
        assert!(has_references("--f(1px)"));
        assert!(!has_references("calc(1px + 2px)"));
    }

    #[test]
    fn custom_property_cascade() {
        let inherited: CustomProperties =
            Arc::new(HashMap::from([("--base".to_owned(), Arc::from("2px"))]));
        let declared = vec![
            ("--a".to_owned(), Some("calc(var(--base) * 2)".to_owned())),
            ("--b".to_owned(), Some("var(--a) var(--a)".to_owned())),
            ("--cycle1".to_owned(), Some("var(--cycle2)".to_owned())),
            ("--cycle2".to_owned(), Some("var(--cycle1)".to_owned())),
            ("--broken".to_owned(), Some("var(--nope)".to_owned())),
            ("--base".to_owned(), Some("3px".to_owned())),
            (
                "--chosen".to_owned(),
                Some("if(style(--a: calc(3px * 2)): yes; else: no)".to_owned()),
            ),
        ];
        let functions = Functions::new();
        let env = Environment::new(&no_attr, &functions, &never, &never, &never);
        let computed = compute_custom_properties(&inherited, &declared, &env);
        let get = |name: &str| computed.get(name).map(|v| normalize(v));
        assert_eq!(get("--a").unwrap(), "calc(3px * 2)");
        assert_eq!(get("--b").unwrap(), "calc(3px * 2) calc(3px * 2)");
        assert_eq!(get("--cycle1"), None);
        assert_eq!(get("--cycle2"), None);
        assert_eq!(get("--broken"), None);
        assert_eq!(get("--chosen").unwrap(), "yes");
    }
}
