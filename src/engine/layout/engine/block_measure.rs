//! Intrinsic block-size probes do not publish paint, hit-test, or observer state.
use super::*;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(super) struct MeasureKey {
    node: NodeId,
    width: u32,
    height_basis: Option<u32>,
    used_width: Option<(u32, u32)>,
}

impl MeasureKey {
    pub(super) fn node(&self) -> NodeId {
        self.node
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn intrinsic_block_height(
        &mut self,
        node: &NodeRef,
        width: f32,
        height_basis: Option<f32>,
        used_width: Option<UsedInlineSize>,
    ) -> f32 {
        let height_basis = self.read_height_basis(node, height_basis);
        let key = MeasureKey {
            node: node.id(),
            width: width.to_bits(),
            height_basis: height_basis.map(f32::to_bits),
            used_width: used_width
                .map(|size| (size.outer.to_bits(), size.percentage_basis.to_bits())),
        };
        if let Some((height, reads)) = self.intrinsic_block_heights.get(&key) {
            self.replay_reads(reads);
            return *height;
        }
        let cached = self.cached_probe(key, node);
        if let super::probe_cache::CachedProbe::Hit(height, reads) = cached {
            self.replay_reads(&reads);
            self.intrinsic_block_heights.insert(key, (height, reads));
            return height;
        }
        // Share per-layout caches with the probe: nested grids/tables reuse the same
        // intrinsic measurements when their resolved boxes are laid out for painting.
        let mut probe = LayoutEngine {
            page: self.page,
            styles: self.styles,
            measurer: &mut *self.measurer,
            emit_paint: false,
            retain_fragments: false,
            scroll_gutters: HashMap::new(),
            measurement_cache: std::mem::take(&mut self.measurement_cache),
            intrinsic_block_heights: std::mem::take(&mut self.intrinsic_block_heights),
            intrinsic_widths: std::mem::take(&mut self.intrinsic_widths),
            margin_profiles: std::mem::take(&mut self.margin_profiles),
            inline_box_cache: std::mem::take(&mut self.inline_box_cache),
            viewport: self.viewport,
            output: LayoutOutput::default(),
            positioned_flow_scopes: Vec::new(),
            floats: Default::default(),
            reads: std::mem::take(&mut self.reads),
        };
        probe.begin_reads();
        let height = probe
            .layout_block(node, 0.0, 0.0, width, height_basis, used_width)
            .bottom;
        let reads = probe.end_reads();
        self.reads = probe.reads;
        self.measurement_cache = probe.measurement_cache;
        self.inline_box_cache = probe.inline_box_cache;
        self.intrinsic_block_heights = probe.intrinsic_block_heights;
        self.intrinsic_widths = probe.intrinsic_widths;
        self.margin_profiles = probe.margin_profiles;
        if let super::probe_cache::CachedProbe::Miss(inputs) = cached {
            self.remember_probe(key, inputs, height, reads.clone());
        }
        self.intrinsic_block_heights.insert(key, (height, reads));
        height
    }
}
