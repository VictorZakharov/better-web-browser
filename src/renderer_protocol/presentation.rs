//! Validated immutable renderer output retained by the browser process.

mod coalescing;
pub(super) mod codec;
mod diagnostics;
#[cfg(test)]
mod image_tests;
mod layout;
mod layout_sanitize;
mod reader;

pub use diagnostics::{
    AttributeDiagnostics, CustomPropertyDiagnostics, NodeDiagnostics, NodeIdentityDiagnostics,
    PageDiagnostics, ResourceDiagnostics, SelectorDiagnostics, ShadowRootDiagnostics,
    StyleDiagnostics,
};

use super::{AccessibilityUpdate, DocumentId, ProtocolError};
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
    pub media: Option<MediaRuntimeReport>,
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
    pub frames_presented: u64,
    pub dropped_frames: u64,
    pub width: u32,
    pub height: u32,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::engine::{FontSpec, PositionedGlyph, RectF};
    use crate::renderer_protocol::{DocumentNodeId, SemanticActions, SemanticNode, SemanticRole};

    pub(super) fn sample() -> RendererPresentation {
        let url = "https://example.test/".to_string();
        let accessibility_root = DocumentNodeId::new((1_u128 << 64) | 1).unwrap();
        RendererPresentation {
            document: DocumentId::new(1).unwrap(),
            revision: 1,
            clock_advanced: false,
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
                    node_id: None,
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
            runtime: RuntimeReport {
                media: Some(MediaRuntimeReport {
                    active: true,
                    playing: true,
                    current_time_100ns: 12_000_000,
                    duration_100ns: 30_000_000,
                    backend: "test backend".into(),
                    mime_type: "video/mp4".into(),
                    video_codec: "H.264".into(),
                    audio_codec: "AAC-LC".into(),
                    encoded_queue_bytes: 1024,
                    encoded_queue_limit_bytes: 2048,
                    decoded_frame_queue_limit: 1,
                    frames_presented: 2,
                    width: 320,
                    height: 240,
                    ..MediaRuntimeReport::default()
                }),
                ..RuntimeReport::default()
            },
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
            page_diagnostics: PageDiagnostics {
                error: None,
                selectors: vec![SelectorDiagnostics {
                    selector: "#main".into(),
                    total_matches: 1,
                    matches: vec![NodeDiagnostics {
                        attribute_count: 2,
                        attributes: vec![
                            AttributeDiagnostics {
                                name: "id".into(),
                                value: "main".into(),
                            },
                            AttributeDiagnostics {
                                name: "hidden".into(),
                                value: String::new(),
                            },
                        ],
                        shadow_root: Some(ShadowRootDiagnostics {
                            child_count: 1,
                            descendant_count: 3,
                            text_length: 12,
                        }),
                        ..NodeDiagnostics::default()
                    }],
                    ..SelectorDiagnostics::default()
                }],
            },
            accessibility: AccessibilityUpdate {
                full: true,
                root: accessibility_root,
                focus: accessibility_root,
                nodes: vec![SemanticNode {
                    id: accessibility_root,
                    role: SemanticRole::RootWebArea,
                    name: "glyphs".into(),
                    value: String::new(),
                    description: String::new(),
                    bounds: RectF {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 100.0,
                    },
                    children: Vec::new(),
                    level: None,
                    disabled: false,
                    read_only: false,
                    actions: SemanticActions::default(),
                    selection: None,
                }],
                added: Vec::new(),
                removed: Vec::new(),
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
        assert_eq!(decoded.runtime.media, sample().runtime.media);
        assert_eq!(decoded.page_diagnostics, sample().page_diagnostics);
        assert_eq!(decoded.accessibility, sample().accessibility);
    }

    #[test]
    fn balanced_opacity_groups_round_trip_through_the_checked_codec() {
        let mut presentation = sample();
        let bounds = RectF {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        };
        presentation.layout.items.insert(
            0,
            DisplayItem::BeginOpacity {
                bounds,
                opacity: 0.5,
            },
        );
        presentation
            .layout
            .items
            .push(DisplayItem::EndOpacity { bounds });

        let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
        assert!(matches!(
            decoded.layout.items.as_slice(),
            [
                DisplayItem::BeginOpacity { opacity, .. },
                DisplayItem::Text { .. },
                DisplayItem::EndOpacity { .. }
            ] if (*opacity - 0.5).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn nonnegative_subpixel_font_sizes_round_trip_through_the_checked_codec() {
        for size in [0.0, 0.25] {
            let mut presentation = sample();
            let DisplayItem::Text { font, .. } = &mut presentation.layout.items[0] else {
                panic!("sample presentation should contain text");
            };
            font.size = size;

            let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
            let DisplayItem::Text { font, .. } = &decoded.layout.items[0] else {
                panic!("text item was not preserved");
            };
            assert_eq!(font.size, size);
        }
    }

    #[test]
    fn negative_font_sizes_still_fail_closed() {
        let mut presentation = sample();
        let DisplayItem::Text { font, .. } = &mut presentation.layout.items[0] else {
            panic!("sample presentation should contain text");
        };
        font.size = -0.25;
        let bytes = presentation.encode().unwrap();

        assert!(matches!(
            RendererPresentation::decode(&bytes),
            Err(ProtocolError::InvalidPayload("font size"))
        ));
    }

    #[test]
    fn invalid_accessibility_semantics_fail_closed() {
        let mut presentation = sample();
        presentation
            .accessibility
            .nodes
            .push(presentation.accessibility.nodes[0].clone());
        assert!(matches!(
            presentation.encode(),
            Err(ProtocolError::InvalidPayload("accessibility node"))
        ));

        presentation.accessibility.nodes.truncate(1);
        presentation.accessibility.nodes[0].bounds.width = f32::INFINITY;
        assert!(matches!(
            presentation.encode(),
            Err(ProtocolError::InvalidPayload("accessibility bounds"))
        ));

        let mut presentation = sample();
        presentation.accessibility.nodes[0].name =
            "x".repeat(crate::limits::MAX_ACCESSIBILITY_NODE_TEXT_BYTES + 1);
        assert!(matches!(
            presentation.encode(),
            Err(ProtocolError::InvalidPayload("accessibility node text"))
        ));
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

    #[test]
    fn oversized_page_diagnostics_fail_closed() {
        let mut presentation = sample();
        presentation.page_diagnostics.selectors.clear();
        presentation.page_diagnostics.error =
            Some("x".repeat(crate::limits::MAX_PAGE_DIAGNOSTIC_BYTES));
        assert!(matches!(
            presentation.encode(),
            Err(ProtocolError::InvalidPayload("page diagnostics"))
        ));
    }

    #[test]
    fn inconsistent_diagnostic_attribute_counts_fail_closed() {
        let mut presentation = sample();
        let node = &mut presentation.page_diagnostics.selectors[0].matches[0];
        node.attributes_truncated = true;

        assert!(matches!(
            presentation.encode(),
            Err(ProtocolError::InvalidPayload("page diagnostics"))
        ));
    }
}
