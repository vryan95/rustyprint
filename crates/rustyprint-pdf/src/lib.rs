//! Writes display lists to PDF: fonts, images, links and effects.
//!
//! Output is deterministic: the same document always produces the same bytes.

mod font;

use std::sync::Arc;

use miniz_oxide::deflate::compress_to_vec_zlib;
use pdf_writer::types::{
    ActionType, AnnotationType, BlendMode as PdfBlendMode, FunctionShadingType, LineCapStyle,
    LineJoinStyle, MaskType, TextRenderingMode,
};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use rustyprint_layout::display::Group;
use rustyprint_layout::display::LineStyle;
use rustyprint_layout::display::PrintMarks;
use rustyprint_layout::display::{
    Bookmark, Destination, Document, LinkTarget, Page, Shading, ShadingKind,
};
use rustyprint_layout::{
    Color, DisplayItem, FillRule, Font, Image, ImageData, Path, PathSegment, Point, TextRun,
};

use crate::font::FontResource;

/// CSS pixels to PDF points.
const PX_TO_PT: f32 = 0.75;

/// Room for printer's marks outside the bleed, in points.
const MARKS_AREA: f32 = 24.0;

/// Zlib compression level for streams.
const COMPRESSION_LEVEL: u8 = 6;

/// The most threads that compress content streams.
const MAX_COMPRESSION_THREADS: usize = 8;

/// Pages whose content streams wait for compression, per compressing thread.
const PAGES_PER_THREAD: usize = 4;

/// Errors returned by [`write`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to subset font {name}: {source}")]
    Subset {
        name: String,
        source: subsetter::Error,
    },
}

/// Writes `document` as a PDF file.
pub fn write(document: &Document) -> Result<Vec<u8>, Error> {
    let mut writer = Writer {
        pdf: Pdf::new(),
        refs: Refs::default(),
        fonts: Vec::new(),
        alphas: Vec::new(),
        images: Vec::new(),
        shadings: 0,
        groups: 0,
        palette: Palette::default(),
    };
    writer.write(document)?;
    Ok(writer.pdf.finish())
}

struct Writer {
    pdf: Pdf,
    refs: Refs,
    /// Fonts in order of first use, so object numbering is deterministic.
    fonts: Vec<FontResource>,
    /// Graphics states for each alpha value used, by alpha.
    alphas: Vec<(u8, Ref)>,
    /// Images in order of first use.
    images: Vec<ImageResource>,
    /// Shadings written so far, for unique resource names.
    shadings: usize,
    /// Groups written so far, for unique resource names.
    groups: usize,
    /// How native colors are written.
    palette: Palette,
}

/// How native colors are written: the document's color profiles as ICC-based color spaces.
#[derive(Default)]
struct Palette {
    /// For each profile: its color space resource name, ICC stream and component count.
    profiles: Vec<(String, Ref, usize)>,
    /// The profile of `device-cmyk()` colors; without one they are device CMYK.
    device_cmyk: Option<u32>,
}

/// A native color as it is set in a content stream.
enum NativeFill<'a> {
    Cmyk([f32; 4]),
    /// A named ICC-based color space, and the color's components with their count.
    Icc(&'a str, [f32; 4], usize),
}

impl Palette {
    fn native(&self, native: rustyprint_layout::display::NativeColor) -> Option<NativeFill<'_>> {
        let components = native.components.map(|c| f32::from(c) / 65535.0);
        let space = if native.space == rustyprint_layout::display::DEVICE_CMYK {
            match self.device_cmyk {
                Some(index) => index,
                None => return Some(NativeFill::Cmyk(components)),
            }
        } else {
            native.space
        };
        let (name, _, count) = self.profiles.get(space as usize)?;
        Some(NativeFill::Icc(name, components, *count))
    }
}

/// Allocates PDF object IDs in sequence.
#[derive(Default)]
pub(crate) struct Refs(i32);

impl Refs {
    pub fn next(&mut self) -> Ref {
        self.0 += 1;
        Ref::new(self.0)
    }
}

/// How many threads compress content streams: one per core, up to
/// [`MAX_COMPRESSION_THREADS`].
fn compression_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(MAX_COMPRESSION_THREADS)
}

/// Compresses each stream, on up to `threads` threads; the results keep their order.
fn compress_all(streams: Vec<Vec<u8>>, threads: usize) -> Vec<Vec<u8>> {
    let compress = |data: &Vec<u8>| compress_to_vec_zlib(data, COMPRESSION_LEVEL);
    if threads < 2 || streams.len() < 2 {
        return streams.iter().map(compress).collect();
    }
    let per_thread = streams.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let workers: Vec<_> = streams
            .chunks(per_thread)
            .map(|chunk| scope.spawn(move || chunk.iter().map(compress).collect::<Vec<_>>()))
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| match worker.join() {
                Ok(compressed) => compressed,
                Err(panic) => std::panic::resume_unwind(panic),
            })
            .collect()
    })
}

impl Writer {
    /// Compresses content streams and writes them under their IDs.
    fn write_streams(&mut self, pending: Vec<(Ref, Vec<u8>)>, threads: usize) {
        let (ids, streams): (Vec<Ref>, Vec<Vec<u8>>) = pending.into_iter().unzip();
        for (id, data) in ids.into_iter().zip(compress_all(streams, threads)) {
            self.pdf.stream(id, &data).filter(Filter::FlateDecode);
        }
    }

