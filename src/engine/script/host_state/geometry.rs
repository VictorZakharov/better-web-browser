//! Synchronous layout checkpoints and opt-in phase timing.
use super::*;

#[derive(Default)]
pub struct LayoutFlushMetrics {
    pub(crate) style: Duration,
    pub(crate) layout: Duration,
}

pub(crate) type LayoutFlushCallback =
    Box<dyn FnMut(&RenderInvalidation, &mut LayoutFlushMetrics) -> Option<HashMap<NodeId, RectF>>>;

impl HostState {
    pub(in crate::engine::script) fn flush_layout_if_needed(&mut self) {
        let version = self.document.subtree_mutation_version();
        if self.layout_geometry_initialized && self.layout_geometry_version == version {
            return;
        }
        let Some(flush) = self.layout_flush.as_mut() else {
            return;
        };
        let invalidation = self.pending_layout_invalidation.take(self.mutation_count);
        let mut metrics = LayoutFlushMetrics::default();
        if let Some(geometry) = flush(&invalidation, &mut metrics) {
            self.layout_geometry = geometry;
        }
        self.host_call_profile
            .record_elapsed("layoutFlush::style", metrics.style);
        self.host_call_profile
            .record_elapsed("layoutFlush::layout", metrics.layout);
        self.layout_geometry_version = version;
        self.layout_geometry_initialized = true;
    }
}
