//! Real Opus PCM reaches the existing contained, silent playback and seek clock.

use super::super::{
    MediaCodecFamily, MediaSession, MediaWorkerState, SERIAL, decode_base64, decode_options,
};
use std::time::Duration;

fn fixtures() -> [(Vec<u8>, u16); 2] {
    [
        (
            include_str!("../../fixtures/media/test-0.4s-opus.ogg.base64"),
            1,
        ),
        (
            include_str!("../../fixtures/media/test-0.4s-opus-stereo.ogg.base64"),
            2,
        ),
    ]
    .map(|(encoded, channels)| (decode_base64(encoded), channels))
}

#[test]
fn contained_opus_mono_and_stereo_play_pause_end_seek_and_replace_without_video() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut options = decode_options();
    options.silent_audio = true;
    let mut session = MediaSession::launch(options).expect("launch hidden Opus media worker");
    let mut sources = fixtures().to_vec();
    sources.push((
        decode_base64(include_str!(
            "../../fixtures/media/test-0.4s-opus.webm.base64"
        )),
        1,
    ));
    for (bytes, channels) in sources {
        let (source, report) = session.decode_owned_audio_fixture(&bytes).unwrap();
        assert_eq!(report.audio_codec, MediaCodecFamily::Opus);
        assert_eq!(report.video_codec, MediaCodecFamily::None);
        assert_eq!((report.video_samples, report.video_decoded_bytes), (0, 0));
        assert_eq!(
            (report.audio_sample_rate, report.audio_channels),
            (48_000, channels)
        );
        assert_eq!(report.duration_100ns, 4_000_000);
        assert_eq!(report.audio_decoded_bytes, 19_200 * u64::from(channels) * 2);
        let initial = session.owned_fixture_playback_state(source).unwrap();
        assert_eq!(
            (initial.position_100ns, initial.playing, initial.ended),
            (0, false, false)
        );
        assert!(
            session
                .set_owned_fixture_playback(source, true, 0)
                .unwrap()
                .playing
        );
        std::thread::sleep(Duration::from_millis(50));
        let advanced = session.owned_fixture_playback_state(source).unwrap();
        assert!(advanced.position_100ns > 0 && advanced.position_100ns <= report.duration_100ns);
        let paused = session
            .set_owned_fixture_playback(source, false, 0)
            .unwrap();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            session
                .owned_fixture_playback_state(source)
                .unwrap()
                .position_100ns,
            paused.position_100ns
        );
        for position in [0, 0, 1_234_567, 0, 2_000_000, 4_000_000, 5_000_000, 0] {
            let sought = session
                .seek_owned_fixture_playback(source, position)
                .unwrap_or_else(|error| {
                    panic!(
                        "Opus {channels} channels seek {position}: {error}; {:?}",
                        session.snapshot()
                    )
                });
            assert_eq!(sought.position_100ns, position.min(report.duration_100ns));
            assert_eq!(sought.ended, position >= report.duration_100ns);
            assert!(!sought.playing);
        }
        // Replacing this playing source must create a fresh paused timeline,
        // not carry a prior codec/channel format or output clock forward.
        session.set_owned_fixture_playback(source, true, 0).unwrap();
    }
    session
        .shutdown()
        .expect("retire active Opus playback and worker");
    assert_eq!(session.snapshot().state, MediaWorkerState::Exited);
    assert_eq!(session.snapshot().exit_code, Some(0));
}

#[test]
fn malformed_opus_is_source_scoped_and_valid_replacement_and_sibling_keep_working() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut options = decode_options();
    options.silent_audio = true;
    let mut victim = MediaSession::launch(options.clone()).expect("launch hidden Opus worker");
    let mut sibling = MediaSession::launch(options).expect("launch hidden sibling worker");
    for (bytes, _) in fixtures() {
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        let mut chained = bytes.clone();
        chained.extend_from_slice(&bytes);
        for damaged in [bytes[..bytes.len() - 1].to_vec(), corrupt, chained] {
            assert!(victim.decode_owned_audio_fixture(&damaged).is_err());
            assert_eq!(victim.snapshot().state, MediaWorkerState::Running);
            victim
                .ping(91)
                .expect("Opus corruption is not an IPC failure");
            sibling
                .ping(92)
                .expect("malformed Opus does not damage its sibling");
        }
        let (source, report) = victim.decode_owned_audio_fixture(&bytes).unwrap();
        assert_eq!(report.audio_codec, MediaCodecFamily::Opus);
        victim
            .seek_owned_fixture_playback(source, 2_000_000)
            .unwrap();
    }
    victim.shutdown().unwrap();
    sibling.shutdown().unwrap();
}
