//! Page sizes, lengths and margins as people write them.

use crate::{Error, Sides, Size};

/// CSS pixels per centimetre.
pub(crate) const CM: f32 = 96.0 / 2.54;
/// CSS pixels per millimetre.
const MM: f32 = CM / 10.0;

fn invalid(message: String) -> Error {
    Error::InvalidOptions(message)
}

/// A page size by name: `A3`, `A4`, `A5`, `Letter` or `Legal`, in any case.
pub fn page_size(name: &str) -> Result<Size, Error> {
    let mm = |width: f32, height: f32| Size::new(width * MM, height * MM);
    Ok(match name.trim().to_ascii_lowercase().as_str() {
        "a3" => mm(297.0, 420.0),
        "a4" => Size::A4,
        "a5" => mm(148.0, 210.0),
        "letter" => Size::LETTER,
        "legal" => Size::new(8.5 * 96.0, 14.0 * 96.0),
        _ => return Err(invalid(format!("unknown page size `{name}`"))),
    })
}

/// A length such as `2cm`, `15mm`, `0.5in`, `12pt` or `40px`, in CSS pixels. A number without
/// a unit is in pixels.
pub fn parse_length(text: &str) -> Result<f32, Error> {
    let text = text.trim();
    let split = text
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let value: f32 = number
        .parse()
        .ok()
        .filter(|value: &f32| value.is_finite())
        .ok_or_else(|| invalid(format!("invalid length `{text}`")))?;
    let px_per_unit = match unit.to_ascii_lowercase().as_str() {
        "" | "px" => 1.0,
        "mm" => MM,
        "cm" => CM,
        "in" => 96.0,
        "pt" => 96.0 / 72.0,
        _ => return Err(invalid(format!("unknown unit `{unit}` in `{text}`"))),
    };
    Ok(value * px_per_unit)
}

/// Page margins written like the CSS `margin` shorthand: one to four lengths, for the top,
/// right, bottom and left. None may be negative.
pub fn parse_margin(text: &str) -> Result<Sides, Error> {
    let lengths = text
        .split_whitespace()
        .map(parse_length)
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(negative) = lengths.iter().find(|&&length| length < 0.0) {
        return Err(invalid(format!(
            "margins cannot be negative, as `{negative}px` in `{text}` is"
        )));
    }
    let (top, right, bottom, left) = match lengths[..] {
        [all] => (all, all, all, all),
        [vertical, horizontal] => (vertical, horizontal, vertical, horizontal),
        [top, horizontal, bottom] => (top, horizontal, bottom, horizontal),
        [top, right, bottom, left] => (top, right, bottom, left),
        _ => {
            return Err(invalid(format!(
                "a margin has one to four lengths, not `{text}`"
            )));
        }
    };
    Ok(Sides {
        top,
        right,
        bottom,
        left,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_are_case_insensitive() {
        assert_eq!(parse_length("1IN").unwrap(), 96.0);
        assert_eq!(page_size(" a4 ").unwrap(), Size::A4);
    }

    #[test]
    fn margins_need_one_to_four_lengths() {
        assert!(parse_margin("").is_err());
        assert!(parse_margin("1mm -1mm").is_err());
        assert_eq!(parse_margin("1cm").unwrap(), Sides::all(CM));
    }
}
