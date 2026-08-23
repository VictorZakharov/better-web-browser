//! Bounded renderer-to-browser event delivery.

use super::RendererEvent;
use crate::limits::{
    MAX_QUEUED_RENDERER_EVENTS, MAX_QUEUED_RENDERER_FETCH_BATCHES,
    MAX_QUEUED_RENDERER_PRESENTATIONS,
};
use crate::renderer_protocol::ProtocolError;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

pub(super) fn bounded() -> (EventSender, EventReceiver) {
    let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_RENDERER_EVENTS);
    let budget = Arc::new(EventBudget::default());
    (
        EventSender {
            sender,
            budget: Arc::clone(&budget),
        },
        EventReceiver { receiver, budget },
    )
}

pub(super) struct EventSender {
    sender: mpsc::SyncSender<RendererEvent>,
    budget: Arc<EventBudget>,
}

impl EventSender {
    pub(super) fn try_send(&self, event: RendererEvent) -> Result<(), ProtocolError> {
        let presentation = matches!(&event, RendererEvent::Presentation(_));
        if presentation && !self.budget.try_reserve_presentation() {
            return Err(ProtocolError::InvalidPayload(
                "browser presentation-event queue exhausted",
            ));
        }
        let fetch_batch = matches!(&event, RendererEvent::FetchBatch(_));
        if fetch_batch && !self.budget.try_reserve_fetch_batch() {
            return Err(ProtocolError::InvalidPayload(
                "browser Fetch-batch event queue exhausted",
            ));
        }
        self.sender.try_send(event).map_err(|error| {
            if presentation {
                self.budget.release_presentation();
            }
            if fetch_batch {
                self.budget.release_fetch_batch();
            }
            match error {
                mpsc::TrySendError::Full(_) => {
                    ProtocolError::InvalidPayload("browser renderer-event queue exhausted")
                }
                mpsc::TrySendError::Disconnected(_) => {
                    ProtocolError::InvalidPayload("browser renderer-event receiver closed")
                }
            }
        })
    }
}

pub(super) struct EventReceiver {
    receiver: mpsc::Receiver<RendererEvent>,
    budget: Arc<EventBudget>,
}

impl EventReceiver {
    pub(super) fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<RendererEvent, mpsc::RecvTimeoutError> {
        self.receiver
            .recv_timeout(timeout)
            .inspect(|event| self.release(event))
    }

    pub(super) fn try_recv(&self) -> Result<RendererEvent, mpsc::TryRecvError> {
        self.receiver
            .try_recv()
            .inspect(|event| self.release(event))
    }

    fn release(&self, event: &RendererEvent) {
        if matches!(event, RendererEvent::Presentation(_)) {
            self.budget.release_presentation();
        }
        if matches!(event, RendererEvent::FetchBatch(_)) {
            self.budget.release_fetch_batch();
        }
    }
}

#[derive(Default)]
struct EventBudget {
    queued_presentations: AtomicUsize,
    queued_fetch_batches: AtomicUsize,
}

impl EventBudget {
    fn try_reserve_presentation(&self) -> bool {
        self.queued_presentations
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                (queued < MAX_QUEUED_RENDERER_PRESENTATIONS).then_some(queued + 1)
            })
            .is_ok()
    }

    fn release_presentation(&self) {
        let previous = self.queued_presentations.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }

    fn try_reserve_fetch_batch(&self) -> bool {
        self.queued_fetch_batches
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                (queued < MAX_QUEUED_RENDERER_FETCH_BATCHES).then_some(queued + 1)
            })
            .is_ok()
    }

    fn release_fetch_batch(&self) {
        let previous = self.queued_fetch_batches.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_payload_budgets_are_released_for_reuse() {
        let budget = EventBudget::default();
        for _ in 0..MAX_QUEUED_RENDERER_PRESENTATIONS {
            assert!(budget.try_reserve_presentation());
        }
        assert!(!budget.try_reserve_presentation());
        budget.release_presentation();
        assert!(budget.try_reserve_presentation());

        for _ in 0..MAX_QUEUED_RENDERER_FETCH_BATCHES {
            assert!(budget.try_reserve_fetch_batch());
        }
        assert!(!budget.try_reserve_fetch_batch());
        budget.release_fetch_batch();
        assert!(budget.try_reserve_fetch_batch());
    }
}
