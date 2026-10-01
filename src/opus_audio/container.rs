//! One Opus PCM contract, with container-specific admission and trimming.

use super::{Limits, OggStream};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

pub(crate) enum Stream {
    Ogg(Box<OggStream>),
    Webm(Box<crate::webm_opus::Stream>),
}

impl Stream {
    pub(crate) fn open(
        source: Arc<[u8]>,
        limits: Limits,
        cancelled: Option<&AtomicBool>,
        deadline: Instant,
    ) -> Result<Self, String> {
        if source.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
            crate::webm_opus::Stream::open(source, limits, cancelled, deadline)
                .map(|stream| Self::Webm(Box::new(stream)))
        } else {
            OggStream::open(source, limits, cancelled, deadline)
                .map(|stream| Self::Ogg(Box::new(stream)))
        }
    }

    pub(crate) fn channels(&self) -> u16 {
        match self {
            Self::Ogg(stream) => stream.channels(),
            Self::Webm(stream) => stream.channels(),
        }
    }

    pub(crate) fn frames(&self) -> u64 {
        match self {
            Self::Ogg(stream) => stream.frames(),
            Self::Webm(stream) => stream.frames(),
        }
    }

    pub(crate) fn next_pcm(
        &mut self,
        cancelled: Option<&AtomicBool>,
        deadline: Instant,
    ) -> Result<Option<Vec<f32>>, String> {
        match self {
            Self::Ogg(stream) => stream.next_pcm(cancelled, deadline),
            Self::Webm(stream) => stream.next_pcm(cancelled, deadline),
        }
    }
}
