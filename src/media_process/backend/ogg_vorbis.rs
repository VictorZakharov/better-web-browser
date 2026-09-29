//! Bounded Ogg/Vorbis decoding in the restricted media worker. The MIME layer
//! advertises this format only because both the probe and pull decoder consume
//! actual PCM from the same allowlisted codec.

use super::DecodedMedia;
use crate::limits::{
    MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES, MAX_MEDIA_DECODED_SAMPLES,
    MAX_MEDIA_DECODED_SOURCE_BYTES, MAX_MEDIA_DURATION_100NS, MAX_MEDIA_ENCODED_QUEUE_BYTES,
    MEDIA_COMMAND_TIMEOUT,
};
use crate::media_protocol::{
    MediaBufferedExtent, MediaCodecFamily, MediaDecodeReport, MediaLimits,
};
use crate::ogg_vorbis_headers;
use lewton::inside_ogg::OggStreamReader;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;

pub(super) fn is_ogg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"OggS")
}

pub(super) fn decode(
    bytes: &[u8],
    limits: MediaLimits,
    started: Instant,
) -> Result<DecodedMedia, String> {
    if bytes.is_empty()
        || bytes.len() > MAX_MEDIA_ENCODED_QUEUE_BYTES
        || bytes.len() as u64 > limits.max_encoded_bytes
    {
        return Err("encoded Ogg source exceeds worker limits".into());
    }
    let (sample_rate, channels) = ogg_vorbis_headers::preflight(bytes, 2)?;
    let mut reader = OggStreamReader::new(Cursor::new(bytes))
        .map_err(|error| format!("open Ogg/Vorbis decoder: {error}"))?;
    if reader.ident_hdr.audio_sample_rate != sample_rate
        || u16::from(reader.ident_hdr.audio_channels) != channels
    {
        return Err("Ogg/Vorbis headers changed during decoder initialization".into());
    }
    let serial = reader.stream_serial();
    let mut packets = 0_u32;
    let mut nonempty_packets = 0_u32;
    let mut decoded_bytes = 0_u64;
    let mut frames = 0_u64;
    let mut last_timestamp = 0_i64;
    loop {
        if started.elapsed() > MEDIA_COMMAND_TIMEOUT {
            return Err("Ogg/Vorbis decode exceeded worker time limit".into());
        }
        let Some(samples) = reader
            .read_dec_packet_itl()
            .map_err(|error| format!("decode Ogg/Vorbis packet: {error}"))?
        else {
            break;
        };
        if reader.stream_serial() != serial {
            // Lewton can advance to a chained logical stream. This worker has
            // one immutable format and one seek timeline per source.
            return Err("chained Ogg/Vorbis streams are not supported".into());
        }
        packets = packets
            .checked_add(1)
            .ok_or_else(|| "Ogg/Vorbis packet count overflow".to_string())?;
        if packets as usize > MAX_MEDIA_DECODED_SAMPLES {
            return Err("Ogg/Vorbis packet count exceeds worker limit".into());
        }
        let packet_bytes = checked_pcm_bytes(&samples, channels)?;
        decoded_bytes = decoded_bytes
            .checked_add(packet_bytes as u64)
            .ok_or_else(|| "Ogg/Vorbis decoded length overflow".to_string())?;
        if decoded_bytes > MAX_MEDIA_DECODED_SOURCE_BYTES {
            return Err("Ogg/Vorbis decoded bytes exceed worker limit".into());
        }
        if !samples.is_empty() {
            nonempty_packets += 1;
            last_timestamp = i64::try_from(frames_to_100ns(frames, sample_rate)?)
                .map_err(|_| "Ogg/Vorbis timestamp overflow")?;
            frames = frames
                .checked_add(samples.len() as u64 / u64::from(channels))
                .ok_or_else(|| "Ogg/Vorbis frame count overflow".to_string())?;
        }
    }
    let duration = frames_to_100ns(frames, sample_rate)?;
    if frames == 0 || duration == 0 || decoded_bytes == 0 {
        return Err("Ogg/Vorbis source produced no PCM".into());
    }
    let report = MediaDecodeReport {
        buffered: MediaBufferedExtent {
            audio_start_100ns: 0,
            audio_end_100ns: duration,
            ..MediaBufferedExtent::default()
        },
        encoded_bytes: bytes.len() as u64,
        video_codec: MediaCodecFamily::None,
        audio_codec: MediaCodecFamily::Vorbis,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 0,
        video_height: 0,
        audio_sample_rate: sample_rate,
        audio_channels: channels,
        video_samples: 0,
        audio_samples: nonempty_packets,
        video_decoded_bytes: 0,
        audio_decoded_bytes: decoded_bytes,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 0,
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: last_timestamp,
        duration_100ns: duration,
        decode_micros: started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
    };
    report
        .validate(limits)
        .map_err(|error| format!("validate Ogg/Vorbis source: {error}"))?;
    Ok(DecodedMedia {
        report,
        playback: None,
    })
}

