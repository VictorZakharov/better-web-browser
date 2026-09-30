//! Bounded compressed-audio decoding in the restricted media worker.
//! Video and fragmented MP4 stay on their existing Media Foundation paths.

use super::DecodedMedia;
use crate::encoded_audio;
pub(super) use crate::encoded_audio::Kind;
use crate::limits::{
    MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_DURATION_100NS, MAX_MEDIA_ENCODED_QUEUE_BYTES,
    MEDIA_COMMAND_TIMEOUT,
};
use crate::media_protocol::{
    MediaBufferedExtent, MediaCodecFamily, MediaDecodeReport, MediaLimits,
};
use std::sync::Arc;
use std::time::Instant;

mod source;
use source::Stream;

#[cfg(test)]
mod container_tests;
#[cfg(test)]
mod tests;

pub(super) fn codec(kind: Kind) -> MediaCodecFamily {
    match kind {
        Kind::Mp3 => MediaCodecFamily::Mp3,
        Kind::AacM4a => MediaCodecFamily::Aac,
        Kind::AacAdts => MediaCodecFamily::AacLc,
        Kind::VorbisWebm => MediaCodecFamily::Vorbis,
        Kind::FlacOgg => MediaCodecFamily::Flac,
    }
}

/// Select only streams Symphonia can prove are the expected audio-only format.
/// In particular, this must not intercept H.264/AAC or fragmented adaptive MP4.
pub(super) fn classify(bytes: &[u8]) -> Option<Kind> {
    if bytes.is_empty() {
        return None;
    }
    let kind = encoded_audio::sniff(bytes).or_else(|| {
        // ISO BMFF permits leading free/skip boxes. Keep the existing ordinary
        // M4A admission policy even when the lightweight signature is absent.
        crate::iso_bmff_audio::ordinary_audio_edit(bytes)
            .is_ok()
            .then_some(Kind::AacM4a)
    })?;
    if matches!(kind, Kind::Mp3 | Kind::AacM4a) {
        if bytes.len() > MAX_MEDIA_ENCODED_QUEUE_BYTES {
            return None;
        }
        Stream::open(Arc::from(bytes), kind).ok().map(|_| kind)
    } else {
        // Recognition and validation are separate: a damaged supported
        // container must fail its decoder, never fall back to a host codec.
        Some(kind)
    }
}

pub(super) fn decode(
    bytes: &[u8],
    kind: Kind,
    limits: MediaLimits,
    started: Instant,
) -> Result<DecodedMedia, String> {
    if bytes.is_empty()
        || bytes.len() > MAX_MEDIA_ENCODED_QUEUE_BYTES
        || bytes.len() as u64 > limits.max_encoded_bytes
    {
        return Err("encoded compressed audio exceeds worker limits".into());
    }
    let mut stream = Stream::open(Arc::from(bytes), kind)?;
    while stream.next_pcm()?.is_some() {
        if started.elapsed() > MEDIA_COMMAND_TIMEOUT {
            return Err("compressed audio decode exceeded worker time limit".into());
        }
    }
    let rate = stream
        .sample_rate
        .ok_or("compressed audio source produced no PCM")?;
    let channels = stream
        .channels
        .ok_or("compressed audio source produced no PCM")?;
    let duration = frames_to_100ns(stream.frames, rate)?;
    if stream.samples == 0 || stream.decoded_bytes == 0 || duration == 0 {
        return Err("compressed audio source produced no PCM".into());
    }
    let report = MediaDecodeReport {
        buffered: MediaBufferedExtent {
            audio_start_100ns: 0,
            audio_end_100ns: duration,
            ..MediaBufferedExtent::default()
        },
        encoded_bytes: bytes.len() as u64,
        video_codec: MediaCodecFamily::None,
        audio_codec: codec(kind),
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 0,
        video_height: 0,
        audio_sample_rate: rate,
        audio_channels: channels,
        video_samples: 0,
        audio_samples: stream.samples,
        video_decoded_bytes: 0,
        audio_decoded_bytes: stream.decoded_bytes,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 0,
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: stream.last_timestamp_100ns,
        duration_100ns: duration,
        decode_micros: started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
    };
    report
        .validate(limits)
        .map_err(|error| format!("validate compressed audio source: {error}"))?;
    Ok(DecodedMedia {
        report,
        playback: None,
    })
}

pub(in crate::media_process) struct CompressedDecoder {
    source: Arc<[u8]>,
    stream: Stream,
    kind: Kind,
    expected_samples: u32,
    sample_rate: u32,
    channels: u16,
    pending: Option<Vec<u8>>,
}

