//! Bounded renderer-to-browser event delivery.

mod notification;

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
            notification: notification::Notification::default(),
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
    notification: notification::Notification,
}

pub(super) struct EventSender {
    queue: Arc<EventQueue>,
}

impl EventSender {
    pub(super) fn pending(&self) -> usize {
        self.queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .events
            .len()
    }

    pub(super) fn send(&self, event: RendererEvent) -> Result<(), ProtocolError> {
        if matches!(
            event,
            RendererEvent::FetchBatch { .. }
                | RendererEvent::FetchAbort { .. }
                | RendererEvent::StorageMutation(_)
                | RendererEvent::WebSocketCommand(_)
        ) {
            self.send_lossless(event)
        } else {
            self.send_coalesced(event, true)
        }
    }

    pub(super) fn try_send(&self, event: RendererEvent) -> Result<(), ProtocolError> {
        self.send_coalesced(event, false)
    }

    fn send_coalesced(&self, mut event: RendererEvent, wait: bool) -> Result<(), ProtocolError> {
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
            RendererEvent::VideoFrame(next) => {
                // One retained video frame per session, not an unbounded playback backlog.
                state
                    .events
                    .retain(|queued| !matches!(queued, RendererEvent::VideoFrame(_)));
                RendererEvent::VideoFrame(next)
            }
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
            RendererEvent::RuntimeUpdate(next) => {
                if let Some(RendererEvent::RuntimeUpdate(previous)) = state.events.back_mut()
                    && previous.document == next.document
                {
                    **previous = (**previous).clone().coalesce(*next)?;
                    let notify = state.notification.request();
                    drop(state);
                    self.queue.changed.notify_one();
                    notification::deliver(notify);
                    return Ok(());
                }
                RendererEvent::RuntimeUpdate(next)
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
        while state.receiver_open && state.events.len() >= MAX_QUEUED_RENDERER_EVENTS {
            if !wait {
                return Err(ProtocolError::InvalidPayload(
                    "browser renderer-event queue exhausted",
                ));
            }
            // A storage/fetch burst may fill every slot before its trailing
            // presentation or diagnostic arrives. Preserve that FIFO barrier
            // with backpressure too; a full valid queue is not a protocol error.
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
        let notify = state.notification.request();
        drop(state);
        self.queue.changed.notify_one();
        notification::deliver(notify);
        Ok(())
    }

    fn send_lossless(&self, event: RendererEvent) -> Result<(), ProtocolError> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while state.receiver_open
            && (state.events.len() >= MAX_QUEUED_RENDERER_EVENTS
                || queued_fetch_batches(&state.events) >= MAX_QUEUED_RENDERER_FETCH_BATCHES
                || storage_bytes(&state.events).saturating_add(event_storage_bytes(&event))
                    > crate::limits::MAX_PENDING_STORAGE_BYTES)
        {
            // Fetch and storage writes are valid page work. Apply bounded
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
        let notify = state.notification.request();
        drop(state);
        self.queue.changed.notify_one();
        notification::deliver(notify);
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

fn event_storage_bytes(event: &RendererEvent) -> usize {
    match event {
        RendererEvent::StorageMutation(request) => request.mutation.byte_len(),
        _ => 0,
    }
}

fn storage_bytes(events: &VecDeque<RendererEvent>) -> usize {
    events.iter().map(event_storage_bytes).sum()
}

fn event_document(event: &RendererEvent) -> Option<crate::renderer_protocol::DocumentId> {
    match event {
        RendererEvent::FetchBatch { document, .. }
        | RendererEvent::FetchAbort { document, .. }
        | RendererEvent::DocumentFailed { document, .. }
        | RendererEvent::PointerCursor(crate::renderer_protocol::PointerCursorResult {
            document,
            ..
        })
        | RendererEvent::NavigationRequested { document, .. } => Some(*document),
        RendererEvent::Presentation(presentation) => Some(presentation.document),
        RendererEvent::VideoFrame(update) => Some(update.identity.document),
        RendererEvent::RuntimeUpdate(update) => Some(update.document),
        RendererEvent::CookieMutation(mutation) => Some(mutation.document),
        RendererEvent::StorageMutation(request) => Some(request.document),
        RendererEvent::WebSocketCommand(command) => Some(command.document),
        RendererEvent::FullscreenRequested(request) => Some(request.document),
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
    pub(super) fn pending(&self) -> usize {
        self.queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .events
            .len()
    }

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
        self.try_recv_if(|_| true)
    }

    pub(super) fn try_recv_if(
        &self,
        accepts: impl FnOnce(&RendererEvent) -> bool,
    ) -> Result<RendererEvent, mpsc::TryRecvError> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.events.front().is_some_and(|event| !accepts(event)) {
            return Err(mpsc::TryRecvError::Empty);
        }
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
        state.notification = notification::Notification::default();
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
mod storage_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod video_tests;