    fn write(&mut self, document: &Document) -> Result<(), Error> {
        let catalog_id = self.refs.next();
        let page_tree_id = self.refs.next();
        let page_ids: Vec<Ref> = document.pages.iter().map(|_| self.refs.next()).collect();

        let outline_id = (!document.bookmarks.is_empty()).then(|| self.refs.next());
        let mut catalog = self.pdf.catalog(catalog_id);
        catalog.pages(page_tree_id);
        if let Some(outline_id) = outline_id {
            catalog.outlines(outline_id);
        }
        catalog.finish();
        self.pdf
            .pages(page_tree_id)
            .kids(page_ids.iter().copied())
            .count(page_ids.len() as i32);

        let mut annotations: Vec<Vec<Ref>> = Vec::with_capacity(page_ids.len());
        // Color profiles are written once, and every content stream can use them.
        for (index, profile) in document.color_profiles.iter().enumerate() {
            let id = self.refs.next();
            let data = compress_to_vec_zlib(&profile.data, COMPRESSION_LEVEL);
            let mut stream = self.pdf.icc_profile(id, &data);
            stream.filter(Filter::FlateDecode);
            stream.n(profile.components as i32);
            let alternate = match profile.components {
                1 => Name(b"DeviceGray"),
                4 => Name(b"DeviceCMYK"),
                _ => Name(b"DeviceRGB"),
            };
            stream.alternate_name(alternate);
            stream.finish();
            self.palette
                .profiles
                .push((format!("Cs{index}"), id, profile.components));
        }
        self.palette.device_cmyk = document.device_cmyk;
        // Content streams are compressed in parallel, a batch of pages at a time, and written
        // after their pages under the IDs taken in page order.
        let threads = compression_threads();
        let mut pending: Vec<(Ref, Vec<u8>)> = Vec::new();
        for (page, &page_id) in document.pages.iter().zip(&page_ids) {
            rustyprint_dom::budget::check_deadline();
            if pending.len() >= threads * PAGES_PER_THREAD {
                self.write_streams(std::mem::take(&mut pending), threads);
            }
            let width = page.size.width * PX_TO_PT;
            let height = page.size.height * PX_TO_PT;

            let mut content = Content::new();
            let bleed = page.bleed * PX_TO_PT;
            let marked = page.marks.crop || page.marks.cross;
            if marked {
                marks(&mut content, page.marks, width, height, bleed);
            }
            let mut used = Resources::default();
            self.render(&mut content, &mut used, &page.items, height);
            let content_id = self.refs.next();
            pending.push((content_id, content.finish().into_vec()));

            // Content is drawn from the trim box's corner; the media box grows around it for
            // the bleed and the marks.
            let extent = bleed + if marked { MARKS_AREA } else { 0.0 };
            let mut page_writer = self.pdf.page(page_id);
            page_writer
                .parent(page_tree_id)
                .media_box(Rect::new(-extent, -extent, width + extent, height + extent))
                .contents(content_id);
            if extent > 0.0 {
                page_writer
                    .bleed_box(Rect::new(-bleed, -bleed, width + bleed, height + bleed))
                    .trim_box(Rect::new(0.0, 0.0, width, height));
            }
            if page.rotate != 0 {
                page_writer.rotate(page.rotate);
            }
            let link_ids: Vec<Ref> = page.links.iter().map(|_| self.refs.next()).collect();
            if !link_ids.is_empty() {
                page_writer.annotations(link_ids.iter().copied());
            }
            annotations.push(link_ids);
            write_resources(
                &mut page_writer.resources(),
                &used,
                &self.fonts,
                &self.alphas,
                &self.images,
                &self.palette,
            );
        }
        self.write_streams(pending, threads);

        for (page, ids) in document.pages.iter().zip(&annotations) {
            let height = page.size.height * PX_TO_PT;
            for (link, &id) in page.links.iter().zip(ids) {
                let rect = link.rect;
                let mut annotation = self.pdf.annotation(id);
                annotation
                    .subtype(AnnotationType::Link)
                    .rect(Rect::new(
                        rect.x * PX_TO_PT,
                        height - rect.bottom() * PX_TO_PT,
                        rect.right() * PX_TO_PT,
                        height - rect.y * PX_TO_PT,
                    ))
                    .border(0.0, 0.0, 0.0, None);
                match &link.target {
                    LinkTarget::Uri(uri) => {
                        annotation
                            .action()
                            .action_type(ActionType::Uri)
                            .uri(Str(uri.as_bytes()));
                    }
                    LinkTarget::Internal(destination) => {
                        let (page, top) = target(document, &page_ids, destination);
                        annotation
                            .action()
                            .action_type(ActionType::GoTo)
                            .destination()
                            .page(page)
                            .xyz(0.0, top, None);
                    }
                }
            }
        }

        if let Some(outline_id) = outline_id {
            self.outline(outline_id, document, &page_ids);
        }

        for resource in std::mem::take(&mut self.images) {
            self.write_image(&resource);
        }

        for &(alpha, id) in &self.alphas {
            let value = f32::from(alpha) / 255.0;
            self.pdf
                .ext_graphics(id)
                .non_stroking_alpha(value)
                .stroking_alpha(value);
        }

        for font in &self.fonts {
            font.write(&mut self.pdf, &mut self.refs)?;
        }

        let info_id = self.refs.next();
        let mut info = self.pdf.document_info(info_id);
        info.producer(TextStr("rustyprint"));
        if let Some(title) = &document.title {
            info.title(TextStr(title));
        }
        info.finish();
        Ok(())
    }

