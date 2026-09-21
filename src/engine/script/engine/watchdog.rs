use super::value::{JsError, JsErrorKind, JsResult};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

const EXECUTION_TIMEOUT: Duration = if cfg!(test) {
    // Keep runaway-script tests fast without making ordinary bounded V8 work fail when the
    // complete test suite saturates the hosted runner. Production retains the stricter runtime
    // policy below; this branch only controls the test binary.
    Duration::from_millis(500)
} else {
    Duration::from_secs(2)
};

enum Command {
    Arm(u64),
    Stop,
}

pub(super) struct ExecutionWatchdog {
    sender: mpsc::Sender<Command>,
    active: Arc<AtomicU64>,
    timed_out: Arc<AtomicBool>,
    next_generation: u64,
    worker: Option<thread::JoinHandle<()>>,
}

struct IsolateEntry {
    isolate: *mut v8::OwnedIsolate,
    /// Whether a page isolate was already entered (nested same-isolate
    /// entry). Restored on drop so Rust-side V8 users observe the exact
    /// nesting state instead of a blind clear.
    was_entered: bool,
}

impl IsolateEntry {
    fn new(isolate: &mut v8::OwnedIsolate) -> Self {
        // SAFETY: Context serializes access on its owning thread. This balances the matching exit
        // in Drop and temporarily restores whichever retained document isolate was current.
        let was_entered = crate::engine::pattern_eval::set_page_isolate_entered(true);
        unsafe { isolate.enter() };
        Self {
            isolate,
            was_entered,
        }
    }
}

impl Drop for IsolateEntry {
    fn drop(&mut self) {
        // SAFETY: this guard is dropped before another isolate can be entered on this thread.
        unsafe { (*self.isolate).exit() };
        crate::engine::pattern_eval::set_page_isolate_entered(self.was_entered);
    }
}

impl ExecutionWatchdog {
    pub(super) fn new(handle: v8::IsolateHandle) -> JsResult<Self> {
        let (sender, receiver) = mpsc::channel();
        let active = Arc::new(AtomicU64::new(0));
        let timed_out = Arc::new(AtomicBool::new(false));
        let worker_active = Arc::clone(&active);
        let worker_timed_out = Arc::clone(&timed_out);
        let worker = thread::Builder::new()
            .name("breeze-v8-watchdog".into())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    let Command::Arm(mut generation) = command else {
                        return;
                    };
                    loop {
                        match receiver.recv_timeout(EXECUTION_TIMEOUT) {
                            Ok(Command::Arm(replacement)) => generation = replacement,
                            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if worker_active
                                    .compare_exchange(
                                        generation,
                                        0,
                                        Ordering::AcqRel,
                                        Ordering::Acquire,
                                    )
                                    .is_ok()
                                {
                                    worker_timed_out.store(true, Ordering::Release);
                                    handle.terminate_execution();
                                }
                                break;
                            }
                        }
                    }
                }
            })
            .map_err(|error| JsError {
                kind: JsErrorKind::Error,
                message: format!("could not start the V8 execution watchdog: {error}"),
            })?;
        Ok(Self {
            sender,
            active,
            timed_out,
            next_generation: 1,
            worker: Some(worker),
        })
    }

    pub(super) fn run<T>(
        &mut self,
        isolate: &mut v8::OwnedIsolate,
        action: impl FnOnce(&mut v8::OwnedIsolate) -> JsResult<T>,
    ) -> JsResult<T> {
        self.timed_out.store(false, Ordering::Release);
        let generation = self.next_generation;
        self.next_generation = self.next_generation.checked_add(1).unwrap_or(1);
        self.active.store(generation, Ordering::Release);
        self.sender
            .send(Command::Arm(generation))
            .map_err(|_| JsError {
                kind: JsErrorKind::Error,
                message: "V8 execution watchdog is unavailable".into(),
            })?;

        let entry = IsolateEntry::new(isolate);
        let result = action(isolate);
        self.active.store(0, Ordering::Release);
        let timed_out = self.timed_out.swap(false, Ordering::AcqRel);
        drop(entry);
        if timed_out {
            Err(JsError {
                kind: JsErrorKind::Range,
                message: format!(
                    "JavaScript execution time limit exceeded after {} ms",
                    EXECUTION_TIMEOUT.as_millis()
                ),
            })
        } else {
            result
        }
    }
}

impl Drop for ExecutionWatchdog {
    fn drop(&mut self) {
        self.active.store(0, Ordering::Release);
        let _ = self.sender.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
