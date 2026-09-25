//! Synchronous layout checkpoints and opt-in phase timing.
use super::*;

#[derive(Default)]
pub struct LayoutFlushMetrics {
    pub(crate) hit_test_snapshot: Option<crate::engine::layout::HitTestSnapshot>,
    pub(crate) hit_test_requested: bool,
    pub(crate) fragments: Option<std::sync::Arc<crate::engine::layout::FragmentGeometry>>,
    pub(crate) sticky_offsets: Option<HashMap<NodeId, (f32, f32)>>,
    pub(crate) scroll_changed: bool,
    pub(crate) scroll_boxes: Option<HashMap<NodeId, crate::engine::layout::ScrollBox>>,
    pub(crate) resize_boxes: Option<HashMap<NodeId, crate::engine::layout::ResizeBox>>,
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
        self.flush_layout(false);
    }

    pub(in crate::engine::script) fn flush_hit_test_if_needed(&mut self) {
        self.flush_layout(true);
    }

    fn flush_layout(&mut self, hit_test_requested: bool) {
        let version = self.document.subtree_mutation_version();
        let scroll_changed = !self.sticky_offsets.is_empty()
            && (self.geometry_scroll_dirty
                || self.geometry_scroll_offset != self.document.scroll_offset.get());
        if self.layout_geometry_initialized
            && self.layout_geometry_version == version
            && !scroll_changed
            && (!hit_test_requested || self.hit_test_geometry_version == Some(version))
        {
            return;
        }
        let Some(flush) = self.layout_flush.as_mut() else {
            return;
        };
        let invalidation = self.pending_layout_invalidation.take(self.mutation_count);
        let mut metrics = LayoutFlushMetrics {
            scroll_changed,
            hit_test_requested,
            profile: self.host_call_profile.is_enabled(),
            ..LayoutFlushMetrics::default()
        };
        if let Some(geometry) = flush(&invalidation, &mut metrics) {
            self.layout_geometry = geometry;
            self.layout_fragments = metrics.fragments.take().unwrap_or_default();
        }
        if let Some(snapshot) = metrics.hit_test_snapshot.take() {
            self.hit_test_snapshot = snapshot;
            self.hit_test_geometry_version = Some(version);
        }
        if let Some(boxes) = metrics.resize_boxes {
            self.resize_boxes = boxes;
        }
        if let Some(boxes) = metrics.scroll_boxes {
            self.scroll_boxes = boxes;
        }
        if let Some(offsets) = metrics.sticky_offsets {
            self.sticky_offsets = offsets;
        }
        self.geometry_scroll_offset = self.document.scroll_offset.get();
        self.geometry_scroll_dirty = false;
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
