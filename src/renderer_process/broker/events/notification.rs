//! Coalesced, optional integration with the consumer's native event loop.

use super::EventReceiver;
use std::sync::Arc;

type Callback = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub(super) struct Notification {
    callback: Option<Callback>,
    pending: bool,
}

impl Notification {
    pub(super) fn request(&mut self) -> Option<Callback> {
        if self.pending {
            return None;
        }
        let callback = self.callback.clone()?;
        self.pending = true;
        Some(callback)
    }
}

pub(super) fn deliver(callback: Option<Callback>) {
    // Never invoke application code while holding the event-queue mutex.
    if let Some(callback) = callback {
        callback();
    }
}

impl EventReceiver {
    pub(in super::super) fn set_notifier(&self, callback: impl Fn() + Send + Sync + 'static) {
        let mut state = self.queue.state.lock().unwrap_or_else(|p| p.into_inner());
        if !state.receiver_open {
            return;
        }
        state.notification.callback = Some(Arc::new(callback));
        state.notification.pending = false;
        let notify = if state.events.is_empty() {
            None
        } else {
            state.notification.request()
        };
        drop(state);
        deliver(notify);
    }

    pub(in super::super) fn finish_drain(&self) -> bool {
        let mut state = self.queue.state.lock().unwrap_or_else(|p| p.into_inner());
        let remaining = !state.events.is_empty();
        // Producers and this re-arm share the queue lock: an arrival either belongs
        // to the remaining batch or observes an armed notifier, never neither.
        // A busy consumer continues at low priority instead of endlessly posting
        // messages ahead of Win32 input, WM_PAINT and WM_TIMER.
        if !remaining {
            state.notification.pending = false;
        }
        remaining
    }
}

#[cfg(test)]
mod tests;
