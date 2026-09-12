use super::*;

#[test]
fn frame_metadata_uses_each_adaptive_segments_dimensions() {
    let video = backend::DecodedVideoSample {
        bytes: vec![0; 24],
        stride: 4,
        width: 4,
        height: 4,
        timestamp_100ns: 10,
        duration_100ns: 20,
    };
    assert_eq!(
        (video_frame_metadata(7, 8, &video).width, video.height),
        (4, 4)
    );

    let next = backend::DecodedVideoSample {
        width: 2,
        height: 2,
        stride: 2,
        bytes: vec![0; 6],
        ..video
    };
    let metadata = video_frame_metadata(7, 9, &next);
    assert_eq!((metadata.width, metadata.height), (2, 2));
}
