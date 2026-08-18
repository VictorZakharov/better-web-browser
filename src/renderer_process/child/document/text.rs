//! Renderer-owned font discovery, shaping, fallback, and rasterization.
//!
//! Page font bytes and page text stay inside the AppContainer. The browser receives only bounded
//! glyph placements and premultiplied raster assets over the validated presentation protocol.

mod raster;

use self::raster::{GlyphAsset, GlyphRasterCache};
use crate::engine::{FontSpec, PositionedGlyph, ShapedText, TextMeasurer, WebFont};
use crate::renderer_protocol::PresentedGlyphRaster;
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, Weight, Wrap};
use std::collections::{HashMap, HashSet};

const MAX_SHAPE_CACHE_ENTRIES: usize = 16_384;
const MAX_SHAPE_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHED_TEXT_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ShapeKey {
    text: String,
    family: String,
    size: u32,
    weight: u16,
    italic: bool,
    letter_spacing: u32,
    word_spacing: u32,
}

pub(in crate::renderer_process::child) struct RendererTextSystem {
    fonts: FontSystem,
    buffer: Buffer,
    rasters: GlyphRasterCache,
    shapes: HashMap<ShapeKey, ShapedText>,
    shape_cache_bytes: usize,
    dpi: u32,
    glyph_epoch: u64,
    next_raster_run_id: u64,
    registered_web_fonts: usize,
    pub(super) measure_calls: usize,
    pub(super) shape_cache_hits: usize,
    pub(super) shape_cache_misses: usize,
    pub(super) shape_cache_flushes: usize,
}

impl RendererTextSystem {
    pub(in crate::renderer_process::child) fn new(dpi: u32) -> Self {
        let mut buffer = Buffer::new_empty(Metrics::new(16.0, 19.2));
        buffer.set_wrap(Wrap::None);
        Self {
            fonts: FontSystem::new(),
            buffer,
            rasters: GlyphRasterCache::default(),
            shapes: HashMap::new(),
            shape_cache_bytes: 0,
            dpi: dpi.max(1),
            glyph_epoch: 1,
            next_raster_run_id: 1,
            registered_web_fonts: 0,
            measure_calls: 0,
            shape_cache_hits: 0,
            shape_cache_misses: 0,
            shape_cache_flushes: 0,
        }
    }

    pub(super) fn set_dpi(&mut self, dpi: u32) {
        let dpi = dpi.max(1);
        if self.dpi != dpi {
            self.dpi = dpi;
            self.reset_glyph_state();
        }
    }

    pub(super) fn register_web_fonts(&mut self, fonts: &[WebFont]) {
        if self.registered_web_fonts == fonts.len() {
            return;
        }
        // Rebuilding the database drops all prior page-owned byte buffers at navigation/font-set
        // boundaries. It also avoids exposing font parsing or global font registration to the
        // privileged browser process.
        self.fonts = FontSystem::new();
        for font in fonts {
            register_web_font(&mut self.fonts, font);
        }
        self.registered_web_fonts = fonts.len();
        self.reset_glyph_state();
    }

    pub(super) fn reset_for_navigation(&mut self) {
        if self.registered_web_fonts > 0 {
            self.fonts = FontSystem::new();
            self.registered_web_fonts = 0;
        }
        // The browser clears presented glyph resources on every document's first presentation.
        // Advance the epoch even when only system fonts were used so cache hits in the new
        // document cannot reference rasters that the browser deliberately discarded.
        self.reset_glyph_state();
    }

    pub(super) fn glyph_epoch(&self) -> u64 {
        self.glyph_epoch
    }

    pub(super) fn take_pending_glyphs(&mut self) -> Vec<PresentedGlyphRaster> {
        self.rasters.take_pending()
    }

    pub(super) fn shape_cache_entries(&self) -> usize {
        self.shapes.len()
    }

    fn reset_glyph_state(&mut self) {
        self.shapes.clear();
        self.shape_cache_bytes = 0;
        self.rasters.clear();
        self.glyph_epoch = self.glyph_epoch.wrapping_add(1).max(1);
        self.next_raster_run_id = 1;
    }

