//! The state of form controls as the parser leaves it (HTML § 4.10): values, checkedness,
//! selectedness and validity. Scripting is disabled, so nothing changes after parsing and
//! every control is in its initial state.

use crate::{Document, Element, NodeId, local_name};

/// The state of an `<input>` from its `type` attribute; unknown types are text fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Hidden,
    Text,
    Search,
    Tel,
    Url,
    Email,
    Password,
    Date,
    Month,
    Week,
    Time,
    DatetimeLocal,
    Number,
    Range,
    Color,
    Checkbox,
    Radio,
    File,
    Submit,
    Image,
    Reset,
    Button,
}

impl InputType {
    pub fn of(element: &Element) -> Self {
        let value = element.attr("type").unwrap_or("").to_ascii_lowercase();
        match value.as_str() {
            "hidden" => Self::Hidden,
            "search" => Self::Search,
            "tel" => Self::Tel,
            "url" => Self::Url,
            "email" => Self::Email,
            "password" => Self::Password,
            "date" => Self::Date,
            "month" => Self::Month,
            "week" => Self::Week,
            "time" => Self::Time,
            "datetime-local" => Self::DatetimeLocal,
            "number" => Self::Number,
            "range" => Self::Range,
            "color" => Self::Color,
            "checkbox" => Self::Checkbox,
            "radio" => Self::Radio,
            "file" => Self::File,
            "submit" => Self::Submit,
            "image" => Self::Image,
            "reset" => Self::Reset,
            "button" => Self::Button,
            _ => Self::Text,
        }
    }

    /// Whether the user types the value as text, so `placeholder` and `readonly` apply.
    pub fn is_textual(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Search
                | Self::Tel
                | Self::Url
                | Self::Email
                | Self::Password
                | Self::Date
                | Self::Month
                | Self::Week
                | Self::Time
                | Self::DatetimeLocal
                | Self::Number
        )
    }

    /// Whether the value is a number, date or time that `min`, `max` and `step` apply to.
    fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Date
                | Self::Month
                | Self::Week
                | Self::Time
                | Self::DatetimeLocal
                | Self::Number
                | Self::Range
        )
    }

    /// Whether it is a button that submits its form.
    pub fn is_submit(self) -> bool {
        matches!(self, Self::Submit | Self::Image)
    }
}

fn is(document: &Document, id: NodeId, name: &crate::LocalName) -> bool {
    document.element(id).is_some_and(|e| e.is_html(name))
}

/// The type of an `<input>` element, or `None` for other elements.
pub fn input_type(document: &Document, id: NodeId) -> Option<InputType> {
    let element = document.element(id)?;
    element
        .is_html(&local_name!("input"))
        .then(|| InputType::of(element))
}

/// The value of an `<input>`: its `value` attribute, sanitized for its type (HTML
/// § 4.10.5.1). Range and color inputs always have a value.
pub fn input_value(document: &Document, id: NodeId) -> String {
    let Some(element) = document.element(id) else {
        return String::new();
    };
    let raw = element.attr("value").unwrap_or("");
    let strip_newlines = |s: &str| s.chars().filter(|c| !matches!(c, '\n' | '\r')).collect();
    match InputType::of(element) {
        InputType::Text | InputType::Search | InputType::Tel | InputType::Password => {
            strip_newlines(raw)
        }
        InputType::Url => strip_newlines(raw).trim_matches(is_space).to_owned(),
        InputType::Email => {
            let value: String = strip_newlines(raw);
            if element.has_attr("multiple") {
                value
                    .split(',')
                    .map(|s| s.trim_matches(is_space))
                    .collect::<Vec<_>>()
                    .join(",")
            } else {
                value.trim_matches(is_space).to_owned()
            }
        }
        InputType::Number => match parse_float(raw) {
            Some(_) => raw.to_owned(),
            None => String::new(),
        },
        InputType::Date
        | InputType::Month
        | InputType::Week
        | InputType::Time
        | InputType::DatetimeLocal => {
            if parse_numeric(InputType::of(element), raw).is_some() {
                raw.to_owned()
            } else {
                String::new()
            }
        }
        InputType::Range => format_number(range_value(element)),
        InputType::Color => {
            let valid = raw.len() == 7
                && raw.starts_with('#')
                && raw[1..].chars().all(|c| c.is_ascii_hexdigit());
            if valid {
                raw.to_ascii_lowercase()
            } else {
                "#000000".to_owned()
            }
        }
        InputType::Hidden
        | InputType::Checkbox
        | InputType::Radio
        | InputType::File
        | InputType::Submit
        | InputType::Image
        | InputType::Reset
        | InputType::Button => raw.to_owned(),
    }
}