impl CompressedDecoder {
    pub(super) fn open(
        bytes: &[u8],
        kind: Kind,
        expected_samples: u32,
        expected_sample_rate: u32,
        expected_channels: u16,
    ) -> Result<Self, String> {
        if bytes.len() > MAX_MEDIA_ENCODED_QUEUE_BYTES
            || expected_samples == 0
            || expected_samples as usize > MAX_MEDIA_DECODED_SAMPLES
        {
            return Err("compressed audio playback exceeds worker limits".into());
        }
        let source: Arc<[u8]> = Arc::from(bytes);
        let mut stream = Stream::open(source.clone(), kind)?;
        let pending = stream
            .next_pcm()?
            .ok_or("compressed audio playback produced no PCM")?;
        if stream.sample_rate != Some(expected_sample_rate)
            || stream.channels != Some(expected_channels)
        {
            return Err("compressed audio format disagreed with decode report".into());
        }
        Ok(Self {
            source,
            stream,
            kind,
            expected_samples,
            sample_rate: expected_sample_rate,
            channels: expected_channels,
            pending: Some(pending),
        })
    }

    pub(in crate::media_process) fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        // Symphonia seeks to a packet boundary. Restarting and discarding PCM
        // gives the browser clock an exact sample boundary across both codecs.
        // A zero-position seek reopens the stream without decoding a packet.
        // Keep the verified format independently of that fresh stream's state.
        let rate = self.sample_rate;
        let channels = self.channels;
        let target_frame =
            u64::try_from(u128::from(position_100ns) * u128::from(rate) / 10_000_000)
                .map_err(|_| "compressed audio seek target overflow")?;
        self.stream = Stream::open(self.source.clone(), self.kind)?;
        self.pending = None;
        let started = Instant::now();
        let mut frame = 0_u64;
        while frame < target_frame {
            if started.elapsed() > MEDIA_COMMAND_TIMEOUT {
                return Err("compressed audio seek exceeded worker time limit".into());
            }
            let Some(bytes) = self.read_packet()? else {
                break;
            };
            let packet_frames = bytes.len() as u64 / (u64::from(channels) * 2);
            let end = frame.saturating_add(packet_frames);
            if end > target_frame {
                let skip = usize::try_from(target_frame - frame)
                    .ok()
                    .and_then(|frames| frames.checked_mul(usize::from(channels) * 2))
                    .ok_or_else(|| "compressed audio seek offset overflow".to_string())?;
                self.pending = Some(bytes[skip..].to_vec());
                break;
            }
            frame = end;
        }
        Ok(())
    }

    pub(in crate::media_process) fn next_sample(&mut self) -> Result<Option<Vec<u8>>, String> {
        if let Some(bytes) = self.pending.take() {
            return Ok(Some(bytes));
        }
        self.read_packet()
    }

    fn read_packet(&mut self) -> Result<Option<Vec<u8>>, String> {
        let pcm = self.stream.next_pcm()?;
        if self.stream.samples > self.expected_samples {
            return Err("compressed audio playback exceeded decoded report".into());
        }
        if pcm.is_none() && self.stream.samples != self.expected_samples {
            return Err("compressed audio playback ended before decoded report".into());
        }
        Ok(pcm)
    }
}

fn mp3_has_declared_frames(bytes: &[u8]) -> bool {
    // Untagged MP3 track lengths are often bitrate estimates. Only Xing,
    // Info, or VBRI frame-count metadata is evidence for truncation checks.
    let start = if bytes.starts_with(b"ID3") && bytes.len() >= 10 {
        let size = bytes[6..10].iter().fold(0usize, |value, byte| {
            (value << 7) | usize::from(byte & 0x7f)
        });
        10 + size + if bytes[5] & 0x10 != 0 { 10 } else { 0 }
    } else {
        0
    };
    let end = start.saturating_add(2_048).min(bytes.len());
    bytes.get(start..end).is_some_and(|prefix| {
        prefix
            .windows(4)
            .any(|marker| marker == b"Xing" || marker == b"Info" || marker == b"VBRI")
    })
}

fn frames_to_100ns(frames: u64, sample_rate: u32) -> Result<u64, String> {
    let duration = frames
        .checked_mul(10_000_000)
        .and_then(|value| value.checked_div(u64::from(sample_rate)))
        .ok_or_else(|| "compressed audio duration overflow".to_string())?;
    if duration > MAX_MEDIA_DURATION_100NS {
        return Err("compressed audio duration exceeds worker limit".into());
    }
    Ok(duration)
}