    /// Writes display items into `content`, noting the resources they use in `res`. Groups
    /// become transparency group XObjects with content streams of their own.
    fn render(
        &mut self,
        content: &mut Content,
        res: &mut Resources,
        items: &[DisplayItem],
        height: f32,
    ) {
        let mut index = 0;
        while index < items.len() {
            let item = &items[index];
            index += 1;
            if let DisplayItem::PushGroup(group) = item {
                let end = group_end(items, index);
                self.group(content, res, group, &items[index..end], height);
                index = (end + 1).min(items.len());
                continue;
            }
            let alpha = item_alpha(item);
            if alpha < 255 {
                content.save_state();
                content.set_parameters(Name(alpha_name(alpha).as_bytes()));
                if !res.alphas.contains(&alpha) {
                    res.alphas.push(alpha);
                }
                if !self.alphas.iter().any(|(a, _)| *a == alpha) {
                    let id = self.refs.next();
                    self.alphas.push((alpha, id));
                }
            }
            match item {
                DisplayItem::Text(run) => {
                    let font = self.text(content, height, run);
                    if !res.fonts.contains(&font) {
                        res.fonts.push(font);
                    }
                }
                DisplayItem::Rect { rect, color } => {
                    set_fill(content, *color, &self.palette);
                    content.rect(
                        rect.x * PX_TO_PT,
                        height - rect.bottom() * PX_TO_PT,
                        rect.width * PX_TO_PT,
                        rect.height * PX_TO_PT,
                    );
                    content.fill_nonzero();
                }
                DisplayItem::Polygon { points, color } => {
                    set_fill(content, *color, &self.palette);
                    path(content, points, height);
                    content.fill_nonzero();
                }
                DisplayItem::Polyline {
                    points,
                    width,
                    color,
                    closed,
                } => {
                    content.save_state();
                    set_stroke(content, *color, &self.palette);
                    content.set_line_width(width * PX_TO_PT);
                    content.set_line_join(LineJoinStyle::RoundJoin);
                    let mut pts = points
                        .iter()
                        .map(|p| (p.x * PX_TO_PT, height - p.y * PX_TO_PT));
                    if let Some((x, y)) = pts.next() {
                        content.move_to(x, y);
                    }
                    for (x, y) in pts {
                        content.line_to(x, y);
                    }
                    if *closed {
                        content.close_path();
                    }
                    content.stroke();
                    content.restore_state();
                }
                DisplayItem::Line {
                    from,
                    to,
                    width,
                    color,
                    style,
                } => {
                    content.save_state();
                    set_stroke(content, *color, &self.palette);
                    content.set_line_width(width * PX_TO_PT);
                    let w = width * PX_TO_PT;
                    // Fit the pattern to the line so it starts and ends with a dash or dot.
                    let length = (to.x - from.x).hypot(to.y - from.y) * PX_TO_PT;
                    match style {
                        LineStyle::Solid => {}
                        LineStyle::Dashed => {
                            let dashes = ((length / (3.0 * w) + 1.0) / 2.0).round().max(1.0);
                            let dash = length / (2.0 * dashes - 1.0);
                            content.set_dash_pattern([dash, dash], 0.0);
                        }
                        LineStyle::Dotted => {
                            let dots = (length / (2.0 * w)).round().max(1.0) + 1.0;
                            content.set_line_cap(LineCapStyle::RoundCap);
                            content.set_dash_pattern([0.0, length / (dots - 1.0)], 0.0);
                        }
                    }
                    path(content, &[*from, *to], height);
                    content.stroke();
                    content.restore_state();
                }
                DisplayItem::PushClip(rect) => {
                    content.save_state();
                    content.rect(
                        rect.x * PX_TO_PT,
                        height - rect.bottom() * PX_TO_PT,
                        rect.width * PX_TO_PT,
                        rect.height * PX_TO_PT,
                    );
                    content.clip_nonzero();
                    content.end_path();
                }
                DisplayItem::Path { path, color, rule } => {
                    set_fill(content, *color, &self.palette);
                    curves(content, path, height);
                    match rule {
                        FillRule::NonZero => content.fill_nonzero(),
                        FillRule::EvenOdd => content.fill_even_odd(),
                    };
                }
                DisplayItem::StrokePath {
                    path,
                    width,
                    color,
                    style,
                } => {
                    content.save_state();
                    set_stroke(content, *color, &self.palette);
                    let w = width * PX_TO_PT;
                    content.set_line_width(w);
                    // Fit a whole number of dashes or dots around the path.
                    let length = path.length() * PX_TO_PT;
                    let period = |target: f32| length / (length / target).round().max(1.0);
                    match style {
                        LineStyle::Solid => {}
                        LineStyle::Dashed => {
                            let dash = period(6.0 * w) / 2.0;
                            content.set_dash_pattern([dash, dash], 0.0);
                        }
                        LineStyle::Dotted => {
                            content.set_line_cap(LineCapStyle::RoundCap);
                            content.set_dash_pattern([0.0, period(2.0 * w)], 0.0);
                        }
                    }
                    curves(content, path, height);
                    content.stroke();
                    content.restore_state();
                }
                DisplayItem::PushClipPath { path, rule } => {
                    content.save_state();
                    curves(content, path, height);
                    match rule {
                        FillRule::NonZero => content.clip_nonzero(),
                        FillRule::EvenOdd => content.clip_even_odd(),
                    };
                    content.end_path();
                }
                DisplayItem::Image {
                    image,
                    rect,
                    smooth,
                    orientation,
                    ..
                } => {
                    let index = self.image_index(image, *smooth);
                    if !res.images.contains(&index) {
                        res.images.push(index);
                    }
                    content.save_state();
                    content.transform(image_matrix(
                        *orientation,
                        rect.x * PX_TO_PT,
                        height - rect.bottom() * PX_TO_PT,
                        rect.width * PX_TO_PT,
                        rect.height * PX_TO_PT,
                    ));
                    content.x_object(Name(self.images[index].name.as_bytes()));
                    content.restore_state();
                }
                DisplayItem::Gradient { rect, shading } => {
                    let resource = self.shading(shading, height);
                    content.save_state();
                    content.rect(
                        rect.x * PX_TO_PT,
                        height - rect.bottom() * PX_TO_PT,
                        rect.width * PX_TO_PT,
                        rect.height * PX_TO_PT,
                    );
                    content.clip_nonzero();
                    content.end_path();
                    if let Some((name, id)) = resource.mask {
                        content.set_parameters(Name(name.as_bytes()));
                        res.states.push((name, id));
                    }
                    if let Some(matrix) = ellipse_matrix(shading, height) {
                        content.transform(matrix);
                    }
                    content.shading(Name(resource.name.as_bytes()));
                    content.restore_state();
                    res.shadings.push((resource.name, resource.id));
                }
                DisplayItem::PushClipText(runs) => {
                    for font in self.clip_text(content, height, runs) {
                        if !res.fonts.contains(&font) {
                            res.fonts.push(font);
                        }
                    }
                }
                DisplayItem::PushTransform([a, b, c, d, e, f]) => {
                    // The matrix in PDF space, whose y axis points up from the bottom.
                    content.save_state();
                    content.transform([
                        *a,
                        -b,
                        -c,
                        *d,
                        c * height + e * PX_TO_PT,
                        height * (1.0 - d) - f * PX_TO_PT,
                    ]);
                }
                DisplayItem::PopClip => {
                    content.restore_state();
                }
                // Groups are written above; a group without a start, or a backdrop layout did
                // not resolve, paints nothing.
                DisplayItem::PushGroup(_) | DisplayItem::PopGroup | DisplayItem::Backdrop(_) => {}
            }
            if alpha < 255 {
                content.restore_state();
            }
        }
    }

