use super::{
    PageLoadReport, RendererPresentation, RendererRuntimeUpdate, RuntimeReport, StyleReport,
};
use crate::renderer_protocol::ProtocolError;

impl RendererPresentation {
    /// Combines queued renderer output while preserving the observable effect of processing each
    /// presentation in order. The latest presentation owns snapshot state, while edge-triggered
    /// runtime output, metrics, and one-shot resources must survive queue compaction.
    pub(crate) fn coalesce(
        mut self,
        mut next: RendererPresentation,
    ) -> Result<RendererPresentation, ProtocolError> {
        if self.document != next.document {
            return Err(ProtocolError::InvalidPayload(
                "presentation coalescing document",
            ));
        }
        if self.revision >= next.revision {
            return Err(ProtocolError::InvalidPayload(
                "presentation coalescing revision",
            ));
        }

        next.accessibility = self.accessibility.coalesce(next.accessibility)?;
        next.runtime = self.runtime.coalesce(next.runtime);
        next.style = self.style.coalesce(next.style);
        next.load = self.load.coalesce(next.load);
        self.images.append(&mut next.images);
        next.images = self.images;
        if self.glyph_epoch == next.glyph_epoch {
            self.glyphs.append(&mut next.glyphs);
            next.glyphs = self.glyphs;
        }
        Ok(next)
    }
}

impl RuntimeReport {
    pub(crate) fn coalesce(mut self, mut next: Self) -> Self {
        next.scripts_executed = self.scripts_executed.saturating_add(next.scripts_executed);
        next.dom_mutations = self.dom_mutations.saturating_add(next.dom_mutations);
        self.errors.append(&mut next.errors);
        next.errors = self.errors;
        self.console.append(&mut next.console);
        next.console = self.console;
        self.diagnostics.append(&mut next.diagnostics);
        next.diagnostics = self.diagnostics;
        if next.navigation_url.is_none() {
            next.navigation_url = self.navigation_url;
        }
        self.cookie_updates.append(&mut next.cookie_updates);
        next.cookie_updates = self.cookie_updates;
        next.runtime_stopped |= self.runtime_stopped;
        next.render_requested |= self.render_requested;
        next
    }
}

impl StyleReport {
    fn coalesce(self, next: Self) -> Self {
        Self {
            invalidated_nodes: self
                .invalidated_nodes
                .saturating_add(next.invalidated_nodes),
            total_styles: self.total_styles.saturating_add(next.total_styles),
            recomputed_styles: self
                .recomputed_styles
                .saturating_add(next.recomputed_styles),
            changed_styles: self.changed_styles.saturating_add(next.changed_styles),
            removed_styles: self.removed_styles.saturating_add(next.removed_styles),
            layout_changed: self.layout_changed || next.layout_changed,
            full_rebuild: self.full_rebuild || next.full_rebuild,
        }
    }
}

impl PageLoadReport {
    pub(crate) fn coalesce(self, next: Self) -> Self {
        Self {
            parse_micros: self.parse_micros.saturating_add(next.parse_micros),
            html_parse_micros: self
                .html_parse_micros
                .saturating_add(next.html_parse_micros),
            resource_processing_micros: self
                .resource_processing_micros
                .saturating_add(next.resource_processing_micros),
            script_micros: self.script_micros.saturating_add(next.script_micros),
            style_micros: self.style_micros.saturating_add(next.style_micros),
            layout_micros: self.layout_micros.saturating_add(next.layout_micros),
            text_measure_count: self
                .text_measure_count
                .saturating_add(next.text_measure_count),
            text_shape_cache_hits: self
                .text_shape_cache_hits
                .saturating_add(next.text_shape_cache_hits),
            text_shape_cache_misses: self
                .text_shape_cache_misses
                .saturating_add(next.text_shape_cache_misses),
            text_shape_cache_flushes: self
                .text_shape_cache_flushes
                .saturating_add(next.text_shape_cache_flushes),
            text_shape_cache_entries: next.text_shape_cache_entries,
            font_catalog_micros: self
                .font_catalog_micros
                .saturating_add(next.font_catalog_micros),
            font_select_micros: self
                .font_select_micros
                .saturating_add(next.font_select_micros),
            open_type_shape_micros: self
                .open_type_shape_micros
                .saturating_add(next.open_type_shape_micros),
            glyph_raster_micros: self
                .glyph_raster_micros
                .saturating_add(next.glyph_raster_micros),
            presentation_encode_micros: self
                .presentation_encode_micros
                .saturating_add(next.presentation_encode_micros),
            presentation_decode_micros: self
                .presentation_decode_micros
                .saturating_add(next.presentation_decode_micros),
        }
    }
}

