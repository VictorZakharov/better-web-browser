use super::{
    PageLoadReport, RendererPresentation, RendererRuntimeUpdate, RuntimeReport, StyleReport,
};
use crate::limits::MAX_RUNTIME_REPORT_ENTRIES;
use crate::renderer_protocol::ProtocolError;
mod metadata;
mod resources;

impl RendererPresentation {
    /// Combines queued renderer output while preserving the observable effect of processing each
    /// presentation in order. The latest presentation owns snapshot state, while edge-triggered
    /// runtime output, metrics, and one-shot resources must survive queue compaction. A second
    /// result means the resource union would exceed its budget: deliver both originals in order.
    pub(crate) fn coalesce(
        mut self,
        mut next: RendererPresentation,
    ) -> Result<(RendererPresentation, Option<RendererPresentation>), ProtocolError> {
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

        if !metadata::presentation_merge_is_bounded(&self, &next) {
            return Ok((self, Some(next)));
        }
        resources::merge(&mut self, &mut next);
        next.accessibility = self.accessibility.coalesce(next.accessibility)?;
        next.clock_advanced |= self.clock_advanced;
        next.runtime = self.runtime.coalesce(next.runtime)?;
        next.style = self.style.coalesce(next.style);
        next.load = self.load.coalesce(next.load);
        Ok((next, None))
    }

    pub(crate) fn one_shot_resource_bytes(&self) -> usize {
        resources::resource_bytes(self).saturating_add(metadata::runtime_bytes(&self.runtime))
    }
}

impl RuntimeReport {
    pub(crate) fn coalesce(mut self, mut next: Self) -> Result<Self, ProtocolError> {
        super::wheel::validate(&self.wheel_acknowledgements)?;
        if next.viewport_scroll_y.is_some() {
            // The same absolute-position barrier resets the aggregate below. Keep
            // verdicts observable, but do not reapply their superseded defaults.
            // Incoming contributions follow the new anchor and remain intact.
            for value in &mut self.wheel_acknowledgements {
                value.viewport_delta_y = 0.0;
            }
        }
        self.wheel_acknowledgements
            .append(&mut next.wheel_acknowledgements);
        super::wheel::validate(&self.wheel_acknowledgements)?;
        next.wheel_acknowledgements = self.wheel_acknowledgements;
        for (prior, incoming) in [
            (self.errors.len(), next.errors.len()),
            (self.console.len(), next.console.len()),
            (self.diagnostics.len(), next.diagnostics.len()),
            (self.history_actions.len(), next.history_actions.len()),
            (self.cookie_updates.len(), next.cookie_updates.len()),
        ] {
            if prior
                .checked_add(incoming)
                .is_none_or(|total| total > MAX_RUNTIME_REPORT_ENTRIES)
            {
                return Err(ProtocolError::InvalidPayload(
                    "coalesced runtime entry count",
                ));
            }
        }
        if self.history_traversal_ack.is_some()
            && next.history_traversal_ack.is_some()
            && self.history_traversal_ack != next.history_traversal_ack
        {
            return Err(ProtocolError::InvalidPayload(
                "coalesced history acknowledgements",
            ));
        }
        if self.native_text_rejection.is_some() && next.native_text_rejection.is_some() {
            return Err(ProtocolError::InvalidPayload(
                "coalesced native text rejections",
            ));
        }
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
            next.navigation_options = self.navigation_options;
        }
        if next.viewport_scroll_y.is_none() {
            next.viewport_scroll_y = self.viewport_scroll_y;
            next.viewport_wheel_delta_y =
                (self.viewport_wheel_delta_y as f64 + next.viewport_wheel_delta_y as f64)
                    .clamp(-(f32::MAX as f64), f32::MAX as f64) as f32;
        }
        self.history_actions.append(&mut next.history_actions);
        next.history_actions = self.history_actions;
        if next.history_traversal_ack.is_none() {
            next.history_traversal_ack = self.history_traversal_ack;
        }
        if next.native_text_rejection.is_none() {
            next.native_text_rejection = self.native_text_rejection;
        }
        self.cookie_updates.append(&mut next.cookie_updates);
        next.cookie_updates = self.cookie_updates;
        next.runtime_stopped |= self.runtime_stopped;
        next.render_requested |= self.render_requested;
        Ok(next)
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
            script_fetch_micros: self
                .script_fetch_micros
                .saturating_add(next.script_fetch_micros),
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
    pub(crate) fn coalesce(self, mut next: Self) -> Result<(Self, Option<Self>), ProtocolError> {
        if self.document != next.document {
            return Err(ProtocolError::InvalidPayload(
                "runtime-update coalescing document",
            ));
        }
        if !metadata::runtime_merge_is_bounded(&self, &next) {
            return Ok((self, Some(next)));
        }
        next.clock_advanced |= self.clock_advanced;
        next.runtime = self.runtime.coalesce(next.runtime)?;
        next.load = self.load.coalesce(next.load);
        Ok((next, None))
    }

    pub(crate) fn one_shot_resource_bytes(&self) -> usize {
        metadata::runtime_update_overhead(self)
            .saturating_add(metadata::runtime_bytes(&self.runtime))
    }
}

#[cfg(test)]
mod tests;
