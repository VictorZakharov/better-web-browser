use super::*;

#[test]
fn owned_fragmented_tracks_decode_independently_without_fabricating_the_other_extent() {
    let video = fixture(include_str!(
        "../../../tests/fixtures/media/test-1s-video-fragmented.mp4.base64"
    ));
    let audio = fixture(include_str!(
        "../../../tests/fixtures/media/test-1s-audio-fragmented.mp4.base64"
    ));
    let decoded = decode_append(&video, &[], MediaLimits::default()).unwrap();
    assert!(decoded.audio.is_none());
    assert_eq!(decoded.buffered.audio_end_100ns, 0);
    assert!(decoded.buffered.video_end_100ns > 0);
    let mut decoder = decoded.video.unwrap();
    let mut frames = 0;
    while let Some(frame) = decoder.next_frame().unwrap() {
        assert_eq!((frame.width, frame.height), (320, 240));
        assert!(!frame.bytes.is_empty());
        frames += 1;
        assert!(frames <= 64);
    }
    assert!(frames >= 30);

    let decoded = decode_append(&[], &audio, MediaLimits::default()).unwrap();
    assert!(decoded.video.is_none());
    assert_eq!(decoded.buffered.video_end_100ns, 0);
    assert!(decoded.buffered.audio_end_100ns > 0);
    let report = decoded.audio.unwrap();
    let mut decoder = AudioDecoder::open(
        &audio,
        report.audio_samples,
        report.audio_sample_rate,
        report.audio_channels,
    )
    .unwrap();
    assert!(!decoder.next_sample().unwrap().unwrap().is_empty());
}

#[test]
fn seeking_beyond_buffered_video_is_exhaustion_not_a_decode_failure() {
    let video = fixture(include_str!(
        "../../../tests/fixtures/media/test-1s-video-fragmented.mp4.base64"
    ));
    let decoded = decode_append(&video, &[], MediaLimits::default()).unwrap();
    let end = decoded.buffered.video_end_100ns;
    let mut decoder = decoded.video.unwrap();
    decoder.seek(end + 1_000_000).unwrap();
    assert!(
        decoder
            .next_frame()
            .expect("valid preroll is not corrupt")
            .is_none()
    );
    decoder.seek(0).unwrap();
    assert!(
        decoder
            .next_frame()
            .expect("seek back after exhaustion")
            .is_some()
    );
}

fn fixture(encoded: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let (mut accumulator, mut bits) = (0_u32, 0_u32);
    for byte in encoded.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => panic!("invalid owned fixture encoding"),
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
        }
    }
    output
}
