//! Intrinsic block-size probe results that survive from one layout pass to the next.
//
// A probe lays out one box on its own (fresh floats, positioned scopes and scroll gutters) at a
// given width and height basis, so its result depends only on that box's composed subtree, the
// layout environment and the image inputs it read. A cached result is reused while:
//
// - the subtree's DOM (`Node::subtree_mutation_version`, which also covers control state,
//   focus and popover changes) and computed styles (`StyleSet` subtree generation) are
//   unchanged, as is the parent's own style, which some boxes consult (flex/grid parents,
//   white-space at the box edges);
// - every recorded image input reads the same, and the viewport, scrollbar size, canvas
//   policy, fullscreen root and text measurer epoch match the pass that computed it.
//
// Nodes inside shadow trees are not cached: their composed subtree can include slotted light
// DOM whose mutations do not advance the shadow-tree node's version. Results unused by a full
// pass are dropped at its end, which bounds the cache by the live document.
use super::block_measure::MeasureKey;
use super::layout_reads::LayoutReads;
use super::*;
use std::collections::HashSet;

const MAX_ENTRIES: usize = 65_536;

/// Everything outside the DOM, styles and image reads that a probe's result depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ProbeEnvironment {
    pub(super) viewport: (f32, f32),
    pub(super) style_viewport_width: f32,
    pub(super) scrollbar: f32,
    pub(super) canvas_replaced: bool,
    pub(super) fullscreen: Option<NodeId>,
    pub(super) measurer_epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Inputs {
    dom: u64,
    own: u64,
    subtree: u64,
    parent_own: Option<u64>,
}

#[derive(Debug)]
struct Entry {
    height: f32,
    inputs: Inputs,
    reads: LayoutReads,
}

#[derive(Debug, Default)]
pub(crate) struct ProbeCache {
    environment: Option<ProbeEnvironment>,
    entries: HashMap<MeasureKey, Entry>,
    used: HashSet<MeasureKey>,
}

impl ProbeCache {
    /// Starts a pass; a different environment invalidates every result.
    pub(super) fn begin_pass(&mut self, environment: Option<ProbeEnvironment>) {
        if self.environment != environment {
            self.entries.clear();
            self.environment = environment;
        }
        self.used.clear();
    }

    /// Drops results a complete pass did not use.
    pub(super) fn finish_pass(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.entries.retain(|key, _| used.contains(key));
    }

    fn get(&mut self, key: MeasureKey, inputs: Inputs, page: &Page) -> Option<(f32, LayoutReads)> {
        let entry = self.entries.get(&key)?;
        if entry.inputs != inputs || !entry.reads.hold(page) {
            self.entries.remove(&key);
            return None;
        }
        self.used.insert(key);
        Some((entry.height, entry.reads.clone()))
    }

    fn insert(&mut self, key: MeasureKey, inputs: Inputs, height: f32, reads: LayoutReads) {
        if self.entries.len() < MAX_ENTRIES || self.entries.contains_key(&key) {
            self.used.insert(key);
            self.entries.insert(
                key,
                Entry {
                    height,
                    inputs,
                    reads,
                },
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// The inputs a cached probe of `node` must match, or `None` when it cannot be cached.
    fn probe_inputs(&self, node: &NodeRef) -> Option<Inputs> {
        self.page.layout_probes.borrow().environment?;
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
        })
    }

    pub(super) fn cached_probe(&self, key: MeasureKey, node: &NodeRef) -> CachedProbe {
        let Some(inputs) = self.probe_inputs(node) else {
            return CachedProbe::Uncacheable;
        };
        match self
            .page
            .layout_probes
            .borrow_mut()
            .get(key, inputs, self.page)
        {
            Some((height, reads)) => CachedProbe::Hit(height, reads),
            None => CachedProbe::Miss(inputs),
        }
    }

    pub(super) fn remember_probe(
        &self,
        key: MeasureKey,
        inputs: Inputs,
        height: f32,
        reads: LayoutReads,
    ) {
        self.page
            .layout_probes
            .borrow_mut()
            .insert(key, inputs, height, reads);
    }
}

pub(super) enum CachedProbe {
    Hit(f32, LayoutReads),
    Miss(Inputs),
    Uncacheable,
}

#[cfg(test)]
mod tests;