    /// Writes the items of a group as a transparency group XObject and paints it with the
    /// group's opacity, blend mode and mask.
    fn group(
        &mut self,
        content: &mut Content,
        res: &mut Resources,
        group: &Group,
        items: &[DisplayItem],
        height: f32,
    ) {
        let mut inner = Content::new();
        let mut used = Resources::default();
        self.render(&mut inner, &mut used, items, height);
        let data = compress_to_vec_zlib(&inner.finish(), COMPRESSION_LEVEL);
        let form_id = self.refs.next();
        let mut form = self.pdf.form_xobject(form_id, &data);
        form.filter(Filter::FlateDecode);
        form.bbox(Rect::new(-1.0e5, -1.0e5, 1.0e5, 1.0e5));
        form.group()
            .transparency()
            .isolated(group.isolated)
            .knockout(false)
            .color_space()
            .device_rgb();
        write_resources(
            &mut form.resources(),
            &used,
            &self.fonts,
            &self.alphas,
            &self.images,
            &self.palette,
        );
        form.finish();

        let mask = group.mask.as_ref().map(|mask| {
            let index = self.image_index(&mask.image, true);
            let image = &self.images[index];
            let (image_name, image_id) = (image.name.clone(), image.id);
            let r = mask.rect;
            let mut mask_content = Content::new();
            mask_content.save_state();
            mask_content.transform([
                r.width * PX_TO_PT,
                0.0,
                0.0,
                r.height * PX_TO_PT,
                r.x * PX_TO_PT,
                height - r.bottom() * PX_TO_PT,
            ]);
            mask_content.x_object(Name(image_name.as_bytes()));
            mask_content.restore_state();
            let data = mask_content.finish();
            let mask_id = self.refs.next();
            let mut form = self.pdf.form_xobject(mask_id, &data);
            form.bbox(Rect::new(-1.0e5, -1.0e5, 1.0e5, 1.0e5));
            form.group().transparency().color_space().device_gray();
            form.resources()
                .x_objects()
                .pair(Name(image_name.as_bytes()), image_id);
            form.finish();
            mask_id
        });

        let state_id = self.refs.next();
        let mut state = self.pdf.ext_graphics(state_id);
        state
            .non_stroking_alpha(group.opacity)
            .stroking_alpha(group.opacity)
            .blend_mode(blend_mode(group.blend));
        if let Some(mask_id) = mask {
            state
                .soft_mask()
                .subtype(MaskType::Luminosity)
                .group(mask_id);
        }
        state.finish();

        self.groups += 1;
        let state_name = format!("GS{}", self.groups);
        let form_name = format!("Fm{}", self.groups);
        content.save_state();
        content.set_parameters(Name(state_name.as_bytes()));
        content.x_object(Name(form_name.as_bytes()));
        content.restore_state();
        res.states.push((state_name, state_id));
        res.forms.push((form_name, form_id));
    }

