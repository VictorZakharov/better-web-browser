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
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::engine::{FontSpec, PositionedGlyph, RectF};

    fn sample() -> RendererPresentation {
        let url = "https://example.test/".to_string();
        RendererPresentation {
            document: DocumentId::new(1).unwrap(),
            revision: 1,
            title: "glyphs".into(),
            final_url: url.clone(),
            status: 200,
            character_set: "utf-8".into(),
            reader: Document {
                title: "glyphs".into(),
                source_url: url,
                blocks: Vec::new(),
                truncated: false,
            },
            layout: PresentedLayout {
                items: vec![DisplayItem::Text {
                    rect: RectF {
                        x: 1.0,
                        y: 2.0,
                        width: 3.0,
                        height: 4.0,
                    },
                    text: "A".into(),
                    font: FontSpec {
                        family: "sans-serif".into(),
                        size: 16.0,
                        weight: 400,
                        italic: false,
                        underline: false,
                        letter_spacing: 0.5,
                        word_spacing: 1.0,
                    },
                    color: Color::BLACK,
                    link: None,
                    raster_run_id: 7,
                    glyphs: vec![PositionedGlyph {
                        raster_id: 1,
                        x: 0.0,
                        y: 0.0,
                        width: 1.0,
                        height: 1.0,
                        color: false,
                    }],
                }],
                content_height: 10.0,
                background: Color::WHITE,
                forms: Vec::new(),
            },
            images: Vec::new(),
            glyph_epoch: 1,
            glyphs: vec![PresentedGlyphRaster {
                id: 1,
                image: DecodedImage {
                    width: 1,
                    height: 1,
                    bgra: vec![255; 4],
                },
                color: false,
            }],
            runtime: RuntimeReport::default(),
            style: StyleReport::default(),
            load: PageLoadReport {
                font_catalog_micros: 11,
                font_select_micros: 12,
                open_type_shape_micros: 13,
                glyph_raster_micros: 14,
                presentation_encode_micros: 15,
                presentation_decode_micros: 16,
                ..PageLoadReport::default()
            },
            next_timer_micros: None,
        }
    }

    #[test]
    fn glyph_runs_and_rasters_round_trip_through_the_checked_codec() {
        let decoded = RendererPresentation::decode(&sample().encode().unwrap()).unwrap();
        assert_eq!(decoded.glyph_epoch, 1);
        assert_eq!(decoded.glyphs.len(), 1);
        let DisplayItem::Text {
            raster_run_id,
            glyphs,
            font,
            ..
        } = &decoded.layout.items[0]
        else {
            panic!("text item was not preserved");
        };
        assert_eq!(*raster_run_id, 7);
        assert_eq!(glyphs[0].raster_id, 1);
        assert_eq!(font.letter_spacing, 0.5);
        assert_eq!(font.word_spacing, 1.0);
        assert_eq!(decoded.load.font_catalog_micros, 11);
        assert_eq!(decoded.load.font_select_micros, 12);
        assert_eq!(decoded.load.open_type_shape_micros, 13);
        assert_eq!(decoded.load.glyph_raster_micros, 14);
        assert_eq!(decoded.load.presentation_encode_micros, 15);
        assert_eq!(decoded.load.presentation_decode_micros, 16);
    }

    #[test]
    fn duplicate_or_zero_glyph_resources_fail_closed() {
        let mut presentation = sample();
        presentation.glyphs.push(presentation.glyphs[0].clone());
        assert!(matches!(
            presentation.encode(),
            Err(ProtocolError::InvalidPayload("glyph raster"))
        ));
        presentation.glyphs.truncate(1);
        presentation.glyphs[0].id = 0;
        assert!(presentation.encode().is_err());
    }
}
