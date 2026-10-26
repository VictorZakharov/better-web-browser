//! Change generations that let layout reuse results computed from unchanged styles.
//
// Every node with a computed style carries two generations: its own, which advances when its
// style or generated content changes, and its subtree's, which also advances when any composed
// descendant's does. A layout that records a subtree's generation can later reuse a result
// derived only from that subtree's styles while the generation is unchanged.
//
// Generations come from one process-wide counter, so values recorded against one `StyleSet`
// never match another. A new value is drawn only after layout has observed the current one,
// which keeps an ancestor walk short (it stops at the first ancestor already carrying the
// current value) without ever letting a recorded value survive a later change.
use super::*;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
pub(super) struct LayoutGenerations {
    /// One entry per node, so marking a node and stopping at its parent costs two lookups.
    generations: NodeMap<NodeId, Generations>,
    current: Cell<u64>,
    observed: Cell<bool>,
}

/// A node's own and subtree generations; 0 is unknown, since generations start at 1. An
/// ancestor of a marked node can carry a subtree generation before its own is known.
#[derive(Clone, Copy, Debug, Default)]
struct Generations {
    own: u64,
    subtree: u64,
}

impl LayoutGenerations {
    /// Records that `node`'s style or generated content changed, or that it was (re)computed
    /// without a previous value to compare against.
    pub(super) fn mark(&mut self, node: &NodeRef) {
        if self.observed.get() || self.current.get() == 0 {
            self.current
                .set(NEXT_GENERATION.fetch_add(1, Ordering::Relaxed));
            self.observed.set(false);
        }
        let current = self.current.get();
        let entry = self.generations.entry(node.id()).or_default();
        entry.own = current;
        if std::mem::replace(&mut entry.subtree, current) == current {
            return;
        }
        let mut ancestor = Node::composed_parent(node);
        while let Some(node) = ancestor {
            let entry = self.generations.entry(node.id()).or_default();
            if std::mem::replace(&mut entry.subtree, current) == current {
                break;
            }
            ancestor = Node::composed_parent(&node);
        }
    }

    /// A copy for another style set in which every known node carries one fresh generation, so
    /// no value a layout recorded against this set matches the copy.
    pub(super) fn renumbered(&self) -> Self {
        let fresh = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        let renumber = |generation: u64| if generation == 0 { 0 } else { fresh };
        Self {
            generations: self
                .generations
                .iter()
                .map(|(&node, generations)| {
                    let own = renumber(generations.own);
                    (
                        node,
                        Generations {
                            own,
                            subtree: renumber(generations.subtree),
                        },
                    )
                })
                .collect(),
            current: Cell::new(fresh),
            observed: Cell::new(false),
        }
    }

    /// Forgets every node; their generations stay unknown until they are computed again.
    pub(super) fn clear(&mut self) {
        self.generations.clear();
    }

    pub(super) fn forget(&mut self, node: NodeId) {
        self.generations.remove(&node);
    }

    /// Forgets `removed`, whose styles were dropped while `anchor` stays rendered, and marks
    /// `anchor`'s ancestry changed so no enclosing result survives the removal.
    pub(super) fn forget_below(&mut self, removed: &HashSet<NodeId>, anchor: &NodeRef) {
        for node in removed {
            self.forget(*node);
        }
        if let Some(anchor) = std::iter::successors(Some(anchor.clone()), Node::composed_parent)
            .find(|node| !removed.contains(&node.id()))
        {
            self.mark(&anchor);
        }
    }

    pub(super) fn retain(&mut self, keep: impl Fn(&NodeId) -> bool) {
        self.generations.retain(|node, _| keep(node));
    }

    fn observe(&self, node: NodeId, generation: impl Fn(&Generations) -> u64) -> Option<u64> {
        self.observed.set(true);
        self.generations
            .get(&node)
            .map(generation)
            .filter(|&generation| generation != 0)
    }
}

impl StyleSet {
    /// The generation of `node`'s own computed style, or `None` when it is unknown.
    pub(crate) fn own_layout_generation(&self, node: NodeId) -> Option<u64> {
        self.layout_generations
            .observe(node, |generations| generations.own)
    }

    /// The generation of the computed styles in `node`'s composed subtree, or `None` when
    /// unknown. It advances whenever any of them changes.
    pub(crate) fn subtree_layout_generation(&self, node: NodeId) -> Option<u64> {
        self.layout_generations
            .observe(node, |generations| generations.subtree)
    }
}

#[cfg(test)]
mod tests;
