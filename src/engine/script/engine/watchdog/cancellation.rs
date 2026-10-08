//! Thread-safe retirement of one script agent, including tasks already executing.
use super::super::value::{JsError, JsErrorKind, JsResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Default)]
pub struct ScriptCancellation {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    interrupted: AtomicBool,
}

#[derive(Default)]
struct State {
    retired: bool,
    handle: Option<v8::IsolateHandle>,
    active: Option<u64>,
    reason: Option<Reason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reason {
    Timeout,
    Cancelled,
}

impl ScriptCancellation {
    /// Permanently retire this agent. Safe from another thread while author
    /// JavaScript is running, and safe before its isolate has been created.
    /// This is not a reusable "cancel next task" flag: navigation and Worker
    /// termination must never allow the retired realm to execute again.
    pub fn cancel(&self) {
        let mut state = self.state();
        state.retired = true;
        if state.active.is_some() {
            self.interrupt(&mut state, Reason::Cancelled);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.state().retired
    }

    pub(super) fn attach(&self, handle: v8::IsolateHandle) -> JsResult<()> {
        let mut state = self.state();
        if state.retired {
            return Err(cancelled_error());
        }
        if state.handle.is_some() {
            return Err(JsError {
                kind: JsErrorKind::Error,
                message: "script cancellation control already belongs to an agent".into(),
            });
        }
        state.handle = Some(handle);
        Ok(())
    }

    pub(super) fn begin(&self, generation: u64) -> JsResult<()> {
        let mut state = self.state();
        if state.retired {
            return Err(cancelled_error());
        }
        state.active = Some(generation);
        state.reason = None;
        self.shared.interrupted.store(false, Ordering::Release);
        Ok(())
    }

    pub(super) fn timeout(&self, generation: u64) {
        let mut state = self.state();
        if state.active == Some(generation) && state.reason.is_none() {
            self.interrupt(&mut state, Reason::Timeout);
        }
    }

    pub(super) fn finish(&self) -> Option<Reason> {
        let mut state = self.state();
        state.active = None;
        state.reason.take()
    }

    pub(super) fn detach(&self) {
        let mut state = self.state();
        state.active = None;
        state.handle = None;
        // A control must never be rebound to a new document after disposal.
        state.retired = true;
    }

    pub(super) fn interrupted(&self) -> &AtomicBool {
        &self.shared.interrupted
    }

    fn interrupt(&self, state: &mut State, reason: Reason) {
        state.reason = Some(reason);
        self.shared.interrupted.store(true, Ordering::Release);
        // Serialize the V8 call with begin/finish/detach. Otherwise a timeout
        // can win the old generation check, pause, then terminate a NEW task.
        // No author code runs under this lock. IsolateHandle is V8's supported
        // cross-thread API and remains safe even after isolate disposal.
        if let Some(handle) = &state.handle {
            handle.terminate_execution();
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

pub(super) fn cancelled_error() -> JsError {
    JsError {
        kind: JsErrorKind::Error,
        message: "JavaScript execution cancelled: script agent has been retired".into(),
    }
}

#[cfg(test)]
mod tests;
