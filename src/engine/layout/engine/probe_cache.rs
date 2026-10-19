//! Intrinsic block-size probe results reused across layout passes.
//
// A probe lays out one box on its own (fresh floats, positioned scopes and scroll gutters) at a
// given width and height basis, so its height depends only on what `reuse` validates.
use super::block_measure::MeasureKey;
use super::layout_reads::LayoutReads;
use super::reuse::{Inputs, LayoutReuse};
use super::*;

const MAX_ENTRIES: usize = 65_536;

#[derive(Debug)]
pub(super) struct ProbeEntry {
    height: f32,
    inputs: Inputs,
    reads: LayoutReads,
}

impl LayoutReuse {
    fn probe(
        &mut self,
        key: MeasureKey,
        inputs: Inputs,
        page: &Page,
        styles: &StyleSet,
    ) -> Option<(f32, LayoutReads)> {
        let entry = self.probes.get(&key)?;
        if entry.inputs != inputs || !entry.reads.hold(page, styles) {
            self.probes.remove(&key);
            return None;
        }
        let found = (entry.height, entry.reads.clone());
        self.keep(key.node());
        Some(found)
    }

    fn insert_probe(&mut self, key: MeasureKey, inputs: Inputs, height: f32, reads: LayoutReads) {
        if self.probes.len() < MAX_ENTRIES || self.probes.contains_key(&key) {
            self.keep(key.node());
            self.probes.insert(
                key,
                ProbeEntry {
                    height,
                    inputs,
                    reads,
                },
            );
        }
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn cached_probe(&self, key: MeasureKey, node: &NodeRef) -> CachedProbe {
        let Some(inputs) = self.reuse_inputs(node) else {
            return CachedProbe::Uncacheable;
        };
        match self
            .page
            .layout_reuse
            .borrow_mut()
            .probe(key, inputs, self.page, self.styles)
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
            .layout_reuse
            .borrow_mut()
            .insert_probe(key, inputs, height, reads);
    }
}

pub(super) enum CachedProbe {
    Hit(f32, LayoutReads),
    Miss(Inputs),
    Uncacheable,
}

#[cfg(test)]
mod tests;
