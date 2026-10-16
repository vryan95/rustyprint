//! Render times for four documents: a one-page letter, a 20-page report, an invoice with a
//! 200-row table and a book of 3,000 paragraphs, whose body has thousands of children.
//!
//! ```text
//! cargo bench -p rustyprint --bench render
//! ```

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use rustyprint::{RenderOptions, render};

const STYLE: &str = "<style>
body { font: 11pt/1.4 sans-serif }
h1, h2 { font-family: serif }
table { border-collapse: collapse; width: 100% }
th, td { border-bottom: 1px solid #ccc; padding: 2pt 4pt; text-align: left }
td.amount, th.amount { text-align: right }
@page { @bottom-center { content: counter(page) ' of ' counter(pages) } }
</style>";

const PARAGRAPH: &str = "The quick brown fox jumps over the lazy dog, and then it jumps back \
    again, because the dog never minded. Pack my box with five dozen liquor jugs; how vexingly \
    quick daft zebras jump. ";

fn letter() -> String {
    let body = (0..5)
        .map(|_| format!("<p>{}</p>", PARAGRAPH.repeat(2)))
        .collect::<String>();
    format!(
        "{STYLE}<address>Example Ltd<br>1 High Street<br>Springfield</address>\
         <p>12 October 2026</p><p>Dear customer,</p>{body}<p>Yours sincerely,</p><p>A. Writer</p>"
    )
}

fn report() -> String {
    let mut html = format!("{STYLE}<h1>Annual report</h1>");
    for chapter in 1..=14 {
        html.push_str(&format!("<h2>Chapter {chapter}</h2>"));
        for _ in 0..8 {
            html.push_str(&format!("<p>{}</p>", PARAGRAPH.repeat(3)));
        }
        html.push_str("<ul>");
        for item in 1..=5 {
            html.push_str(&format!("<li>Point {item} of chapter {chapter}</li>"));
        }
        html.push_str("</ul>");
    }
    html
}

fn invoice() -> String {
    let rows = (1..=200)
        .map(|i| {
            format!(
                "<tr><td>{i}</td><td>Item number {i}, standard size</td><td class=amount>{}</td>\
                 <td class=amount>{}.00</td></tr>",
                i % 7 + 1,
                (i % 7 + 1) * 12
            )
        })
        .collect::<String>();
    format!(
        "{STYLE}<h1>Invoice 42</h1><p>Example Ltd, 1 High Street, Springfield</p>\
         <table><thead><tr><th>#</th><th>Description</th><th class=amount>Qty</th>\
         <th class=amount>Total</th></tr></thead><tbody>{rows}</tbody></table>"
    )
}

fn book() -> String {
    // Every fourth word emphasised, so the body's children lie apart in the DOM, as in real
    // documents.
    let paragraph = PARAGRAPH
        .split(' ')
        .enumerate()
        .map(|(i, word)| match i % 4 {
            1 => format!("<em>{word}</em>"),
            _ => word.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ");
    let body = (0..3000)
        .map(|_| format!("<p>{paragraph}</p>\n"))
        .collect::<String>();
    format!("{STYLE}<h1>A long book</h1>\n{body}")
}

fn pages(html: &str) -> usize {
    let pdf = render(html, &RenderOptions::default())
        .expect("renders")
        .pdf;
    lopdf::Document::load_mem(&pdf)
        .expect("valid PDF")
        .get_pages()
        .len()
}

fn benches(c: &mut Criterion) {
    let documents = [
        ("letter", letter()),
        ("report", report()),
        ("invoice", invoice()),
        ("book", book()),
    ];
    let options = RenderOptions::default();
    for (name, html) in &documents {
        eprintln!("{name}: {} pages", pages(html));
        c.bench_function(name, |b| {
            b.iter(|| render(black_box(html), &options).expect("renders"))
        });
    }
}

criterion_group! {
    name = render_benches;
    // Rendering takes milliseconds, so fewer samples still give a stable estimate.
    config = Criterion::default().sample_size(20);
    targets = benches
}
criterion_main!(render_benches);
