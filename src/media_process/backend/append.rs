use super::*;
use crate::media_protocol::MediaBufferedExtent;
#[cfg(test)]
#[path = "append_tests.rs"]
mod tests;

pub(in crate::media_process) struct DecodedAppend {
    pub(in crate::media_process) video: Option<VideoDecoder>,
    pub(in crate::media_process) audio: Option<AudioTrackReport>,
    pub(in crate::media_process) buffered: MediaBufferedExtent,
}

pub(in crate::media_process) fn decode_append(
    video_bytes: &[u8],
    audio_bytes: &[u8],
    limits: MediaLimits,
) -> Result<DecodedAppend, String> {
    let mut buffered = MediaBufferedExtent::default();
    let video = if video_bytes.is_empty() {
        None
    } else {
        let track = fragmented_mp4::parse_video(video_bytes, limits)?;
        buffered.video_start_100ns = track
            .samples
            .iter()
            .map(|sample| sample.timestamp_100ns)
            .min()
            .unwrap_or(0)
            .max(0);
        buffered.video_end_100ns = track.duration_100ns();
        Some(VideoDecoder::open_fragmented(track, limits)?)
    };
    let audio = if audio_bytes.is_empty() {
        None
    } else {
        let (report, _) = adaptive_audio::inspect(audio_bytes, limits)?;
        buffered.audio_start_100ns = report.audio_first_timestamp_100ns.max(0);
        buffered.audio_end_100ns = report.duration_100ns;
        Some(report)
    };
    buffered.validate().map_err(|error| error.to_string())?;
    Ok(DecodedAppend {
        video,
        audio,
        buffered,
    })
}
