//! Realm-owned streaming codecs. A retained permit covers the entire worker
//! lifetime, including cancellation; close/reopen cannot evade concurrency caps.
mod compressed;
mod config;
mod decoder;
mod encoder;
mod flac_encoder;
mod host;
mod packet_encoder;
mod pcm;
#[cfg(test)]
pub(in crate::engine::script) mod test_packets;
#[cfg(test)]
mod tests;

use config::Config;
pub(super) use host::dispatch;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

const MAX_SESSIONS: usize = 8;
const MAX_PROCESS_WORKERS: usize = 64;
static PROCESS_WORKERS: AtomicUsize = AtomicUsize::new(0);
const MAX_PACKET_BYTES: usize = 61_440;
const MAX_INPUT_BYTES: usize = 512 * 1024;

#[derive(Debug)]
struct Output {
    bytes: Vec<u8>,
    format: &'static str,
    timestamp: i64,
    duration: u64,
    frames: u32,
    sample_rate: u32,
    channels: u32,
    description: Option<Vec<u8>>,
}

enum Command {
    Input { bytes: Vec<u8>, timestamp: i64 },
    Flush,
}

type ResultMessage = Result<Vec<Output>, String>;

struct Session {
    sender: mpsc::SyncSender<Command>,
    receiver: mpsc::Receiver<ResultMessage>,
    cancelled: Arc<AtomicBool>,
    busy: bool,
}

#[derive(Default)]
pub(super) struct AudioCodecs {
    next_id: u32,
    sessions: HashMap<u32, Session>,
    workers: Arc<AtomicUsize>,
}

struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
        PROCESS_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

impl AudioCodecs {
    fn start(&mut self, config: Config, encode: bool) -> Result<u32, String> {
        config.validate(encode)?;
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("audio codec session IDs exhausted")?;
        self.workers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_SESSIONS).then_some(count + 1)
            })
            .map_err(|_| "eight audio codec workers are active; close an unused codec")?;
        if PROCESS_WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_PROCESS_WORKERS).then_some(count + 1)
            })
            .is_err()
        {
            self.workers.fetch_sub(1, Ordering::AcqRel);
            return Err(
                "process audio codec concurrency limit reached; close unused codecs".into(),
            );
        }
        let permit = Permit(Arc::clone(&self.workers));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, commands) = mpsc::sync_channel(1);
        let (results, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("breeze-audio-codec".into())
            .spawn(move || {
                let _permit = permit;
                worker(config, encode, commands, results, &worker_cancelled);
            })
            .map_err(|_| "audio codec worker thread is unavailable")?;
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
            && bytes.len() > MAX_INPUT_BYTES
        {
            return Err("audio command exceeds its 512 KiB byte budget".into());
        }
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or("audio codec session is closed")?;
        if session.busy {
            return Err("audio codec command is already pending".into());
        }
        session
            .sender
            .try_send(command)
            .map_err(|_| "audio codec worker stopped or is saturated")?;
        session.busy = true;
        Ok(())
    }

    fn poll(&mut self, id: u32) -> Result<Option<Vec<Output>>, String> {
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or("audio codec session is closed")?;
        if !session.busy {
            return Err("audio codec has no pending command".into());
        }
        match session.receiver.try_recv() {
            Ok(result) => {
                session.busy = false;
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("audio codec worker stopped".into()),
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

impl Drop for AudioCodecs {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

fn worker(
    config: Config,
    encode: bool,
    commands: mpsc::Receiver<Command>,
    results: mpsc::SyncSender<ResultMessage>,
    cancelled: &AtomicBool,
) {
    enum Codec {
        Encode(encoder::Encoder),
        Decode(decoder::Decoder),
        Pcm(pcm::Decoder),
        Compressed(compressed::Decoder),
        Flac(flac_encoder::Encoder),
    }
    let codec = if encode && config.codec == "flac" {
        flac_encoder::Encoder::new(&config).map(Codec::Flac)
    } else if encode {
        encoder::Encoder::new(&config).map(Codec::Encode)
    } else if pcm::supported(&config.codec) {
        pcm::Decoder::new(&config).map(Codec::Pcm)
    } else if compressed::supported(&config.codec) {
        compressed::Decoder::new(&config).map(Codec::Compressed)
    } else {
        decoder::Decoder::new(&config).map(Codec::Decode)
    };
    let mut codec = match codec {
        Ok(codec) => codec,
        Err(error) => {
            let _ = results.send(Err(error));
            return;
        }
    };
    if cancelled.load(Ordering::Acquire) || results.send(Ok(Vec::new())).is_err() {
        return;
    }
    while let Ok(command) = commands.recv() {
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let result = match (&mut codec, command) {
            (Codec::Flac(codec), Command::Input { bytes, timestamp }) => {
                codec.encode(&bytes, timestamp, cancelled)
            }
            (Codec::Flac(codec), Command::Flush) => codec.flush(cancelled),
            (Codec::Encode(codec), Command::Input { bytes, timestamp }) => {
                codec.encode(&bytes, timestamp, cancelled)
            }
            (Codec::Encode(codec), Command::Flush) => codec.flush(cancelled),
            (Codec::Decode(codec), Command::Input { bytes, timestamp }) => {
                codec.decode(&bytes, timestamp)
            }
            (Codec::Decode(codec), Command::Flush) => Ok(codec.flush()),
            (Codec::Pcm(codec), Command::Input { bytes, timestamp }) => {
                codec.decode(&bytes, timestamp)
            }
            (Codec::Pcm(_), Command::Flush) => Ok(Vec::new()),
            (Codec::Compressed(codec), Command::Input { bytes, timestamp }) => {
                codec.decode(&bytes, timestamp)
            }
            (Codec::Compressed(_), Command::Flush) => Ok(Vec::new()),
        };
        let failed = result.is_err();
        if cancelled.load(Ordering::Acquire) || results.send(result).is_err() || failed {
            return;
        }
    }
}
