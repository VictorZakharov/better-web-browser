//! Bounded newest-wins delivery for browser-owned viewport synchronization.

use crate::renderer_protocol::{DocumentId, PresentedViewport};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Update {
    pub(super) document: DocumentId,
    pub(super) viewport: PresentedViewport,
}

#[derive(Default)]
struct State {
    pending: Option<Update>,
    receiver_open: bool,
}

pub(super) struct Sender {
    state: Arc<Mutex<State>>,
}

pub(super) struct Receiver {
    state: Arc<Mutex<State>>,
}

pub(super) fn bounded() -> (Sender, Receiver) {
    let state = Arc::new(Mutex::new(State {
        pending: None,
        receiver_open: true,
    }));
    (
        Sender {
            state: Arc::clone(&state),
        },
        Receiver { state },
    )
}

impl Sender {
    pub(super) fn send(&self, update: Update) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.receiver_open {
            return Err("renderer broker has exited".into());
        }
        state.pending = Some(update);
        Ok(())
    }

    pub(super) fn pending(&self) -> usize {
        usize::from(
            self.state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pending
                .is_some(),
        )
    }
}

impl Receiver {
    pub(super) fn take(&self) -> Option<Update> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending
            .take()
    }

    pub(super) fn pending(&self) -> usize {
        usize::from(
            self.state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pending
                .is_some(),
        )
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.receiver_open = false;
        state.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_viewport_replaces_pressure_from_the_same_or_retired_document() {
        let (sender, receiver) = bounded();
        sender.send(update(1, 640.0)).unwrap();
        sender.send(update(1, 800.0)).unwrap();
        let replacement = update(2, 1024.0);
        sender.send(replacement).unwrap();
        assert_eq!(sender.pending(), 1);
        assert_eq!(receiver.take(), Some(replacement));
        assert_eq!(receiver.pending(), 0);
    }

    #[test]
    fn closing_receiver_rejects_later_viewports() {
        let (sender, receiver) = bounded();
        drop(receiver);
        assert_eq!(
            sender.send(update(1, 640.0)),
            Err("renderer broker has exited".into())
        );
    }

    fn update(document: u64, width: f32) -> Update {
        Update {
            document: DocumentId::new(document).unwrap(),
            viewport: PresentedViewport {
                width,
                height: 600.0,
                style_width: width,
                dpi: 96,
                prefers_dark_color_scheme: false,
            },
        }
    }
}
