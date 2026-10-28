//! Appending a retained layout to a pass's output, and replaying the page's painted layouts in
//! the script layout snapshot.
//
// Script geometry reads (CSSOM View, observers) lay the document out on a snapshot of the page
// whose output keeps boxes and fragments but no display items. Such a pass retains nothing of
// its own, since its subtrees emit no items, so every forced layout used to lay out the whole
// document even right after the page laid out the same one. The snapshot shares the page's DOM,
// forks its styles with the same layout generations (`StyleSet::fork`) and runs in the same
// environment, so a painted result the page retained holds for the snapshot whenever its inputs
// and reads match there: the snapshot replays its geometry, without items, and leaves the
// page's results untouched.
use super::*;

/// What a replay appends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Replay {
    /// A painting pass: items, paint order and geometry.
    Paint,
    /// A geometry-only pass: boxes and fragments, no items or paint order.
    Geometry,
    /// A block-size probe, which keeps nothing but the block size and floats.
    Measure,
}

impl Replay {
    pub(super) fn of(emit_paint: bool, retain_fragments: bool) -> Self {
        match (emit_paint, retain_fragments) {
            (true, _) => Self::Paint,
            (false, true) => Self::Geometry,
            (false, false) => Self::Measure,
        }
    }
}

/// A retained layout appended to the output, finished by `finish_replay` once the cache that
/// holds it is released.
pub(super) struct Appended {
    serial: u64,
    bottom: f32,
    offset: (f32, f32),
    item_start: usize,
    node_start: usize,
    items: usize,
    reads: LayoutReads,
    flow_level: Option<i32>,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// Appends `entry` placed at `origin`, or returns `None` when a float placed earlier in its
    /// shared float context now reaches into it.
    pub(super) fn append_retained(
        &mut self,
        entry: &SubtreeEntry,
        (x, y): (f32, f32),
        mode: Replay,
    ) -> Option<Appended> {
        let (dx, dy) = (x - entry.origin.0, y - entry.origin.1);
        if let Some((top, _)) = &entry.shared_floats
            && !self.floats.clear_below(self.floats.len(), top + dy)
        {
            return None;
        }
        for (side, rect) in entry.shared_floats.iter().flat_map(|(_, floats)| floats) {
            let rect = RectF {
                x: rect.x + dx,
                y: rect.y + dy,
                ..*rect
            };
            self.floats.add(*side, rect);
        }
        let items = if mode == Replay::Paint {
            entry.items.len()
        } else {
            0
        };
        self.page.count_layout(|stats| {
            stats.subtrees_replayed += 1;
            stats.items_replayed += items as u64;
        });
        let item_start = self.output.items.len();
        let node_start = self.output.node_paint_order.len();
        if mode == Replay::Paint {
            self.output.items.extend(entry.items.iter().cloned());
            self.output
                .node_paint_order
                .extend_from_slice(&entry.paint_order);
        }
        if mode != Replay::Measure {
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
            fragments.extend_subtree(
                &entry.fragments,
                first_line.wrapping_sub(entry.first_line) as i64,
            );
            fragments.next_line = first_line + entry.lines;
        }
        Some(Appended {
            serial: entry.serial,
            bottom: entry.bottom + dy,
            offset: (dx, dy),
            item_start,
            node_start,
            items,
            reads: entry.reads.clone(),
            flow_level: entry.flow_level,
        })
    }

    pub(super) fn finish_replay(
        &mut self,
        node: &NodeRef,
        appended: Appended,
        mode: Replay,
    ) -> BlockMetrics {
        self.replay_reads(&appended.reads);
        let metrics = BlockMetrics {
            bottom: appended.bottom,
        };
        if mode == Replay::Measure {
            return metrics;
        }
        self.replayed_roots.insert(node.id(), appended.serial);
        if mode == Replay::Paint {
            self.retained_items(appended.items);
        }
        let (dx, dy) = appended.offset;
        if dx != 0.0 || dy != 0.0 {
            let item_end = self.output.items.len();
            self.translate_layout_subtree(Some(node), appended.item_start, item_end, dx, dy);
        }
        if let Some(level) = appended.flow_level
            && let Some(parent) = self.positioned_flow_scopes.last_mut()
        {
            parent.push(InFlowPaintRange {
                node: node.id(),
                level,
                items: appended.item_start..self.output.items.len(),
                nodes: appended.node_start..self.output.node_paint_order.len(),
            });
        }
        metrics
    }

    /// Replays the painted layout that the page this snapshot copies retained for `key`, when
    /// its inputs and reads hold here.
    pub(super) fn replay_source(
        &mut self,
        key: &SubtreeKey,
        node: &NodeRef,
        inputs: Inputs,
        origin: (f32, f32),
        mode: Replay,
    ) -> Option<BlockMetrics> {
        let page = self.page;
        let source = page.layout_reuse.borrow().source.upgrade()?;
        let source = source.try_borrow().ok()?;
        let entry = source.subtrees.get(&key.painted())?;
        let mut reuse = page.layout_reuse.borrow_mut();
        if !reuse.shares_environment(&source)
            || entry.inputs != inputs
            || !entry.reads.hold(page, self.styles, &mut reuse.held)
        {
            return None;
        }
        drop(reuse);
        let appended = self.append_retained(entry, origin, mode)?;
        drop(source);
        Some(self.finish_replay(node, appended, mode))
    }
}
