//! Bounded native FLAC decoding in the restricted media worker. Both the
//! complete-source probe and pull playback use Claxon rather than an optional
//! host Media Foundation codec.

use super::DecodedMedia;
use crate::limits::{
    MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES, MAX_MEDIA_DECODED_SAMPLES,
    MAX_MEDIA_DECODED_SOURCE_BYTES, MAX_MEDIA_DURATION_100NS, MAX_MEDIA_ENCODED_QUEUE_BYTES,
    MEDIA_COMMAND_TIMEOUT,
};
use crate::media_protocol::{
    MediaBufferedExtent, MediaCodecFamily, MediaDecodeReport, MediaLimits,
};
use claxon::metadata::StreamInfo;
use claxon::{FlacReader, FlacReaderOptions};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;

// Claxon parses FLAC metadata before yielding a reader, including comments and
// application blocks that it may allocate. Bound their count and total byte
// span before invoking it; no metadata is retained by this worker.
const MAX_FLAC_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_FLAC_METADATA_BLOCKS: usize = 64;

pub(super) fn is_flac(bytes: &[u8]) -> bool {
    bytes.starts_with(b"fLaC")
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
        return Err("encoded FLAC source exceeds worker limits".into());
    }
    scan_metadata(bytes)?;
    let mut reader = open_reader(Cursor::new(bytes))?;
    let info = reader.streaminfo();
    validate_info(info)?;

    let mut buffer = Vec::new();
    let mut blocks = 0_u32;
    let mut frames = 0_u64;
    let mut decoded_bytes = 0_u64;
    let mut last_timestamp = 0_i64;
    loop {
        if started.elapsed() > MEDIA_COMMAND_TIMEOUT {
            return Err("FLAC decode exceeded worker time limit".into());
        }
        let Some(block) = reader
            .blocks()
            .read_next_or_eof(buffer)
            .map_err(|error| format!("decode FLAC frame: {error}"))?
        else {
            break;
        };
        blocks = blocks
            .checked_add(1)
            .ok_or_else(|| "FLAC block count overflow".to_string())?;
        if blocks as usize > MAX_MEDIA_DECODED_SAMPLES {
            return Err("FLAC block count exceeds worker limit".into());
        }
        let pcm = render_block(&block, info)?;
        decoded_bytes = decoded_bytes
            .checked_add(pcm.len() as u64)
            .ok_or_else(|| "FLAC decoded length overflow".to_string())?;
        if decoded_bytes > MAX_MEDIA_DECODED_SOURCE_BYTES {
            return Err("FLAC decoded bytes exceed worker limit".into());
        }
        last_timestamp = i64::try_from(frames_to_100ns(frames, info.sample_rate)?)
            .map_err(|_| "FLAC timestamp overflow")?;
        frames = frames
            .checked_add(u64::from(block.duration()))
            .ok_or_else(|| "FLAC frame count overflow".to_string())?;
        buffer = block.into_buffer();
    }
    if info.samples.is_some_and(|declared| declared != frames) {
        return Err("FLAC decoded frame count disagrees with STREAMINFO".into());
    }
    let duration = frames_to_100ns(frames, info.sample_rate)?;
    if frames == 0 || duration == 0 || decoded_bytes == 0 {
        return Err("FLAC source produced no PCM".into());
    }
    let report = MediaDecodeReport {
        buffered: MediaBufferedExtent {
            audio_start_100ns: 0,
            audio_end_100ns: duration,
            ..MediaBufferedExtent::default()
        },
        encoded_bytes: bytes.len() as u64,
        video_codec: MediaCodecFamily::None,
        audio_codec: MediaCodecFamily::Flac,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 0,
        video_height: 0,
        audio_sample_rate: info.sample_rate,
        audio_channels: info.channels as u16,
        video_samples: 0,
        audio_samples: blocks,
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
        .map_err(|error| format!("validate FLAC source: {error}"))?;
    Ok(DecodedMedia {
        report,
        playback: None,
        foundation: None,
    })
}

