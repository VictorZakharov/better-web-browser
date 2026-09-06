use super::*;
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
