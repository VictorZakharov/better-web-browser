//! Replays a block subtree's retained layout instead of laying it out again.
//
// A block in normal flow (or relatively positioned) lays out from its arguments and the floats
// of its formatting context, and nothing it emits depends on where it is placed except by
// translation, unless one of its absolutely positioned descendants resolves against a
// containing block established outside it: such a box is placed against that block and must not
// move with the subtree, so the subtree is not retained. A block that establishes its own formatting context starts with no
// floats. Any other block shares its parent's float context, so it is retained only when every
// float already in that context ends above all of its boxes, and replayed only when that holds
// again at the new position; the floats it adds itself are retained and added back. Its output
// is then a function of the box's subtree, its arguments and the inputs `reuse` validates, and
// a later pass can append a copy and translate it to the new position, exactly as an ancestor
// moves an already laid-out subtree (`translate_layout_subtree`).
//
// Subtrees containing fixed-position boxes, which are laid out against the viewport, or
// table-fixup boxes whose identifiers are new in every pass are not retained. Sticky boxes are
// laid out like relatively positioned ones; their scroll-dependent offset is applied to the
// finished output afterwards (`update_sticky_positions`), so a subtree retains them unshifted.
// Each retained subtree keeps its own copy of what it emitted, captured as soon as its layout
// finishes and before any ancestor moves it, so a nested subtree can still be replayed after an
// enclosing one changes.
// Entries are bounded by size so that very large containers recompute while their children
// replay. A subtree is retained as soon as it is laid out, so new content and a document's first
// layout are laid out once and replay from the next pass, unless its root was also laid out in
// the previous pass from a different DOM or styles: the ancestors of content that keeps changing
// (as scripts fill a page in) would otherwise copy what they emit on every pass. Once such a
// subtree holds for a pass, that pass retains it. That holds across a change of arguments or
// environment: after a scrollbar narrows the viewport, which invalidates every result, the next
// pass at the new width replays instead of laying out the whole document a second time.
//
// Measuring layouts (block-size probes, which neither paint nor keep fragments) discard their
// output, emit no items and so retain nothing of their own. Geometry does not depend on whether a
// layout paints, so a measuring layout replays the painted entry with the same arguments and takes
// only its block size and floats. Without that, a grid probing an item whose subtree changed,
// such as Wikipedia's page grids, laid out the item's entire subtree again. A probe in a painting
// pass also lays out for painting, and retains, each subtree below the box it measures that would
// be retained, so the final layout of the box replays it: otherwise content new in a pass, such as
// the sections a parser step appended to an article inside those grids, was laid out once to be
// measured and again to be painted. The measured box itself is measured, since its final layout is
// usually stretched to its grid area and so has other arguments.
use super::layout_reads::LayoutReads;
use super::reuse::{Inputs, LayoutReuse};
use super::*;
use append::Replay;

/// Even a subtree emitting one item is cheaper to replay than to lay out again: the cells of a
/// large table hold a few items each, and a change in one cell lays out every other cell twice.
const MIN_ITEMS: usize = 1;
/// Larger containers recompute; their retained children still replay.
const MAX_ITEMS: usize = 32_768;
const MAX_ENTRIES: usize = 16_384;

/// Serials are unique in the process, so a layout a snapshot replays from its page keeps its
/// identity there (see `SubtreeEntry::serial`).
static NEXT_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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

impl SubtreeKey {
    /// The same layout as a painting pass that retains fragments performs it.
    fn painted(self) -> Self {
        Self {
            paint: true,
            fragments: true,
            ..self
        }
    }
}

#[derive(Debug)]
pub(super) struct SubtreeEntry {
    /// Unique in the process, so results derived from this one (such as its scrollable
    /// overflow) can tell it from a later layout of the same node.
    serial: u64,
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
    /// For a root sharing its parent's float context: the top of its highest box, which every
    /// earlier float must end above, and the floats it added.
    shared_floats: Option<(f32, Vec<(Float, RectF)>)>,
}

