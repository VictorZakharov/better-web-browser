//! Replays a block subtree's retained layout instead of laying it out again.
//
// A block that establishes its own formatting context and sits in normal flow (or is relatively
// positioned) lays out from its arguments alone: its floats start empty, its absolutely
// positioned descendants resolve against boxes inside it or against its own containing area,
// and nothing it emits depends on where it is placed except by translation. Its output is
// therefore a function of the box's subtree, its arguments and the inputs `reuse` validates, and
// a later pass can append a copy and translate it to the new position, exactly as an ancestor
// moves an already laid-out subtree (`translate_layout_subtree`).
//
// Subtrees containing viewport-dependent boxes (fixed or sticky positioning) or table-fixup
// boxes (whose identifiers are new in every pass) are not retained. Each retained subtree keeps
// its own copy of what it emitted, captured as soon as its layout finishes and before any
// ancestor moves it, so a nested subtree can still be replayed after an enclosing one changes.
// Entries are bounded by size so that very large containers recompute while their children
// replay.
use super::layout_reads::LayoutReads;
use super::reuse::{Inputs, LayoutReuse};
use super::*;

/// Subtrees emitting fewer items are cheaper to lay out than to copy and validate.
const MIN_ITEMS: usize = 8;
/// Larger containers recompute; their retained children still replay.
const MAX_ITEMS: usize = 4096;
const MAX_ENTRIES: usize = 16_384;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(in crate::engine::layout) struct SubtreeKey {
    pub(super) node: NodeId,
    paint: bool,
    fragments: bool,
    width: u32,
    height: Option<u32>,
    used_width: Option<(u32, u32)>,
    used_height: Option<u32>,
}

#[derive(Debug)]
pub(super) struct SubtreeEntry {
    inputs: Inputs,
    reads: LayoutReads,
    origin: (f32, f32),
    bottom: f32,
    items: Vec<DisplayItem>,
    paint_order: Vec<NodeId>,
    /// The box subtree, retained so a replay also keeps its descendants' entries.
    nodes: Vec<NodeId>,
    node_bounds: Vec<(NodeId, RectF)>,
    resize_boxes: Vec<(NodeId, ResizeBox)>,
    scroll_boxes: Vec<(NodeId, ScrollBox)>,
    clip_paths: Vec<(NodeId, RectF)>,
    transformed: transforms::Geometry,
    fragments: FragmentGeometry,
    first_line: u64,
    lines: u64,
    /// The stacking level a relatively positioned root reported to its parent's flow scope.
    flow_level: Option<i32>,
}

/// A layout in progress whose result may be retained.
pub(in crate::engine::layout) struct Capture {
    key: SubtreeKey,
    inputs: Inputs,
    origin: (f32, f32),
    item_start: usize,
    node_start: usize,
    first_line: u64,
    flow_ranges: usize,
}

