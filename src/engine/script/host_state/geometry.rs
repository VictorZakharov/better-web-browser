//! Synchronous layout checkpoints and opt-in phase timing.
use super::*;

#[derive(Default)]
pub struct LayoutFlushMetrics {
    pub(crate) content_height: Option<f32>,
    pub(crate) profile: bool,
    pub(crate) style: Duration,
    pub(crate) layout: Duration,
    pub(crate) text_measure: Duration,
    pub(crate) layout_style_changed: bool,
    pub(crate) rebuilt_rules: bool,
    pub(crate) elements: Duration,
    pub(crate) pseudos: Duration,
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
        let mut metrics = LayoutFlushMetrics {
            profile: self.host_call_profile.is_enabled(),
            ..LayoutFlushMetrics::default()
        };
        if let Some(geometry) = flush(&invalidation, &mut metrics) {
            self.layout_geometry = geometry;
        }
        if let Some(height) = metrics.content_height {
            self.layout_content_height = height;
        }
        self.host_call_profile
            .record_elapsed("layoutFlush::style", metrics.style);
        self.host_call_profile.record_elapsed(
            if metrics.rebuilt_rules {
                "layoutFlush::style-full"
            } else {
                "layoutFlush::style-incremental"
            },
            metrics.style,
        );
        self.host_call_profile
            .record_elapsed("layoutFlush::layout", metrics.layout);
        self.host_call_profile
            .record_elapsed("layoutFlush::text-measure", metrics.text_measure);
        self.host_call_profile.record_elapsed(
            if metrics.layout_style_changed {
                "layoutFlush::layout-style-change"
            } else {
                "layoutFlush::layout-intrinsic-or-initial"
            },
            metrics.layout,
        );
        self.host_call_profile
            .record_elapsed("layoutFlush::elements", metrics.elements);
        self.host_call_profile
            .record_elapsed("layoutFlush::pseudos", metrics.pseudos);
        self.layout_geometry_version = version;
        self.layout_geometry_initialized = true;
    }
}