    fn shape_text(&mut self, text: &str, spec: &FontSpec) -> ShapedText {
        self.measure_calls = self.measure_calls.saturating_add(1);
        let key = ShapeKey::new(text, spec);
        if let Some(shaped) = self.shapes.get(&key) {
            self.shape_cache_hits = self.shape_cache_hits.saturating_add(1);
            return shaped.clone();
        }
        self.shape_cache_misses = self.shape_cache_misses.saturating_add(1);

        let font_size = spec.size.clamp(1.0, 768.0);
        let line_height = (font_size * 1.2).max(font_size);
        self.buffer.set_metrics_and_size(
            Metrics::new(font_size, line_height),
            None,
            Some(line_height * 2.0),
        );
        let family = css_family(&spec.family);
        let mut attrs = Attrs::new()
            .family(family)
            .weight(Weight(spec.weight.clamp(1, 1000)))
            .style(if spec.italic {
                Style::Italic
            } else {
                Style::Normal
            });
        if spec.letter_spacing != 0.0 {
            attrs = attrs.letter_spacing(spec.letter_spacing / font_size);
        }
        self.buffer.set_text(text, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.fonts, false);

        let scale = self.dpi as f32 / 96.0;
        let mut shaped = ShapedText {
            raster_run_id: self.allocate_raster_run_id(),
            ..ShapedText::default()
        };
        for run in self.buffer.layout_runs() {
            let spaces = whitespace_glyph_positions(run.text, run.glyphs);
            shaped.width = shaped
                .width
                .max((run.line_w + spec.word_spacing * spaces.len() as f32).max(0.0));
            shaped.height = shaped.height.max(run.line_top + run.line_height);
            for glyph in run.glyphs {
                let shift = word_spacing_shift(glyph.x, &spaces, spec.word_spacing);
                let physical = glyph.physical((shift * scale, run.line_y * scale), scale);
                let Some(asset) = self
                    .rasters
                    .get_or_insert(&mut self.fonts, physical.cache_key)
                else {
                    continue;
                };
                shaped
                    .glyphs
                    .push(positioned_glyph(physical.x, physical.y, scale, asset));
            }
        }
        if shaped.height <= 0.0 {
            shaped.height = line_height;
        }
        if text.len() <= MAX_CACHED_TEXT_BYTES {
            let cached_bytes = shape_cache_entry_bytes(&key, &shaped);
            if cached_bytes > MAX_SHAPE_CACHE_BYTES {
                return shaped;
            }
            if self.shapes.len() >= MAX_SHAPE_CACHE_ENTRIES
                || self.shape_cache_bytes.saturating_add(cached_bytes) > MAX_SHAPE_CACHE_BYTES
            {
                self.shapes.clear();
                self.shape_cache_bytes = 0;
                self.shape_cache_flushes = self.shape_cache_flushes.saturating_add(1);
            }
            self.shape_cache_bytes += cached_bytes;
            self.shapes.insert(key, shaped.clone());
        }
        shaped
    }

    fn allocate_raster_run_id(&mut self) -> u64 {
        if self.next_raster_run_id == u64::MAX {
            self.reset_glyph_state();
        }
        let id = self.next_raster_run_id;
        self.next_raster_run_id += 1;
        id
    }
}

fn shape_cache_entry_bytes(key: &ShapeKey, shaped: &ShapedText) -> usize {
    std::mem::size_of::<ShapeKey>()
        .saturating_add(key.text.len())
        .saturating_add(key.family.len())
        .saturating_add(std::mem::size_of::<ShapedText>())
        .saturating_add(
            shaped
                .glyphs
                .len()
                .saturating_mul(std::mem::size_of::<PositionedGlyph>()),
        )
}

impl TextMeasurer for RendererTextSystem {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        let shaped = self.shape_text(text, font);
        (shaped.width, shaped.height)
    }

    fn shape(&mut self, text: &str, font: &FontSpec) -> ShapedText {
        self.shape_text(text, font)
    }
}

impl ShapeKey {
    fn new(text: &str, spec: &FontSpec) -> Self {
        Self {
            text: text.to_string(),
            family: spec.family.clone(),
            size: spec.size.to_bits(),
            weight: spec.weight,
            italic: spec.italic,
            letter_spacing: spec.letter_spacing.to_bits(),
            word_spacing: spec.word_spacing.to_bits(),
        }
    }
}

fn css_family(value: &str) -> Family<'_> {
    let family = value
        .split(',')
        .next()
        .unwrap_or("sans-serif")
        .trim()
        .trim_matches(['\'', '"']);
    match family.to_ascii_lowercase().as_str() {
        "serif" | "ui-serif" => Family::Serif,
        "monospace" | "ui-monospace" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        "sans-serif" | "system-ui" | "ui-sans-serif" => Family::SansSerif,
        _ => Family::Name(family),
    }
}

fn register_web_font(fonts: &mut FontSystem, font: &WebFont) {
    let existing = fonts
        .db()
        .faces()
        .map(|face| face.id)
        .collect::<HashSet<_>>();
    fonts.db_mut().load_font_data(font.sfnt.clone());
    let mut loaded = fonts
        .db()
        .faces()
        .filter(|face| !existing.contains(&face.id))
        .cloned()
        .collect::<Vec<_>>();
    for mut face in loaded.drain(..) {
        fonts.db_mut().remove_face(face.id);
        if let Some((_, language)) = face.families.first().cloned() {
            face.families.insert(0, (font.family.clone(), language));
        }
        face.weight = Weight(font.weight.clamp(1, 1000));
        face.style = if font.italic {
            Style::Italic
        } else {
            Style::Normal
        };
        fonts.db_mut().push_face_info(face);
    }
}

fn whitespace_glyph_positions(text: &str, glyphs: &[cosmic_text::LayoutGlyph]) -> Vec<f32> {
    glyphs
        .iter()
        .filter(|glyph| {
            text.get(glyph.start..glyph.end)
                .is_some_and(|cluster| cluster.chars().any(char::is_whitespace))
        })
        .map(|glyph| glyph.x)
        .collect()
}

fn word_spacing_shift(x: f32, spaces: &[f32], spacing: f32) -> f32 {
    spacing * spaces.iter().filter(|space_x| **space_x < x).count() as f32
}

fn positioned_glyph(
    physical_x: i32,
    physical_y: i32,
    scale: f32,
    asset: GlyphAsset,
) -> PositionedGlyph {
    PositionedGlyph {
        raster_id: asset.id,
        x: (physical_x + asset.left) as f32 / scale,
        y: (physical_y - asset.top) as f32 / scale,
        width: asset.width as f32 / scale,
        height: asset.height as f32 / scale,
        color: asset.color,
    }
}

#[cfg(test)]
mod tests;
