//! One native owner thread. Realm callers never move ANGLE contexts across threads.
use super::{
    BackendContexts, MAX_COMMAND_BYTES, MAX_CONTEXTS, MAX_UPLOAD_BYTES, PixelReply, gl, json,
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
    Batch(Vec<(u32, String)>),
    Pixels(u32, String, Option<Vec<u8>>),
    Snapshot(u32),
    TaskBoundary(Vec<u32>),
    ResourceDiagnostics(Vec<u32>),
    #[cfg(test)]
    NativeTest(fn()),
}
enum Reply {
    Created(Option<u32>),
    Command(Value),
    Batch(Vec<u32>),
    Pixels(PixelReply),
    Snapshot(Option<Bitmap>),
    TaskBoundary(Vec<u32>),
    ResourceDiagnostics(Vec<String>),
    #[cfg(test)]
    NativeTest(Option<String>),
}
struct Request {
    operation: Operation,
    reply: mpsc::SyncSender<Reply>,
}

#[cfg(test)]
mod ownership_tests;
mod realm;
pub(crate) use realm::Contexts;

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
            Err(mpsc::RecvTimeoutError::Timeout) => {
                backend.progress_compilers();
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let batch_ids: HashSet<_> = match &operation {
            Operation::Batch(commands) => commands.iter().map(|(id, _)| *id).collect(),
            _ => HashSet::new(),
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
                Reply::Command(backend.execute_owned(id, &command, bytes)),
                Some(id),
            ),
            Operation::Batch(commands) => {
                let mut lost = HashSet::new();
                for (id, command) in commands {
                    if !lost.contains(&id)
                        && backend.execute(id, &command, None).get("lost") == Some(&json!(true))
                    {
                        lost.insert(id);
                    }
                }
                (Reply::Batch(lost.into_iter().collect()), None)
            }
            Operation::Snapshot(id) => (Reply::Snapshot(backend.snapshot(id)), Some(id)),
            Operation::TaskBoundary(ids) => {
                let lost = backend.complete_task(&ids);
                for id in &lost {
                    leases.remove(id);
                }
                (Reply::TaskBoundary(lost), None)
            }
            Operation::ResourceDiagnostics(ids) => (
                Reply::ResourceDiagnostics(backend.resource_diagnostics(&ids)),
                None,
            ),
            Operation::Pixels(id, command, bytes) => (
                Reply::Pixels(backend.read_pixels_owned(id, &command, bytes)),
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
        if reply.send(response).is_err() {
            for id in orphan.into_iter().chain(batch_ids) {
                backend.remove(id);
                leases.remove(&id);
            }
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
