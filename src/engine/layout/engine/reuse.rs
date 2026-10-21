//! Layout results that survive from one pass to the next, and when they still hold.
//
// Two kinds of results are retained: intrinsic block-size probes (`probe_cache`) and laid-out
// subtrees (`subtree_replay`). Each depends only on one box's composed subtree, the arguments it
// was laid out with, the layout environment and the image or scroll inputs it read. A result is
// reused while:
//
// - the subtree's DOM (`Node::subtree_mutation_version`, which also covers control state,
//   focus and popover changes) and computed styles (`StyleSet` subtree generation) are
//   unchanged, as is the parent's own style, which some boxes consult (flex/grid parents,
//   white-space at the box edges), and the zoom its ancestors apply;
// - every recorded input (`layout_reads`) reads the same, and the viewport, scrollbar size,
//   canvas policy, fullscreen root and text measurer epoch match the pass that computed it.
//
// Nodes inside shadow trees are not reused: their composed subtree can include slotted light
// DOM whose mutations do not advance the shadow-tree node's version. Results for nodes a
// complete pass neither computed nor reused are dropped at its end, which bounds the cache by
// the live document.
use super::block_measure::MeasureKey;
pub(crate) use super::box_tree::RetainedBoxes;
use super::*;
use std::collections::HashSet;

/// Everything outside the DOM, styles and recorded reads that a retained result depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ReuseEnvironment {
    pub(super) viewport: (f32, f32),
    pub(super) style_viewport_width: f32,
    pub(super) scrollbar: f32,
    pub(super) canvas_replaced: bool,
    pub(super) fullscreen: Option<NodeId>,
    pub(super) measurer_epoch: u64,
}

/// The DOM and style state of one box's subtree that a retained result was computed from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Inputs {
    dom: u64,
    own: u64,
    subtree: u64,
    parent_own: Option<u64>,
    /// Zoom applied by ancestors scales the box tree's used styles without restyling them.
    zoom: u32,
}

#[derive(Debug, Default)]
pub(crate) struct LayoutReuse {
    environment: Option<ReuseEnvironment>,
    pub(super) probes: HashMap<MeasureKey, probe_cache::ProbeEntry>,
    pub(super) subtrees: HashMap<subtree_replay::SubtreeKey, subtree_replay::SubtreeEntry>,
    /// Measuring layouts of this pass; see `measured`.
    pub(super) measured: HashMap<subtree_replay::SubtreeKey, measured::Measured>,
    pub(super) used: HashSet<NodeId>,
    /// When and from what each node was last laid out as a subtree root; see `subtree_replay`.
    /// Unlike results, this survives an environment change.
    pub(super) sighted: HashMap<NodeId, subtree_replay::Sighting>,
    pub(super) pass: u64,
}

impl LayoutReuse {
    /// Starts a pass; a different environment invalidates every result.
    pub(super) fn begin_pass(&mut self, environment: Option<ReuseEnvironment>) {
        if self.environment != environment {
            self.probes.clear();
            self.subtrees.clear();
            self.environment = environment;
        }
        self.used.clear();
        self.measured.clear();
        self.pass += 1;
    }

    /// Drops results for nodes a complete pass neither computed nor reused.
    pub(super) fn finish_pass(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.probes.retain(|key, _| used.contains(&key.node()));
        self.subtrees.retain(|key, _| used.contains(&key.node));
        let pass = self.pass;
        self.sighted.retain(|_, sighting| sighting.last == pass);
    }

    pub(super) fn keep(&mut self, node: NodeId) {
        self.used.insert(node);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> (usize, usize) {
        (self.probes.len(), self.subtrees.len())
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// The inputs a retained result for `node` must match, or `None` when it cannot be reused.
    pub(super) fn reuse_inputs(&self, node: &NodeRef) -> Option<Inputs> {
        self.page.layout_reuse.borrow().environment?;
        if !matches!(Node::tree_root(node).data, NodeData::Document) {
            return None;
        }
        let parent_own = match Node::composed_parent(node) {
            Some(parent) if parent.element().is_some() => {
                Some(self.styles.own_layout_generation(parent.id())?)
            }
            _ => None,
        };
        Some(Inputs {
            dom: node.subtree_mutation_version(),
            own: self.styles.own_layout_generation(node.id())?,
            subtree: self.styles.subtree_layout_generation(node.id())?,
            parent_own,
            zoom: self.styles.get(node).effective_zoom.to_bits(),
        })
    }
}