impl LayoutReuse {
    fn subtree(
        &mut self,
        key: &SubtreeKey,
        inputs: Inputs,
        page: &Page,
        styles: &StyleSet,
    ) -> Option<&SubtreeEntry> {
        let entry = self.subtrees.get(key)?;
        if entry.inputs != inputs || !entry.reads.hold(page, styles) {
            self.subtrees.remove(key);
            return None;
        }
        self.used.extend(self.subtrees[key].nodes.iter().copied());
        self.subtrees.get(key)
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn subtree_key(
        &self,
        node: &NodeRef,
        width: f32,
        height: Option<f32>,
        used_width: Option<UsedInlineSize>,
        used_height: Option<f32>,
    ) -> SubtreeKey {
        SubtreeKey {
            node: node.id(),
            paint: self.emit_paint,
            fragments: self.retain_fragments,
            width: width.to_bits(),
            height: height.map(f32::to_bits),
            used_width: used_width
                .map(|size| (size.outer.to_bits(), size.percentage_basis.to_bits())),
            used_height: used_height.map(f32::to_bits),
        }
    }

    /// Whether `node` may be retained at all; checked before laying it out.
    fn replayable_root(&self, node: &NodeRef) -> bool {
        let style = self.styles.get(node);
        matches!(style.position, Position::Static | Position::Relative)
            && style.float == Float::None
            && style.display != Display::None
            && !node.is_generated_pseudo()
            && !matches!(node.tag_name(), Some("html" | "body"))
            && self.block_establishes_context(node)
    }

    /// Appends `node`'s retained layout at (`x`, `y`) when every input still matches.
    pub(in crate::engine::layout) fn replay_subtree(
        &mut self,
        key: &SubtreeKey,
        node: &NodeRef,
        x: f32,
        y: f32,
    ) -> Option<BlockMetrics> {
        if !self.replayable_root(node) {
            return None;
        }
        let inputs = self.reuse_inputs(node)?;
        let mut reuse = self.page.layout_reuse.borrow_mut();
        let entry = reuse.subtree(key, inputs, self.page, self.styles)?;
        let item_start = self.output.items.len();
        let node_start = self.output.node_paint_order.len();
        self.output.items.extend(entry.items.iter().cloned());
        self.output
            .node_paint_order
            .extend_from_slice(&entry.paint_order);
        self.output
            .node_bounds
            .extend(entry.node_bounds.iter().copied());
        self.output
            .resize_boxes
            .extend(entry.resize_boxes.iter().copied());
        self.output
            .scroll_boxes
            .extend(entry.scroll_boxes.iter().copied());
        self.output
            .clip_paths
            .extend(entry.clip_paths.iter().copied());
        self.output.transformed.extend(entry.transformed.clone());
        let fragments = std::sync::Arc::make_mut(&mut self.output.fragments);
        let first_line = fragments.next_line;
        let mut replayed = entry.fragments.clone();
        replayed.shift_lines(first_line.wrapping_sub(entry.first_line) as i64);
        fragments.extend_subtree(&replayed);
        fragments.next_line = first_line + entry.lines;
        let (dx, dy) = (x - entry.origin.0, y - entry.origin.1);
        let bottom = entry.bottom + dy;
        let flow_level = entry.flow_level;
        let reads = entry.reads.clone();
        drop(reuse);
        self.replay_reads(&reads);
        if dx != 0.0 || dy != 0.0 {
            let item_end = self.output.items.len();
            self.translate_layout_subtree(Some(node), item_start, item_end, dx, dy);
        }
        if let Some(level) = flow_level
            && let Some(parent) = self.positioned_flow_scopes.last_mut()
        {
            parent.push(InFlowPaintRange {
                node: node.id(),
                level,
                items: item_start..self.output.items.len(),
                nodes: node_start..self.output.node_paint_order.len(),
            });
        }
        Some(BlockMetrics { bottom })
    }

    /// Starts recording a layout of `node` that may be retained; pair with `finish_capture`.
    pub(in crate::engine::layout) fn begin_capture(
        &self,
        key: SubtreeKey,
        node: &NodeRef,
        x: f32,
        y: f32,
    ) -> Option<Capture> {
        if !self.replayable_root(node) {
            return None;
        }
        let inputs = self.reuse_inputs(node)?;
        self.begin_reads();
        Some(Capture {
            key,
            inputs,
            origin: (x, y),
            item_start: self.output.items.len(),
            node_start: self.output.node_paint_order.len(),
            first_line: self.output.fragments.next_line,
            flow_ranges: self.positioned_flow_scopes.last().map_or(0, Vec::len),
        })
    }

    pub(in crate::engine::layout) fn finish_capture(
        &mut self,
        capture: Capture,
        node: &NodeRef,
        metrics: BlockMetrics,
    ) {
        let reads = self.end_reads();
        let items = &self.output.items[capture.item_start..];
        if !(MIN_ITEMS..=MAX_ITEMS).contains(&items.len()) {
            return;
        }
        let nodes = self.styles.descendants(node);
        let retainable = nodes.iter().all(|descendant| {
            let style = self.styles.get(descendant);
            !self.styles.is_anonymous(descendant.id())
                && !matches!(style.position, Position::Fixed | Position::Sticky)
        });
        if !retainable {
            return;
        }
        let ids = nodes.iter().map(|node| node.id()).collect::<Vec<_>>();
        let flow_level = (self.positioned_flow_scopes.last().map_or(0, Vec::len)
            > capture.flow_ranges)
            .then(|| self.styles.get(node).z_index.unwrap_or(0));
        let entry = SubtreeEntry {
            inputs: capture.inputs,
            reads,
            origin: capture.origin,
            bottom: metrics.bottom,
            items: items.to_vec(),
            paint_order: self.output.node_paint_order[capture.node_start..].to_vec(),
            node_bounds: pick(&self.output.node_bounds, &ids),
            resize_boxes: pick(&self.output.resize_boxes, &ids),
            scroll_boxes: pick(&self.output.scroll_boxes, &ids),
            clip_paths: pick(&self.output.clip_paths, &ids),
            transformed: self.output.transformed.subset(&ids),
            fragments: self.output.fragments.subset(&ids),
            first_line: capture.first_line,
            lines: self.output.fragments.next_line - capture.first_line,
            flow_level,
            nodes: ids,
        };
        let mut reuse = self.page.layout_reuse.borrow_mut();
        if reuse.subtrees.len() < MAX_ENTRIES || reuse.subtrees.contains_key(&capture.key) {
            reuse.keep(node.id());
            reuse.subtrees.insert(capture.key, entry);
        }
    }
}

fn pick<V: Copy>(map: &HashMap<NodeId, V>, ids: &[NodeId]) -> Vec<(NodeId, V)> {
    ids.iter()
        .filter_map(|id| map.get(id).map(|value| (*id, *value)))
        .collect()
}

#[cfg(test)]
mod tests;
