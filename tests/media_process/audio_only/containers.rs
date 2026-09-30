use super::super::{
    MediaCodecFamily, MediaSession, MediaWorkerState, SERIAL, decode_base64, decode_options,
};
use std::time::Duration;

fn fixtures() -> [(Vec<u8>, MediaCodecFamily); 3] {
    [
        (
            decode_base64(include_str!(
                "../../fixtures/media/test-0.4s-tone.aac.base64"
            )),
            MediaCodecFamily::AacLc,
        ),
        (
            decode_base64(include_str!(
                "../../fixtures/media/test-0.4s-tone.webm.base64"
            )),
            MediaCodecFamily::Vorbis,
        ),
        (
            decode_base64(include_str!(
                "../../fixtures/media/test-0.4s-tone.oga.base64"
            )),
            MediaCodecFamily::Flac,
        ),
    ]
}

#[test]
fn new_audio_containers_play_pause_and_seek_without_a_host_codec_or_video_frame() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (bytes, codec) in fixtures() {
        let mut options = decode_options();
        options.silent_audio = true;
        let mut session = MediaSession::launch(options).expect("launch hidden media worker");
        let (source, report) = session.decode_owned_audio_fixture(&bytes).unwrap();
        assert_eq!(report.audio_codec, codec);
        assert_eq!(report.video_codec, MediaCodecFamily::None);
        assert_eq!((report.video_samples, report.video_decoded_bytes), (0, 0));
        assert_eq!(
            (report.audio_sample_rate, report.audio_channels),
            (44_100, 1)
        );
        assert!((3_000_000..=5_000_000).contains(&report.duration_100ns));
        assert!(
            session
                .set_owned_fixture_playback(source, true, 0)
                .unwrap()
                .playing
        );
        std::thread::sleep(Duration::from_millis(40));
        assert!(
            session
                .owned_fixture_playback_state(source)
                .unwrap()
                .position_100ns
                > 0
        );
        let paused = session
            .set_owned_fixture_playback(source, false, 0)
            .unwrap();
        assert!(!paused.playing);
        for position in [1_234_567, 0, 2_000_000] {
            let sought = session
                .seek_owned_fixture_playback(source, position)
                .unwrap_or_else(|error| {
                    panic!(
                        "seek {codec:?} at {position}: {error}; worker: {:?}",
                        session.snapshot()
                    )
                });
            assert_eq!(sought.position_100ns, position);
            assert!(!sought.playing);
        }
        session.shutdown().expect("clean media worker shutdown");
    }
}

#[test]
fn truncated_new_containers_fail_in_the_worker_without_fallback_or_sibling_damage() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut options = decode_options();
    options.silent_audio = true;
    let mut sibling = MediaSession::launch(options.clone()).expect("launch hidden sibling");
    for (bytes, codec) in fixtures() {
        let mut victim = MediaSession::launch(options.clone()).expect("launch hidden media worker");
        assert!(
            victim
                .decode_owned_audio_fixture(&bytes[..bytes.len() - 1])
                .is_err()
        );
        assert_eq!(victim.snapshot().state, MediaWorkerState::Running);
        victim
            .ping(36)
            .expect("a source error is not a protocol failure");
        let (_, report) = victim
            .decode_owned_audio_fixture(&bytes)
            .expect("replace rejected source");
        assert_eq!(report.audio_codec, codec);
        victim.shutdown().expect("clean recovered worker shutdown");
        sibling.ping(37).expect("sibling survives malformed source");
    }
    sibling.shutdown().expect("clean sibling shutdown");
}

#[test]
fn unsupported_opus_and_video_mixed_webm_fail_without_partial_audio_playback() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut options = decode_options();
    options.silent_audio = true;
    let mut session = MediaSession::launch(options).expect("launch hidden media worker");
    for encoded in [
        include_str!("../../fixtures/media/test-0.4s-opus.webm.base64"),
        include_str!("../../fixtures/media/test-0.4s-mixed.webm.base64"),
    ] {
        let bytes = decode_base64(encoded);
        assert!(session.decode_owned_audio_fixture(&bytes).is_err());
        session
            .ping(38)
            .expect("unsupported codec is a source-scoped error");
    }
    let (bytes, _) = fixtures().into_iter().nth(1).unwrap();
    let (_, report) = session
        .decode_owned_audio_fixture(&bytes)
        .expect("replace unsupported WebM");
    assert_eq!(report.audio_codec, MediaCodecFamily::Vorbis);
    session.shutdown().expect("clean media worker shutdown");
}
