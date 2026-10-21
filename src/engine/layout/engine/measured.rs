//! Measuring layouts repeated within one pass.
//
// Nested grids lay the same item out several times in one pass: the outer grid's probe lays out
// the inner grid, which probes its item and then lays it out at the size it found, before the
// painted layout. Measuring layouts (which neither paint nor keep fragments) of a changed subtree
// cannot replay a retained result, so each one laid the subtree out again. Within a pass the DOM,
// styles and layout inputs cannot change, so a measuring layout of a block with the same
// arguments in a formatting context without floats produces the same block size and floats; the
// first one is kept until the pass ends.
use super::layout_reads::LayoutReads;
use super::subtree_replay::SubtreeKey;
use super::*;

/// A measuring layout's result relative to the position it was laid out at.
#[derive(Debug)]
pub(super) struct Measured {
    height: f32,
    floats: Vec<(Float, RectF)>,
    reads: LayoutReads,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// Whether this layout of `node` may reuse or record a measured result: it neither paints nor
    /// keeps fragments, and no float in its formatting context can intrude.
    pub(in crate::engine::layout) fn measures_alone(&self, node: &NodeRef) -> bool {
        !self.emit_paint
            && !self.retain_fragments
            && self.floats.len() == 0
            && self.replayable_root(node)
    }

    /// The result of an earlier measuring layout of `key` in this pass, placed at (`x`, `y`).
    pub(in crate::engine::layout) fn replay_measured(
        &mut self,
        key: &SubtreeKey,
        node: &NodeRef,
        x: f32,
        y: f32,
    ) -> Option<BlockMetrics> {
        if !self.measures_alone(node) {
            return None;
        }
        let reuse = self.page.layout_reuse.borrow();
        let measured = reuse.measured.get(key)?;
        for (side, rect) in &measured.floats {
            let rect = RectF {
                x: rect.x + x,
                y: rect.y + y,
                ..*rect
            };
            self.floats.add(*side, rect);
        }
        let bottom = y + measured.height;
        let reads = measured.reads.clone();
        drop(reuse);
        self.replay_reads(&reads);
        Some(BlockMetrics { bottom })
    }

    /// Records a measuring layout of `key` at (`x`, `y`) whose reads the innermost frame holds.
    pub(in crate::engine::layout) fn remember_measured(
        &mut self,
        key: SubtreeKey,
        x: f32,
        y: f32,
        metrics: BlockMetrics,
    ) {
        let reads = self.end_reads();
        let floats = self
            .floats
            .since(0)
            .into_iter()
            .map(|(side, rect)| {
                let rect = RectF {
                    x: rect.x - x,
                    y: rect.y - y,
                    ..rect
                };
                (side, rect)
            })
            .collect();
        let measured = Measured {
            height: metrics.bottom - y,
            floats,
            reads,
        };
        self.page
            .layout_reuse
            .borrow_mut()
            .measured
            .insert(key, measured);
    }
}
