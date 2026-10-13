//! Renders an HTML file several times in one process and reports how long each render took.
//!
//! ```text
//! cargo run --release -p rustyprint --example bench -- report.html 500 [out.pdf]
//! ```
//!
//! Relative URLs resolve against the file's directory; nothing is fetched from the network.

use std::time::Instant;

use rustyprint::{Assets, RenderOptions, render};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: bench <file.html> [runs] [out.pdf]");
    let runs: usize = args
        .next()
        .map_or(1, |n| n.parse().expect("runs is a number"));
    let out = args.next();
    let html = std::fs::read_to_string(&path).expect("readable HTML file");
    let directory = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let options = RenderOptions {
        assets: Assets::directory(directory),
        ..RenderOptions::default()
    };
    let mut times = Vec::with_capacity(runs);
    let mut last = None;
    let start = Instant::now();
    for _ in 0..runs {
        let t = Instant::now();
        let output = render(&html, &options).expect("render succeeds");
        times.push(t.elapsed().as_secs_f64());
        last = Some(output);
    }
    let total = start.elapsed().as_secs_f64();
    let output = last.expect("at least one run");
    let pages = lopdf::Document::load_mem(&output.pdf)
        .map(|d| d.get_pages().len())
        .unwrap_or(0);
    if let Some(out) = out {
        std::fs::write(out, &output.pdf).expect("writable output");
    }
    times.sort_by(f64::total_cmp);
    println!(
        "rustyprint runs={runs} pages={pages} bytes={} total={total:.3}s mean={:.4}s median={:.4}s min={:.4}s max={:.4}s warnings={}",
        output.pdf.len(),
        total / runs as f64,
        times[times.len() / 2],
        times[0],
        times[times.len() - 1],
        output.warnings.len()
    );
}
