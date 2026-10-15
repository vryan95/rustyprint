use std::ops::Range;

use harfrust::{Buffer, Direction, Language, ShapeOptions as Options, ShaperFont, Tag};

use crate::Font;

/// A shaped glyph. Lengths are in CSS pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    /// Glyph ID in the font.
    pub id: u16,
    /// Distance from this glyph's origin to the next one's.
    pub x_advance: f32,
    /// Horizontal offset of the glyph from its origin.
    pub x_offset: f32,
    /// Vertical offset of the glyph from the baseline, positive downwards.
    pub y_offset: f32,
    /// Byte range of the source text this glyph represents. Glyphs that share a cluster
    /// (for example a base letter and its mark) all point at the same range.
    pub text: Range<usize>,
}

/// An OpenType feature turned on (1), off (0) or set to another value for the whole text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    pub tag: [u8; 4],
    pub value: u32,
}

impl Feature {
    pub fn new(tag: &[u8; 4], value: u32) -> Self {
        Self { tag: *tag, value }
    }
}

/// How to shape text.
#[derive(Debug, Clone, Default)]
pub struct ShapeOptions<'a> {
    /// Right-to-left text, whose glyphs come out in visual order (last character first).
    pub rtl: bool,
    /// A BCP 47 language tag, which selects language-specific forms.
    pub language: Option<&'a str>,
    /// OpenType features to turn on or off.
    pub features: &'a [Feature],
}

/// Shapes `text` with `font` at `size` CSS pixels per em.
pub fn shape(font: &Font, size: f32, text: &str) -> Vec<Glyph> {
    shape_range(font, size, text, 0..text.len(), &ShapeOptions::default())
}

/// Shapes `text[range]`, using the text around it as context. Glyph text ranges are offsets
/// into `text`.
pub fn shape_range(
    font: &Font,
    size: f32,
    text: &str,
    range: Range<usize>,
    options: &ShapeOptions,
) -> Vec<Glyph> {
    let shaper = ShaperFont::new(font.shaping());
    let mut buffer = Buffer::new();
    buffer.push_str(&text[range.clone()]);
    // HarfBuzz looks at up to five characters on each side.
    let before = &text[..range.start];
    let pre = before
        .char_indices()
        .rev()
        .nth(4)
        .map_or(before, |(i, _)| &before[i..]);
    let after = &text[range.end..];
    let post = after
        .char_indices()
        .nth(5)
        .map_or(after, |(i, _)| &after[..i]);
    buffer.set_pre_context(pre);
    buffer.set_post_context(post);
    buffer.set_direction(if options.rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    if let Some(language) = options.language.and_then(|l| l.parse::<Language>().ok()) {
        buffer.set_language(Some(language));
    }
    buffer.guess_segment_properties();
    let features: Vec<harfrust::Feature> = options
        .features
        .iter()
        .map(|f| harfrust::Feature::new(Tag::new(&f.tag), f.value, ..))
        .collect();
    let plan = font.shape_plan(
        buffer.direction(),
        buffer.script(),
        buffer.language(),
        &features,
    );
    let options = Options::new().plan(Some(&plan)).features(&features);
    harfrust::shape(&shaper, &mut buffer, options)
        .expect("the buffer has a direction and is not shaped yet");
    let output = buffer;

    // A cluster's text runs up to the next larger cluster value.
    let mut bounds: Vec<usize> = output
        .glyph_infos()
        .iter()
        .map(|info| info.cluster as usize)
        .collect();
    bounds.sort_unstable();
    bounds.dedup();
    bounds.push(range.len());

    let scale = font.em_scale(size);
    output
        .glyph_infos()
        .iter()
        .zip(output.glyph_positions())
        .map(|(info, pos)| {
            let start = info.cluster as usize;
            let end = bounds[bounds.partition_point(|&b| b <= start)];
            Glyph {
                id: u16::try_from(info.glyph_id).expect("glyph IDs fit in u16"),
                x_advance: pos.x_advance as f32 * scale,
                x_offset: pos.x_offset as f32 * scale,
                y_offset: -(pos.y_offset as f32) * scale,
                text: range.start + start..range.start + end,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_cover_the_text() {
        let text = "Hello";
        let glyphs = shape(&Font::bundled(), 16.0, text);
        assert_eq!(glyphs.len(), 5);
        let covered: String = glyphs.iter().map(|g| &text[g.text.clone()]).collect();
        assert_eq!(covered, text);
        assert!(glyphs.iter().all(|g| g.x_advance > 0.0));
    }

    #[test]
    fn ligature_covers_its_letters() {
        let text = "office";
        let glyphs = shape(&Font::bundled(), 16.0, text);
        let covered: String = glyphs.iter().map(|g| &text[g.text.clone()]).collect();
        assert_eq!(covered, text);
    }

    #[test]
    fn advances_scale_with_size() {
        let small: f32 = shape(&Font::bundled(), 10.0, "Hi")
            .iter()
            .map(|g| g.x_advance)
            .sum();
        let large: f32 = shape(&Font::bundled(), 20.0, "Hi")
            .iter()
            .map(|g| g.x_advance)
            .sum();
        assert!((large - 2.0 * small).abs() < 1e-3);
    }

    #[test]
    fn ranges_are_absolute_and_rtl_is_visual() {
        let text = "ab cd";
        let options = ShapeOptions {
            rtl: true,
            ..Default::default()
        };
        let glyphs = shape_range(&Font::bundled(), 16.0, text, 3..5, &options);
        assert_eq!(glyphs.len(), 2);
        assert_eq!(glyphs[0].text, 4..5);
        assert_eq!(glyphs[1].text, 3..4);
    }

    #[test]
    fn features_switch_ligatures_off() {
        let off = [Feature::new(b"liga", 0)];
        let options = ShapeOptions {
            features: &off,
            ..Default::default()
        };
        let font = Font::bundled();
        let with = shape(&font, 16.0, "ffi").len();
        let without = shape_range(&font, 16.0, "ffi", 0..3, &options).len();
        assert!(without >= with);
        assert_eq!(without, 3);
    }
}
