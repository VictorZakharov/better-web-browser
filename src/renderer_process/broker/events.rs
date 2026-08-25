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

        event = match event {
            RendererEvent::Presentation(next) => {
                let previous = state
                    .events
                    .iter()
                    .position(|queued| {
                        matches!(queued, RendererEvent::Presentation(previous) if previous.document == next.document)
                    })
                    .and_then(|index| state.events.remove(index))
                    .map(|event| match event {
                        RendererEvent::Presentation(presentation) => presentation,
                        _ => unreachable!("presentation position changed while queue was locked"),
                    });
                state
                    .events
                    .retain(|queued| !matches!(queued, RendererEvent::Presentation(_)));
                let next = match previous {
                    Some(previous) => previous.coalesce(*next)?,
                    None => *next,
                };
                RendererEvent::Presentation(Box::new(next))
            }
            RendererEvent::PointerCursor(next) => {
                if state.events.iter().any(|queued| {
                    matches!(
                        queued,
                        RendererEvent::PointerCursor(previous)
                            if previous.document == next.document
                                && previous.sequence >= next.sequence
                    )
                }) {
                    return Ok(());
                }
                state.events.retain(|queued| {
                    !matches!(queued, RendererEvent::PointerCursor(previous) if previous.document == next.document)
                });
                RendererEvent::PointerCursor(next)
            }
            event => event,
        };
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
        | RendererEvent::PointerCursor(crate::renderer_protocol::PointerCursorResult {
            document,
            ..
        })
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
mod tests;
