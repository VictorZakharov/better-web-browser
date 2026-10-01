//! Incremental encoders for one granted microphone track per recorder.
//! Native state retains at most a PCM/codec packet, not an entire recording.

use std::collections::HashMap;

mod flac;
mod opus;
mod opus_sink;
#[cfg(test)]
mod tests;

const MAX_RECORDERS: usize = 8;

#[derive(Default)]
pub(super) struct MediaRecorders {
    next_id: u32,
    sessions: HashMap<u32, Session>,
}

enum Session {
    Flac(flac::Session),
    Opus(opus::Session),
}

pub(super) fn target_bitrate(kind: &str, requested: u32) -> u32 {
    if matches!(kind, "opus" | "webm-opus") {
        requested.clamp(500, 512_000)
    } else {
        requested
    }
}

impl MediaRecorders {
    pub(super) fn open(
        &mut self,
        kind: &str,
        bitrate: u32,
        constant: bool,
    ) -> Result<u32, &'static str> {
        if self.sessions.len() >= MAX_RECORDERS {
            return Err("Too many active audio recorders");
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("Recorder ID limit reached")?;
        let session = match kind {
            "flac" => Session::Flac(flac::Session::new()?),
            "opus" => Session::Opus(opus::Session::new(
                id,
                target_bitrate(kind, bitrate),
                constant,
            )),
            "webm-opus" => Session::Opus(opus::Session::new_webm(
                id,
                target_bitrate(kind, bitrate),
                constant,
            )),
            _ => return Err("Unsupported audio recorder format"),
        };
        self.next_id = id;
        self.sessions.insert(id, session);
        Ok(id)
    }

    pub(super) fn format_supported(&self, id: u32, rate: usize, channels: usize) -> bool {
        match self.sessions.get(&id) {
            Some(Session::Flac(session)) => session.format_supported(rate, channels),
            Some(Session::Opus(session)) => session.format_supported(rate, channels),
            None => false,
        }
    }

    pub(super) fn append(
        &mut self,
        id: u32,
        rate: usize,
        channels: usize,
        pcm: &[u8],
    ) -> Result<Vec<u8>, &'static str> {
        match self.sessions.get_mut(&id).ok_or("Unknown audio recorder")? {
            Session::Flac(session) => session.append(rate, channels, pcm),
            Session::Opus(session) => session.append(rate, channels, pcm),
        }
    }

    pub(super) fn finish(&mut self, id: u32) -> Result<Vec<u8>, &'static str> {
        match self.sessions.remove(&id).ok_or("Unknown audio recorder")? {
            Session::Flac(mut session) => session.finish(),
            Session::Opus(mut session) => session.finish(),
        }
    }

    pub(super) fn cancel(&mut self, id: u32) {
        self.sessions.remove(&id);
    }
}
