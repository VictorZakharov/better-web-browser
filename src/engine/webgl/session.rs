//! One native owner thread. Realm callers never move ANGLE contexts across threads.
use super::{
    BackendContexts, MAX_CONTEXTS, MAX_SHADER_BYTES, MAX_UPLOAD_BYTES, PixelReply, gl, json,
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, OnceLock, Weak, mpsc},
    time::Duration,
};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);
static OWNER: OnceLock<Option<mpsc::SyncSender<Request>>> = OnceLock::new();
static RETIRED: Mutex<Vec<u32>> = Mutex::new(Vec::new());
type Bitmap = (u32, u32, Vec<u8>);
enum Operation {
    Create(u32, u32, String, Weak<()>),
    Command(u32, String, Option<Vec<u8>>),
    Pixels(u32, String, Option<Vec<u8>>),
    Snapshot(u32),
    TaskBoundary(Vec<u32>),
    #[cfg(test)]
    NativeTest(fn()),
}
enum Reply {
    Created(Option<u32>),
    Command(Value),
    Pixels(PixelReply),
    Snapshot(Option<Bitmap>),
    TaskBoundary(Vec<u32>),
    #[cfg(test)]
    NativeTest(Option<String>),
}
struct Request {
    operation: Operation,
    reply: mpsc::SyncSender<Reply>,
}

