//! The tracker files and what is built from them.
//!
//! The fixture test suite lives in `tests/fixtures.rs`; run it with
//! `cargo test -p rustyprint-tests`. The progress page, `progress.html`, is generated from the
//! trackers with `cargo run -p rustyprint-tests --bin progress`.

pub mod progress;

/// Tracker files by file stem, with their titles.
pub const TRACKERS: &[(&str, &str)] = &[
    ("html-elements", "HTML elements"),
    ("css-properties", "CSS properties"),
    ("css-selectors", "CSS selectors"),
    ("css-at-rules", "CSS at-rules"),
    ("css-values", "CSS values"),
];

/// The statuses a tracker row can have, from least to most complete.
pub const STATUSES: &[&str] = &["todo", "parsed", "partial", "done"];

/// A row of a tracker table.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// The heading the row is under.
    pub section: String,
    /// The tracked name, such as `background` or `:hover`.
    pub name: String,
    /// The first cell as written, in Markdown.
    pub label: String,
    pub milestone: Option<u32>,
    pub status: String,
    /// The last cell, in Markdown.
    pub notes: String,
}

/// The rows of the tracker tables in a tracker file.
pub fn tracker_items(text: &str) -> Vec<Item> {
    let mut items = Vec::new();
    let mut section = String::new();
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix('#') {
            section = heading.trim_start_matches('#').trim().to_owned();
            continue;
        }
        if !line.starts_with("| ") || line.starts_with("| ---") {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split(" | ").map(str::trim).collect();
        let Some(index) = cells
            .iter()
            .skip(1)
            .position(|c| STATUSES.contains(c))
            .map(|i| i + 1)
        else {
            continue;
        };
        let label = cells[0];
        let name = between(label, '`', '`')
            .or_else(|| between(label, '[', ']'))
            .unwrap_or(label);
        items.push(Item {
            section: section.clone(),
            name: name.to_owned(),
            label: label.to_owned(),
            milestone: cells[index - 1].parse().ok(),
            status: cells[index].to_owned(),
            notes: if index + 1 < cells.len() {
                cells[cells.len() - 1].to_owned()
            } else {
                String::new()
            },
        });
    }
    items
}

fn between(text: &str, open: char, close: char) -> Option<&str> {
    let start = text.find(open)? + open.len_utf8();
    let end = start + text[start..].find(close)?;
    Some(&text[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_rows_with_and_without_description_columns() {
        let text = "## Text\n\n| Status | Meaning |\n| --- | --- |\n| `todo` | Not started |\n\n\
                    | Element | Description | Milestone | Status | Notes |\n| --- | --- | --- | --- | --- |\n\
                    | [`p`](https://x) | Paragraph | 3 | done |  |\n\
                    | `max-lines` | 4 | partial | Experimental |\n";
        let items = tracker_items(text);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "p");
        assert_eq!(items[0].milestone, Some(3));
        assert_eq!(items[0].notes, "");
        assert_eq!(items[1].name, "max-lines");
        assert_eq!(items[1].status, "partial");
        assert_eq!(items[1].notes, "Experimental");
        assert_eq!(items[1].section, "Text");
    }
}