/// The last pass that laid a node out as a subtree root, and the inputs it used. Sightings
/// last only until the next pass that does not lay the node out.
#[derive(Debug)]
pub(super) struct Sighting {
    /// Zero before the first sighting; passes count from one.
    pub(super) last: u64,
    inputs: Inputs,
    /// Not laid out in the previous pass, or laid out from the same inputs.
    settled: bool,
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
    floats: usize,
    /// Laid out for painting inside a measuring probe (`begin_layout_capture`).
    painted_in_probe: bool,
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
        if entry.inputs != inputs || !entry.reads.hold(page, styles, &mut self.held) {
            self.subtrees.remove(key);
            return None;
        }
        self.used.extend(self.subtrees[key].nodes.iter().copied());
        self.subtrees.get(key)
    }

    /// Whether a layout of `key` should be retained: its root was not laid out in the previous
    /// pass or was laid out from the same inputs, or its retained result is being replaced.
    fn settled(&mut self, key: SubtreeKey, inputs: Inputs) -> bool {
        let pass = self.pass;
        let sighting = self.sighted.entry(key.node).or_insert(Sighting {
            last: 0,
            inputs,
            settled: false,
        });
        if sighting.last != pass {
            sighting.settled = sighting.last == 0 || sighting.inputs == inputs;
            sighting.last = pass;
            sighting.inputs = inputs;
        }
        self.subtrees.contains_key(&key) || sighting.settled
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
    pub(super) fn replayable_root(&self, node: &NodeRef) -> bool {
        let style = self.styles.get(node);
        matches!(style.position, Position::Static | Position::Relative)
            && style.float == Float::None
            && style.display != Display::None
            && !node.is_generated_pseudo()
            && !matches!(node.tag_name(), Some("html" | "body"))
            && (style.display == Display::Block || self.block_establishes_context(node))
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
        let page = self.page;
        let mut reuse = page.layout_reuse.borrow_mut();
        // A measuring layout discards its output, so it emits no items and retains nothing of its
        // own. It replays the geometry of the painted layout with the same arguments instead.
        let mode = Replay::of(self.emit_paint, self.retain_fragments);
        let key = if mode == Replay::Measure && !reuse.subtrees.contains_key(key) {
            key.painted()
        } else {
            *key
        };
        if reuse.subtree(&key, inputs, page, self.styles).is_none() {
            drop(reuse);
            return self.replay_source(&key, node, inputs, (x, y), mode);
        }
        let appended = self.append_retained(&reuse.subtrees[&key], (x, y), mode)?;
        drop(reuse);
        Some(self.finish_replay(node, appended, mode))
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
        if !self.page.layout_reuse.borrow_mut().settled(key, inputs) {
            return None;
        }
        self.begin_reads();
        Some(Capture {
            key,
            inputs,
            origin: (x, y),
            item_start: self.output.items.len(),
            node_start: self.output.node_paint_order.len(),
            first_line: self.output.fragments.next_line,
            flow_ranges: self.positioned_flow_scopes.last().map_or(0, Vec::len),
            floats: self.floats.len(),
            painted_in_probe: false,
        })
    }

    /// Starts recording a layout of `node` as `begin_capture` does, and in a block-size probe of
    /// a painting pass switches a layout that would be retained to painting (see the module
    /// comment); pair with `finish_capture`.
    pub(in crate::engine::layout) fn begin_layout_capture(
        &mut self,
        key: SubtreeKey,
        node: &NodeRef,
        x: f32,
        y: f32,
    ) -> Option<Capture> {
        if self.emit_paint || self.probe_root.is_none_or(|root| root == node.id()) {
            return self.begin_capture(key, node, x, y);
        }
        // A relatively positioned root reports its stacking level to its parent's flow scope,
        // which a measuring probe does not keep.
        self.positioned_flow_scopes.push(Vec::new());
        let Some(mut capture) = self.begin_capture(key.painted(), node, x, y) else {
            self.positioned_flow_scopes.pop();
            return None;
        };
        capture.painted_in_probe = true;
        self.emit_paint = true;
        self.retain_fragments = true;
        Some(capture)
    }

    pub(in crate::engine::layout) fn finish_capture(
        &mut self,
        capture: Capture,
        node: &NodeRef,
        metrics: BlockMetrics,
    ) {
        let painted_in_probe = capture.painted_in_probe;
        self.retain_capture(capture, node, metrics);
        if painted_in_probe {
            self.emit_paint = false;
            self.retain_fragments = false;
            self.positioned_flow_scopes.pop();
        }
    }

    fn retain_capture(&mut self, capture: Capture, node: &NodeRef, metrics: BlockMetrics) {
        let frame = self.close_reads();
        // Absolutely positioned boxes placed against a containing block outside the subtree
        // would move with a copy of it.
        if frame.outside_containing_block {
            return;
        }
        let items = &self.output.items[capture.item_start..];
        // Around a retained descendant holding most of the items, laying the block out again
        // costs little more than replaying it; retaining both would copy those items twice.
        if !(MIN_ITEMS..=MAX_ITEMS).contains(&items.len())
            || items.len().saturating_sub(frame.retained_items) * 4 < items.len()
        {
            return;
        }
        let nodes = self.styles.descendants(node);
        let retainable = nodes.iter().all(|descendant| {
            let style = self.styles.get(descendant);
            !self.styles.is_transient_anonymous(descendant.id())
                && style.position != Position::Fixed
        });
        if !retainable {
            return;
        }
        let ids = nodes.iter().map(|node| node.id()).collect::<Vec<_>>();
        let node_bounds = pick(&self.output.node_bounds, &ids);
        let shared_floats = if self.block_establishes_context(node) {
            None
        } else {
            let top = node_bounds
                .iter()
                .map(|(_, rect)| rect.y)
                .fold(capture.origin.1, f32::min);
            if !self.floats.clear_below(capture.floats, top) {
                return;
            }
            Some((top, self.floats.since(capture.floats)))
        };
        let flow_level = (self.positioned_flow_scopes.last().map_or(0, Vec::len)
            > capture.flow_ranges)
            .then(|| self.styles.get(node).z_index.unwrap_or(0));
        let serial = NEXT_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let entry = SubtreeEntry {
            serial,
            inputs: capture.inputs,
            reads: self.subtree_frame_reads(&frame, node),
            origin: capture.origin,
            bottom: metrics.bottom,
            items: items.to_vec(),
            paint_order: self.output.node_paint_order[capture.node_start..].to_vec(),
            node_bounds,
            resize_boxes: pick(&self.output.resize_boxes, &ids),
            scroll_boxes: pick(&self.output.scroll_boxes, &ids),
            clip_paths: pick(&self.output.clip_paths, &ids),
            transformed: self.output.transformed.subset(&ids),
            fragments: self.output.fragments.subset(&ids),
            first_line: capture.first_line,
            lines: self.output.fragments.next_line - capture.first_line,
            flow_level,
            shared_floats,
            nodes: ids,
        };
        let retained = entry.items.len();
        let mut reuse = self.page.layout_reuse.borrow_mut();
        if reuse.subtrees.len() < MAX_ENTRIES || reuse.subtrees.contains_key(&capture.key) {
            reuse.keep(node.id());
            reuse.subtrees.insert(capture.key, entry);
            drop(reuse);
            self.retained_items(retained);
        }
    }
}

fn pick<V: Copy>(map: &HashMap<NodeId, V>, ids: &[NodeId]) -> Vec<(NodeId, V)> {
    ids.iter()
        .filter_map(|id| map.get(id).map(|value| (*id, *value)))
        .collect()
}

mod append;
#[cfg(test)]
mod tests;