impl RendererRuntimeUpdate {
    pub(crate) fn coalesce(self, mut next: Self) -> Result<Self, ProtocolError> {
        if self.document != next.document {
            return Err(ProtocolError::InvalidPayload(
                "runtime-update coalescing document",
            ));
        }
        next.runtime = self.runtime.coalesce(next.runtime);
        next.load = self.load.coalesce(next.load);
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::DecodedImage;
    use crate::renderer_protocol::PresentedImage;
    use crate::renderer_protocol::presentation::tests::sample;

    #[test]
    fn preserves_ordered_deltas_and_one_shot_resources() {
        let mut first = sample();
        first.runtime = RuntimeReport {
            scripts_executed: 2,
            dom_mutations: 3,
            errors: vec!["first error".into()],
            console: vec!["first console".into()],
            diagnostics: vec!["first diagnostic".into()],
            navigation_url: Some("https://example.test/redirect".into()),
            cookie_updates: vec!["first=1".into()],
            runtime_active: true,
            render_requested: true,
            ..RuntimeReport::default()
        };
        first.style.invalidated_nodes = 2;
        first.style.total_styles = 3;
        first.load.parse_micros = 5;
        first.load.text_measure_count = 6;
        first.load.text_shape_cache_entries = 7;
        first.images.push(PresentedImage {
            url: "https://example.test/first.png".into(),
            image: DecodedImage {
                width: 1,
                height: 1,
                bgra: vec![1; 4],
            },
        });

        let mut next = sample();
        next.revision = 2;
        next.title = "newest snapshot".into();
        next.runtime = RuntimeReport {
            scripts_executed: 4,
            dom_mutations: 5,
            errors: vec!["next error".into()],
            console: vec!["next console".into()],
            diagnostics: vec!["next diagnostic".into()],
            cookie_updates: vec!["next=2".into()],
            runtime_stopped: true,
            ..RuntimeReport::default()
        };
        next.style.invalidated_nodes = 4;
        next.style.total_styles = 6;
        next.load.parse_micros = 9;
        next.load.text_measure_count = 10;
        next.load.text_shape_cache_entries = 11;
        next.images.push(PresentedImage {
            url: "https://example.test/next.png".into(),
            image: DecodedImage {
                width: 1,
                height: 1,
                bgra: vec![2; 4],
            },
        });
        next.glyphs[0].id = 2;

        let combined = first.coalesce(next).unwrap();
        assert_eq!(combined.revision, 2);
        assert_eq!(combined.title, "newest snapshot");
        assert_eq!(combined.runtime.scripts_executed, 6);
        assert_eq!(combined.runtime.dom_mutations, 8);
        assert_eq!(combined.runtime.errors, ["first error", "next error"]);
        assert_eq!(combined.runtime.console, ["first console", "next console"]);
        assert_eq!(
            combined.runtime.diagnostics,
            ["first diagnostic", "next diagnostic"]
        );
        assert_eq!(combined.runtime.cookie_updates, ["first=1", "next=2"]);
        assert_eq!(
            combined.runtime.navigation_url.as_deref(),
            Some("https://example.test/redirect")
        );
        assert!(!combined.runtime.runtime_active);
        assert!(combined.runtime.runtime_stopped);
        assert!(combined.runtime.render_requested);
        assert_eq!(combined.style.invalidated_nodes, 6);
        assert_eq!(combined.style.total_styles, 9);
        assert_eq!(combined.load.parse_micros, 14);
        assert_eq!(combined.load.text_measure_count, 16);
        assert_eq!(combined.load.text_shape_cache_entries, 11);
        assert_eq!(
            combined
                .images
                .iter()
                .map(|image| image.url.as_str())
                .collect::<Vec<_>>(),
            [
                "https://example.test/first.png",
                "https://example.test/next.png"
            ]
        );
        assert_eq!(
            combined
                .glyphs
                .iter()
                .map(|glyph| glyph.id)
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn drops_glyphs_from_an_obsolete_epoch() {
        let first = sample();
        let mut next = sample();
        next.revision = 2;
        next.glyph_epoch = 2;
        next.glyphs[0].id = 2;

        let combined = first.coalesce(next).unwrap();
        assert_eq!(combined.glyph_epoch, 2);
        assert_eq!(combined.glyphs.len(), 1);
        assert_eq!(combined.glyphs[0].id, 2);
    }
}
