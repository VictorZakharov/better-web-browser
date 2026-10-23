//! Intrinsic inline-size contributions reused across layout passes.
//
// A box's min-content and max-content contributions depend on its composed subtree, its
// percentage basis and the layout environment, which is what `reuse` validates; image inputs
// they read are recorded and must still hold. A grid with an auto track asks for every item's
// contributions in every pass, so without this a one-character change elsewhere measured all
// the text of a large item again.
use super::layout_reads::LayoutReads;
use super::reuse::{Inputs, LayoutReuse};
use super::*;

const MAX_ENTRIES: usize = 65_536;

/// A node, its percentage basis bits and whether outer (margin-box) widths are wanted.
pub(super) type WidthKey = (NodeId, Option<u32>, bool);

#[derive(Debug)]
pub(super) struct WidthEntry {
    widths: (f32, f32),
    inputs: Inputs,
    reads: LayoutReads,
}

impl LayoutReuse {
    fn widths(
        &mut self,
        key: WidthKey,
        inputs: Inputs,
        page: &Page,
        styles: &StyleSet,
    ) -> Option<((f32, f32), LayoutReads)> {
        let entry = self.widths.get(&key)?;
        if entry.inputs != inputs || !entry.reads.hold(page, styles) {
            self.widths.remove(&key);
            return None;
        }
        let found = (entry.widths, entry.reads.clone());
        self.keep(key.0);
        Some(found)
    }

    fn insert_widths(&mut self, key: WidthKey, entry: WidthEntry) {
        if self.widths.len() < MAX_ENTRIES || self.widths.contains_key(&key) {
            self.keep(key.0);
            self.widths.insert(key, entry);
        }
    }
}

pub(in crate::engine::layout) enum CachedWidths {
    Hit((f32, f32), LayoutReads),
    Miss(Inputs),
    Uncacheable,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn cached_widths(
        &self,
        node: &NodeRef,
        basis: Option<f32>,
        outer: bool,
    ) -> CachedWidths {
        let Some(inputs) = self.reuse_inputs(node) else {
            return CachedWidths::Uncacheable;
        };
        let key = (node.id(), basis.map(f32::to_bits), outer);
        match self
            .page
            .layout_reuse
            .borrow_mut()
            .widths(key, inputs, self.page, self.styles)
        {
            Some((widths, reads)) => CachedWidths::Hit(widths, reads),
            None => CachedWidths::Miss(inputs),
        }
    }

    pub(in crate::engine::layout) fn remember_widths(
        &self,
        node: &NodeRef,
        basis: Option<f32>,
        outer: bool,
        inputs: Inputs,
        widths: (f32, f32),
        reads: LayoutReads,
    ) {
        let key = (node.id(), basis.map(f32::to_bits), outer);
        self.page.layout_reuse.borrow_mut().insert_widths(
            key,
            WidthEntry {
                widths,
                inputs,
                reads,
            },
        );
    }
}

#[cfg(test)]
mod tests;
