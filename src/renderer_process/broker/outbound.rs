//! Nonblocking browser-to-renderer IPC writes.
//!
//! The broker owns renderer liveness and must never block in `WriteFile` when a page task stops
//! reading its command pipe. A dedicated writer may block, while this bounded mailbox leaves the
//! broker free to enforce heartbeat and shutdown deadlines.

use super::wake::BrokerWake;
use crate::limits::{MAX_QUEUED_BROWSER_COMMANDS, MAX_QUEUED_BROWSER_WRITES};
use crate::renderer_protocol::{BrowserMessage, FrameWriter, RendererSessionId};
use std::fs::File;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

#[derive(Default)]
struct State {
    queued: usize,
    open: bool,
    failure: Option<String>,
}

pub(super) struct Sender {
    messages: mpsc::Sender<BrowserMessage>,
    state: Arc<Mutex<State>>,
    wake: BrokerWake,
}

#[derive(Clone)]
pub(super) struct Diagnostics {
    state: Arc<Mutex<State>>,
}

pub(super) fn spawn(
    writer: FrameWriter<File>,
    session: RendererSessionId,
    wake: BrokerWake,
) -> Result<(Sender, Diagnostics, JoinHandle<()>), String> {
    let (messages, receiver) = mpsc::channel();
    let state = Arc::new(Mutex::new(State {
        open: true,
        ..State::default()
    }));
    let worker_state = Arc::clone(&state);
    let worker_wake = wake.clone();
    let handle = std::thread::Builder::new()
        .name(format!("breeze-renderer-ipc-write-{}", session.get()))
        .spawn(move || run(writer, receiver, worker_state, worker_wake))
        .map_err(|error| format!("start renderer IPC writer: {error}"))?;
    let diagnostics = Diagnostics {
        state: Arc::clone(&state),
    };
    Ok((
        Sender {
            messages,
            state,
            wake,
        },
        diagnostics,
        handle,
    ))
}

impl Diagnostics {
    pub(super) fn pending(&self) -> usize {
        lock(&self.state).queued
    }
}

impl Sender {
    pub(super) fn pending(&self) -> usize {
        lock(&self.state).queued
    }

    pub(super) fn send_browser(&self, message: &BrowserMessage) -> Result<(), String> {
        {
            let mut state = lock(&self.state);
            if let Some(error) = state.failure.as_ref() {
                return Err(error.clone());
            }
            if !state.open {
                return Err("renderer IPC writer has exited".into());
            }
            if state.queued >= MAX_QUEUED_BROWSER_WRITES {
                return Err("renderer command pipe backlog is exhausted".into());
            }
            state.queued += 1;
        }
        if self.messages.send(message.clone()).is_err() {
            let mut state = lock(&self.state);
            state.queued = state.queued.saturating_sub(1);
            state.open = false;
            return Err("renderer IPC writer has exited".into());
        }
        self.wake.notify();
        Ok(())
    }

    /// Ordinary page commands stop draining before they can hide backpressure in the writer.
    /// Lifecycle transfers and heartbeats may use the larger bounded mailbox directly.
    pub(super) fn has_page_command_capacity(&self) -> bool {
        let state = lock(&self.state);
        state.open && state.failure.is_none() && state.queued < MAX_QUEUED_BROWSER_COMMANDS
    }

    pub(super) fn take_failure(&self) -> Option<String> {
        lock(&self.state).failure.take()
    }
}

fn run(
    mut writer: FrameWriter<File>,
    receiver: mpsc::Receiver<BrowserMessage>,
    state: Arc<Mutex<State>>,
    wake: BrokerWake,
) {
    while let Ok(message) = receiver.recv() {
        let result = writer.send_browser(&message);
        {
            let mut state = lock(&state);
            state.queued = state.queued.saturating_sub(1);
            if let Err(error) = result {
                state.failure = Some(error.to_string());
                state.open = false;
            }
        }
        wake.notify();
        if lock(&state).failure.is_some() {
            return;
        }
    }
    lock(&state).open = false;
    wake.notify();
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_capacity_preserves_outer_command_backpressure() {
        let (messages, _receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(State {
            open: true,
            queued: MAX_QUEUED_BROWSER_COMMANDS,
            failure: None,
        }));
        let sender = Sender {
            messages,
            state,
            wake: BrokerWake::default(),
        };
        assert!(!sender.has_page_command_capacity());
    }
}
