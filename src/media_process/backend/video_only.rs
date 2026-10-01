//! A complete H.264 resource with no audio track uses a video-owned clock.

use super::*;

pub(super) fn decode(
    video_bytes: &[u8],
    encoded_bytes: u64,
    limits: MediaLimits,
    started: Instant,
    video_width: u32,
    video_height: u32,
    video: stream::StreamSummary,
) -> Result<DecodedMedia, String> {
    if video.samples == 0 || video.end_100ns == 0 {
        return Err("video-only source produced no timed H.264 samples".into());
    }
    let report = MediaDecodeReport {
        buffered: crate::media_protocol::MediaBufferedExtent {
            video_start_100ns: video.first_timestamp.unwrap_or(0).max(0),
            video_end_100ns: video.end_100ns,
            ..Default::default()
        },
        encoded_bytes,
        video_codec: MediaCodecFamily::H264,
        audio_codec: MediaCodecFamily::None,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width,
        video_height,
        audio_sample_rate: 0,
        audio_channels: 0,
        video_samples: video.samples,
        audio_samples: 0,
        video_decoded_bytes: video.bytes,
        audio_decoded_bytes: 0,
        video_first_timestamp_100ns: video.first_timestamp.unwrap_or(0),
        video_last_timestamp_100ns: video.last_timestamp.unwrap_or(0),
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: 0,
        duration_100ns: video.end_100ns,
        decode_micros: elapsed_micros(started),
    };
    report
        .validate(limits)
        .map_err(|error| format!("validate decoded video-only source: {error}"))?;
    // Media Foundation decodes complete fragmented MP4 but does not permit seeking
    // its Source Reader. The existing bounded fragment parser and H.264 transform
    // support seeks; ordinary MP4 continues through the Source Reader.
    let playback = if let Ok(track) = fragmented_mp4::parse_video(video_bytes, limits) {
        VideoDecoder::open_fragmented(track, limits)?
    } else {
        VideoDecoder::open(video_bytes, limits, report.video_samples)?
    };
    Ok(DecodedMedia {
        report,
        playback: Some(playback),
        foundation: None,
    })
}