    /// Writes the document outline: each bookmark nests under the closest one before it with
    /// a lower level.
    fn outline(&mut self, outline_id: Ref, document: &Document, page_ids: &[Ref]) {
        let bookmarks: &[Bookmark] = &document.bookmarks;
        let ids: Vec<Ref> = bookmarks.iter().map(|_| self.refs.next()).collect();
        let mut parents: Vec<Option<usize>> = Vec::with_capacity(bookmarks.len());
        let mut open: Vec<usize> = Vec::new();
        for (index, bookmark) in bookmarks.iter().enumerate() {
            while open
                .last()
                .is_some_and(|&last| bookmarks[last].level >= bookmark.level)
            {
                open.pop();
            }
            parents.push(open.last().copied());
            open.push(index);
        }
        // The children of each bookmark, and the top-level bookmarks last.
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); bookmarks.len() + 1];
        for (index, parent) in parents.iter().enumerate() {
            children[parent.unwrap_or(bookmarks.len())].push(index);
        }
        let mut descendants = vec![0; bookmarks.len()];
        for index in (0..bookmarks.len()).rev() {
            if let Some(parent) = parents[index] {
                descendants[parent] += 1 + descendants[index];
            }
        }

        let top = &children[bookmarks.len()];
        self.pdf
            .outline(outline_id)
            .first(ids[top[0]])
            .last(ids[top[top.len() - 1]])
            .count(bookmarks.len() as i32);
        for (index, bookmark) in bookmarks.iter().enumerate() {
            let siblings = &children[parents[index].unwrap_or(bookmarks.len())];
            let position = siblings
                .iter()
                .position(|&i| i == index)
                .expect("a bookmark is among its parent's children");
            let mut item = self.pdf.outline_item(ids[index]);
            item.title(TextStr(&bookmark.title))
                .parent(parents[index].map_or(outline_id, |p| ids[p]));
            if position > 0 {
                item.prev(ids[siblings[position - 1]]);
            }
            if let Some(&next) = siblings.get(position + 1) {
                item.next(ids[next]);
            }
            let kids = &children[index];
            if let (Some(&first), Some(&last)) = (kids.first(), kids.last()) {
                item.first(ids[first])
                    .last(ids[last])
                    .count(descendants[index]);
            }
            let (page, top) = target(document, page_ids, &bookmark.target);
            item.dest().page(page).xyz(0.0, top, None);
        }
    }

    /// Writes a shading, and for stops that are not all opaque a graphics state whose soft
    /// mask is a gray shading of their alpha.
    fn shading(&mut self, shading: &Shading, page_height: f32) -> ShadingResource {
        let index = self.shadings;
        self.shadings += 1;
        let color: Vec<(f32, Vec<f32>)> = shading
            .stops
            .iter()
            .map(|(t, c)| (*t, vec![channel(c.r), channel(c.g), channel(c.b)]))
            .collect();
        let id = self.shading_object(shading, &color, page_height);
        let mask = if shading.stops.iter().all(|(_, c)| c.a == 255) {
            None
        } else {
            let alpha: Vec<(f32, Vec<f32>)> = shading
                .stops
                .iter()
                .map(|(t, c)| (*t, vec![channel(c.a)]))
                .collect();
            let mask_shading = self.shading_object(shading, &alpha, page_height);
            let mut content = Content::new();
            if let Some(matrix) = ellipse_matrix(shading, page_height) {
                content.transform(matrix);
            }
            content.shading(Name(b"Sh"));
            let data = content.finish();
            let form_id = self.refs.next();
            let mut form = self.pdf.form_xobject(form_id, &data);
            form.bbox(Rect::new(-1.0e5, -1.0e5, 1.0e5, 1.0e5));
            form.group().transparency().color_space().device_gray();
            form.resources().shadings().pair(Name(b"Sh"), mask_shading);
            form.finish();
            let state_id = self.refs.next();
            self.pdf
                .ext_graphics(state_id)
                .soft_mask()
                .subtype(MaskType::Luminosity)
                .group(form_id);
            Some((format!("M{index}"), state_id))
        };
        ShadingResource {
            name: format!("Sh{index}"),
            id,
            mask,
        }
    }

    /// Writes a shading dictionary whose function gives `values` at its stops: three RGB
    /// components, or one gray component.
    fn shading_object(
        &mut self,
        shading: &Shading,
        values: &[(f32, Vec<f32>)],
        page_height: f32,
    ) -> Ref {
        let function = self.stops_function(values);
        let id = self.refs.next();
        let mut dict = self.pdf.function_shading(id);
        match shading.kind {
            ShadingKind::Axial { start, end } => {
                dict.shading_type(FunctionShadingType::Axial).coords([
                    start.x * PX_TO_PT,
                    page_height - start.y * PX_TO_PT,
                    end.x * PX_TO_PT,
                    page_height - end.y * PX_TO_PT,
                ]);
            }
            ShadingKind::Radial { center, r0, r1, .. } => {
                let (x, y) = (center.x * PX_TO_PT, page_height - center.y * PX_TO_PT);
                dict.shading_type(FunctionShadingType::Radial).coords([
                    x,
                    y,
                    r0 * PX_TO_PT,
                    x,
                    y,
                    r1 * PX_TO_PT,
                ]);
            }
        }
        if values.first().is_some_and(|(_, v)| v.len() == 1) {
            dict.color_space().device_gray();
        } else {
            dict.color_space().device_rgb();
        }
        dict.function(function).extend([true, true]);
        dict.finish();
        id
    }

    /// A function over [0, 1] that interpolates linearly between the values at the stops.
    fn stops_function(&mut self, stops: &[(f32, Vec<f32>)]) -> Ref {
        let mut stops = stops.to_vec();
        if stops.len() == 1 {
            stops.push((1.0, stops[0].1.clone()));
        }
        // Stitching bounds must increase; stops at the same place become a sharp step.
        for i in 1..stops.len() {
            if stops[i].0 <= stops[i - 1].0 {
                stops[i].0 = stops[i - 1].0 + 1.0e-5;
            }
        }
        let segments: Vec<Ref> = stops
            .windows(2)
            .map(|pair| {
                let id = self.refs.next();
                self.pdf
                    .exponential_function(id)
                    .domain([0.0, 1.0])
                    .c0(pair[0].1.iter().copied())
                    .c1(pair[1].1.iter().copied())
                    .n(1.0);
                id
            })
            .collect();
        if segments.len() == 1 {
            return segments[0];
        }
        let last = stops.last().map_or(1.0, |s| s.0.max(1.0));
        let id = self.refs.next();
        self.pdf
            .stitching_function(id)
            .domain([0.0, last])
            .functions(segments.iter().copied())
            .bounds(stops[1..stops.len() - 1].iter().map(|s| s.0))
            .encode(segments.iter().flat_map(|_| [0.0, 1.0]));
        id
    }

    /// Returns the index of `font` in `self.fonts`, adding it on first use.
    fn font_index(&mut self, font: &Font) -> usize {
        if let Some(index) = self
            .fonts
            .iter()
            .position(|resource| resource.font == *font)
        {
            return index;
        }
        let type0_id = self.refs.next();
        let name = format!("F{}", self.fonts.len());
        self.fonts
            .push(FontResource::new(font.clone(), name, type0_id));
        self.fonts.len() - 1
    }

    /// The glyphs of a run as `TJ` items, and the index of the run's font.
    fn positioned(&mut self, run: &TextRun) -> (usize, Vec<Positioned>) {
        let index = self.font_index(&run.font);
        let resource = &mut self.fonts[index];

        // Glyph positions are written in thousandths of an em, the unit of `TJ` adjustments.
        let to_em = 1000.0 / (run.size * run.font.size_adjust());
        let mut items = Vec::new();
        // Text rise outlasts the text object, so every run sets it.
        let mut rise = f32::NAN;
        let mut start = 0;
        while start < run.glyphs.len() {
            let range = run.glyphs[start].text.clone();
            let end = start
                + run.glyphs[start..]
                    .iter()
                    .take_while(|g| g.text == range)
                    .count();
            let cluster = &run.glyphs[start..end];
            let text = &run.text[range];
            // The cluster's text goes to its glyph with the widest advance: marks, such as
            // Arabic dots, have none and are shared between letters.
            let base = (0..cluster.len())
                .rev()
                .max_by(|&a, &b| {
                    let advance = |i: usize| cluster[i].x_advance.abs();
                    advance(a).total_cmp(&advance(b))
                })
                .expect("clusters are not empty");
            let mut mapped = cluster.len() == 1;
            let cluster_start = items.len();
            for (i, glyph) in cluster.iter().enumerate() {
                let glyph_rise = -glyph.y_offset * PX_TO_PT;
                if glyph_rise != rise {
                    rise = glyph_rise;
                    items.push(Positioned::Rise(rise));
                }
                let (cid, maps_to_text) =
                    resource.use_glyph(glyph.id, if i == base { text } else { "" });
                mapped &= maps_to_text;
                let width = resource.width(glyph.id);
                let x_offset = glyph.x_offset * to_em;
                let advance = glyph.x_advance * to_em;
                if x_offset != 0.0 {
                    items.push(Positioned::Adjust(-x_offset));
                }
                items.push(Positioned::Glyph(cid));
                let adjust = width - advance + x_offset;
                if adjust.abs() > 1e-3 {
                    items.push(Positioned::Adjust(adjust));
                }
            }
            // Text the font's ToUnicode map cannot give is copied from the content instead.
            if !mapped && !text.is_empty() {
                items.insert(cluster_start, Positioned::ActualText(text.to_owned()));
                items.push(Positioned::EndActualText);
            }
            start = end;
        }
        (index, items)
    }

    /// Paints a text run and returns the index of the font it used.
    fn text(&mut self, content: &mut Content, page_height: f32, run: &TextRun) -> usize {
        let (index, items) = self.positioned(run);
        let name = self.fonts[index].name.clone();

        // Which passes paint the glyphs: a stroke painted first needs its own pass.
        let passes: Vec<(TextRenderingMode, Option<(Color, f32)>)> = match run.stroke {
            Some(stroke) if run.stroke_first && run.fill => vec![
                (
                    TextRenderingMode::Stroke,
                    Some((stroke.color, stroke.width)),
                ),
                (TextRenderingMode::Fill, None),
            ],
            Some(stroke) if run.fill => vec![(
                TextRenderingMode::FillStroke,
                Some((stroke.color, stroke.width)),
            )],
            Some(stroke) => vec![(
                TextRenderingMode::Stroke,
                Some((stroke.color, stroke.width)),
            )],
            // Synthesized bold strokes the outline in the fill color.
            None if run.embolden > 0.0 => vec![(
                TextRenderingMode::FillStroke,
                Some((run.color, run.embolden)),
            )],
            // Text over color glyphs is only there to be searched and copied.
            None if !run.fill => vec![(TextRenderingMode::Invisible, None)],
            None => vec![(TextRenderingMode::Fill, None)],
        };
        for (mode, stroke) in passes {
            content.begin_text();
            set_fill(content, run.color, &self.palette);
            if let Some((color, width)) = stroke {
                set_stroke(content, color, &self.palette);
                content.set_line_width(width * PX_TO_PT);
            }
            if mode != TextRenderingMode::Fill {
                content.set_text_rendering_mode(mode);
            }
            show(content, &name, run, page_height, &items);
            if mode != TextRenderingMode::Fill {
                content.set_text_rendering_mode(TextRenderingMode::Fill);
            }
            content.end_text();
        }
        index
    }

    /// Starts clipping to the glyphs of `runs`, which share one text object so that their
    /// outlines add up to one clipping path. Returns the indices of the fonts used.
    fn clip_text(
        &mut self,
        content: &mut Content,
        page_height: f32,
        runs: &[TextRun],
    ) -> Vec<usize> {
        let mut fonts = Vec::new();
        content.save_state();
        content.begin_text();
        content.set_text_rendering_mode(TextRenderingMode::Clip);
        for run in runs {
            let (index, items) = self.positioned(run);
            let name = self.fonts[index].name.clone();
            show(content, &name, run, page_height, &items);
            if !fonts.contains(&index) {
                fonts.push(index);
            }
        }
        content.end_text();
        fonts
    }

    /// Returns the index of `image` in `self.images`, adding it on first use.
    fn image_index(&mut self, image: &Arc<Image>, smooth: bool) -> usize {
        if let Some(index) = self
            .images
            .iter()
            .position(|resource| Arc::ptr_eq(&resource.image, image) && resource.smooth == smooth)
        {
            return index;
        }
        let id = self.refs.next();
        let name = format!("Im{}", self.images.len());
        self.images.push(ImageResource {
            image: image.clone(),
            id,
            name,
            smooth,
        });
        self.images.len() - 1
    }

    /// Writes an image XObject, with a soft mask for its alpha channel.
    fn write_image(&mut self, resource: &ImageResource) {
        let image = &resource.image;
        let (width, height) = (image.width as i32, image.height as i32);
        match &image.data {
            ImageData::Jpeg { data, components } => {
                let mut xobject = self.pdf.image_xobject(resource.id, data);
                xobject.filter(Filter::DctDecode);
                xobject.width(width).height(height).bits_per_component(8);
                xobject.interpolate(resource.smooth);
                if *components == 1 {
                    xobject.color_space().device_gray();
                } else {
                    xobject.color_space().device_rgb();
                }
            }
            ImageData::Pixels {
                samples,
                gray,
                alpha,
            } => {
                let mask = alpha.as_ref().map(|alpha| {
                    let id = self.refs.next();
                    (id, compress_to_vec_zlib(alpha, COMPRESSION_LEVEL))
                });
                let data = compress_to_vec_zlib(samples, COMPRESSION_LEVEL);
                let mut xobject = self.pdf.image_xobject(resource.id, &data);
                xobject.filter(Filter::FlateDecode);
                xobject.width(width).height(height).bits_per_component(8);
                xobject.interpolate(resource.smooth);
                if *gray {
                    xobject.color_space().device_gray();
                } else {
                    xobject.color_space().device_rgb();
                }
                if let Some((id, _)) = &mask {
                    xobject.s_mask(*id);
                }
                xobject.finish();
                if let Some((id, data)) = &mask {
                    let mut xobject = self.pdf.image_xobject(*id, data);
                    xobject.filter(Filter::FlateDecode);
                    xobject.width(width).height(height).bits_per_component(8);
                    xobject.color_space().device_gray();
                }
            }
        }
    }
}