#[derive(Default)]
pub(crate) struct Contexts {
    live: HashSet<u32>,
    leases: HashMap<u32, Arc<()>>,
    asynchronous: HashSet<u32>,
    sync_replies: super::sync_reply_cache::Cache,
}
impl Contexts {
    pub(crate) fn create(&mut self, width: u32, height: u32, options: &str) -> Option<u32> {
        if self.live.len() >= MAX_CONTEXTS || options.len() > 1024 {
            return None;
        }
        let api = serde_json::from_str::<super::Options>(options).ok()?.api;
        let lease = Arc::new(());
        let Reply::Created(Some(id)) = request(Operation::Create(
            width,
            height,
            options.into(),
            Arc::downgrade(&lease),
        ))?
        else {
            return None;
        };
        self.live.insert(id);
        self.leases.insert(id, lease);
        if api == super::ApiVersion::Two {
            self.asynchronous.insert(id);
        }
        Some(id)
    }
    pub(crate) fn remove(&mut self, id: u32) {
        self.sync_replies.remove(id);
        if self.live.remove(&id) {
            self.leases.remove(&id);
            self.asynchronous.remove(&id);
            retire(std::iter::once(id));
        }
    }
    pub(crate) fn clear(&mut self) {
        self.sync_replies.clear();
        if self.live.is_empty() {
            return;
        }
        retire(self.live.drain());
        self.leases.clear();
        self.asynchronous.clear();
    }
    /// Called by the embedder only after an HTML task and its microtask checkpoint.
    /// Ordinary commands, presentation and nested checkpoints never publish results.
    pub(crate) fn complete_task(&mut self) {
        self.sync_replies.clear();
        if self.asynchronous.is_empty() {
            return;
        }
        let ids: Vec<_> = self.asynchronous.iter().copied().collect();
        let lost = match request(Operation::TaskBoundary(ids.clone())) {
            Some(Reply::TaskBoundary(lost)) => lost,
            // A stopped/timed-out owner cannot leave usable cached GPU handles.
            _ => ids,
        };
        for id in lost {
            self.remove(id);
        }
    }
    pub(crate) fn execute(&mut self, id: u32, command: &str, bytes: Option<&[u8]>) -> Value {
        if !self.live.contains(&id) {
            return json!({"lost":true});
        }
        // Reject before copying data into the bounded submission queue.
        let (serialized, bytes) = if command.len() > MAX_SHADER_BYTES + 4096
            || bytes.is_some_and(|b| b.len() > MAX_UPLOAD_BYTES)
        {
            (
                json!({"op":"bridgeError","i":[gl::OUT_OF_MEMORY]}).to_string(),
                None,
            )
        } else {
            if let Some(value) = self.sync_replies.get(id, command) {
                return value;
            }
            (command.into(), bytes.map(<[u8]>::to_vec))
        };
        match request(Operation::Command(id, serialized, bytes)) {
            Some(Reply::Command(value)) => {
                self.sync_replies.record(id, command, &value);
                value
            }
            _ => {
                self.sync_replies.remove(id);
                json!({"lost":true})
            }
        }
    }
    pub(crate) fn read_pixels(
        &mut self,
        id: u32,
        command: &str,
        bytes: Option<&[u8]>,
    ) -> PixelReply {
        if !self.live.contains(&id) {
            return PixelReply::Lost;
        }
        if command.len() > 1024 || bytes.is_some_and(|b| b.len() > MAX_UPLOAD_BYTES) {
            self.execute(id, r#"{"op":"bridgeError","i":[1285]}"#, None);
            return PixelReply::Error;
        }
        match request(Operation::Pixels(
            id,
            command.into(),
            bytes.map(<[u8]>::to_vec),
        )) {
            Some(Reply::Pixels(value)) => {
                if matches!(value, PixelReply::Lost) {
                    self.sync_replies.remove(id);
                }
                value
            }
            _ => {
                self.sync_replies.remove(id);
                PixelReply::Lost
            }
        }
    }
    pub(crate) fn snapshot(&mut self, id: u32) -> Option<Bitmap> {
        if !self.live.contains(&id) {
            return None;
        }
        match request(Operation::Snapshot(id)) {
            Some(Reply::Snapshot(bitmap)) => bitmap,
            _ => None,
        }
    }
}
impl Drop for Contexts {
    fn drop(&mut self) {
        self.clear();
    }
}

fn request(operation: Operation) -> Option<Reply> {
    let owner = OWNER
        .get_or_init(|| {
            // Only one pending large upload is admitted. This thread owns display creation,
            // GLSL compiler TLS, drawing and final destruction for every realm in the renderer.
            let (sender, incoming) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("breeze-webgl".into())
                .stack_size(8 * 1024 * 1024)
                .spawn(move || run(incoming))
                .ok()
                .map(|_| sender)
        })
        .as_ref()?;
    let (reply, incoming) = mpsc::sync_channel(1);
    let mut pending = Request { operation, reply };
    let deadline = std::time::Instant::now() + RESPONSE_TIMEOUT;
    loop {
        match owner.try_send(pending) {
            Ok(()) => break,
            Err(mpsc::TrySendError::Disconnected(_)) => return None,
            Err(mpsc::TrySendError::Full(value)) => {
                if std::time::Instant::now() >= deadline {
                    return None;
                }
                pending = value;
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    incoming
        .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        .ok()
}

fn run(incoming: mpsc::Receiver<Request>) {
    let mut backend = BackendContexts::default();
    let mut leases: HashMap<u32, Weak<()>> = HashMap::new();
    loop {
        let pending = incoming.recv_timeout(Duration::from_millis(10));
        let retired = std::mem::take(
            &mut *RETIRED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for id in retired {
            backend.remove(id);
            leases.remove(&id);
        }
        leases.retain(|id, lease| {
            if lease.strong_count() == 0 {
                backend.remove(*id);
                false
            } else {
                true
            }
        });
        let Request { operation, reply } = match pending {
            Ok(request) => request,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let (response, orphan) = match operation {
            Operation::Create(width, height, options, lease) => {
                let id = if lease.strong_count() == 0 {
                    None
                } else {
                    backend.create(width, height, &options)
                };
                if let Some(id) = id {
                    leases.insert(id, lease);
                }
                (Reply::Created(id), id)
            }
            Operation::Command(id, command, bytes) => (
                Reply::Command(backend.execute(id, &command, bytes.as_deref())),
                Some(id),
            ),
            Operation::Snapshot(id) => (Reply::Snapshot(backend.snapshot(id)), Some(id)),
            Operation::TaskBoundary(ids) => {
                let lost = backend.complete_task(&ids);
                for id in &lost {
                    leases.remove(id);
                }
                (Reply::TaskBoundary(lost), None)
            }
            Operation::Pixels(id, command, bytes) => (
                Reply::Pixels(backend.read_pixels(id, &command, bytes.as_deref())),
                Some(id),
            ),
            #[cfg(test)]
            Operation::NativeTest(probe) => {
                let error = std::panic::catch_unwind(probe).err().map(|payload| {
                    payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).into()))
                        .unwrap_or_else(|| "native test panicked".into())
                });
                (Reply::NativeTest(error), None)
            }
        };
        // A timed-out creation/command cannot leave an unowned native context alive.
        if reply.send(response).is_err()
            && let Some(id) = orphan
        {
            backend.remove(id);
            leases.remove(&id);
        }
    }
}
fn retire(ids: impl Iterator<Item = u32>) {
    // Destruction never waits behind a full upload queue. Every accepted name is
    // retired once; the number of live native contexts bounds this cleanup list.
    RETIRED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(ids);
}

#[cfg(test)]
pub(super) fn run_native_test(probe: fn()) {
    // Direct capability probes must obey the production single-owner contract.
    // Parallel Rust test threads must not drive the shared ANGLE display while
    // other contexts are executing on the owner. Preserve failures on the caller
    // rather than unwinding and permanently killing the shared owner thread.
    match request(Operation::NativeTest(probe)) {
        Some(Reply::NativeTest(None)) => {}
        Some(Reply::NativeTest(Some(error))) => panic!("{error}"),
        _ => panic!("native test owner did not reply"),
    }
}
