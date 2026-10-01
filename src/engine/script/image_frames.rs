//! Realm-owned image decode sessions. Codec work never runs on V8's control thread.
//!
//! A worker slot stays occupied until the thread exits, even when its session is
//! closed. Repeated start/close must not circumvent the native concurrency limit.
mod codecs;
mod host;
mod pixels;
#[cfg(test)]
pub(in crate::engine::script) mod test_fixtures;
#[cfg(test)]
mod tests;

pub(super) use host::dispatch;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

const MAX_SESSIONS: usize = 2;
const MAX_FRAMES: usize = 256;
const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;
const CHUNK_BYTES: usize = 64 * 1024;

pub(super) struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    timestamp: u64,
    duration: Option<u64>,
    rotation: u16,
    flip: bool,
}

pub(super) struct Frames {
    frames: Vec<Frame>,
    poster: Option<Frame>,
    repetitions: Option<u32>,
    animated: bool,
}

enum State {
    Pending(mpsc::Receiver<Result<Frames, String>>),
    Ready(Frames),
    Failed(String),
}

struct Session {
    cancelled: Arc<AtomicBool>,
    state: State,
}

#[derive(Default)]
pub(super) struct ImageFrames {
    next_id: u32,
    sessions: HashMap<u32, Session>,
    workers: Arc<AtomicUsize>,
}

struct WorkerPermit(Arc<AtomicUsize>);

impl Drop for WorkerPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl ImageFrames {
    fn start(&mut self, mime: &str, bytes: &[u8], ignore_profile: bool) -> Result<u32, String> {
        crate::engine::image_decode::DecodeLimits::CANVAS.check_source(bytes)?;
        if !codecs::supported(mime) {
            return Err("unsupported image media type".into());
        }
        if self.sessions.len() >= MAX_SESSIONS {
            return Err("the two-session image decoder limit was reached; close a decoder".into());
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("image decoder ID limit reached")?;
        self.workers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_SESSIONS).then_some(count + 1)
            })
            .map_err(|_| "image decoder workers are still busy; retry after they finish")?;
        let permit = WorkerPermit(Arc::clone(&self.workers));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let mime = mime.to_owned();
        let bytes = bytes.to_vec();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("breeze-image-frames".into())
            .spawn(move || {
                let _permit = permit;
                let result = codecs::decode(&mime, &bytes, ignore_profile, &worker_cancelled);
                if !worker_cancelled.load(Ordering::Acquire) {
                    let _ = sender.send(result);
                }
            })
            .map_err(|_| "image decoder thread is unavailable")?;
        self.next_id = id;
        self.sessions.insert(
            id,
            Session {
                cancelled,
                state: State::Pending(receiver),
            },
        );
        Ok(id)
    }

    fn poll(&mut self, id: u32) -> Result<Option<&Frames>, String> {
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or("image decoder is closed")?;
        if let State::Pending(receiver) = &session.state {
            session.state = match receiver.try_recv() {
                Ok(Ok(frames)) => State::Ready(frames),
                Ok(Err(error)) => State::Failed(error),
                Err(mpsc::TryRecvError::Empty) => return Ok(None),
                Err(mpsc::TryRecvError::Disconnected) => {
                    State::Failed("image decoder worker stopped".into())
                }
            };
        }
        match &session.state {
            State::Ready(frames) => Ok(Some(frames)),
            State::Failed(error) => Err(error.clone()),
            State::Pending(_) => unreachable!(),
        }
    }

    fn close(&mut self, id: u32) {
        if let Some(session) = self.sessions.remove(&id) {
            session.cancelled.store(true, Ordering::Release);
        }
    }

    pub(super) fn cancel_all(&mut self) {
        for (_, session) in self.sessions.drain() {
            session.cancelled.store(true, Ordering::Release);
        }
    }
}

impl Drop for ImageFrames {
    fn drop(&mut self) {
        self.cancel_all();
    }
}