/// The ASCII white space of HTML.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{c}' | '\r')
}

/// The value of a `<textarea>`: its text, with line breaks normalized to line feeds.
pub fn textarea_value(document: &Document, id: NodeId) -> String {
    document
        .text_content(id)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// A number without a trailing `.0`, as HTML serializes numbers.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Parses a valid floating-point number (HTML § 2.3.4.3).
pub fn parse_float(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'-') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    let whole = digits(&mut i);
    let mut fraction = 0;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        fraction = digits(&mut i);
        if fraction == 0 {
            return None;
        }
    }
    if whole == 0 && fraction == 0 {
        return None;
    }
    if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
        i += 1;
        if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return None;
        }
    }
    if i != bytes.len() {
        return None;
    }
    text.parse().ok().filter(|v: &f64| v.is_finite())
}

/// Days from 1970-01-01 to a date in the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Exactly `n` ASCII digits as a number.
fn fixed_digits(text: &str, n: usize) -> Option<i64> {
    (text.len() == n && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// A year of four or more digits, greater than zero.
fn year(text: &str) -> Option<i64> {
    let year: i64 = (text.len() >= 4 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()?;
    (year > 0).then_some(year)
}

/// `YYYY-MM` as months since 1970-01.
fn parse_month(text: &str) -> Option<(i64, i64)> {
    let (y, m) = text.rsplit_once('-')?;
    let (y, m) = (year(y)?, fixed_digits(m, 2)?);
    (1..=12).contains(&m).then_some((y, m))
}

/// `YYYY-MM-DD` as days since 1970-01-01.
fn parse_date(text: &str) -> Option<i64> {
    let (ym, d) = text.rsplit_once('-')?;
    let (y, m) = parse_month(ym)?;
    let d = fixed_digits(d, 2)?;
    (1..=days_in_month(y, m))
        .contains(&d)
        .then(|| days_from_civil(y, m, d))
}

/// `HH:MM[:SS[.s+]]` as milliseconds since midnight.
fn parse_time(text: &str) -> Option<f64> {
    let mut parts = text.splitn(3, ':');
    let h = fixed_digits(parts.next()?, 2)?;
    let m = fixed_digits(parts.next()?, 2)?;
    let seconds = match parts.next() {
        None => 0.0,
        Some(s) => {
            let (whole, fraction) = match s.split_once('.') {
                Some((w, f)) if (1..=3).contains(&f.len()) => (w, Some(f)),
                Some(_) => return None,
                None => (s, None),
            };
            let whole = fixed_digits(whole, 2)?;
            let fraction = match fraction {
                Some(f) if f.bytes().all(|b| b.is_ascii_digit()) => {
                    f.parse::<f64>().ok()? / 10f64.powi(f.len() as i32)
                }
                Some(_) => return None,
                None => 0.0,
            };
            if whole > 59 {
                return None;
            }
            whole as f64 + fraction
        }
    };
    if h > 23 || m > 59 {
        return None;
    }
    Some(((h * 60 + m) as f64 * 60.0 + seconds) * 1000.0)
}

/// `YYYY-Www` as weeks since the week of 1970-01-01.
fn parse_week(text: &str) -> Option<i64> {
    let (y, w) = text.split_once("-W")?;
    let (y, w) = (year(y)?, fixed_digits(w, 2)?);
    // A year has 53 weeks if it starts on a Thursday, or on a Wednesday in a leap year.
    let jan1 = (days_from_civil(y, 1, 1) + 3).rem_euclid(7); // 0 is Monday
    let weeks = if jan1 == 3 || (jan1 == 2 && is_leap(y)) {
        53
    } else {
        52
    };
    if !(1..=weeks).contains(&w) {
        return None;
    }
    // The Monday of week 1 is the Monday of the week with the year's first Thursday.
    let first_monday = days_from_civil(y, 1, 4) - (days_from_civil(y, 1, 4) + 3).rem_euclid(7);
    Some((first_monday + (w - 1) * 7 + 3).div_euclid(7))
}

/// The value of a numeric, date or time input as a number, in the unit its `step` counts:
/// the number itself, days, months, weeks or milliseconds.
fn parse_numeric(ty: InputType, text: &str) -> Option<f64> {
    match ty {
        InputType::Number | InputType::Range => parse_float(text),
        InputType::Date => parse_date(text).map(|d| d as f64),
        InputType::Month => parse_month(text).map(|(y, m)| ((y - 1970) * 12 + m - 1) as f64),
        InputType::Week => parse_week(text).map(|w| w as f64),
        InputType::Time => parse_time(text),
        InputType::DatetimeLocal => {
            let (date, time) = text.split_once(['T', ' '])?;
            Some(parse_date(date)? as f64 * 86_400_000.0 + parse_time(time)?)
        }
        _ => None,
    }
}

/// The default step and how many of the value's units one unit of `step` is.
fn step_scale(ty: InputType) -> (f64, f64) {
    match ty {
        InputType::Time | InputType::DatetimeLocal => (60.0, 1000.0),
        _ => (1.0, 1.0),
    }
}

/// The allowed value step, in the value's units, or `None` for `step=any`.
fn allowed_step(element: &Element, ty: InputType) -> Option<f64> {
    let (default, scale) = step_scale(ty);
    match element.attr("step") {
        Some(step) if step.trim().eq_ignore_ascii_case("any") => None,
        Some(step) => Some(
            parse_float(step.trim())
                .filter(|s| *s > 0.0)
                .unwrap_or(default)
                * scale,
        ),
        None => Some(default * scale),
    }
}

/// The minimum and maximum of a numeric input, with range inputs defaulting to 0 and 100.
fn limits(element: &Element, ty: InputType) -> (Option<f64>, Option<f64>) {
    let parse = |name: &str| {
        element
            .attr(name)
            .and_then(|v| parse_numeric(ty, v.trim_matches(is_space)))
    };
    let (min, max) = (parse("min"), parse("max"));
    if ty == InputType::Range {
        let min = min.unwrap_or(0.0);
        let max = max.unwrap_or(100.0);
        // A maximum below the minimum becomes the minimum.
        (Some(min), Some(max.max(min)))
    } else {
        (min, max)
    }
}

/// The value of a range input: the `value` attribute, or else the midpoint, clamped to the
/// limits and moved to the nearest allowed step.
pub fn range_value_of(document: &Document, id: NodeId) -> Option<(f64, f64, f64)> {
    let element = document.element(id)?;
    if InputType::of(element) != InputType::Range {
        return None;
    }
    let (min, max) = limits(element, InputType::Range);
    let (min, max) = (min.unwrap_or(0.0), max.unwrap_or(100.0));
    Some((range_value(element), min, max))
}

fn range_value(element: &Element) -> f64 {
    let ty = InputType::Range;
    let (min, max) = limits(element, ty);
    let (min, max) = (min.unwrap_or(0.0), max.unwrap_or(100.0));
    let mut value = element
        .attr("value")
        .and_then(parse_float)
        .unwrap_or(if max < min {
            min
        } else {
            min + (max - min) / 2.0
        });
    value = value.clamp(min, max);
    if let Some(step) = allowed_step(element, ty) {
        let steps = ((value - min) / step + 0.5).floor();
        value = min + steps * step;
        if value > max {
            value -= step;
        }
        value = value.max(min);
    }
    value
}

/// The form an element belongs to: the one its `form` attribute names, or else its nearest
/// `<form>` ancestor.
pub fn form_owner(document: &Document, id: NodeId) -> Option<NodeId> {
    let element = document.element(id)?;
    if let Some(form) = element.attr("form") {
        return document.descendants(document.root()).find(|&n| {
            document
                .element(n)
                .is_some_and(|e| e.is_html(&local_name!("form")) && e.id() == Some(form))
        });
    }
    document
        .ancestors(id)
        .find(|&a| is(document, a, &local_name!("form")))
}

/// The radio buttons in the same group as `id`, including it (HTML § 4.10.5.1.18).
fn radio_group(document: &Document, id: NodeId) -> Vec<NodeId> {
    let element = document.element(id).expect("radio buttons are elements");
    let name = element.attr("name").unwrap_or("");
    if name.is_empty() {
        return vec![id];
    }
    let owner = form_owner(document, id);
    let tree = document.ancestors(id).last().unwrap_or(id);
    document
        .descendants(tree)
        .filter(|&n| {
            document.element(n).is_some_and(|e| {
                e.is_html(&local_name!("input"))
                    && InputType::of(e) == InputType::Radio
                    && e.attr("name") == Some(name)
            }) && form_owner(document, n) == owner
        })
        .collect()
}

/// Whether a checkbox or radio button is checked. Of radio buttons in a group with
/// `checked`, the parser leaves only the last one checked.
pub fn is_checked(document: &Document, id: NodeId) -> bool {
    let Some(element) = document.element(id) else {
        return false;
    };
    if !element.is_html(&local_name!("input")) || !element.has_attr("checked") {
        return false;
    }
    match InputType::of(element) {
        InputType::Checkbox => true,
        InputType::Radio => {
            let group = radio_group(document, id);
            let last = group
                .iter()
                .rev()
                .find(|&&n| document.element(n).is_some_and(|e| e.has_attr("checked")));
            last == Some(&id)
        }
        _ => false,
    }
}

/// The options of a `<select>`, `<datalist>` or `<optgroup>`, in tree order: its `<option>`
/// children and those of its `<optgroup>` children.
pub fn options(document: &Document, id: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    for child in document.element_children(id) {
        if is(document, child, &local_name!("option")) {
            out.push(child);
        } else if is(document, child, &local_name!("optgroup")) {
            out.extend(
                document
                    .element_children(child)
                    .filter(|&c| is(document, c, &local_name!("option"))),
            );
        }
    }
    out
}

/// How many rows a `<select>` shows: its `size`, or 4 with `multiple` and 1 without.
pub fn display_size(element: &Element) -> u32 {
    element
        .attr("size")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(if element.has_attr("multiple") { 4 } else { 1 })
}

/// Whether a `<select>` shows a list box rather than a drop-down button.
pub fn is_list_box(element: &Element) -> bool {
    element.has_attr("multiple") || display_size(element) > 1
}

fn is_disabled_option(document: &Document, option: NodeId) -> bool {
    document
        .element(option)
        .is_some_and(|e| e.has_attr("disabled"))
        || document
            .parent_element(option)
            .and_then(|p| document.element(p))
            .is_some_and(|p| p.is_html(&local_name!("optgroup")) && p.has_attr("disabled"))
}

/// The selected options of a `<select>` (HTML § 4.10.7, the selectedness setting
/// algorithm). Without `multiple`, only the last option with `selected` is selected, and a
/// drop-down without one selects its first enabled option.
pub fn selected_options(document: &Document, select: NodeId) -> Vec<NodeId> {
    let Some(element) = document.element(select) else {
        return Vec::new();
    };
    let options = options(document, select);
    let marked = || {
        options
            .iter()
            .copied()
            .filter(|&o| document.element(o).is_some_and(|e| e.has_attr("selected")))
    };
    if element.has_attr("multiple") {
        return marked().collect();
    }
    if let Some(last) = marked().next_back() {
        return vec![last];
    }
    if display_size(element) == 1 {
        return options
            .iter()
            .copied()
            .find(|&o| !is_disabled_option(document, o))
            .into_iter()
            .collect();
    }
    Vec::new()
}

/// Whether an `<option>` is selected in its `<select>`, or has `selected` outside one.
pub fn is_selected(document: &Document, option: NodeId) -> bool {
    let select = document
        .ancestors(option)
        .take(2)
        .find(|&a| is(document, a, &local_name!("select")));
    match select {
        Some(select) => selected_options(document, select).contains(&option),
        None => document
            .element(option)
            .is_some_and(|e| e.has_attr("selected")),
    }
}

/// The label of an `<option>`: its `label` attribute, or else its text with white space
/// collapsed.
pub fn option_label(document: &Document, option: NodeId) -> String {
    if let Some(label) = document
        .element(option)
        .and_then(|e| e.attr("label"))
        .filter(|l| !l.is_empty())
    {
        return label.to_owned();
    }
    document
        .text_content(option)
        .split(is_space)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The value of an `<option>`: its `value` attribute, or else its text.
fn option_value(document: &Document, option: NodeId) -> String {
    match document.element(option).and_then(|e| e.attr("value")) {
        Some(value) => value.to_owned(),
        None => option_label(document, option),
    }
}

/// Whether a control is disabled: by its `disabled` attribute or a disabled `<fieldset>`
/// ancestor, outside that fieldset's first `<legend>` (HTML § 4.10.18.5).
pub fn is_disabled(document: &Document, id: NodeId) -> bool {
    let Some(element) = document.element(id) else {
        return false;
    };
    if element.has_attr("disabled") {
        return true;
    }
    if element.is_html(&local_name!("option")) {
        return is_disabled_option(document, id);
    }
    if element.is_html(&local_name!("optgroup")) {
        return false;
    }
    let mut child = id;
    for ancestor in document.ancestors(id) {
        if let Some(e) = document.element(ancestor)
            && e.is_html(&local_name!("fieldset"))
            && e.has_attr("disabled")
        {
            let first_legend = document
                .element_children(ancestor)
                .find(|&c| is(document, c, &local_name!("legend")));
            if first_legend != Some(child) {
                return true;
            }
        }
        child = ancestor;
    }
    false
}

/// The type of a `<button>`: `submit`, `reset` or `button`.
pub fn button_type(element: &Element) -> &'static str {
    match element.attr("type").map(str::to_ascii_lowercase).as_deref() {
        Some("reset") => "reset",
        Some("button") => "button",
        _ => "submit",
    }
}

/// Whether an element is a submit button: a `<button>` of type `submit`, or an `<input>`
/// of type `submit` or `image`.
fn is_submit_button(document: &Document, id: NodeId) -> bool {
    let Some(element) = document.element(id) else {
        return false;
    };
    if element.is_html(&local_name!("button")) {
        return button_type(element) == "submit";
    }
    element.is_html(&local_name!("input")) && InputType::of(element).is_submit()
}

/// Whether an element is its form's default button: the first submit button in tree order
/// whose form owner is that form.
pub fn is_default_button(document: &Document, id: NodeId) -> bool {
    if !is_submit_button(document, id) {
        return false;
    }
    let Some(form) = form_owner(document, id) else {
        return false;
    };
    let tree = document.ancestors(id).last().unwrap_or(id);
    document
        .descendants(tree)
        .find(|&n| is_submit_button(document, n) && form_owner(document, n) == Some(form))
        == Some(id)
}

/// Whether an element takes part in constraint validation: a submittable element that is
/// not disabled, read-only, a non-submit button, or inside a `<datalist>`.
pub fn is_validated(document: &Document, id: NodeId) -> bool {
    let Some(element) = document.element(id) else {
        return false;
    };
    let candidate = if element.is_html(&local_name!("input")) {
        let ty = InputType::of(element);
        !matches!(ty, InputType::Hidden | InputType::Reset | InputType::Button)
            && !(element.has_attr("readonly") && (ty.is_textual() || ty == InputType::Range))
    } else if element.is_html(&local_name!("textarea")) {
        !element.has_attr("readonly")
    } else if element.is_html(&local_name!("button")) {
        button_type(element) == "submit"
    } else {
        element.is_html(&local_name!("select"))
    };
    candidate
        && !is_disabled(document, id)
        && !document
            .ancestors(id)
            .any(|a| is(document, a, &local_name!("datalist")))
}

/// Whether a control's value suffers from no validity problem: a missing required value,
/// a type mismatch, or a value outside its limits or steps. Pattern and length limits only
/// apply to values the user edited, which a static page never has.
pub fn is_valid(document: &Document, id: NodeId) -> bool {
    let Some(element) = document.element(id) else {
        return true;
    };
    if !is_validated(document, id) {
        return true;
    }
    let required = element.has_attr("required");
    if element.is_html(&local_name!("textarea")) {
        return !(required && textarea_value(document, id).is_empty());
    }
    if element.is_html(&local_name!("select")) {
        if !required {
            return true;
        }
        let selected = selected_options(document, id);
        // The placeholder label option: a first option with an empty value, in a drop-down
        // without `multiple`.
        let options = options(document, id);
        let placeholder = !element.has_attr("multiple")
            && display_size(element) == 1
            && options.first().is_some_and(|&first| {
                document.parent_element(first) == Some(id)
                    && option_value(document, first).is_empty()
            });
        return selected
            .iter()
            .any(|&o| !(placeholder && Some(&o) == options.first()));
    }
    if !element.is_html(&local_name!("input")) {
        return true;
    }
    let ty = InputType::of(element);
    let value = input_value(document, id);
    match ty {
        InputType::Checkbox => return !(required && !is_checked(document, id)),
        InputType::Radio => {
            let group = radio_group(document, id);
            let required = group
                .iter()
                .any(|&n| document.element(n).is_some_and(|e| e.has_attr("required")));
            return !(required && !group.iter().any(|&n| is_checked(document, n)));
        }
        // A file input never has a selected file.
        InputType::File => return !required,
        _ => {}
    }
    if required && value.is_empty() && ty != InputType::Range && ty != InputType::Color {
        return false;
    }
    if value.is_empty() {
        return true;
    }
    match ty {
        InputType::Email => {
            let all = if element.has_attr("multiple") {
                value.split(',').all(is_email)
            } else {
                is_email(&value)
            };
            if !all {
                return false;
            }
        }
        InputType::Url if !is_absolute_url(&value) => return false,
        _ => {}
    }
    numeric_state(element, ty, &value) == Some(Range::In) || !ty.is_numeric()
}

/// Where a numeric value is relative to its limits and steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Range {
    In,
    Under,
    Over,
    /// Within the limits but between steps.
    Step,
}

fn numeric_state(element: &Element, ty: InputType, value: &str) -> Option<Range> {
    if !ty.is_numeric() {
        return None;
    }
    let number = parse_numeric(ty, value)?;
    let (min, max) = limits(element, ty);
    if min.is_some_and(|min| number < min) {
        return Some(Range::Under);
    }
    if max.is_some_and(|max| number > max) {
        return Some(Range::Over);
    }
    if let Some(step) = allowed_step(element, ty) {
        // The step base: `min`, or else the `value` attribute, or else zero.
        let base = min
            .or_else(|| {
                element
                    .attr("value")
                    .and_then(|v| parse_numeric(ty, v.trim_matches(is_space)))
            })
            .unwrap_or(0.0);
        let steps = (number - base) / step;
        if (steps - steps.round()).abs() > 1e-9 * steps.abs().max(1.0) {
            return Some(Range::Step);
        }
    }
    Some(Range::In)
}

/// Whether an `<input>` has range limitations, so that `:in-range` or `:out-of-range`
/// applies, and whether its value is outside them. `None` when neither applies.
pub fn out_of_range(document: &Document, id: NodeId) -> Option<bool> {
    let element = document.element(id)?;
    if !element.is_html(&local_name!("input")) || !is_validated(document, id) {
        return None;
    }
    let ty = InputType::of(element);
    if !ty.is_numeric() {
        return None;
    }
    let (min, max) = limits(element, ty);
    if min.is_none() && max.is_none() {
        return None;
    }
    let value = input_value(document, id);
    Some(matches!(
        numeric_state(element, ty, &value),
        Some(Range::Under | Range::Over)
    ))
}

/// Whether a radio button's group has no checked button, or a `<progress>` has no valid
/// `value`: the states `:indeterminate` matches. Checkboxes are only indeterminate from
/// script.
pub fn is_indeterminate(document: &Document, id: NodeId) -> bool {
    let Some(element) = document.element(id) else {
        return false;
    };
    if element.is_html(&local_name!("progress")) {
        return element.attr("value").and_then(parse_float).is_none();
    }
    element.is_html(&local_name!("input"))
        && InputType::of(element) == InputType::Radio
        && !radio_group(document, id)
            .iter()
            .any(|&n| is_checked(document, n))
}

/// A valid email address (HTML § 4.10.5.1.5).
fn is_email(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    let local_ok = !local.is_empty()
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".!#$%&'*+/=?^_`{|}~-".contains(c));
    let label_ok = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    };
    local_ok && domain.split('.').all(label_ok)
}

