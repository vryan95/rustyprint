//! Presentational hints: HTML attributes that map to CSS (HTML § 10 Rendering).
//!
//! Hints for properties the engine does not support yet are dropped when parsed.

use cssparser::color::parse_named_color;
use rustyprint_dom::{Document, Element, LocalName, NodeId, local_name, ns};

/// The CSS declarations an element's attributes imply, as `(property, value)` pairs.
pub fn presentational_hints(document: &Document, node: NodeId) -> Vec<(&'static str, String)> {
    let Some(element) = document.element(node) else {
        return Vec::new();
    };
    if element.name.ns != ns!(html) {
        return Vec::new();
    }
    let mut hints = Vec::new();
    let mut add = |property: &'static str, value: String| hints.push((property, value));
    let name = &element.name.local;
    let attr = |a: &str| element.attr(a);
    let is = |names: &[LocalName]| names.contains(name);

    // Background and text colors.
    if is(&[
        local_name!("body"),
        local_name!("table"),
        local_name!("thead"),
        local_name!("tbody"),
        local_name!("tfoot"),
        local_name!("tr"),
        local_name!("td"),
        local_name!("th"),
    ]) && let Some(color) = attr("bgcolor").and_then(legacy_color)
    {
        add("background-color", color);
    }
    if *name == local_name!("body") {
        if let Some(color) = attr("text").and_then(legacy_color) {
            add("color", color);
        }
        for (attribute, property) in [
            ("marginheight", "margin-top"),
            ("marginheight", "margin-bottom"),
            ("topmargin", "margin-top"),
            ("bottommargin", "margin-bottom"),
            ("marginwidth", "margin-left"),
            ("marginwidth", "margin-right"),
            ("leftmargin", "margin-left"),
            ("rightmargin", "margin-right"),
        ] {
            if let Some(value) = attr(attribute).and_then(dimension) {
                add(property, value);
            }
        }
    }

    // `align`.
    if let Some(align) = attr("align").map(str::to_ascii_lowercase) {
        if is(&[
            local_name!("div"),
            local_name!("p"),
            local_name!("h1"),
            local_name!("h2"),
            local_name!("h3"),
            local_name!("h4"),
            local_name!("h5"),
            local_name!("h6"),
            local_name!("legend"),
            local_name!("thead"),
            local_name!("tbody"),
            local_name!("tfoot"),
            local_name!("tr"),
            local_name!("td"),
            local_name!("th"),
        ]) {
            match align.as_str() {
                "left" | "right" | "justify" => add("text-align", align.clone()),
                "center" | "middle" => add("text-align", "center".into()),
                _ => {}
            }
        } else if *name == local_name!("caption") {
            match align.as_str() {
                "top" | "bottom" => add("caption-side", align.clone()),
                "left" | "right" | "center" | "justify" => add("text-align", align.clone()),
                _ => {}
            }
        } else if *name == local_name!("table") {
            match align.as_str() {
                "left" | "right" => add("float", align.clone()),
                "center" => {
                    add("margin-inline-start", "auto".into());
                    add("margin-inline-end", "auto".into());
                }
                _ => {}
            }
        } else if *name == local_name!("hr") {
            match align.as_str() {
                "left" => {
                    add("margin-left", "0".into());
                    add("margin-right", "auto".into());
                }
                "right" => {
                    add("margin-left", "auto".into());
                    add("margin-right", "0".into());
                }
                "center" => {
                    add("margin-left", "auto".into());
                    add("margin-right", "auto".into());
                }
                _ => {}
            }
        } else if is(&[
            local_name!("img"),
            local_name!("object"),
            local_name!("embed"),
            local_name!("iframe"),
            local_name!("input"),
            local_name!("video"),
        ]) {
            match align.as_str() {
                "left" | "right" => add("float", align.clone()),
                "top" => add("vertical-align", "top".into()),
                "middle" | "absmiddle" | "abscenter" => add("vertical-align", "middle".into()),
                "bottom" | "baseline" => add("vertical-align", "baseline".into()),
                "texttop" => add("vertical-align", "text-top".into()),
                _ => {}
            }
        }
    }

    // `valign`.
    if is(&[
        local_name!("col"),
        local_name!("colgroup"),
        local_name!("thead"),
        local_name!("tbody"),
        local_name!("tfoot"),
        local_name!("tr"),
        local_name!("td"),
        local_name!("th"),
    ]) && let Some(valign) = attr("valign").map(str::to_ascii_lowercase)
        && matches!(valign.as_str(), "top" | "middle" | "bottom" | "baseline")
    {
        add("vertical-align", valign);
    }

    // Dimensions.
    let sized = [
        local_name!("table"),
        local_name!("col"),
        local_name!("colgroup"),
        local_name!("hr"),
        local_name!("td"),
        local_name!("th"),
        local_name!("img"),
        local_name!("object"),
        local_name!("embed"),
        local_name!("iframe"),
        local_name!("video"),
        local_name!("canvas"),
    ];
    if is(&sized)
        && let Some(width) = attr("width").and_then(non_zero_dimension)
    {
        add("width", width);
    }
    let tall = [
        local_name!("table"),
        local_name!("tr"),
        local_name!("td"),
        local_name!("th"),
        local_name!("img"),
        local_name!("object"),
        local_name!("embed"),
        local_name!("iframe"),
        local_name!("video"),
        local_name!("canvas"),
    ];
    if is(&tall)
        && let Some(height) = attr("height").and_then(non_zero_dimension)
    {
        add("height", height);
    }

    // `width` and `height` give the aspect ratio until the image's own is known
    // (HTML § 15.4.4).
    if is(&[
        local_name!("img"),
        local_name!("video"),
        local_name!("canvas"),
    ]) && let (Some(w), Some(h)) = (
        attr("width")
            .filter(|v| !v.contains('%'))
            .and_then(non_negative_integer),
        attr("height")
            .filter(|v| !v.contains('%'))
            .and_then(non_negative_integer),
    ) && w > 0
        && h > 0
    {
        add("aspect-ratio", format!("auto {w} / {h}"));
    }

    // Images and embedded content.
    if is(&[
        local_name!("img"),
        local_name!("object"),
        local_name!("embed"),
        local_name!("iframe"),
        local_name!("video"),
    ]) {
        if let Some(space) = attr("hspace").and_then(dimension) {
            add("margin-left", space.clone());
            add("margin-right", space);
        }
        if let Some(space) = attr("vspace").and_then(dimension) {
            add("margin-top", space.clone());
            add("margin-bottom", space);
        }
    }
    if is(&[local_name!("img"), local_name!("object")])
        && let Some(border) = attr("border").and_then(non_negative_integer)
    {
        add("border-width", format!("{border}px"));
        add("border-style", "solid".into());
    }
    if *name == local_name!("iframe")
        && attr("frameborder").is_some_and(|f| f == "0" || f.eq_ignore_ascii_case("no"))
    {
        add("border", "none".into());
    }

    // Tables.
    if *name == local_name!("table") {
        // The border styles of tables with `border` are in the UA stylesheet, where `frame`
        // and `rules` can change them.
        if let Some(border) = attr("border") {
            let width = non_negative_integer(border).unwrap_or(1);
            add("border-width", format!("{width}px"));
        }
        if let Some(spacing) = attr("cellspacing").and_then(non_negative_integer) {
            add("border-spacing", format!("{spacing}px"));
        }
        if let Some(color) = attr("bordercolor").and_then(legacy_color) {
            add("border-color", color);
        }
    }
    if is(&[local_name!("td"), local_name!("th")]) {
        if let Some(table) = table_of(document, node)
            && let Some(padding) = table.attr("cellpadding").and_then(non_negative_integer)
        {
            add("padding", format!("{padding}px"));
        }
        if element.has_attr("nowrap") && attr("width").is_none() {
            add("white-space", "nowrap".into());
        }
    }

    // Fonts.
    if *name == local_name!("font") {
        if let Some(color) = attr("color").and_then(legacy_color) {
            add("color", color);
        }
        if let Some(face) = attr("face").filter(|f| !f.trim().is_empty()) {
            let families: Vec<String> = face.split(',').map(|f| quote(f.trim())).collect();
            add("font-family", families.join(", "));
        }
        if let Some(size) = attr("size").and_then(legacy_font_size) {
            add("font-size", size.into());
        }
    }

    // Horizontal rules.
    if *name == local_name!("hr") {
        if let Some(color) = attr("color").and_then(legacy_color) {
            add("border-style", "solid".into());
            add("border-color", color.clone());
            add("background-color", color);
        } else if element.has_attr("noshade") {
            add("border-style", "solid".into());
            add("border-color", "gray".into());
            add("background-color", "gray".into());
        }
        if let Some(size) = attr("size")
            .and_then(non_negative_integer)
            .filter(|&s| s > 0)
        {
            if size == 1 {
                add("border-bottom-width", "0".into());
            } else {
                add("height", format!("{}px", size - 2));
            }
        }
    }

    // Lists: `start` and `reversed` reset the list's counter, and `value` sets an item's.
    if *name == local_name!("ol") {
        let start = attr("start").and_then(integer);
        if attr("reversed").is_some() {
            match start {
                Some(start) => add(
                    "counter-reset",
                    format!("reversed(list-item) {}", start.saturating_add(1)),
                ),
                None => add("counter-reset", "reversed(list-item)".into()),
            }
        } else if let Some(start) = start {
            add(
                "counter-reset",
                format!("list-item {}", start.saturating_sub(1)),
            );
        }
    }
    if *name == local_name!("li")
        && let Some(value) = attr("value").and_then(integer)
    {
        add("counter-set", format!("list-item {value}"));
    }
    if let Some(kind) = attr("type") {
        let style = match (name, kind) {
            (&local_name!("ol") | &local_name!("li"), "1") => Some("decimal"),
            (&local_name!("ol") | &local_name!("li"), "a") => Some("lower-alpha"),
            (&local_name!("ol") | &local_name!("li"), "A") => Some("upper-alpha"),
            (&local_name!("ol") | &local_name!("li"), "i") => Some("lower-roman"),
            (&local_name!("ol") | &local_name!("li"), "I") => Some("upper-roman"),
            (&local_name!("ul") | &local_name!("li"), kind) => {
                match kind.to_ascii_lowercase().as_str() {
                    "disc" => Some("disc"),
                    "circle" => Some("circle"),
                    "square" => Some("square"),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(style) = style {
            add("list-style-type", style.into());
        }
    }
    hints
}

/// The table an element belongs to.
fn table_of(document: &Document, node: NodeId) -> Option<&Element> {
    document
        .ancestors(node)
        .filter_map(|a| document.element(a))
        .find(|e| e.is_html(&local_name!("table")))
}

fn quote(family: &str) -> String {
    let mut out = String::new();
    let _ = cssparser::serialize_string(family, &mut out);
    out
}

/// HTML "rules for parsing non-negative integers".
fn non_negative_integer(value: &str) -> Option<u32> {
    let digits: String = value
        .trim_start()
        .trim_start_matches('+')
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// HTML "rules for parsing integers".
fn integer(value: &str) -> Option<i32> {
    let value = value.trim_start();
    let (negative, rest) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let magnitude: i64 = digits.parse().ok()?;
    let value = if negative { -magnitude } else { magnitude };
    i32::try_from(value).ok()
}

/// HTML "rules for parsing dimension values": a number of pixels or a percentage.
fn dimension(value: &str) -> Option<String> {
    let value = value.trim_start();
    let end = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let number: f64 = value[..end].trim_end_matches('.').parse().ok()?;
    if value[end..].starts_with('%') {
        Some(format!("{number}%"))
    } else {
        Some(format!("{number}px"))
    }
}

/// Like [`dimension`], ignoring zero.
fn non_zero_dimension(value: &str) -> Option<String> {
    dimension(value).filter(|d| !d.starts_with("0px") && !d.starts_with("0%"))
}

/// HTML "rules for parsing a legacy font size", for `<font size>`.
fn legacy_font_size(value: &str) -> Option<&'static str> {
    let value = value.trim();
    let (mode, digits) = match value.chars().next()? {
        '+' => (1, &value[1..]),
        '-' => (-1, &value[1..]),
        _ => (0, value),
    };
    let number: i32 = digits
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()?;
    let size = match mode {
        1 => 3 + number,
        -1 => 3 - number,
        _ => number,
    }
    .clamp(1, 7);
    Some(
        [
            "x-small",
            "small",
            "medium",
            "large",
            "x-large",
            "xx-large",
            "xxx-large",
        ][size as usize - 1],
    )
}

/// HTML "rules for parsing a legacy color value", returned as CSS.
pub fn legacy_color(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("transparent") {
        return None;
    }
    if let Ok((r, g, b)) = parse_named_color(&value.to_ascii_lowercase()) {
        return Some(format!("rgb({r}, {g}, {b})"));
    }
    let bytes = value.as_bytes();
    if bytes.len() == 4 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit) {
        let digit = |i: usize| u8::from_str_radix(&value[i..=i], 16).ok().map(|d| d * 17);
        return Some(format!("rgb({}, {}, {})", digit(1)?, digit(2)?, digit(3)?));
    }
    // Characters outside the BMP count as "00"; everything not hex becomes "0".
    let mut text: String = value
        .chars()
        .flat_map(|c| {
            if (c as u32) > 0xFFFF {
                vec!['0', '0']
            } else {
                vec![c]
            }
        })
        .collect();
    text = text.chars().take(128).collect();
    if let Some(rest) = text.strip_prefix('#') {
        text = rest.to_owned();
    }
    let mut digits: Vec<char> = text
        .chars()
        .map(|c| if c.is_ascii_hexdigit() { c } else { '0' })
        .collect();
    while digits.is_empty() || !digits.len().is_multiple_of(3) {
        digits.push('0');
    }
    let length = digits.len() / 3;
    let mut parts: Vec<Vec<char>> = digits.chunks(length).map(<[char]>::to_vec).collect();
    if length > 8 {
        for part in &mut parts {
            part.drain(..length - 8);
        }
    }
    while parts[0].len() > 2 && parts.iter().all(|p| p[0] == '0') {
        for part in &mut parts {
            part.remove(0);
        }
    }
    let component = |part: &[char]| -> Option<u8> {
        let hex: String = part.iter().take(2).collect();
        u8::from_str_radix(&hex, 16).ok()
    };
    Some(format!(
        "rgb({}, {}, {})",
        component(&parts[0])?,
        component(&parts[1])?,
        component(&parts[2])?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_colors() {
        assert_eq!(legacy_color("red").unwrap(), "rgb(255, 0, 0)");
        assert_eq!(legacy_color("#0f0").unwrap(), "rgb(0, 255, 0)");
        assert_eq!(legacy_color("#123456").unwrap(), "rgb(18, 52, 86)");
        // The famous "chucknorris" is red.
        assert_eq!(legacy_color("chucknorris").unwrap(), "rgb(192, 0, 0)");
        assert_eq!(legacy_color("transparent"), None);
    }

    #[test]
    fn dimensions() {
        assert_eq!(dimension("100").unwrap(), "100px");
        assert_eq!(dimension(" 50% ").unwrap(), "50%");
        assert_eq!(dimension("12.5px").unwrap(), "12.5px");
        assert_eq!(dimension("abc"), None);
        assert_eq!(non_zero_dimension("0"), None);
    }

    #[test]
    fn font_sizes() {
        assert_eq!(legacy_font_size("3"), Some("medium"));
        assert_eq!(legacy_font_size("+2"), Some("x-large"));
        assert_eq!(legacy_font_size("-5"), Some("x-small"));
        assert_eq!(legacy_font_size("9"), Some("xxx-large"));
    }

    #[test]
    fn element_hints() {
        let doc = Document::parse(
            r#"<body bgcolor=yellow text=navy><table border=2 cellpadding=4><tr><td id=c align=center>x</table>"#,
        );
        let body = doc.find_html(&local_name!("body")).unwrap();
        let hints = presentational_hints(&doc, body);
        assert!(hints.contains(&("background-color", "rgb(255, 255, 0)".into())));
        assert!(hints.contains(&("color", "rgb(0, 0, 128)".into())));
        let td = doc.find_html(&local_name!("td")).unwrap();
        let hints = presentational_hints(&doc, td);
        assert!(hints.contains(&("text-align", "center".into())));
        assert!(hints.contains(&("padding", "4px".into())));
    }
}
