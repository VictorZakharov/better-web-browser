//! Shared depth counters for bounded broker queues that use standard-library channels.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Default)]
pub(super) struct QueueDepth(Arc<AtomicUsize>);

impl QueueDepth {
    pub(super) fn begin_enqueue(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn finish_dequeue(&self) {
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |depth| {
                Some(depth.saturating_sub(1))
            });
    }

    pub(super) fn pending(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}
