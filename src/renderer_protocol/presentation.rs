//! Validated immutable renderer output retained by the browser process.

mod codec;
mod layout;
mod reader;

use super::{DocumentId, ProtocolError};
use crate::document::Document;
use crate::engine::css::Color;
use crate::engine::{DecodedImage, DisplayItem, FormSpec, LayoutOutput};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeReport {
    pub scripts_executed: u64,
    pub dom_mutations: u64,
    pub errors: Vec<String>,
    pub console: Vec<String>,
    pub diagnostics: Vec<String>,
    pub navigation_url: Option<String>,
    pub cookie_updates: Vec<String>,
    pub runtime_active: bool,
    pub runtime_stopped: bool,
    pub render_requested: bool,
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
    pub style_micros: u64,
    pub layout_micros: u64,
    pub text_measure_count: u64,
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
        Self {
            items: layout.items,
            content_height: layout.content_height,
            background: layout.background,
            forms: layout.forms.into_values().collect(),
        }
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
        }
    }
}

#[derive(Clone, Debug)]
pub struct PresentedImage {
    pub url: String,
    pub image: DecodedImage,
}

#[derive(Clone, Debug)]
pub struct RendererPresentation {
    pub document: DocumentId,
    pub revision: u64,
    pub title: String,
    pub final_url: String,
    pub status: u16,
    pub character_set: String,
    pub reader: Document,
    pub layout: PresentedLayout,
    pub images: Vec<PresentedImage>,
    pub runtime: RuntimeReport,
    pub style: StyleReport,
    pub load: PageLoadReport,
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