pub(in crate::media_process) struct VorbisDecoder {
    source: Arc<[u8]>,
    reader: OggStreamReader<Cursor<Arc<[u8]>>>,
    serial: u32,
    sample_rate: u32,
    channels: u16,
    packets: u32,
    pending: Option<Vec<u8>>,
}

impl VorbisDecoder {
    pub(in crate::media_process) fn open(
        bytes: &[u8],
        expected_packets: u32,
        expected_sample_rate: u32,
        expected_channels: u16,
    ) -> Result<Self, String> {
        if bytes.len() > MAX_MEDIA_ENCODED_QUEUE_BYTES
            || expected_packets == 0
            || expected_packets as usize > MAX_MEDIA_DECODED_SAMPLES
        {
            return Err("Ogg/Vorbis playback exceeds worker limits".into());
        }
        let format = ogg_vorbis_headers::preflight(bytes, 2)?;
        if format != (expected_sample_rate, expected_channels) {
            return Err("Ogg/Vorbis format disagreed with decode report".into());
        }
        let source: Arc<[u8]> = Arc::from(bytes);
        let reader = OggStreamReader::new(Cursor::new(source.clone()))
            .map_err(|error| format!("open playback Ogg/Vorbis source: {error}"))?;
        let serial = reader.stream_serial();
        Ok(Self {
            source,
            reader,
            serial,
            sample_rate: expected_sample_rate,
            channels: expected_channels,
            packets: 0,
            pending: None,
        })
    }

    pub(in crate::media_process) fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        // Restarting and discarding is sample-accurate. Lewton's built-in seek
        // is page-granular, which cannot set the browser's PCM clock exactly.
        let target_frame =
            u64::try_from(u128::from(position_100ns) * u128::from(self.sample_rate) / 10_000_000)
                .map_err(|_| "Ogg/Vorbis seek target overflow")?;
        let started = Instant::now();
        self.reader = OggStreamReader::new(Cursor::new(self.source.clone()))
            .map_err(|error| format!("restart Ogg/Vorbis decoder: {error}"))?;
        self.packets = 0;
        self.pending = None;
        let mut frame = 0_u64;
        while frame < target_frame {
            if started.elapsed() > MEDIA_COMMAND_TIMEOUT {
                return Err("Ogg/Vorbis seek exceeded worker time limit".into());
            }
            let Some(bytes) = self.read_packet()? else {
                break;
            };
            let packet_frames = bytes.len() as u64 / (u64::from(self.channels) * 2);
            let end = frame.saturating_add(packet_frames);
            if end > target_frame {
                let skip = usize::try_from(target_frame - frame)
                    .ok()
                    .and_then(|frames| frames.checked_mul(usize::from(self.channels) * 2))
                    .ok_or_else(|| "Ogg/Vorbis seek offset overflow".to_string())?;
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
        loop {
            let Some(samples) = self
                .reader
                .read_dec_packet_itl()
                .map_err(|error| format!("decode playback Ogg/Vorbis packet: {error}"))?
            else {
                return Ok(None);
            };
            if self.reader.stream_serial() != self.serial {
                return Err("chained Ogg/Vorbis streams are not supported".into());
            }
            self.packets += 1;
            if self.packets as usize > MAX_MEDIA_DECODED_SAMPLES {
                return Err("Ogg/Vorbis playback packet count exceeds worker limit".into());
            }
            let packet_bytes = checked_pcm_bytes(&samples, self.channels)?;
            if packet_bytes == 0 {
                continue;
            }
            let mut bytes = Vec::with_capacity(packet_bytes);
            for sample in samples {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            return Ok(Some(bytes));
        }
    }
}

fn checked_pcm_bytes(samples: &[i16], channels: u16) -> Result<usize, String> {
    if !samples.len().is_multiple_of(usize::from(channels)) {
        return Err("Ogg/Vorbis PCM channel alignment is invalid".into());
    }
    let bytes = samples
        .len()
        .checked_mul(2)
        .ok_or_else(|| "Ogg/Vorbis PCM packet length overflow".to_string())?;
    if bytes > MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES {
        return Err("Ogg/Vorbis PCM packet exceeds worker limit".into());
    }
    Ok(bytes)
}

fn frames_to_100ns(frames: u64, sample_rate: u32) -> Result<u64, String> {
    let duration = frames
        .checked_mul(10_000_000)
        .and_then(|value| value.checked_div(u64::from(sample_rate)))
        .ok_or_else(|| "Ogg/Vorbis duration overflow".to_string())?;
    if duration > MAX_MEDIA_DURATION_100NS {
        return Err("Ogg/Vorbis duration exceeds worker limit".into());
    }
    Ok(duration)
}

#[cfg(test)]
mod tests;
