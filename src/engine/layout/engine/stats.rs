//! Counts and times of the work layout passes do, so that work proportional to the document
//! rather than to what changed shows up as counts, not only as milliseconds. Passes add to
//! `Page::layout_stats`; the renderer takes the sum with each update it reports.
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutStats {
    pub passes: u64,
    /// Box-tree nodes whose children were built again.
    pub boxes_rebuilt: u64,
    /// Block layouts computed rather than replayed, including measuring layouts.
    pub blocks_laid_out: u64,
    pub subtrees_replayed: u64,
    /// Display items in each pass's output before paint-order finalization.
    pub items_emitted: u64,
    /// Display items copied from replayed subtrees, including copies measuring layouts discard.
    pub items_replayed: u64,
    pub box_tree_micros: u64,
    pub block_layout_micros: u64,
    pub inline_geometry_micros: u64,
    pub overflow_micros: u64,
    pub paint_order_micros: u64,
    pub hit_test_micros: u64,
    pub sticky_micros: u64,
}

impl LayoutStats {
    pub fn saturating_add(self, other: Self) -> Self {
        let (left, right) = (self.to_fields(), other.to_fields());
        Self::from_fields(std::array::from_fn(|index| {
            left[index].saturating_add(right[index])
        }))
    }

    /// The fields in declaration order, as the renderer protocol carries them.
    pub fn to_fields(self) -> [u64; 13] {
        [
            self.passes,
            self.boxes_rebuilt,
            self.blocks_laid_out,
            self.subtrees_replayed,
            self.items_emitted,
            self.items_replayed,
            self.box_tree_micros,
            self.block_layout_micros,
            self.inline_geometry_micros,
            self.overflow_micros,
            self.paint_order_micros,
            self.hit_test_micros,
            self.sticky_micros,
        ]
    }

    pub fn from_fields(fields: [u64; 13]) -> Self {
        let [
            passes,
            boxes_rebuilt,
            blocks_laid_out,
            subtrees_replayed,
            items_emitted,
            items_replayed,
            box_tree_micros,
            block_layout_micros,
            inline_geometry_micros,
            overflow_micros,
            paint_order_micros,
            hit_test_micros,
            sticky_micros,
        ] = fields;
        Self {
            passes,
            boxes_rebuilt,
            blocks_laid_out,
            subtrees_replayed,
            items_emitted,
            items_replayed,
            box_tree_micros,
            block_layout_micros,
            inline_geometry_micros,
            overflow_micros,
            paint_order_micros,
            hit_test_micros,
            sticky_micros,
        }
    }
}

impl crate::engine::page::Page {
    /// The layout work done since the last call.
    pub(crate) fn take_layout_stats(&self) -> LayoutStats {
        self.layout_stats.take()
    }

    pub(in crate::engine::layout) fn count_layout(&self, count: impl FnOnce(&mut LayoutStats)) {
        let mut stats = self.layout_stats.get();
        count(&mut stats);
        self.layout_stats.set(stats);
    }
}

/// Times consecutive phases of one pass.
pub(super) struct PhaseClock(Instant);

impl PhaseClock {
    pub(super) fn start() -> Self {
        Self(Instant::now())
    }

    /// Starts the next phase without counting the time since the previous one.
    pub(super) fn restart(&mut self) {
        self.0 = Instant::now();
    }

    /// Microseconds since the previous lap or the start.
    pub(super) fn lap(&mut self) -> u64 {
        let now = Instant::now();
        let micros = now.duration_since(self.0).as_micros();
        self.0 = now;
        u64::try_from(micros).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod workload;
