//! Bounded Ogg/Opus playback through the shared presentation-sample decoder.

use super::DecodedMedia;
use crate::limits::{MAX_MEDIA_ENCODED_QUEUE_BYTES, MEDIA_COMMAND_TIMEOUT};
use crate::media_protocol::{
    MediaBufferedExtent, MediaCodecFamily, MediaDecodeReport, MediaLimits,
};
use crate::opus_audio::SAMPLE_RATE;
use std::sync::Arc;
use std::time::Instant;

mod playback;
mod stream;
pub(in crate::media_process) use playback::OpusDecoder;
use stream::PcmStream;

#[cfg(test)]
mod tests;

pub(super) fn decode(
    bytes: &[u8],
    limits: MediaLimits,
    started: Instant,
) -> Result<DecodedMedia, String> {
    if bytes.is_empty()
        || bytes.len() > MAX_MEDIA_ENCODED_QUEUE_BYTES
        || bytes.len() as u64 > limits.max_encoded_bytes
    {
        return Err("encoded Ogg/Opus source exceeds worker limits".into());
    }
    let deadline = started + MEDIA_COMMAND_TIMEOUT;
    let mut stream = PcmStream::open(Arc::from(bytes), deadline)?;
    while stream.next_sample(deadline)?.is_some() {}
    let duration = stream::frames_to_100ns(stream.frames)?;
    if stream.samples == 0 || stream.decoded_bytes == 0 || duration == 0 {
        return Err("Ogg/Opus source produced no presentation PCM".into());
    }
    let report = MediaDecodeReport {
        buffered: MediaBufferedExtent {
            audio_start_100ns: 0,
            audio_end_100ns: duration,
            ..MediaBufferedExtent::default()
        },
        encoded_bytes: bytes.len() as u64,
        video_codec: MediaCodecFamily::None,
        audio_codec: MediaCodecFamily::Opus,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 0,
        video_height: 0,
        audio_sample_rate: SAMPLE_RATE,
        audio_channels: stream.channels,
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
        .map_err(|error| format!("validate Ogg/Opus source: {error}"))?;
    Ok(DecodedMedia {
        report,
        playback: None,
    })
}
