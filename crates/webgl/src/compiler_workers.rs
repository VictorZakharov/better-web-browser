//! Renderer-wide, bounded execution of ANGLE-owned compile/link closures.
//! Native contexts remain on the GPU owner thread; ANGLE owns task lifetimes and synchronization.
use mozangle::egl::ffi as egl;
use std::{
    ffi::c_void,
    sync::{Arc, Condvar, Mutex, OnceLock, mpsc},
    thread,
};

const WORKERS: usize = 4;
const QUEUED_TASKS: usize = 128;
type Callback = unsafe extern "C" fn(*mut c_void);
type PostTask = unsafe extern "C" fn(*mut c_void, Callback, *mut c_void);

unsafe extern "C" {
    fn breeze_angle_set_worker_delegate(
        display: *const c_void,
        entry: *const c_void,
        post: PostTask,
    ) -> bool;
}

struct Task {
    callback: Callback,
    data: *mut c_void,
}

// SAFETY: only ANGLE's delegate contract creates these tasks. It explicitly
// transfers this closure to a worker, retains its referenced native resources,
// and frees the opaque allocation after the callback runs exactly once.
unsafe impl Send for Task {}

impl Task {
    fn run(self) {
        // SAFETY: the native callback owns data; neither pointer comes from author code.
        unsafe { (self.callback)(self.data) };
    }
}

struct Pool {
    sender: mpsc::SyncSender<Work>,
    pending: Arc<Pending>,
}

#[derive(Default)]
struct Pending {
    count: Mutex<usize>,
    finished: Condvar,
}

impl Pending {
    fn add(&self) {
        *self.count.lock().unwrap_or_else(|error| error.into_inner()) += 1;
    }

    fn wait(&self) {
        let count = self.count.lock().unwrap_or_else(|error| error.into_inner());
        drop(
            self.finished
                .wait_while(count, |count| *count != 0)
                .unwrap_or_else(|error| error.into_inner()),
        );
    }

    fn link_capacity(&self) -> bool {
        // Pinned ProgramD3D::GraphicsProgramLinkEvent posts three executable
        // tasks. Count includes running work, so reserving against the queue
        // size alone is conservative even if all four workers are occupied.
        // The GPU owner is the sole native producer. Only nonblocking status
        // queries occur before ready submission; workers can only free slots.
        *self.count.lock().unwrap_or_else(|error| error.into_inner()) <= QUEUED_TASKS - 3
    }
}

struct Work {
    task: Task,
    pending: Arc<Pending>,
}

impl Work {
    fn run(self) {
        self.task.run();
        let mut count = self
            .pending
            .count
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *count -= 1;
        if *count == 0 {
            self.pending.finished.notify_all();
        }
    }
}

impl Pool {
    fn new(workers: usize, capacity: usize) -> Result<Self, String> {
        assert!(workers > 0 && workers <= WORKERS);
        assert!(capacity > 0 && capacity <= QUEUED_TASKS);
        let (sender, receiver) = mpsc::sync_channel::<Work>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..workers {
            let receiver = receiver.clone();
            thread::Builder::new()
                .name(format!("breeze-angle-compiler-{index}"))
                .stack_size(8 * 1024 * 1024)
                .spawn(move || {
                    loop {
                        // The mailbox lock protects only receipt, never native execution.
                        let task = receiver
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .recv();
                        match task {
                            Ok(task) => task.run(),
                            Err(_) => break,
                        }
                    }
                })
                .map_err(|error| format!("Start bounded ANGLE compiler worker: {error}"))?;
        }
        Ok(Self {
            sender,
            pending: Arc::default(),
        })
    }

    fn post(&self, task: Task) {
        // Backpressure at 128 queued closures, not an unbounded native work list.
        // A disconnected worker pool must still finish ANGLE's event exactly once.
        self.pending.add();
        let work = Work {
            task,
            pending: self.pending.clone(),
        };
        if let Err(error) = self.sender.send(work) {
            error.0.run();
        }
    }
}

static POOL: OnceLock<Result<Pool, String>> = OnceLock::new();

pub(super) fn accepts_ready_link() -> bool {
    POOL.get()
        .is_some_and(|pool| pool.as_ref().is_ok_and(|pool| pool.pending.link_capacity()))
}

#[cfg(test)]
static NATIVE_SUBMISSIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(super) fn native_submissions() -> usize {
    NATIVE_SUBMISSIONS.load(std::sync::atomic::Ordering::SeqCst)
}

unsafe extern "C" fn post_task(_: *mut c_void, callback: Callback, data: *mut c_void) {
    #[cfg(test)]
    NATIVE_SUBMISSIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    // Installation happens only after successful pool creation. Global workers
    // live until renderer exit; no joins execute under Windows' TLS loader lock.
    if let Some(Ok(pool)) = POOL.get() {
        pool.post(Task { callback, data });
    } else {
        // Defensive, non-dropping fallback; this path is not installed normally.
        Task { callback, data }.run();
    }
}

pub(super) fn before_display_initialization() {
    // The pinned provider stores PlatformMethods process-wide and resets them
    // when initializing a new display. Quiesce native workers before that write;
    // only the GPU owner creates displays/submits tasks, so none can race this gap.
    if let Some(Ok(pool)) = POOL.get() {
        pool.pending.wait();
    }
}

pub(super) fn install(display: egl::types::EGLDisplay) -> Result<(), String> {
    POOL.get_or_init(|| Pool::new(WORKERS, QUEUED_TASKS))
        .as_ref()
        .map_err(Clone::clone)?;
    // SAFETY: an initialized display and one fixed linked-provider entry point.
    // The C++ shim verifies the pinned method-name/layout signature before mutation.
    let entry = unsafe { egl::GetProcAddress(c"ANGLEGetDisplayPlatform".as_ptr()) };
    if entry.is_null()
        || !unsafe { breeze_angle_set_worker_delegate(display, entry.cast(), post_task) }
    {
        return Err("ANGLE compiler-delegate signature/entry point unavailable".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
