//! Validated immutable renderer output retained by the browser process.

mod coalescing;
pub(super) mod codec;
mod diagnostics;
#[cfg(test)]
mod image_tests;
mod layout;
mod layout_sanitize;
mod reader;
mod runtime_codec;

pub use diagnostics::{
    AttributeDiagnostics, CustomPropertyDiagnostics, NodeDiagnostics, NodeIdentityDiagnostics,
    PageDiagnostics, ResourceDiagnostics, SelectorDiagnostics, ShadowRootDiagnostics,
    StyleDiagnostics,
};

use super::{AccessibilityUpdate, DocumentId, ProtocolError};
use crate::document::Document;
use crate::engine::css::Color;
use crate::engine::{DecodedImage, DisplayItem, FormSpec, LayoutOutput};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeReport {
    pub scripts_executed: u64,
    pub dom_mutations: u64,
    pub errors: Vec<String>,
    pub console: Vec<String>,
    pub diagnostics: Vec<String>,
    pub navigation_url: Option<String>,
    pub viewport_scroll_y: Option<f32>,
    pub history_updates: Vec<HistoryUpdate>,
    pub cookie_updates: Vec<String>,
    pub runtime_active: bool,
    pub runtime_stopped: bool,
    pub render_requested: bool,
    pub media: Option<MediaRuntimeReport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryUpdate {
    pub url: String,
    pub replace: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaRuntimeReport {
    pub active: bool,
    pub playing: bool,
    pub ended: bool,
    pub current_time_100ns: u64,
    pub duration_100ns: u64,
    pub backend: String,
    pub mime_type: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub encoded_queue_bytes: u64,
    pub encoded_queue_limit_bytes: u64,
    pub decoded_frame_queue_depth: u16,
    pub decoded_frame_queue_limit: u16,
    pub frames_submitted: u64,
    pub dropped_frames: u64,
    pub width: u32,
    pub height: u32,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RendererRuntimeUpdate {
    pub document: DocumentId,
    /// True only when this output completes a browser-requested monotonic clock advance.
    pub clock_advanced: bool,
    pub runtime: RuntimeReport,
    pub load: PageLoadReport,
    pub next_timer_micros: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleReport {
    pub invalidated_nodes: u64,
    pub total_styles: u64,
    pub recomputed_styles: u64,
    pub changed_styles: u64,
    pub removed_styles: u64,
    pub layout_changed: bool,
    pub full_rebuild: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageLoadReport {
    pub parse_micros: u64,
    pub html_parse_micros: u64,
    pub resource_processing_micros: u64,
    pub script_micros: u64,
    pub script_fetch_micros: u64,
    pub style_micros: u64,
    pub layout_micros: u64,
    pub text_measure_count: u64,
    pub text_shape_cache_hits: u64,
    pub text_shape_cache_misses: u64,
    pub text_shape_cache_flushes: u64,
    pub text_shape_cache_entries: u64,
    pub font_catalog_micros: u64,
    pub font_select_micros: u64,
    pub open_type_shape_micros: u64,
    pub glyph_raster_micros: u64,
    pub presentation_encode_micros: u64,
    pub presentation_decode_micros: u64,
}

#[derive(Clone, Debug, Default)]
pub struct PresentedLayout {
    pub items: Vec<DisplayItem>,
    pub content_height: f32,
    pub background: Color,
    pub forms: Vec<FormSpec>,
}

impl PresentedLayout {
    pub fn from_layout(layout: LayoutOutput) -> Self {
        layout_sanitize::sanitize(layout)
    }

    pub fn into_layout(self) -> LayoutOutput {
        LayoutOutput {
            items: self.items,
            content_height: self.content_height,
            background: self.background,
            forms: self
                .forms
                .into_iter()
                .map(|form| (form.node_id, form))
                .collect(),
            node_bounds: Default::default(),
            node_paint_order: Default::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PresentedImage {
    pub url: String,
    pub image: DecodedImage,
}

/// A renderer-rasterized glyph resource. Mask glyphs are tinted by the browser while color
/// glyphs (for example emoji) retain their renderer-produced premultiplied BGRA pixels.
#[derive(Clone, Debug)]
pub struct PresentedGlyphRaster {
    pub id: u32,
    pub image: DecodedImage,
    pub color: bool,
}
#[derive(Clone, Debug)]
pub struct RendererPresentation {
    pub document: DocumentId,
    pub revision: u64,
    /// True only when this output completes a browser-requested monotonic clock advance.
    pub clock_advanced: bool,
    pub title: String,
    pub final_url: String,
    pub status: u16,
    pub character_set: String,
    pub reader: Document,
    pub layout: PresentedLayout,
    pub images: Vec<PresentedImage>,
    pub glyph_epoch: u64,
    pub glyphs: Vec<PresentedGlyphRaster>,
    pub runtime: RuntimeReport,
    pub style: StyleReport,
    pub load: PageLoadReport,
    pub page_diagnostics: PageDiagnostics,
    pub accessibility: AccessibilityUpdate,
    pub next_timer_micros: Option<u64>,
}

impl RendererPresentation {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        codec::encode(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        codec::decode(bytes)
    }
}

#[cfg(test)]
mod tests;
