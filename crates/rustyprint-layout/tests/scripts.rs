//! Text in scripts the Latin families lack, set in the bundled fallback fonts without any
//! `@font-face`.

use rustyprint_css::{StyleOptions, compute_styles};
use rustyprint_dom::Document;
use rustyprint_layout::display::TextRun;
use rustyprint_layout::{
    DisplayItem, Images, PageOptions, Sides, Size, layout_tree, paint_fragment,
};

/// The text runs `body` paints on an 800 × 1000 page area.
fn runs(body: &str) -> Vec<TextRun> {
    let html = format!("<!DOCTYPE html><style>body {{ margin: 0 }}</style><body>{body}");
    let document = Document::parse(&html);
    let load = |_: &str| None;
    let styles = compute_styles(
        &document,
        &StyleOptions {
            page_size: (800.0, 1000.0),
            stylesheet: None,
            load: &load,
            font_metrics: None,
            load_font: None,
        },
    );
    let page = PageOptions {
        size: Size::new(800.0, 1000.0),
        margin: Sides::default(),
    };
    let root = layout_tree(&document, &styles, &page, &Images::none()).unwrap();
    paint_fragment(&root, &Images::none())
        .into_iter()
        .filter_map(|item| match item {
            DisplayItem::Text(run) => Some(run),
            _ => None,
        })
        .collect()
}

/// The PostScript name of each run's font, without its style suffix.
fn families(runs: &[TextRun]) -> Vec<String> {
    runs.iter()
        .map(|run| {
            let name = run.font.postscript_name();
            name.split('-').next().unwrap_or(name).to_owned()
        })
        .collect()
}

#[track_caller]
fn assert_no_missing_glyphs(runs: &[TextRun]) {
    assert!(!runs.is_empty(), "nothing was painted");
    for run in runs {
        for glyph in &run.glyphs {
            assert_ne!(
                glyph.id,
                0,
                "`{}` has no glyph in {}",
                &run.text[glyph.text.clone()],
                run.font.postscript_name()
            );
        }
    }
}

#[cfg(feature = "cjk")]
#[test]
fn simplified_chinese_has_every_glyph() {
    let runs = runs("<p>简体中文排版：汉字、标点和数字。</p>");
    assert_no_missing_glyphs(&runs);
    assert!(families(&runs).iter().all(|f| f == "NotoSansSC"));
}

#[cfg(feature = "cjk")]
#[test]
fn chinese_takes_the_weight_asked_for() {
    let regular = runs("<p>中文</p>");
    let bold = runs("<p><b>中文</b></p>");
    // The variable font's default instance is Thin, so the weight is always set.
    assert_eq!(regular[0].font.variation(b"wght"), Some(400.0));
    assert_eq!(bold[0].font.variation(b"wght"), Some(700.0));
}

#[cfg(feature = "cjk")]
#[test]
fn vertical_chinese_has_every_glyph() {
    let runs = runs("<p style='writing-mode: vertical-rl; height: 300px'>竖排中文，「标点」。</p>");
    assert_no_missing_glyphs(&runs);
    assert!(families(&runs).iter().all(|f| f == "NotoSansSC"));
}

#[test]
fn arabic_letters_join() {
    // The same letter three times: final, medial and initial forms in visual order.
    let joined = letters("<p dir=rtl>ببب</p>");
    let isolated = letters("<p dir=rtl>ب</p>");
    assert_eq!(joined.len(), 3);
    assert!(
        joined[0] != joined[1] && joined[1] != joined[2] && joined[0] != joined[2],
        "{joined:?}"
    );
    assert!(!joined.contains(&isolated[0]), "{joined:?} {isolated:?}");
}

/// The glyphs of each letter of the first run, in visual order: Noto Naskh Arabic draws a
/// letter's dots as a glyph of their own.
fn letters(body: &str) -> Vec<Vec<u16>> {
    let runs = runs(body);
    assert_no_missing_glyphs(&runs);
    let mut letters: Vec<(usize, Vec<u16>)> = Vec::new();
    for glyph in &runs[0].glyphs {
        match letters.last_mut() {
            Some((start, ids)) if *start == glyph.text.start => ids.push(glyph.id),
            _ => letters.push((glyph.text.start, vec![glyph.id])),
        }
    }
    letters.into_iter().map(|(_, ids)| ids).collect()
}

#[test]
fn arabic_runs_right_to_left() {
    let runs = runs("<p dir=rtl>مرحبا</p>");
    assert_no_missing_glyphs(&runs);
    // Glyphs come in visual order, so the first letter is drawn last.
    let glyphs = &runs[0].glyphs;
    assert!(glyphs.first().unwrap().text.start > glyphs.last().unwrap().text.start);
}

#[test]
fn arabic_follows_the_generic_family() {
    assert_eq!(families(&runs("<p>مرحبا</p>")), ["NotoNaskhArabic"]);
    assert_eq!(
        families(&runs("<p style='font-family: sans-serif'>مرحبا</p>")),
        ["NotoSansArabic"]
    );
}

#[test]
fn mixed_arabic_and_english_reorder() {
    let runs = runs("<p>Hello مرحبا world</p>");
    assert_no_missing_glyphs(&runs);
    let arabic = runs
        .iter()
        .find(|r| r.font.postscript_name().starts_with("NotoNaskhArabic"))
        .expect("an Arabic run");
    let latin: Vec<&TextRun> = runs
        .iter()
        .filter(|r| r.font.postscript_name().starts_with("NotoSerif"))
        .collect();
    let hello = latin
        .iter()
        .find(|r| &r.text[r.glyphs[0].text.clone()] == "H")
        .unwrap();
    // The Arabic word sits between the English ones, its letters right to left.
    assert!(hello.origin.x < arabic.origin.x);
    assert!(latin.iter().any(|r| r.origin.x > arabic.origin.x));
    let glyphs = &arabic.glyphs;
    assert!(glyphs.first().unwrap().text.start > glyphs.last().unwrap().text.start);
}

#[test]
fn latin_text_keeps_the_latin_families() {
    assert_eq!(families(&runs("<p>Plain text</p>")), ["NotoSerif"]);
}

#[test]
fn script_fonts_are_named_families() {
    let runs = runs("<p style='font-family: \"Noto Sans Arabic\"'>abc</p>");
    // Noto Sans Arabic has Latin letters of its own.
    assert_eq!(families(&runs), ["NotoSansArabic"]);
    let runs = runs_with_face("local('Noto Naskh Arabic')", "مرحبا");
    assert_eq!(families(&runs), ["NotoNaskhArabic"]);
}

/// The runs of `text` in an `@font-face` family whose source is `src`.
fn runs_with_face(src: &str, text: &str) -> Vec<TextRun> {
    runs(&format!(
        "<style>@font-face {{ font-family: f; src: {src} }}</style>\
         <p style='font-family: f, monospace'>{text}</p>"
    ))
}
