//! Bounded renderer-to-browser event delivery.

use super::RendererEvent;
use crate::limits::{MAX_QUEUED_RENDERER_EVENTS, MAX_QUEUED_RENDERER_FETCH_BATCHES};
use crate::renderer_protocol::ProtocolError;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

pub(super) fn bounded() -> (EventSender, EventReceiver) {
    let queue = Arc::new(EventQueue {
        state: Mutex::new(QueueState {
            events: VecDeque::new(),
            sender_open: true,
            receiver_open: true,
        }),
        available: Condvar::new(),
    });
    (
        EventSender {
            queue: Arc::clone(&queue),
        },
        EventReceiver { queue },
    )
}

struct EventQueue {
    state: Mutex<QueueState>,
    available: Condvar,
}

struct QueueState {
    events: VecDeque<RendererEvent>,
    sender_open: bool,
    receiver_open: bool,
}

pub(super) struct EventSender {
    queue: Arc<EventQueue>,
}

impl EventSender {
    pub(super) fn try_send(&self, mut event: RendererEvent) -> Result<(), ProtocolError> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.receiver_open {
            return Err(ProtocolError::InvalidPayload(
                "browser renderer-event receiver closed",
            ));
        }

        if let RendererEvent::Presentation(next) = &mut event {
            // Layout and paint data are immutable snapshots. Accessibility is incremental, so fold
            // the queued update into its replacement before dropping the older presentation.
            if let Some(previous) = state.events.iter().find_map(|queued| match queued {
                RendererEvent::Presentation(previous)
                    if previous.document == next.document && previous.revision < next.revision =>
                {
                    Some(previous)
                }
                _ => None,
            }) {
                next.accessibility = previous
                    .accessibility
                    .clone()
                    .coalesce(next.accessibility.clone())?;
            }
            state
                .events
                .retain(|queued| !matches!(queued, RendererEvent::Presentation(_)));
        } else if matches!(&event, RendererEvent::FetchBatch(_))
            && state
                .events
                .iter()
                .filter(|queued| matches!(queued, RendererEvent::FetchBatch(_)))
                .count()
                >= MAX_QUEUED_RENDERER_FETCH_BATCHES
        {
            return Err(ProtocolError::InvalidPayload(
                "browser Fetch-batch event queue exhausted",
            ));
        }

        if state.events.len() >= MAX_QUEUED_RENDERER_EVENTS {
            return Err(ProtocolError::InvalidPayload(
                "browser renderer-event queue exhausted",
            ));
        }
        state.events.push_back(event);
        drop(state);
        self.queue.available.notify_one();
        Ok(())
    }
}

impl Drop for EventSender {
    fn drop(&mut self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.sender_open = false;
        drop(state);
        self.queue.available.notify_all();
    }
}

pub(super) struct EventReceiver {
    queue: Arc<EventQueue>,
}

impl EventReceiver {
    pub(super) fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<RendererEvent, mpsc::RecvTimeoutError> {
        let started = Instant::now();
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(event) = state.events.pop_front() {
                return Ok(event);
            }
            if !state.sender_open {
                return Err(mpsc::RecvTimeoutError::Disconnected);
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(mpsc::RecvTimeoutError::Timeout);
            }
            let (next, wait) = self
                .queue
                .available
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
            if wait.timed_out() && state.events.is_empty() {
                return Err(mpsc::RecvTimeoutError::Timeout);
            }
        }
    }

    pub(super) fn try_recv(&self) -> Result<RendererEvent, mpsc::TryRecvError> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match state.events.pop_front() {
            Some(event) => Ok(event),
            None if state.sender_open => Err(mpsc::TryRecvError::Empty),
            None => Err(mpsc::TryRecvError::Disconnected),
        }
    }
}

impl Drop for EventReceiver {
    fn drop(&mut self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.receiver_open = false;
        state.events.clear();
        drop(state);
        self.queue.available.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::renderer_protocol::{
        AccessibilityUpdate, DocumentId, DocumentNodeId, PageLoadReport, PresentedLayout,
        RendererPresentation, RuntimeReport, StyleReport,
    };

    fn presentation(revision: u64) -> RendererEvent {
        let root = DocumentNodeId::new((1_u128 << 64) | 1).unwrap();
        let mut accessibility =
            AccessibilityUpdate::full_root(root, "revision 1", crate::engine::RectF::default());
        if revision > 1 {
            accessibility.full = false;
            accessibility.nodes[0].name = format!("revision {revision}");
        }
        RendererEvent::Presentation(Box::new(RendererPresentation {
            document: DocumentId::new(1).unwrap(),
            revision,
            title: String::new(),
            final_url: "https://example.test/".into(),
            status: 200,
            character_set: "utf-8".into(),
            reader: Document {
                title: String::new(),
                source_url: "https://example.test/".into(),
                blocks: Vec::new(),
                truncated: false,
            },
            layout: PresentedLayout::default(),
            images: Vec::new(),
            glyph_epoch: 0,
            glyphs: Vec::new(),
            runtime: RuntimeReport::default(),
            style: StyleReport::default(),
            load: PageLoadReport::default(),
            page_diagnostics: Default::default(),
            accessibility,
            next_timer_micros: None,
        }))
    }

    #[test]
    fn presentation_bursts_keep_only_the_newest_snapshot() {
        let (sender, receiver) = bounded();
        sender.try_send(presentation(1)).unwrap();
        sender
            .try_send(RendererEvent::Diagnostic {
                code: 7,
                text: "between revisions".into(),
            })
            .unwrap();
        sender.try_send(presentation(2)).unwrap();
        sender.try_send(presentation(3)).unwrap();

        assert!(matches!(
            receiver.try_recv().unwrap(),
            RendererEvent::Diagnostic { code: 7, .. }
        ));
        let RendererEvent::Presentation(presentation) = receiver.try_recv().unwrap() else {
            panic!("newest presentation was not retained");
        };
        assert_eq!(presentation.revision, 3);
        assert!(presentation.accessibility.full);
        assert_eq!(presentation.accessibility.nodes[0].name, "revision 3");
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    fn transactional_fetch_batches_remain_bounded_and_reusable() {
        let (sender, receiver) = bounded();
        sender
            .try_send(RendererEvent::FetchBatch(Vec::new()))
            .unwrap();
        assert!(matches!(
            sender.try_send(RendererEvent::FetchBatch(Vec::new())),
            Err(ProtocolError::InvalidPayload(
                "browser Fetch-batch event queue exhausted"
            ))
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            RendererEvent::FetchBatch(batch) if batch.is_empty()
        ));
        sender
            .try_send(RendererEvent::FetchBatch(Vec::new()))
            .unwrap();
    }
}
