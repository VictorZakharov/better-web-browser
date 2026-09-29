//! Document-owned, bounded Web Audio decoding. Encoded bytes never enter the browser process.
//! The parsing and sample-rate conversion run on a worker thread, not V8's control thread.

#[cfg(test)]
mod tests;
mod wav;

use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;

pub(super) const MAX_ENCODED_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_DECODED_BYTES: usize = 16 * 1024 * 1024;
const MAX_PENDING_DECODES: usize = 2;
const FRAMES_PER_CHUNK: usize = 16_384;

pub(super) struct DecodedAudio {
    pub(super) channels: Vec<Vec<f32>>,
    pub(super) sample_rate: f64,
    pub(super) frames: usize,
}

enum JobState {
    Pending(mpsc::Receiver<Result<DecodedAudio, String>>),
    Ready {
        audio: DecodedAudio,
        channel: usize,
        offset: usize,
    },
}

struct Job {
    cancelled: Arc<AtomicBool>,
    state: JobState,
}

#[derive(Default)]
pub(super) struct AudioDecodes {
    next_id: u32,
    jobs: HashMap<u32, Job>,
}

pub(super) enum Poll {
    Pending,
    Error(String),
    Data {
        channels: usize,
        frames: usize,
        sample_rate: f64,
        channel: usize,
        offset: usize,
        bytes: Vec<u8>,
        done: bool,
    },
}

impl AudioDecodes {
    pub(super) fn cancel(&mut self, id: u32) {
        if let Some(job) = self.jobs.remove(&id) {
            job.cancelled.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn start(&mut self, bytes: Vec<u8>, sample_rate: f64) -> Result<u32, String> {
        if bytes.len() > MAX_ENCODED_BYTES {
            return Err("encoded audio exceeds the 8 MiB decode limit".into());
        }
        if !sample_rate.is_finite() || !(8_000.0..=192_000.0).contains(&sample_rate) {
            return Err("the context sample rate is unsupported".into());
        }
        if self.jobs.len() >= MAX_PENDING_DECODES {
            return Err("too many simultaneous audio decodes".into());
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("audio decode ID limit reached")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        thread::Builder::new()
            .name("breeze-audio-decode".into())
            .spawn(move || {
                let result = wav::decode(&bytes, sample_rate, &worker_cancelled);
                let _ = sender.send(result);
            })
            .map_err(|_| "audio decoder thread is unavailable")?;
        self.next_id = id;
        self.jobs.insert(
            id,
            Job {
                cancelled,
                state: JobState::Pending(receiver),
            },
        );
        Ok(id)
    }

    pub(super) fn poll(&mut self, id: u32) -> Poll {
        let Some(job) = self.jobs.get_mut(&id) else {
            return Poll::Error("audio decode request is no longer active".into());
        };
        if let JobState::Pending(receiver) = &job.state {
            match receiver.try_recv() {
                Ok(Ok(audio)) => {
                    job.state = JobState::Ready {
                        audio,
                        channel: 0,
                        offset: 0,
                    }
                }
                Ok(Err(message)) => {
                    self.jobs.remove(&id);
                    return Poll::Error(message);
                }
                Err(mpsc::TryRecvError::Empty) => return Poll::Pending,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.jobs.remove(&id);
                    return Poll::Error("audio decoder stopped unexpectedly".into());
                }
            }
        }
        let JobState::Ready {
            audio,
            channel,
            offset,
        } = &mut job.state
        else {
            unreachable!();
        };
        let start = *offset;
        let end = (start + FRAMES_PER_CHUNK).min(audio.frames);
        let mut bytes = Vec::with_capacity((end - start) * 4);
        for sample in &audio.channels[*channel][start..end] {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let response = Poll::Data {
            channels: audio.channels.len(),
            frames: audio.frames,
            sample_rate: audio.sample_rate,
            channel: *channel,
            offset: start,
            bytes,
            done: end == audio.frames && *channel + 1 == audio.channels.len(),
        };
        if end == audio.frames {
            *channel += 1;
            *offset = 0;
        } else {
            *offset = end;
        }
        if matches!(&response, Poll::Data { done: true, .. }) {
            self.jobs.remove(&id);
        }
        response
    }
}

impl Drop for AudioDecodes {
    fn drop(&mut self) {
        for job in self.jobs.values() {
            job.cancelled.store(true, Ordering::Relaxed);
        }
    }
}
