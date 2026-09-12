use super::*;

#[test]
fn fractional_frame_timestamps_do_not_create_artificial_buffer_gaps() {
    let mut video = fixture(include_str!(
        "../../../tests/fixtures/media/test-1s-video-fragmented.mp4.base64"
    ));
    let mdhd = video.windows(4).position(|tag| tag == b"mdhd").unwrap();
    video[mdhd + 16..mdhd + 20].copy_from_slice(&30_001_u32.to_be_bytes());
    let mut track = fragmented_mp4::parse_video(&video, MediaLimits::default()).unwrap();
    track.samples.sort_by_key(|sample| sample.timestamp_100ns);
    for pair in track.samples.windows(2) {
        assert_eq!(
            pair[0].timestamp_100ns + pair[0].duration_100ns as i64,
            pair[1].timestamp_100ns,
            "contiguous samples must share the same rounded boundary"
        );
    }
}

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

#[test]
fn seeking_into_a_disjoint_seven_minute_segment_decodes_both_tracks() {
    let video = fixture(include_str!(
        "../../../tests/fixtures/media/test-1s-video-fragmented.mp4.base64"
    ));
    let audio = fixture(include_str!(
        "../../../tests/fixtures/media/test-1s-audio-fragmented.mp4.base64"
    ));
    let mut initial = decode_append(&video, &[], MediaLimits::default())
        .unwrap()
        .video
        .unwrap();
    let shifted_video = retime_fragment(video, 420);
    let shifted_audio = retime_fragment(audio, 420);
    let appended = decode_append(&shifted_video, &shifted_audio, MediaLimits::default()).unwrap();
    assert!(appended.buffered.video_start_100ns >= 4_200_000_000);
    assert!(appended.buffered.audio_start_100ns >= 4_200_000_000);
    initial.append(appended.video.unwrap()).unwrap();
    initial.seek(4_205_000_000).unwrap();
    let frame = initial.next_frame().unwrap().expect("video at 7:00.5");
    assert!(frame.timestamp_100ns >= 4_204_000_000);
    let report = appended.audio.unwrap();
    let mut decoder = AudioDecoder::open(
        &shifted_audio,
        report.audio_samples,
        report.audio_sample_rate,
        report.audio_channels,
    )
    .unwrap();
    decoder.seek(4_205_000_000).unwrap();
    assert!(
        !decoder
            .next_sample()
            .unwrap()
            .expect("PCM at 7:00.5")
            .is_empty()
    );
}

// Retimestamp only the checked-in, licensed one-fragment fixture; no new media asset.
fn retime_fragment(mut bytes: Vec<u8>, seconds: u64) -> Vec<u8> {
    let mdhd = bytes.windows(4).position(|tag| tag == b"mdhd").unwrap();
    assert_eq!(bytes[mdhd + 4], 0);
    let scale = u32::from_be_bytes(bytes[mdhd + 16..mdhd + 20].try_into().unwrap());
    let tfdt = bytes.windows(4).position(|tag| tag == b"tfdt").unwrap();
    assert_eq!(bytes[tfdt + 4], 1);
    bytes[tfdt + 8..tfdt + 16].copy_from_slice(&(seconds * u64::from(scale)).to_be_bytes());
    bytes
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
