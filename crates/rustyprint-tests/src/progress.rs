//! The progress page: a self-contained HTML summary of the tracker files.
//!
//! The page has no timestamp, so the same trackers always give the same page and a test can
//! check that `progress.html` is up to date.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::Path;

use crate::{Item, STATUSES, TRACKERS, tracker_items};

/// Where the page is written, relative to the repository root.
pub const PAGE: &str = "progress.html";

/// Row counts by status.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Counts([usize; 4]);

impl Counts {
    fn add(&mut self, status: &str) {
        if let Some(index) = STATUSES.iter().position(|s| *s == status) {
            self.0[index] += 1;
        }
    }

    fn get(&self, status: &str) -> usize {
        STATUSES
            .iter()
            .position(|s| *s == status)
            .map_or(0, |i| self.0[i])
    }

    fn total(&self) -> usize {
        self.0.iter().sum()
    }

    fn done_percent(&self) -> f64 {
        percent(self.get("done"), self.total())
    }
}

/// The statuses a milestone can have in the plan.
const MILESTONE_STATUSES: &[&str] = &["todo", "partial", "done"];

/// A milestone status as shown on the page.
fn milestone_label(status: &str) -> &str {
    match status {
        "done" => "Complete",
        "partial" => "In progress",
        "todo" => "Not started",
        other => other,
    }
}

fn percent(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 * 100.0 / total as f64
    }
}

/// A milestone from the plan: its name, status and outcome, in Markdown.
struct Milestone {
    name: String,
    /// `todo`, `partial` or `done`.
    status: String,
    outcome: String,
}

