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
        changed: Condvar::new(),
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
    changed: Condvar,
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
    pub(super) fn send(&self, event: RendererEvent) -> Result<(), ProtocolError> {
        if matches!(event, RendererEvent::FetchBatch { .. }) {
            self.send_lossless_fetch(event)
        } else {
            self.try_send(event)
        }
    }

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
        }
        if state.events.len() >= MAX_QUEUED_RENDERER_EVENTS {
            return Err(ProtocolError::InvalidPayload(
                "browser renderer-event queue exhausted",
            ));
        }
        state.events.push_back(event);
        drop(state);
        self.queue.changed.notify_one();
        Ok(())
    }

    fn send_lossless_fetch(&self, event: RendererEvent) -> Result<(), ProtocolError> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while state.receiver_open
            && (state.events.len() >= MAX_QUEUED_RENDERER_EVENTS
                || queued_fetch_batches(&state.events) >= MAX_QUEUED_RENDERER_FETCH_BATCHES)
        {
            // A Fetch batch is valid page work, not a protocol violation. Apply bounded
            // backpressure on the broker thread until the Win32 thread drains its event slot.
            // Closing the browser-side receiver releases this wait during renderer teardown.
            state = self
                .queue
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        if !state.receiver_open {
            return Ok(());
        }
        state.events.push_back(event);
        drop(state);
        self.queue.changed.notify_one();
        Ok(())
    }

    pub(super) fn discard_document(&self, document: crate::renderer_protocol::DocumentId) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .events
            .retain(|event| event_document(event) != Some(document));
        drop(state);
        self.queue.changed.notify_all();
    }
}

fn queued_fetch_batches(events: &VecDeque<RendererEvent>) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, RendererEvent::FetchBatch { .. }))
        .count()
}

fn event_document(event: &RendererEvent) -> Option<crate::renderer_protocol::DocumentId> {
    match event {
        RendererEvent::FetchBatch { document, .. }
        | RendererEvent::TimeAdvanced { document, .. }
        | RendererEvent::DocumentFailed { document, .. }
        | RendererEvent::NavigationRequested { document, .. } => Some(*document),
        RendererEvent::Presentation(presentation) => Some(presentation.document),
        RendererEvent::CookieMutation(mutation) => Some(mutation.document),
        RendererEvent::StorageMutation(request) => Some(request.document),
        RendererEvent::Diagnostic { .. }
        | RendererEvent::Unresponsive
        | RendererEvent::Exited(_) => None,
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
        self.queue.changed.notify_all();
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
                drop(state);
                self.queue.changed.notify_all();
                return Ok(event);
            }
            if !state.receiver_open {
                return Err(mpsc::RecvTimeoutError::Disconnected);
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
                .changed
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
        let result = match state.events.pop_front() {
            Some(event) => Ok(event),
            None if state.receiver_open && state.sender_open => Err(mpsc::TryRecvError::Empty),
            None => Err(mpsc::TryRecvError::Disconnected),
        };
        drop(state);
        if result.is_ok() {
            self.queue.changed.notify_all();
        }
        result
    }

    pub(super) fn close(&self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.receiver_open = false;
        state.events.clear();
        drop(state);
        self.queue.changed.notify_all();
    }
}

impl Drop for EventReceiver {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod teardown_tests;

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
    fn consecutive_fetch_batches_wait_for_browser_drain_without_failing() {
        let (sender, receiver) = bounded();
        let document = DocumentId::new(1).unwrap();
        sender
            .send(RendererEvent::FetchBatch {
                document,
                requests: Vec::new(),
            })
            .unwrap();

        let (completed, completion) = mpsc::channel();
        let producer = std::thread::spawn(move || {
            let result = sender.send(RendererEvent::FetchBatch {
                document,
                requests: Vec::new(),
            });
            completed.send(result).unwrap();
        });
        assert!(matches!(
            completion.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            RendererEvent::FetchBatch {
                document: first,
                requests
            } if first == document && requests.is_empty()
        ));
        completion
            .recv_timeout(Duration::from_secs(1))
            .expect("Fetch producer resumed after the browser drained its slot")
            .unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap(),
            RendererEvent::FetchBatch {
                document: second,
                requests
            } if second == document && requests.is_empty()
        ));
        producer.join().unwrap();
    }

    #[test]
    fn cancelled_transactional_fetch_batches_are_discarded_and_reusable() {
        let (sender, receiver) = bounded();
        let replaced = DocumentId::new(1).unwrap();
        sender
            .send(RendererEvent::FetchBatch {
                document: replaced,
                requests: Vec::new(),
            })
            .unwrap();
        sender.discard_document(replaced);
        let replacement = DocumentId::new(2).unwrap();
        sender
            .send(RendererEvent::FetchBatch {
                document: replacement,
                requests: Vec::new(),
            })
            .unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap(),
            RendererEvent::FetchBatch { document, requests }
                if document == replacement && requests.is_empty()
        ));
    }
}