/// The index of the `PopGroup` that ends a group whose items start at `start`, or the end of
/// the items if the group is not closed.
fn group_end(items: &[DisplayItem], start: usize) -> usize {
    let mut depth = 0;
    for (index, item) in items.iter().enumerate().skip(start) {
        match item {
            DisplayItem::PushGroup(_) => depth += 1,
            DisplayItem::PopGroup if depth == 0 => return index,
            DisplayItem::PopGroup => depth -= 1,
            _ => {}
        }
    }
    items.len()
}

/// The PDF blend mode for a CSS one. PDF has no additive modes, so `plus-lighter` is drawn
/// as `screen` and `plus-darker` as `multiply`.
fn blend_mode(mode: rustyprint_layout::display::BlendMode) -> PdfBlendMode {
    use rustyprint_layout::display::BlendMode as B;
    match mode {
        B::Normal => PdfBlendMode::Normal,
        B::Multiply | B::PlusDarker => PdfBlendMode::Multiply,
        B::Screen | B::PlusLighter => PdfBlendMode::Screen,
        B::Overlay => PdfBlendMode::Overlay,
        B::Darken => PdfBlendMode::Darken,
        B::Lighten => PdfBlendMode::Lighten,
        B::ColorDodge => PdfBlendMode::ColorDodge,
        B::ColorBurn => PdfBlendMode::ColorBurn,
        B::HardLight => PdfBlendMode::HardLight,
        B::SoftLight => PdfBlendMode::SoftLight,
        B::Difference => PdfBlendMode::Difference,
        B::Exclusion => PdfBlendMode::Exclusion,
        B::Hue => PdfBlendMode::Hue,
        B::Saturation => PdfBlendMode::Saturation,
        B::Color => PdfBlendMode::Color,
        B::Luminosity => PdfBlendMode::Luminosity,
    }
}

