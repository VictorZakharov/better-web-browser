//! One replaceable absolute deadline, not an unbounded alarm-command queue.
use super::ScriptCancellation;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Instant;

#[derive(Default)]
struct Schedule {
    deadline: Option<(u64, Instant)>,
    stopped: bool,
}

#[derive(Default)]
struct Signal {
    state: Mutex<Schedule>,
    changed: Condvar,
}

pub(super) struct Timer {
    signal: Arc<Signal>,
    worker: Option<JoinHandle<()>>,
}

impl Timer {
    pub(super) fn start(cancellation: ScriptCancellation) -> std::io::Result<Self> {
        let signal = Arc::new(Signal::default());
        let remote = signal.clone();
        let worker = thread::Builder::new()
            .name("breeze-v8-watchdog".into())
            .spawn(move || {
                while let Some(generation) = remote.wait() {
                    // Never hold the schedule mutex while acquiring the agent's
                    // cancellation mutex. Its generation check owns the race
                    // between an expired alarm and a newly entered task.
                    cancellation.timeout(generation);
                }
            })?;
        Ok(Self {
            signal,
            worker: Some(worker),
        })
    }

    pub(super) fn arm(&self, generation: u64, deadline: Instant) -> bool {
        let mut state = self.signal.state();
        if state.stopped {
            return false;
        }
        state.deadline = Some((generation, deadline));
        self.signal.changed.notify_one();
        true
    }

    pub(super) fn disarm(&self, generation: u64) {
        let mut state = self.signal.state();
        if state
            .deadline
            .is_some_and(|(active, _)| active == generation)
        {
            state.deadline = None;
            self.signal.changed.notify_one();
        }
    }

    pub(super) fn shutdown(&mut self, join: bool) {
        {
            let mut state = self.signal.state();
            state.stopped = true;
            state.deadline = None;
            self.signal.changed.notify_one();
        }
        if let Some(worker) = self.worker.take()
            && join
        {
            let _ = worker.join();
        }
        // A non-joining TLS shutdown still signals the detached thread to exit.
        // Its Arc owns the signal until it stops; no stack storage is borrowed.
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.shutdown(true);
    }
}

impl Signal {
    fn state(&self) -> MutexGuard<'_, Schedule> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn wait(&self) -> Option<u64> {
        let mut state = self.state();
        loop {
            if state.stopped {
                return None;
            }
            match state.deadline {
                None => {
                    state = self
                        .changed
                        .wait(state)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                Some((generation, deadline)) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        state.deadline = None;
                        return Some(generation);
                    }
                    // Recheck the absolute deadline after EVERY wake, including
                    // notifications and spurious wakes. They never restart the
                    // budget, and a replacement arm replaces the single slot.
                    state = self
                        .changed
                        .wait_timeout(state, remaining)
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
