//! Embedding fonts as subset CID-keyed TrueType fonts.

use std::collections::BTreeMap;

use miniz_oxide::deflate::compress_to_vec_zlib;
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str};
use rustyprint_layout::Font;
use subsetter::GlyphRemapper;

use crate::{COMPRESSION_LEVEL, Error, Refs};

const IDENTITY: SystemInfo<'static> = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

/// A font used in the document, and the glyphs used from it.
pub(crate) struct FontResource {
    pub font: Font,
    /// Resource name used in content streams, such as `F0`.
    pub name: String,
    pub type0_id: Ref,
    /// Maps glyph IDs in the font to CIDs, which are glyph IDs in the subset.
    remapper: GlyphRemapper,
    /// Text each CID represents, for copy and paste and search.
    to_unicode: BTreeMap<u16, String>,
}

impl FontResource {
    pub fn new(font: Font, name: String, type0_id: Ref) -> Self {
        Self {
            font,
            name,
            type0_id,
            remapper: GlyphRemapper::new(),
            to_unicode: BTreeMap::new(),
        }
    }

    /// Records that `glyph` is used to show `text` and returns its CID.
    pub fn use_glyph(&mut self, glyph: u16, text: &str) -> u16 {
        let cid = self.remapper.remap(glyph);
        if !text.is_empty() {
            self.to_unicode
                .entry(cid)
                .or_insert_with(|| text.to_owned());
        }
        cid
    }

    /// Advance width of `glyph` in thousandths of an em.
    pub fn width(&self, glyph: u16) -> f32 {
        let advance = self.font.advance(glyph).unwrap_or(0);
        f32::from(advance) * 1000.0 / f32::from(self.font.units_per_em())
    }

    /// Writes the font objects.
    pub fn write(&self, pdf: &mut Pdf, refs: &mut Refs) -> Result<(), Error> {
        let cid_font_id = refs.next();
        let descriptor_id = refs.next();
        let file_id = refs.next();
        let cmap_id = refs.next();

        // A variable font is embedded as the instance it is used at.
        let variations: Vec<(subsetter::Tag, f32)> = self
            .font
            .instance()
            .variations
            .iter()
            .map(|(tag, value)| (subsetter::Tag::new(tag), *value))
            .collect();
        let subset = if variations.is_empty() {
            subsetter::subset(self.font.data(), self.font.index(), &self.remapper)
        } else {
            subsetter::subset_with_variations(
                self.font.data(),
                self.font.index(),
                &variations,
                &self.remapper,
            )
        }
        .map_err(|source| Error::Subset {
            name: self.font.postscript_name().to_owned(),
            source,
        })?;
        // Fonts with CFF outlines are embedded as OpenType, TrueType ones as they are.
        let cff = subset.starts_with(b"OTTO");
        let base_font = format!("{}+{}", self.subset_tag(), self.font.postscript_name());
        let base_font = Name(base_font.as_bytes());

        pdf.type0_font(self.type0_id)
            .base_font(base_font)
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid_font_id)
            .to_unicode(cmap_id);

        let mut cid_font = pdf.cid_font(cid_font_id);
        cid_font
            .subtype(if cff {
                CidFontType::Type0
            } else {
                CidFontType::Type2
            })
            .base_font(base_font)
            .system_info(IDENTITY)
            .font_descriptor(descriptor_id);
        if !cff {
            cid_font.cid_to_gid_map_predefined(Name(b"Identity"));
        }
        let widths: Vec<f32> = self
            .remapper
            .remapped_gids()
            .map(|glyph| self.width(glyph))
            .collect();
        cid_font.widths().consecutive(0, widths);
        cid_font.finish();

        let face = self.font.descriptor();
        let to_em = |value: i16| f32::from(value) * 1000.0 / f32::from(self.font.units_per_em());
        let (x_min, y_min, x_max, y_max) = face.bbox;
        let weight = face.weight;
        let mut flags = FontFlags::SYMBOLIC;
        flags.set(FontFlags::ITALIC, face.italic);
        flags.set(FontFlags::FIXED_PITCH, face.monospaced);
        let mut descriptor = pdf.font_descriptor(descriptor_id);
        descriptor
            .name(base_font)
            .flags(flags)
            .bbox(Rect::new(
                to_em(x_min),
                to_em(y_min),
                to_em(x_max),
                to_em(y_max),
            ))
            .italic_angle(face.italic_angle)
            .ascent(to_em(face.ascender))
            .descent(to_em(face.descender))
            .cap_height(to_em(face.cap_height))
            // PDF requires a stem width, which fonts don't record; estimate it from the weight.
            .stem_v(10.0 + 0.244 * (f32::from(weight) - 50.0));
        if cff {
            descriptor.font_file3(file_id);
        } else {
            descriptor.font_file2(file_id);
        }
        descriptor.finish();

        let data = compress_to_vec_zlib(&subset, COMPRESSION_LEVEL);
        let mut stream = pdf.stream(file_id, &data);
        stream.filter(Filter::FlateDecode);
        if cff {
            stream.pair(Name(b"Subtype"), Name(b"OpenType"));
        }
        stream.finish();

        let mut cmap = UnicodeCmap::new(Name(b"Custom"), IDENTITY);
        for (&cid, text) in &self.to_unicode {
            cmap.pair_with_multiple(cid, text.chars());
        }
        pdf.cmap(cmap_id, &cmap.finish());
        Ok(())
    }

    /// Six uppercase letters identifying the subset, derived from its glyphs so that output
    /// stays deterministic.
    fn subset_tag(&self) -> String {
        // FNV-1a over the glyph IDs.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut add = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        };
        for glyph in self.remapper.remapped_gids() {
            add(&glyph.to_be_bytes());
        }
        // Instances of a variable font are different fonts.
        for (tag, value) in &self.font.instance().variations {
            add(tag);
            add(&value.to_be_bytes());
        }
        (0..6)
            .map(|_| {
                let letter = b'A' + (hash % 26) as u8;
                hash /= 26;
                char::from(letter)
            })
            .collect()
    }
}
