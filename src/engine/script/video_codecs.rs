//! Realm-owned AV1 sessions with nonblocking commands and retained lifetime permits.
mod config;
mod host;
#[cfg(test)]
pub(in crate::engine::script) mod test_packets;
#[cfg(test)]
mod tests;
mod worker;
use config::Config;
pub(super) use host::dispatch;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

const MAX_INPUT: usize = 4 * 1024 * 1024;
const MAX_REALM: usize = 4;
const MAX_PROCESS: usize = 16;
static PROCESS: AtomicUsize = AtomicUsize::new(0);

struct Output {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    timestamp: i64,
    duration: Option<u64>,
}
enum Command {
    Input {
        bytes: Vec<u8>,
        timestamp: i64,
        duration: Option<u64>,
        key: bool,
    },
    Flush,
}
struct Session {
    sender: mpsc::SyncSender<Command>,
    receiver: mpsc::Receiver<Result<Vec<Output>, String>>,
    cancelled: Arc<AtomicBool>,
    busy: bool,
}
#[derive(Default)]
pub(super) struct VideoCodecs {
    next_id: u32,
    sessions: HashMap<u32, Session>,
    workers: Arc<AtomicUsize>,
}
struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
        PROCESS.fetch_sub(1, Ordering::AcqRel);
    }
}
impl VideoCodecs {
    fn start(&mut self, config: Config) -> Result<u32, String> {
        config.validate()?;
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("video codec IDs exhausted")?;
        self.workers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_REALM).then_some(count + 1)
            })
            .map_err(|_| "four video decoder workers are active")?;
        if PROCESS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_PROCESS).then_some(count + 1)
            })
            .is_err()
        {
            self.workers.fetch_sub(1, Ordering::AcqRel);
            return Err("process video decoder concurrency limit reached".into());
        }
        let permit = Permit(Arc::clone(&self.workers));
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&cancelled);
        let (sender, commands) = mpsc::sync_channel(1);
        let (results, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("breeze-av1-decoder".into())
            .spawn(move || {
                let _permit = permit;
                worker::run(config, commands, results, &cancel);
            })
            .map_err(|_| "video decoder thread is unavailable")?;
        self.next_id = id;
        self.sessions.insert(
            id,
            Session {
                sender,
                receiver,
                cancelled,
                busy: true,
            },
        );
        Ok(id)
    }
    fn submit(&mut self, id: u32, command: Command) -> Result<(), String> {
        if let Command::Input { bytes, .. } = &command
            && bytes.len() > MAX_INPUT
        {
            return Err("video packet exceeds 4 MiB".into());
        }
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or("video decoder session is closed")?;
        if session.busy {
            return Err("video decoder command is pending".into());
        }
        session
            .sender
            .try_send(command)
            .map_err(|_| "video decoder worker stopped or is saturated")?;
        session.busy = true;
        Ok(())
    }
    fn poll(&mut self, id: u32) -> Result<Option<Vec<Output>>, String> {
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or("video decoder session is closed")?;
        if !session.busy {
            return Err("video decoder has no pending command".into());
        }
        match session.receiver.try_recv() {
            Ok(result) => {
                session.busy = false;
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("video decoder worker stopped".into()),
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
impl Drop for VideoCodecs {
    fn drop(&mut self) {
        self.cancel_all();
    }
}