pub(in crate::media_process) struct FlacDecoder {
    source: Arc<[u8]>,
    reader: FlacReader<Cursor<Arc<[u8]>>>,
    info: StreamInfo,
    expected_blocks: u32,
    blocks: u32,
    decoded_bytes: u64,
    buffer: Vec<i32>,
    pending: Option<Vec<u8>>,
}

impl FlacDecoder {
    pub(in crate::media_process) fn open(
        bytes: &[u8],
        expected_blocks: u32,
        expected_sample_rate: u32,
        expected_channels: u16,
    ) -> Result<Self, String> {
        if bytes.len() > MAX_MEDIA_ENCODED_QUEUE_BYTES
            || expected_blocks == 0
            || expected_blocks as usize > MAX_MEDIA_DECODED_SAMPLES
        {
            return Err("FLAC playback exceeds worker limits".into());
        }
        scan_metadata(bytes)?;
        let source: Arc<[u8]> = Arc::from(bytes);
        let reader = open_reader(Cursor::new(source.clone()))?;
        let info = reader.streaminfo();
        validate_info(info)?;
        if info.sample_rate != expected_sample_rate || info.channels != u32::from(expected_channels)
        {
            return Err("FLAC format disagreed with decode report".into());
        }
        Ok(Self {
            source,
            reader,
            info,
            expected_blocks,
            blocks: 0,
            decoded_bytes: 0,
            buffer: Vec::new(),
            pending: None,
        })
    }

    pub(in crate::media_process) fn seek(&mut self, position_100ns: u64) -> Result<(), String> {
        // Claxon exposes sequential frames, not sample-addressed seeking. A
        // bounded restart and discard gives the browser's PCM clock an exact
        // sample boundary, including a seek into the middle of a FLAC frame.
        let target_frame = u64::try_from(
            u128::from(position_100ns) * u128::from(self.info.sample_rate) / 10_000_000,
        )
        .map_err(|_| "FLAC seek target overflow")?;
        let started = Instant::now();
        self.reader = open_reader(Cursor::new(self.source.clone()))?;
        self.blocks = 0;
        self.decoded_bytes = 0;
        self.pending = None;
        let mut frame = 0_u64;
        while frame < target_frame {
            if started.elapsed() > MEDIA_COMMAND_TIMEOUT {
                return Err("FLAC seek exceeded worker time limit".into());
            }
            let Some(bytes) = self.read_block()? else {
                break;
            };
            let block_frames = bytes.len() as u64 / (u64::from(self.info.channels) * 2);
            let end = frame.saturating_add(block_frames);
            if end > target_frame {
                let skip = usize::try_from(target_frame - frame)
                    .ok()
                    .and_then(|frames| frames.checked_mul(self.info.channels as usize * 2))
                    .ok_or_else(|| "FLAC seek offset overflow".to_string())?;
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
        self.read_block()
    }

    fn read_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        let Some(block) = self
            .reader
            .blocks()
            .read_next_or_eof(std::mem::take(&mut self.buffer))
            .map_err(|error| format!("decode playback FLAC frame: {error}"))?
        else {
            if self.blocks != self.expected_blocks {
                return Err("FLAC playback ended before decoded report".into());
            }
            return Ok(None);
        };
        self.blocks = self
            .blocks
            .checked_add(1)
            .ok_or_else(|| "FLAC playback block count overflow".to_string())?;
        if self.blocks > self.expected_blocks {
            return Err("FLAC playback exceeded decoded report".into());
        }
        let pcm = render_block(&block, self.info)?;
        self.decoded_bytes = self
            .decoded_bytes
            .checked_add(pcm.len() as u64)
            .ok_or_else(|| "FLAC playback decoded length overflow".to_string())?;
        if self.decoded_bytes > MAX_MEDIA_DECODED_SOURCE_BYTES {
            return Err("FLAC playback decoded bytes exceed worker limit".into());
        }
        self.buffer = block.into_buffer();
        Ok(Some(pcm))
    }
}

fn scan_metadata(bytes: &[u8]) -> Result<(), String> {
    if !is_flac(bytes) {
        return Err("FLAC signature is missing".into());
    }
    let mut offset = 4_usize;
    for index in 0..MAX_FLAC_METADATA_BLOCKS {
        let header = bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "FLAC metadata header is truncated".to_string())?;
        let block_type = header[0] & 0x7f;
        let length =
            (usize::from(header[1]) << 16) | (usize::from(header[2]) << 8) | usize::from(header[3]);
        if (index == 0 && (block_type != 0 || length != 34)) || (index != 0 && block_type == 0) {
            return Err("FLAC STREAMINFO block is invalid".into());
        }
        offset = offset
            .checked_add(4)
            .and_then(|value| value.checked_add(length))
            .ok_or_else(|| "FLAC metadata length overflow".to_string())?;
        if offset > bytes.len() || offset > MAX_FLAC_METADATA_BYTES {
            return Err("FLAC metadata exceeds worker limits or source length".into());
        }
        if header[0] & 0x80 != 0 {
            return if offset < bytes.len() {
                Ok(())
            } else {
                Err("FLAC source contains no audio frames".into())
            };
        }
    }
    Err("FLAC metadata block count exceeds worker limit".into())
}

