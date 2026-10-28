//! Chinese and Arabic text in the PDF, with the bundled fonts: copied text gives the
//! original characters.

use rustyprint::{RenderOptions, render};

/// The text of the PDF's first page in content order, as a viewer copies it: a marked
/// content span's `/ActualText` stands for its glyphs, and other glyphs give the text of
/// their font's ToUnicode map.
fn copied_text(body: &str) -> String {
    use lopdf::content::Content;
    use lopdf::{Document, Object};

    let html = format!("<!DOCTYPE html><meta charset=utf-8><body>{body}");
    let output = render(&html, &RenderOptions::default()).expect("renders");
    assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    let pdf = Document::load_mem(&output.pdf).expect("valid PDF");
    let page = pdf.get_pages()[&1];
    let fonts = pdf.get_page_fonts(page).expect("page fonts");
    let content = Content::decode(&pdf.get_page_content(page)).expect("decodes");

    let utf16 = |bytes: &[u8]| {
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    };
    // CID to text, from each font's ToUnicode CMap of `<cid> <utf16>` pairs.
    let to_unicode = |font: &[u8]| -> Vec<(u16, String)> {
        let dict = fonts[font];
        let stream = pdf
            .get_object(dict.get(b"ToUnicode").unwrap().as_reference().unwrap())
            .unwrap()
            .as_stream()
            .unwrap();
        let cmap = String::from_utf8(stream.decompressed_content().unwrap()).unwrap();
        let hex = |h: &str| -> Vec<u8> {
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                .collect()
        };
        let mut pairs = Vec::new();
        for line in cmap.lines() {
            let parts: Vec<&str> = line
                .split(['<', '>', ' '])
                .filter(|p| !p.is_empty())
                .collect();
            if let [cid, text] = parts[..]
                && cid.len() == 4
                && cid.bytes().all(|b| b.is_ascii_hexdigit())
            {
                let cid = u16::from_str_radix(cid, 16).unwrap();
                pairs.push((cid, utf16(&hex(text))));
            }
        }
        pairs
    };

    let mut text = String::new();
    let mut map = Vec::new();
    let mut actual_text = 0;
    for op in &content.operations {
        match op.operator.as_str() {
            "Tf" => map = to_unicode(op.operands[0].as_name().unwrap()),
            "BDC" => {
                let spans = |o: &Object| o.as_dict().ok()?.get(b"ActualText").ok().cloned();
                if let Some(Object::String(bytes, _)) = op.operands.get(1).and_then(spans) {
                    text += &utf16(&bytes[2..]);
                    actual_text += 1;
                } else if actual_text > 0 {
                    actual_text += 1;
                }
            }
            "EMC" if actual_text > 0 => actual_text -= 1,
            "TJ" | "Tj" if actual_text == 0 => {
                let strings: Vec<&Object> = match &op.operands[0] {
                    Object::Array(items) => items.iter().collect(),
                    other => vec![other],
                };
                for string in strings {
                    if let Object::String(bytes, _) = string {
                        for pair in bytes.chunks(2) {
                            let cid = u16::from_be_bytes([pair[0], pair[1]]);
                            if let Some((_, t)) = map.iter().find(|(c, _)| *c == cid) {
                                text += t;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    text
}

/// The base fonts the PDF embeds.
fn embedded_fonts(body: &str) -> Vec<String> {
    let html = format!("<!DOCTYPE html><meta charset=utf-8><body>{body}");
    let output = render(&html, &RenderOptions::default()).expect("renders");
    let pdf = lopdf::Document::load_mem(&output.pdf).expect("valid PDF");
    pdf.objects
        .values()
        .filter_map(|object| {
            let dict = object.as_dict().ok()?;
            if dict.get(b"Type").ok()?.as_name().ok()? != b"FontDescriptor" {
                return None;
            }
            let name = dict.get(b"FontName").ok()?.as_name().ok()?;
            Some(String::from_utf8_lossy(name).into_owned())
        })
        .collect()
}

#[cfg(feature = "cjk")]
#[test]
fn chinese_copies_as_written() {
    let text = "简体中文排版，汉字与标点。";
    assert_eq!(copied_text(&format!("<p>{text}</p>")), text);
    let fonts = embedded_fonts(&format!("<p>{text}</p>"));
    assert!(fonts.iter().any(|f| f.contains("NotoSansSC")), "{fonts:?}");
}

#[test]
fn arabic_copies_as_written() {
    let text = "مرحبا بالعالم";
    // Glyphs are in visual order, so right-to-left words copy reversed, as from other PDF
    // generators; viewers reorder them. The letters must be the originals, not presentation
    // forms, with nothing left over from the dots drawn as glyphs of their own.
    let copied = copied_text(&format!("<p dir=rtl>{text}</p>"));
    let reversed: String = text.chars().rev().collect();
    assert_eq!(copied, reversed);
    let fonts = embedded_fonts(&format!("<p>{text}</p>"));
    assert!(
        fonts.iter().any(|f| f.contains("NotoNaskhArabic")),
        "{fonts:?}"
    );
}

#[test]
fn latin_documents_embed_no_script_fonts() {
    let fonts = embedded_fonts("<p>Hello world</p>");
    assert!(
        fonts
            .iter()
            .all(|f| !f.contains("Arabic") && !f.contains("NotoSansSC")),
        "{fonts:?}"
    );
}
