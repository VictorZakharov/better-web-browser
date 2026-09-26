use super::*;

#[test]
fn complete_video_only_fixture_seeks_with_media_foundation() {
    use base64::Engine as _;

    let encoded: String =
        include_str!("../../../../tests/fixtures/media/test-1s-video-fragmented.mp4.base64")
            .chars()
            .filter(|character| !character.is_ascii_whitespace())
            .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("decode WPT H.264 fixture");
    let decoded =
        super::super::decode(&bytes, MediaLimits::default()).expect("decode video-only fixture");
    let mut playback = decoded.playback.expect("video decoder");
    playback
        .seek(decoded.report.duration_100ns.saturating_sub(100_000))
        .expect("seek H.264-only MP4");
    assert!(
        playback
            .next_frame()
            .expect("decode sought frame")
            .is_some()
    );
}
use crate::media_process::backend::fragmented_mp4::VideoSample;

#[test]
fn queued_segments_retain_encoded_tracks_not_native_decoders() {
    // Metadata-only tracks deliberately have no decodable bitstream: enqueueing must not
    // activate a native transform. The parser validates real tracks before this boundary.
    let track = VideoTrack {
        width: 16,
        height: 16,
        nal_length_size: 4,
        sequence_header: Vec::new(),
        samples: vec![VideoSample {
            bytes: vec![0, 0, 0, 1, 0],
            timestamp_100ns: 0,
            duration_100ns: 333_333,
            key_frame: true,
        }],
    };
    let limits = MediaLimits::default();
    let mut playback = VideoDecoder::open_fragmented(track.clone(), limits).unwrap();
    for _ in 0..127 {
        playback
            .append(VideoDecoder::open_fragmented(track.clone(), limits).unwrap())
            .unwrap();
    }
    let Decoder::Transform(playback) = playback.inner else {
        panic!("fragmented backend")
    };
    assert_eq!(playback.segments.len(), 128);
    assert_eq!(playback.samples, 128);
    assert!(
        playback.active.is_none(),
        "queued segments opened native decoders"
    );
}