/// Whether a URL is absolute: a scheme, a colon and more.
fn is_absolute_url(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        && !rest.is_empty()
        && !text.contains(char::is_whitespace)
}

/// Whether a `<form>` or `<fieldset>` contains a control that is not valid.
pub fn contains_invalid(document: &Document, id: NodeId) -> bool {
    document
        .descendants(id)
        .skip(1)
        .any(|n| document.element(n).is_some() && !is_valid(document, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn by_id(doc: &Document, id: &str) -> NodeId {
        doc.descendants(doc.root())
            .find(|&n| doc.element(n).is_some_and(|e| e.id() == Some(id)))
            .unwrap()
    }

    #[test]
    fn floats() {
        assert_eq!(parse_float("1.5"), Some(1.5));
        assert_eq!(parse_float("-.5"), Some(-0.5));
        assert_eq!(parse_float("1e3"), Some(1000.0));
        assert_eq!(parse_float("1."), None);
        assert_eq!(parse_float("+1"), None);
        assert_eq!(parse_float(" 1"), None);
        assert_eq!(parse_float("inf"), None);
    }

    #[test]
    fn dates_and_times() {
        assert_eq!(parse_date("1970-01-02"), Some(1));
        assert_eq!(parse_date("2024-02-29"), Some(19_782));
        assert_eq!(parse_date("2023-02-29"), None);
        assert_eq!(parse_time("01:00"), Some(3_600_000.0));
        assert_eq!(parse_time("00:00:01.5"), Some(1500.0));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_week("1970-W01"), Some(0));
        assert!(parse_week("2020-W53").is_some());
        assert_eq!(parse_week("2021-W53"), None);
        assert_eq!(parse_month("1971-02"), Some((1971, 2)));
    }

    #[test]
    fn values_are_sanitized() {
        let doc = Document::parse(
            "<input id=a type=number value=x><input id=b type=color value=#ABCDEF>\
             <input id=c type=range><input id=d type=range min=0 max=10 step=3 value=8>\
             <input id=e type=email value=' a@b.c '>",
        );
        assert_eq!(input_value(&doc, by_id(&doc, "a")), "");
        assert_eq!(input_value(&doc, by_id(&doc, "b")), "#abcdef");
        assert_eq!(input_value(&doc, by_id(&doc, "c")), "50");
        assert_eq!(input_value(&doc, by_id(&doc, "d")), "9");
        assert_eq!(input_value(&doc, by_id(&doc, "e")), "a@b.c");
    }

    #[test]
    fn radio_groups_keep_the_last_checked() {
        let doc = Document::parse(
            "<input id=a type=radio name=g checked><input id=b type=radio name=g checked>\
             <form><input id=c type=radio name=g checked></form>",
        );
        assert!(!is_checked(&doc, by_id(&doc, "a")));
        assert!(is_checked(&doc, by_id(&doc, "b")));
        assert!(is_checked(&doc, by_id(&doc, "c")));
    }

    #[test]
    fn selectedness() {
        let doc = Document::parse(
            "<select id=a><option disabled>x<option id=a2>y<option>z</select>\
             <select id=b><option selected>x<option id=b2 selected>y</select>\
             <select id=c size=3><option>x</select>",
        );
        assert_eq!(
            selected_options(&doc, by_id(&doc, "a")),
            [by_id(&doc, "a2")]
        );
        assert_eq!(
            selected_options(&doc, by_id(&doc, "b")),
            [by_id(&doc, "b2")]
        );
        assert!(selected_options(&doc, by_id(&doc, "c")).is_empty());
    }

    #[test]
    fn validity() {
        let doc = Document::parse(
            "<form id=f><input id=a required><input id=b type=email value=nope>\
             <input id=c type=number min=1 max=5 value=7><input id=d type=number step=2 value=3 min=0>\
             <input id=e type=url value='https://x.y'>\
             <select id=g required><option value=''>Pick<option>A</select></form>",
        );
        for (id, valid) in [
            ("a", false),
            ("b", false),
            ("c", false),
            ("d", false),
            ("e", true),
            ("g", false),
        ] {
            assert_eq!(is_valid(&doc, by_id(&doc, id)), valid, "#{id}");
        }
        assert!(contains_invalid(&doc, by_id(&doc, "f")));
        assert_eq!(out_of_range(&doc, by_id(&doc, "c")), Some(true));
        assert_eq!(out_of_range(&doc, by_id(&doc, "d")), Some(false));
        assert_eq!(out_of_range(&doc, by_id(&doc, "a")), None);
    }
}
