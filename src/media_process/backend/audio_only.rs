//! Complete-source audio without a fabricated video stream.
//!
//! Media Foundation owns container sniffing and decoding inside the restricted worker.
//! Only the native PCM, MP3, and AAC subtypes that this worker can pull as 16-bit PCM
//! are admitted. The browser does not trust an HTTP Content-Type as codec evidence.

use super::*;
use crate::limits::MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES;
use crate::media_protocol::MediaBufferedExtent;
use windows::Win32::Media::MediaFoundation::{MF_MT_AUDIO_BITS_PER_SAMPLE, MFAudioFormat_MP3};

#[cfg(test)]
mod tests;

pub(super) fn decode(
    reader: &IMFSourceReader,
    encoded_bytes: u64,
    limits: MediaLimits,
    started: Instant,
) -> Result<DecodedMedia, String> {
    let stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    select_stream(reader, stream, "audio-only")?;
    let native = unsafe { reader.GetNativeMediaType(stream, 0) }
        .map_err(|error| format!("read native audio-only type: {error}"))?;
    let major = unsafe { native.GetGUID(&MF_MT_MAJOR_TYPE) }
        .map_err(|error| format!("read audio-only major type: {error}"))?;
    let subtype = unsafe { native.GetGUID(&MF_MT_SUBTYPE) }
        .map_err(|error| format!("read audio-only subtype: {error}"))?;
    if major != MFMediaType_Audio {
        return Err("audio-only source did not expose an audio track".into());
    }
    let codec = if subtype == MFAudioFormat_AAC {
        // MF's AAC subtype covers both LC and HE profiles. Do not infer LC here.
        MediaCodecFamily::Aac
    } else if subtype == MFAudioFormat_MP3 {
        MediaCodecFamily::Mp3
    } else if subtype == MFAudioFormat_PCM {
        MediaCodecFamily::Pcm
    } else {
        return Err("audio-only source uses an unsupported codec".into());
    };
    let audio_type = output_type(MFMediaType_Audio, MFAudioFormat_PCM)?;
    unsafe {
        reader
            .SetCurrentMediaType(stream, None, &audio_type)
            .map_err(|error| format!("configure audio-only PCM output: {error}"))?;
    }
    let current = unsafe { reader.GetCurrentMediaType(stream) }
        .map_err(|error| format!("read decoded audio-only format: {error}"))?;
    let sample_rate = unsafe { current.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }
        .map_err(|error| format!("read decoded audio-only sample rate: {error}"))?;
    let channels = unsafe { current.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }
        .map_err(|error| format!("read decoded audio-only channel count: {error}"))?;
    let bits = unsafe { current.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE) }.unwrap_or(16);
    if bits != 16 || sample_rate == 0 || channels == 0 {
        return Err("audio-only PCM output format is unsupported".into());
    }
    let audio = read_stream(
        reader,
        stream,
        "audio-only",
        MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES as u64,
    )?;
    if audio.samples == 0 || audio.bytes == 0 {
        return Err("audio-only source produced no PCM samples".into());
    }
    let first = audio.first_timestamp.unwrap_or(0).max(0);
    // Some PCM sources omit an IMF sample duration. Use the actual decoded sample
    // count and format, never the encoded byte length, for a bounded fallback.
    let decoded_frames = audio.bytes / (u64::from(channels) * 2);
    let pcm_duration = decoded_frames
        .saturating_mul(10_000_000)
        .checked_div(u64::from(sample_rate))
        .unwrap_or(0);
    // The playback clock advances through submitted PCM, not source timestamps.
    // Timestamp gaps cannot be presented until silence insertion is supported.
    let end = (first as u64).saturating_add(pcm_duration);
    let report = MediaDecodeReport {
        buffered: MediaBufferedExtent {
            audio_start_100ns: first,
            audio_end_100ns: end,
            ..MediaBufferedExtent::default()
        },
        encoded_bytes,
        video_codec: MediaCodecFamily::None,
        audio_codec: codec,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 0,
        video_height: 0,
        audio_sample_rate: sample_rate,
        audio_channels: u16::try_from(channels)
            .map_err(|_| "audio-only channel count is not representable")?,
        video_samples: 0,
        audio_samples: audio.samples,
        video_decoded_bytes: 0,
        audio_decoded_bytes: audio.bytes,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 0,
        audio_first_timestamp_100ns: first,
        audio_last_timestamp_100ns: audio.last_timestamp.unwrap_or(first),
        duration_100ns: end,
        decode_micros: elapsed_micros(started),
    };
    report
        .validate(limits)
        .map_err(|error| format!("validate decoded audio-only source: {error}"))?;
    Ok(DecodedMedia {
        report,
        playback: None,
    })
}
