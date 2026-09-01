//! Renderer-owned font discovery, shaping, fallback, and rasterization.
//!
//! Page font bytes and page text stay inside the AppContainer. The browser receives only bounded
//! glyph placements and premultiplied raster assets over the validated presentation protocol.

mod catalog;
mod raster;
mod shape;

use self::catalog::FontCatalog;
use self::raster::{GlyphRasterCache, RasterizedGlyph};
use self::shape::TextShaper;
use crate::engine::{FontSpec, PositionedGlyph, ShapedText, TextMeasurer, WebFont};
use crate::renderer_protocol::{PageLoadReport, PresentedGlyphRaster};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_SHAPE_CACHE_ENTRIES: usize = 16_384;
const MAX_SHAPE_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_MEASUREMENT_CACHE_BYTES: usize = 8 * 1024 * 1024;
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
    catalog: FontCatalog,
    shaper: TextShaper,
    rasters: GlyphRasterCache,
    shapes: HashMap<ShapeKey, ShapedText>,
    shape_cache_bytes: usize,
    measurements: HashMap<ShapeKey, (f32, f32)>,
    measurement_cache_bytes: usize,
    dpi: u32,
    glyph_epoch: u64,
    next_raster_run_id: u64,
    registered_web_fonts: usize,
    pending_font_catalog_time: Duration,
    measure_calls: usize,
    shape_cache_hits: usize,
    shape_cache_misses: usize,
    shape_cache_flushes: usize,
    font_select_time: Duration,
    open_type_time: Duration,
    glyph_raster_time: Duration,
}

impl RendererTextSystem {
    pub(in crate::renderer_process::child) fn new(dpi: u32) -> Self {
        let catalog_started = Instant::now();
        let catalog = FontCatalog::new();
        Self {
            catalog,
            shaper: TextShaper::default(),
            rasters: GlyphRasterCache::default(),
            shapes: HashMap::new(),
            shape_cache_bytes: 0,
            measurements: HashMap::new(),
            measurement_cache_bytes: 0,
            dpi: dpi.max(1),
            glyph_epoch: 1,
            next_raster_run_id: 1,
            registered_web_fonts: 0,
            pending_font_catalog_time: catalog_started.elapsed(),
            measure_calls: 0,
            shape_cache_hits: 0,
            shape_cache_misses: 0,
            shape_cache_flushes: 0,
            font_select_time: Duration::ZERO,
            open_type_time: Duration::ZERO,
            glyph_raster_time: Duration::ZERO,
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
        let started = Instant::now();
        if self.catalog.register_web_fonts(fonts) {
            self.pending_font_catalog_time += started.elapsed();
            self.shaper.clear();
            self.registered_web_fonts = fonts.len();
            self.reset_glyph_state();
        }
    }

    pub(super) fn reset_for_navigation(&mut self) {
        if self.catalog.reset_web_fonts() {
            self.shaper.clear();
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

    pub(super) fn reset_layout_metrics(&mut self) {
        self.measure_calls = 0;
        self.shape_cache_hits = 0;
        self.shape_cache_misses = 0;
        self.shape_cache_flushes = 0;
        self.font_select_time = Duration::ZERO;
        self.open_type_time = Duration::ZERO;
        self.glyph_raster_time = Duration::ZERO;
    }

    pub(super) fn finish_load_report(&mut self, mut report: PageLoadReport) -> PageLoadReport {
        report.text_measure_count = self.measure_calls as u64;
        report.text_shape_cache_hits = self.shape_cache_hits as u64;
        report.text_shape_cache_misses = self.shape_cache_misses as u64;
        report.text_shape_cache_flushes = self.shape_cache_flushes as u64;
        report.text_shape_cache_entries = self.shapes.len() as u64;
        report.font_catalog_micros = micros(std::mem::take(&mut self.pending_font_catalog_time));
        report.font_select_micros = micros(self.font_select_time);
        report.open_type_shape_micros = micros(self.open_type_time);
        report.glyph_raster_micros = micros(self.glyph_raster_time);
        report
    }

    fn reset_glyph_state(&mut self) {
        self.shapes.clear();
        self.shape_cache_bytes = 0;
        self.measurements.clear();
        self.measurement_cache_bytes = 0;
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
        let output = self.shaper.shape(&mut self.catalog, text, spec);
        self.font_select_time += output.font_select_time;
        self.open_type_time += output.open_type_time;
        let scale = self.dpi as f32 / 96.0;
        let mut shaped = ShapedText {
            raster_run_id: self.allocate_raster_run_id(),
            width: output.width,
            height: output.height,
            ..ShapedText::default()
        };
        let raster_started = Instant::now();
        for glyph in output.glyphs {
            let Some(raster) = self.rasters.get_or_insert(
                &glyph.font,
                glyph.glyph_id,
                font_size,
                glyph.x,
                glyph.baseline,
                self.dpi,
            ) else {
                continue;
            };
            shaped.glyphs.push(positioned_glyph(scale, raster));
        }
        self.glyph_raster_time += raster_started.elapsed();
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

    fn measure_text(&mut self, text: &str, spec: &FontSpec) -> (f32, f32) {
        self.measure_calls = self.measure_calls.saturating_add(1);
        let key = ShapeKey::new(text, spec);
        if let Some(shaped) = self.shapes.get(&key) {
            return (shaped.width, shaped.height);
        }
        if let Some(measurement) = self.measurements.get(&key) {
            return *measurement;
        }

        // CSSOM View geometry needs shaped advances, not pixels. Keep HarfRust shaping so line
        // breaks and element rectangles match painted layout, but defer glyph rasterization until
        // a real presentation asks for `shape`.
        let output = self.shaper.shape(&mut self.catalog, text, spec);
        self.font_select_time += output.font_select_time;
        self.open_type_time += output.open_type_time;
        let measurement = (output.width, output.height);
        if text.len() <= MAX_CACHED_TEXT_BYTES {
            let cached_bytes = measurement_cache_entry_bytes(&key);
            if cached_bytes <= MAX_MEASUREMENT_CACHE_BYTES {
                if self.measurement_cache_bytes.saturating_add(cached_bytes)
                    > MAX_MEASUREMENT_CACHE_BYTES
                {
                    self.measurements.clear();
                    self.measurement_cache_bytes = 0;
                }
                self.measurement_cache_bytes += cached_bytes;
                self.measurements.insert(key, measurement);
            }
        }
        measurement
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

fn measurement_cache_entry_bytes(key: &ShapeKey) -> usize {
    std::mem::size_of::<ShapeKey>()
        .saturating_add(key.text.len())
        .saturating_add(key.family.len())
        .saturating_add(std::mem::size_of::<(f32, f32)>())
}

impl TextMeasurer for RendererTextSystem {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        self.measure_text(text, font)
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

fn positioned_glyph(scale: f32, raster: RasterizedGlyph) -> PositionedGlyph {
    let asset = raster.asset;
    PositionedGlyph {
        raster_id: asset.id,
        x: (raster.origin_x + asset.left) as f32 / scale,
        y: (raster.origin_y - asset.top) as f32 / scale,
        width: asset.width as f32 / scale,
        height: asset.height as f32 / scale,
        color: asset.color,
    }
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests;