fn open_reader<R: std::io::Read>(source: R) -> Result<FlacReader<R>, String> {
    FlacReader::new_ext(
        source,
        FlacReaderOptions {
            metadata_only: false,
            read_vorbis_comment: false,
        },
    )
    .map_err(|error| format!("open FLAC decoder: {error}"))
}

fn validate_info(info: StreamInfo) -> Result<(), String> {
    if info.sample_rate == 0
        || info.sample_rate > 384_000
        || !(1..=2).contains(&info.channels)
        || !(4..=32).contains(&info.bits_per_sample)
        || info.min_block_size < 16
        || info.min_block_size > info.max_block_size
    {
        return Err("FLAC STREAMINFO format exceeds PCM playback limits".into());
    }
    if let Some(frames) = info.samples {
        let decoded_bytes = frames
            .checked_mul(u64::from(info.channels) * 2)
            .ok_or_else(|| "FLAC declared PCM length overflow".to_string())?;
        if decoded_bytes > MAX_MEDIA_DECODED_SOURCE_BYTES {
            return Err("FLAC declared PCM length exceeds worker limit".into());
        }
        frames_to_100ns(frames, info.sample_rate)?;
    }
    Ok(())
}

fn render_block(block: &claxon::frame::Block, info: StreamInfo) -> Result<Vec<u8>, String> {
    let frames = block.duration();
    if frames == 0 || frames > u32::from(info.max_block_size) || block.channels() != info.channels {
        return Err("FLAC frame disagrees with STREAMINFO".into());
    }
    let bytes = (frames as usize)
        .checked_mul(info.channels as usize)
        .and_then(|samples| samples.checked_mul(2))
        .ok_or_else(|| "FLAC PCM frame length overflow".to_string())?;
    if bytes > MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES {
        return Err("FLAC PCM frame exceeds worker limit".into());
    }
    let mut pcm = Vec::with_capacity(bytes);
    let minimum = -(1_i64 << (info.bits_per_sample - 1));
    let maximum = (1_i64 << (info.bits_per_sample - 1)) - 1;
    for frame in 0..frames {
        for channel in 0..info.channels {
            let value = block.sample(channel, frame);
            if i64::from(value) < minimum || i64::from(value) > maximum {
                return Err("FLAC sample exceeds STREAMINFO bit depth".into());
            }
            let sample = if info.bits_per_sample > 16 {
                value >> (info.bits_per_sample - 16)
            } else {
                value << (16 - info.bits_per_sample)
            };
            let sample = i16::try_from(sample)
                .map_err(|_| "FLAC PCM16 sample conversion overflow".to_string())?;
            pcm.extend_from_slice(&sample.to_le_bytes());
        }
    }
    Ok(pcm)
}

fn frames_to_100ns(frames: u64, sample_rate: u32) -> Result<u64, String> {
    let duration = frames
        .checked_mul(10_000_000)
        .and_then(|value| value.checked_div(u64::from(sample_rate)))
        .ok_or_else(|| "FLAC duration overflow".to_string())?;
    if duration > MAX_MEDIA_DURATION_100NS {
        return Err("FLAC duration exceeds worker limit".into());
    }
    Ok(duration)
}

#[cfg(test)]
mod tests;