/// The resources one content stream uses, in order of first use.
#[derive(Default)]
struct Resources {
    /// Indices into the writer's fonts.
    fonts: Vec<usize>,
    alphas: Vec<u8>,
    /// Indices into the writer's images.
    images: Vec<usize>,
    shadings: Vec<(String, Ref)>,
    /// Graphics states of shading masks and groups.
    states: Vec<(String, Ref)>,
    /// Group XObjects.
    forms: Vec<(String, Ref)>,
}

/// Lists the resources a content stream uses in its resource dictionary.
fn write_resources(
    resources: &mut pdf_writer::writers::Resources,
    used: &Resources,
    fonts: &[FontResource],
    alphas: &[(u8, Ref)],
    images: &[ImageResource],
    palette: &Palette,
) {
    if !palette.profiles.is_empty() {
        let mut spaces = resources.color_spaces();
        for (name, id, _) in &palette.profiles {
            let mut space = spaces.insert(Name(name.as_bytes())).array();
            space.item(Name(b"ICCBased"));
            space.item(*id);
        }
        spaces.finish();
    }
    let mut font_dict = resources.fonts();
    for &index in &used.fonts {
        let resource = &fonts[index];
        font_dict.pair(Name(resource.name.as_bytes()), resource.type0_id);
    }
    font_dict.finish();
    let mut states = resources.ext_g_states();
    for &alpha in &used.alphas {
        let (_, id) = alphas
            .iter()
            .find(|(a, _)| *a == alpha)
            .expect("registered when used");
        states.pair(Name(alpha_name(alpha).as_bytes()), *id);
    }
    for (name, id) in &used.states {
        states.pair(Name(name.as_bytes()), *id);
    }
    states.finish();
    if !used.shadings.is_empty() {
        let mut shadings = resources.shadings();
        for (name, id) in &used.shadings {
            shadings.pair(Name(name.as_bytes()), *id);
        }
        shadings.finish();
    }
    let mut x_objects = resources.x_objects();
    for &index in &used.images {
        let resource = &images[index];
        x_objects.pair(Name(resource.name.as_bytes()), resource.id);
    }
    for (name, id) in &used.forms {
        x_objects.pair(Name(name.as_bytes()), *id);
    }
}

/// A shading written for one use, and the graphics state that masks its alpha.
struct ShadingResource {
    name: String,
    id: Ref,
    mask: Option<(String, Ref)>,
}

/// The matrix that stretches a radial shading's circles vertically into ellipses, about
/// their center.
fn ellipse_matrix(shading: &Shading, page_height: f32) -> Option<[f32; 6]> {
    match shading.kind {
        ShadingKind::Radial {
            center, scale_y, ..
        } if (scale_y - 1.0).abs() > 1.0e-4 => {
            let y = page_height - center.y * PX_TO_PT;
            Some([1.0, 0.0, 0.0, scale_y, 0.0, y * (1.0 - scale_y)])
        }
        _ => None,
    }
}

/// An image the document uses and its XObject.
struct ImageResource {
    image: Arc<Image>,
    id: Ref,
    name: String,
    /// Whether viewers smooth the image when they scale it.
    smooth: bool,
}