/// The milestones table of `plan.md`.
fn milestones(plan: &str) -> BTreeMap<u32, Milestone> {
    let mut out = BTreeMap::new();
    let mut in_section = false;
    for line in plan.lines() {
        if line.starts_with("## ") {
            in_section = line.contains("Milestones");
            continue;
        }
        if !in_section || !line.starts_with("| ") {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split(" | ").map(str::trim).collect();
        if let [number, name, status, outcome] = cells[..]
            && let Ok(number) = number.parse()
        {
            out.insert(
                number,
                Milestone {
                    name: name.trim_matches('*').to_owned(),
                    status: status.to_owned(),
                    outcome: outcome.to_owned(),
                },
            );
        }
    }
    out
}

type Tracker = (&'static str, &'static str, Vec<Item>);

fn read(root: &Path) -> io::Result<(BTreeMap<u32, Milestone>, Vec<Tracker>)> {
    let plan = fs::read_to_string(root.join("plan.md"))?;
    let mut trackers = Vec::new();
    for (stem, title) in TRACKERS {
        let text = fs::read_to_string(root.join(format!("{stem}.md")))?;
        trackers.push((*stem, *title, tracker_items(&text)));
    }
    Ok((milestones(&plan), trackers))
}

/// Builds the page from the trackers and `plan.md` in `root`.
pub fn page(root: &Path) -> io::Result<String> {
    let (milestones, trackers) = read(root)?;
    Ok(render(&milestones, &trackers))
}

/// Where the milestone statuses in `plan.md` disagree with the trackers in `root`: a status
/// that is not one of [`MILESTONE_STATUSES`], a `done` milestone with `todo` rows, a `todo`
/// milestone with rows that are further along, or rows of a milestone the plan lacks.
pub fn milestone_problems(root: &Path) -> io::Result<Vec<String>> {
    let (milestones, trackers) = read(root)?;
    let mut problems = Vec::new();
    for (number, milestone) in &milestones {
        if !MILESTONE_STATUSES.contains(&milestone.status.as_str()) {
            problems.push(format!(
                "milestone {number} has status `{}`, not one of {MILESTONE_STATUSES:?}",
                milestone.status
            ));
        }
    }
    for (stem, _, items) in &trackers {
        for item in items {
            let Some(number) = item.milestone else {
                continue;
            };
            let row = format!("{stem}.md row `{}` ({})", item.name, item.status);
            match milestones.get(&number).map(|m| m.status.as_str()) {
                None => problems.push(format!("{row}: milestone {number} is not in the plan")),
                Some("done") if item.status == "todo" => {
                    problems.push(format!("{row}: milestone {number} is marked done"))
                }
                Some("todo") if item.status != "todo" => {
                    problems.push(format!("{row}: milestone {number} is marked todo"))
                }
                _ => {}
            }
        }
    }
    Ok(problems)
}

/// Writes the page to `progress.html` in `root`.
pub fn write(root: &Path) -> io::Result<()> {
    fs::write(root.join(PAGE), page(root)?)
}

fn render(milestones: &BTreeMap<u32, Milestone>, trackers: &[(&str, &str, Vec<Item>)]) -> String {
    let mut overall = Counts::default();
    let mut by_milestone: BTreeMap<u32, Counts> = BTreeMap::new();
    for (_, _, items) in trackers {
        for item in items {
            overall.add(&item.status);
            if let Some(m) = item.milestone {
                by_milestone.entry(m).or_default().add(&item.status);
            }
        }
    }
    let mut html = String::new();
    html.push_str(HEAD);

    // Summary.
    let next = milestones.iter().find(|(_, m)| m.status != "done").map_or(
        "Every milestone is complete.".to_owned(),
        |(number, m)| {
            format!(
                "<strong>Next:</strong> milestone {number}, {} ({}).",
                escape(&m.name),
                milestone_label(&m.status).to_lowercase()
            )
        },
    );
    let _ = writeln!(
        html,
        r#"<header>
<h1>rustyprint progress</h1>
<p class="lede">Status of every milestone from <code>plan.md</code>, and of every tracked HTML element and CSS feature from the tracker files. Regenerate with <code>cargo run -p rustyprint-tests --bin progress</code>.</p>
<p class="next">{next}</p>
</header>
<section class="summary">
<div class="big"><span class="value">{:.0}%</span><span class="label">of {} rows done</span></div>"#,
        overall.done_percent(),
        overall.total()
    );
    for status in STATUSES.iter().rev() {
        let _ = writeln!(
            html,
            r#"<div class="stat"><span class="value">{}</span><span class="label"><i class="dot {status}"></i>{}</span></div>"#,
            overall.get(status),
            status_label(status)
        );
    }
    html.push_str("</section>\n");
    html.push_str(&bar(&overall, "wide"));
    html.push_str(LEGEND);

    // Milestones.
    html.push_str(
        "<h2>Milestones</h2>\n<div class=\"scroll\"><table class=\"overview\">\n<thead><tr><th>#</th><th>Milestone</th><th class=\"bar-cell\">Rows</th><th class=\"num\">Done</th><th>State</th></tr></thead>\n<tbody>\n",
    );
    let numbers: Vec<u32> = milestones
        .keys()
        .chain(by_milestone.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for number in numbers {
        let counts = by_milestone.get(&number).copied().unwrap_or_default();
        let (name, status, outcome) =
            milestones
                .get(&number)
                .map_or((String::new(), "none", String::new()), |m| {
                    (
                        escape(&m.name),
                        m.status.as_str(),
                        inline_markdown(&m.outcome),
                    )
                });
        let (rows, done) = if counts.total() == 0 {
            (
                r#"<span class="muted">No tracked rows</span>"#.to_owned(),
                String::new(),
            )
        } else {
            (
                bar(&counts, ""),
                format!("{} / {}", counts.get("done"), counts.total()),
            )
        };
        let _ = writeln!(
            html,
            r#"<tr><td class="num">{number}</td><td><strong>{name}</strong><div class="outcome">{outcome}</div></td><td class="bar-cell">{rows}</td><td class="num">{done}</td><td><span class="pill {}">{}</span></td></tr>"#,
            escape(status),
            escape(milestone_label(status)),
        );
    }
    html.push_str("</tbody></table></div>\n");

    // Trackers.
    html.push_str(
        "<h2>Trackers</h2>\n<div class=\"scroll\"><table class=\"overview\">\n<thead><tr><th>Tracker</th><th class=\"bar-cell\">Rows</th>",
    );
    for status in STATUSES.iter().rev() {
        let _ = write!(html, "<th class=\"num\">{}</th>", status_label(status));
    }
    html.push_str("</tr></thead>\n<tbody>\n");
    for (stem, title, items) in trackers {
        let mut counts = Counts::default();
        items.iter().for_each(|item| counts.add(&item.status));
        let _ = write!(
            html,
            r##"<tr><td><a href="#{stem}">{title}</a> <span class="muted">{stem}.md</span></td><td class="bar-cell">{}</td>"##,
            bar(&counts, "")
        );
        for status in STATUSES.iter().rev() {
            let _ = write!(html, "<td class=\"num\">{}</td>", counts.get(status));
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody></table></div>\n");

    // Every row, filterable.
    html.push_str(
        r#"<h2>All rows</h2>
<div class="filters">
<input id="search" type="search" placeholder="Filter by name or note" aria-label="Filter by name or note">
<select id="milestone" aria-label="Milestone"><option value="">All milestones</option>"#,
    );
    for (number, milestone) in milestones {
        let _ = write!(
            html,
            "<option value=\"{number}\">{number}. {}</option>",
            escape(&milestone.name)
        );
    }
    html.push_str("</select>\n<div class=\"toggles\">");
    for status in STATUSES.iter().rev() {
        let _ = write!(
            html,
            r#"<label><input type="checkbox" class="status" value="{status}" checked><i class="dot {status}"></i>{}</label>"#,
            status_label(status)
        );
    }
    html.push_str("</div>\n<span id=\"shown\" class=\"muted\"></span>\n</div>\n");

    for (stem, title, items) in trackers {
        let _ = write!(
            html,
            "<section class=\"tracker\" id=\"{stem}\">\n<h3>{title}</h3>\n"
        );
        let mut sections: Vec<(&str, Vec<&Item>)> = Vec::new();
        for item in items {
            match sections.last_mut() {
                Some((section, rows)) if *section == item.section => rows.push(item),
                _ => sections.push((&item.section, vec![item])),
            }
        }
        for (section, rows) in sections {
            let mut counts = Counts::default();
            rows.iter().for_each(|item| counts.add(&item.status));
            let _ = writeln!(
                html,
                r#"<details class="group"><summary><span class="group-name">{}</span>{}<span class="muted">{} / {} done</span></summary>
<div class="scroll"><table class="rows"><thead><tr><th>Name</th><th class="num">Milestone</th><th>Status</th><th>Notes</th></tr></thead><tbody>"#,
                escape(section),
                bar(&counts, "small"),
                counts.get("done"),
                counts.total()
            );
            for item in rows {
                let milestone = item.milestone.map_or(String::new(), |m| m.to_string());
                let search =
                    format!("{} {} {}", item.name, item.section, item.notes).to_lowercase();
                let _ = writeln!(
                    html,
                    r#"<tr data-status="{status}" data-milestone="{milestone}" data-search="{}"><td class="name">{}</td><td class="num">{milestone}</td><td><span class="pill {status}">{}</span></td><td class="notes">{}</td></tr>"#,
                    escape(&search),
                    inline_markdown(&item.label),
                    status_label(&item.status),
                    inline_markdown(&item.notes),
                    status = escape(&item.status),
                );
            }
            html.push_str("</tbody></table></div></details>\n");
        }
        html.push_str("</section>\n");
    }
    html.push_str(SCRIPT);
    html.push_str("</main>\n</body>\n</html>\n");
    html
}

fn status_label(status: &str) -> &str {
    match status {
        "done" => "Done",
        "partial" => "Partial",
        "parsed" => "Parsed",
        "todo" => "To do",
        other => other,
    }
}

/// A stacked bar of the counts, most complete first.
fn bar(counts: &Counts, class: &str) -> String {
    let mut out = format!(
        r#"<div class="bar {class}" role="img" aria-label="{}">"#,
        STATUSES
            .iter()
            .rev()
            .map(|s| format!("{} {}", counts.get(s), status_label(s).to_lowercase()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for status in STATUSES.iter().rev() {
        let width = percent(counts.get(status), counts.total());
        if width > 0.0 {
            let _ = write!(
                out,
                r#"<span class="{status}" style="width:{width:.2}%" title="{} {}"></span>"#,
                counts.get(status),
                status_label(status)
            );
        }
    }
    out.push_str("</div>");
    out
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Inline Markdown as HTML: `code`, **bold** and [links](url).
fn inline_markdown(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            let _ = write!(out, "<code>{}</code>", escape(&after[..end]));
            rest = &after[end + 1..];
        } else if let Some(after) = rest.strip_prefix("**")
            && let Some(end) = after.find("**")
        {
            let _ = write!(out, "<strong>{}</strong>", inline_markdown(&after[..end]));
            rest = &after[end + 2..];
        } else if let Some(after) = rest.strip_prefix('[')
            && let Some(close) = after.find("](")
            && let Some(end) = after[close + 2..].find(')')
        {
            let url = &after[close + 2..close + 2 + end];
            let _ = write!(
                out,
                r#"<a href="{}">{}</a>"#,
                escape(url),
                inline_markdown(&after[..close])
            );
            rest = &after[close + 2 + end + 1..];
        } else {
            let c = rest.chars().next().expect("not empty");
            out.push_str(&escape(&c.to_string()));
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

const LEGEND: &str = r#"<p class="legend"><span><i class="dot done"></i>Done: all values supported, with fixtures</span><span><i class="dot partial"></i>Partial: some forms supported; the notes list the gaps</span><span><i class="dot parsed"></i>Parsed: parsed and computed, no layout or paint yet</span><span><i class="dot todo"></i>To do: not started</span></p>
"#;

const HEAD: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rustyprint progress</title>
<style>
:root {
  --bg: #fbfbfa; --surface: #ffffff; --text: #1d1f23; --muted: #6b7079; --line: #e3e4e8;
  --done: #2f9e5b; --partial: #e0a224; --parsed: #4a7fd4; --todo: #d5d7dd;
  --code: #f1f2f4; --link: #2557b8;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #15171a; --surface: #1d2024; --text: #e7e8ea; --muted: #9a9fa8; --line: #31353b;
    --done: #3fb771; --partial: #e8b13f; --parsed: #6b9be6; --todo: #454a52;
    --code: #2a2e34; --link: #8fb3f5;
  }
}
* { box-sizing: border-box }
body { margin: 0; background: var(--bg); color: var(--text);
  font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif }
main { max-width: 1100px; margin: 0 auto; padding: 32px 16px 64px }
h1 { font-size: 28px; margin: 0 0 4px; letter-spacing: -0.01em }
h2 { font-size: 19px; margin: 40px 0 12px }
h3 { font-size: 16px; margin: 28px 0 8px }
.lede, .muted, .outcome { color: var(--muted) }
.next { margin: 8px 0 0; font-size: 15px }
.outcome { font-size: 13px; margin-top: 2px }
code { background: var(--code); border-radius: 4px; padding: 0 4px; font-size: 0.9em;
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace }
a { color: var(--link); text-decoration: none }
a:hover { text-decoration: underline }
.summary { display: flex; flex-wrap: wrap; gap: 12px; margin: 24px 0 12px }
.summary > div { background: var(--surface); border: 1px solid var(--line); border-radius: 10px;
  padding: 12px 16px; min-width: 120px; display: flex; flex-direction: column }
.summary .big { min-width: 180px }
.summary .value { font-size: 26px; font-weight: 650; font-variant-numeric: tabular-nums }
.summary .big .value { font-size: 34px }
.summary .label { color: var(--muted); font-size: 13px; display: flex; align-items: center; gap: 6px }
.bar { display: flex; height: 10px; border-radius: 5px; overflow: hidden; background: var(--todo);
  min-width: 120px }
.bar.wide { height: 14px; border-radius: 7px; margin-bottom: 8px }
.bar.small { width: 120px; height: 6px; flex: none }
.bar span { display: block; height: 100% }
.done { background: var(--done) } .partial { background: var(--partial) }
.parsed { background: var(--parsed) } .todo { background: var(--todo) }
.dot { display: inline-block; width: 9px; height: 9px; border-radius: 50% }
.legend { display: flex; flex-wrap: wrap; gap: 6px 18px; color: var(--muted); font-size: 13px; margin: 0 }
.legend span { display: inline-flex; align-items: center; gap: 6px }
.scroll { overflow-x: auto }
table { width: 100%; border-collapse: collapse; background: var(--surface);
  border: 1px solid var(--line); border-radius: 10px; overflow: hidden }
th, td { padding: 8px 12px; text-align: left; vertical-align: top; border-bottom: 1px solid var(--line) }
th { font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted);
  font-weight: 600 }
tbody tr:last-child td { border-bottom: 0 }
.num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap }
.bar-cell { width: 32%; vertical-align: middle }
.pill { display: inline-block; padding: 1px 8px; border-radius: 999px; font-size: 12px;
  font-weight: 600; white-space: nowrap; color: #fff }
.pill.todo, .pill.none { color: var(--text) }
.pill.none { background: transparent; border: 1px solid var(--line); font-weight: 500 }
.pill.partial { color: #2b2005 }
.filters { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 16px; margin-bottom: 12px;
  position: sticky; top: 0; background: var(--bg); padding: 10px 0; z-index: 1 }
.filters input[type=search], .filters select { font: inherit; padding: 6px 10px; border-radius: 8px;
  border: 1px solid var(--line); background: var(--surface); color: var(--text) }
.filters input[type=search] { flex: 1 1 220px }
.toggles { display: flex; flex-wrap: wrap; gap: 4px 14px }
.toggles label { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; font-size: 14px }
details.group { margin: 6px 0; background: var(--surface); border: 1px solid var(--line); border-radius: 10px }
details.group summary { cursor: pointer; padding: 8px 12px; display: flex; align-items: center;
  gap: 12px; flex-wrap: wrap }
.group-name { font-weight: 600; flex: 1 1 200px }
details.group table { border: 0; border-top: 1px solid var(--line); border-radius: 0 }
.rows .name { white-space: nowrap }
.rows .notes { font-size: 13px; color: var(--muted); min-width: 240px }
@media (max-width: 640px) {
  .bar-cell { width: auto }
  .rows .name { white-space: normal }
}
</style>
</head>
<body>
<main>
"#;

const SCRIPT: &str = r#"<script>
(function () {
  const search = document.getElementById("search");
  const milestone = document.getElementById("milestone");
  const statuses = Array.from(document.querySelectorAll("input.status"));
  const rows = Array.from(document.querySelectorAll("table.rows tbody tr"));
  const shown = document.getElementById("shown");
  function update() {
    const query = search.value.trim().toLowerCase();
    const wanted = new Set(statuses.filter((s) => s.checked).map((s) => s.value));
    const filtering = query !== "" || milestone.value !== "" || wanted.size < statuses.length;
    let count = 0;
    for (const row of rows) {
      const visible = wanted.has(row.dataset.status)
        && (milestone.value === "" || row.dataset.milestone === milestone.value)
        && (query === "" || row.dataset.search.includes(query));
      row.hidden = !visible;
      if (visible) count++;
    }
    for (const group of document.querySelectorAll("details.group")) {
      const any = group.querySelector("tbody tr:not([hidden])") !== null;
      group.hidden = !any;
      if (filtering && any) group.open = true;
    }
    for (const tracker of document.querySelectorAll("section.tracker")) {
      tracker.hidden = tracker.querySelector("details.group:not([hidden])") === null;
    }
    shown.textContent = count + " of " + rows.length + " rows";
  }
  search.addEventListener("input", update);
  milestone.addEventListener("change", update);
  statuses.forEach((s) => s.addEventListener("change", update));
  update();
})();
</script>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_inline_markdown() {
        assert_eq!(
            inline_markdown("[`p`](https://x?a&b) and **bold** <x>"),
            r#"<a href="https://x?a&amp;b"><code>p</code></a> and <strong>bold</strong> &lt;x&gt;"#
        );
        assert_eq!(inline_markdown("a `b"), "a `b");
    }

    #[test]
    fn reads_milestones_from_the_plan() {
        let plan = "## 8. Milestones\n\n| # | Milestone | Status | Outcome |\n\
                    | --- | --- | --- | --- |\n| 1 | **Hello PDF** | done | Workspace. |\n\n\
                    ## 9. Open questions\n| 2 | x | todo | y |\n";
        let milestones = milestones(plan);
        assert_eq!(milestones.len(), 1);
        assert_eq!(milestones[&1].name, "Hello PDF");
        assert_eq!(milestones[&1].status, "done");
        assert_eq!(milestones[&1].outcome, "Workspace.");
    }
}