/// The matrix that draws an image's unit square into the rectangle at `(x, y)` of `w` × `h`
/// (bottom-left corner, in points), turned and flipped by an EXIF orientation. Image rows
/// run from the top, at `v = 1`.
fn image_matrix(orientation: u8, x: f32, y: f32, w: f32, h: f32) -> [f32; 6] {
    // Where the image's u and v axes go in the rectangle's unit square:
    // `X = a u + c v + e` and `Y = b u + d v + f`.
    let [a, b, c, d, e, f] = match orientation {
        2 => [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0],
        3 => [-1.0, 0.0, 0.0, -1.0, 1.0, 1.0],
        4 => [1.0, 0.0, 0.0, -1.0, 0.0, 1.0],
        5 => [0.0, -1.0, -1.0, 0.0, 1.0, 1.0],
        6 => [0.0, -1.0, 1.0, 0.0, 0.0, 1.0],
        7 => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
        8 => [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };
    [a * w, b * h, c * w, d * h, e * w + x, f * h + y]
}

/// Shows the positioned glyphs of `run` in the font resource `font`, inside a text object.
fn show(content: &mut Content, font: &str, run: &TextRun, page_height: f32, items: &[Positioned]) {
    let skew = run.oblique.to_radians().tan();
    content.set_font(
        Name(font.as_bytes()),
        run.size * run.font.size_adjust() * PX_TO_PT,
    );
    content.set_text_matrix([
        1.0,
        0.0,
        skew,
        1.0,
        run.origin.x * PX_TO_PT,
        page_height - run.origin.y * PX_TO_PT,
    ]);
    let mut rest = items;
    while let Some(item) = rest.first() {
        match item {
            Positioned::Rise(rise) => {
                content.set_rise(*rise);
            }
            Positioned::ActualText(text) => {
                content
                    .begin_marked_content_with_properties(Name(b"Span"))
                    .properties()
                    .actual_text(TextStr(text));
            }
            Positioned::EndActualText => {
                content.end_marked_content();
            }
            Positioned::Glyph(_) | Positioned::Adjust(_) => {
                let count = rest
                    .iter()
                    .take_while(|item| matches!(item, Positioned::Glyph(_) | Positioned::Adjust(_)))
                    .count();
                let mut show = content.show_positioned();
                let mut writer = show.items();
                for item in &rest[..count] {
                    match *item {
                        Positioned::Glyph(cid) => writer.show(Str(&cid.to_be_bytes())),
                        Positioned::Adjust(amount) => writer.adjust(amount),
                        _ => unreachable!("only glyphs and adjustments are counted"),
                    };
                }
                rest = &rest[count..];
                continue;
            }
        }
        rest = &rest[1..];
    }
}

/// The page and the top edge in PDF coordinates of a destination.
fn target(document: &Document, page_ids: &[Ref], destination: &Destination) -> (Ref, f32) {
    let index = destination.page.min(page_ids.len().saturating_sub(1));
    let height = document
        .pages
        .get(index)
        .map_or(0.0, |p: &Page| p.size.height);
    (page_ids[index], (height - destination.y) * PX_TO_PT)
}

fn channel(value: u8) -> f32 {
    f32::from(value) / 255.0
}

fn set_fill(content: &mut Content, color: Color, palette: &Palette) {
    match color.native.and_then(|n| palette.native(n)) {
        Some(NativeFill::Cmyk([c, m, y, k])) => {
            content.set_fill_cmyk(c, m, y, k);
        }
        Some(NativeFill::Icc(name, components, count)) => {
            content.set_fill_color_space(Name(name.as_bytes()));
            content.set_fill_color(components[..count].iter().copied());
        }
        None => {
            content.set_fill_rgb(channel(color.r), channel(color.g), channel(color.b));
        }
    }
}

fn set_stroke(content: &mut Content, color: Color, palette: &Palette) {
    match color.native.and_then(|n| palette.native(n)) {
        Some(NativeFill::Cmyk([c, m, y, k])) => {
            content.set_stroke_cmyk(c, m, y, k);
        }
        Some(NativeFill::Icc(name, components, count)) => {
            content.set_stroke_color_space(Name(name.as_bytes()));
            content.set_stroke_color(components[..count].iter().copied());
        }
        None => {
            content.set_stroke_rgb(channel(color.r), channel(color.g), channel(color.b));
        }
    }
}

/// A closed path through `points` (or an open one for two points), in PDF coordinates.
fn path(content: &mut Content, points: &[Point], page_height: f32) {
    let mut points = points
        .iter()
        .map(|p| (p.x * PX_TO_PT, page_height - p.y * PX_TO_PT));
    if let Some((x, y)) = points.next() {
        content.move_to(x, y);
    }
    let mut count = 1;
    for (x, y) in points {
        content.line_to(x, y);
        count += 1;
    }
    if count > 2 {
        content.close_path();
    }
}

/// Draws crop marks at the corners of the trim box and registration crosses beside the
/// middle of each side, outside the bleed. Sizes are in points.
fn marks(content: &mut Content, marks: PrintMarks, width: f32, height: f32, bleed: f32) {
    let gap = bleed + 3.0;
    let length = MARKS_AREA - 6.0;
    content.save_state();
    content.set_stroke_gray(0.0);
    content.set_line_width(0.5);
    fn line(content: &mut Content, x1: f32, y1: f32, x2: f32, y2: f32) {
        content.move_to(x1, y1);
        content.line_to(x2, y2);
    }
    if marks.crop {
        for (x, dx) in [(0.0, -1.0), (width, 1.0)] {
            for (y, dy) in [(0.0, -1.0), (height, 1.0)] {
                // One horizontal and one vertical line along each edge through the corner.
                line(content, x + dx * gap, y, x + dx * (gap + length), y);
                line(content, x, y + dy * gap, x, y + dy * (gap + length));
            }
        }
    }
    if marks.cross {
        let r = length / 4.0;
        let middle = gap + length / 2.0;
        for (cx, cy) in [
            (width / 2.0, height + middle),
            (width / 2.0, -middle),
            (-middle, height / 2.0),
            (width + middle, height / 2.0),
        ] {
            line(content, cx - 2.0 * r, cy, cx + 2.0 * r, cy);
            line(content, cx, cy - 2.0 * r, cx, cy + 2.0 * r);
            // A circle from four Bézier quarters.
            let k = 0.552_284_8 * r;
            content.move_to(cx + r, cy);
            content.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
            content.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
            content.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
            content.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
        }
    }
    content.stroke();
    content.restore_state();
}

/// Adds the contours of `path` to the current path, in PDF coordinates.
fn curves(content: &mut Content, path: &Path, page_height: f32) {
    let pt = |p: Point| (p.x * PX_TO_PT, page_height - p.y * PX_TO_PT);
    for segment in &path.segments {
        match *segment {
            PathSegment::MoveTo(p) => {
                let (x, y) = pt(p);
                content.move_to(x, y);
            }
            PathSegment::LineTo(p) => {
                let (x, y) = pt(p);
                content.line_to(x, y);
            }
            PathSegment::CubicTo(a, b, c) => {
                let ((x1, y1), (x2, y2), (x3, y3)) = (pt(a), pt(b), pt(c));
                content.cubic_to(x1, y1, x2, y2, x3, y3);
            }
            PathSegment::Close => {
                content.close_path();
            }
        }
    }
}

/// The alpha of an item's color; 255 for items without one.
fn item_alpha(item: &DisplayItem) -> u8 {
    match item {
        DisplayItem::Text(run) => run.color.a,
        DisplayItem::Rect { color, .. }
        | DisplayItem::Polygon { color, .. }
        | DisplayItem::Line { color, .. }
        | DisplayItem::Polyline { color, .. }
        | DisplayItem::Path { color, .. }
        | DisplayItem::StrokePath { color, .. } => color.a,
        DisplayItem::Image { alpha, .. } => *alpha,
        DisplayItem::Gradient { .. }
        | DisplayItem::PushClip(_)
        | DisplayItem::PushClipPath { .. }
        | DisplayItem::PushClipText(_)
        | DisplayItem::PushTransform(_)
        | DisplayItem::PopClip
        | DisplayItem::PushGroup(_)
        | DisplayItem::PopGroup
        | DisplayItem::Backdrop(_) => 255,
    }
}

/// The resource name of the graphics state for an alpha value.
fn alpha_name(alpha: u8) -> String {
    format!("A{alpha}")
}

/// An item of a `TJ` array, or what goes between `TJ` operators.
enum Positioned {
    Glyph(u16),
    Adjust(f32),
    /// The text rise of the glyphs that follow, in points.
    Rise(f32),
    /// Starts a span of glyphs that copy as this text.
    ActualText(String),
    EndActualText,
}
