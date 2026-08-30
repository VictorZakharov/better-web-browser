use super::{MediaSession, SERIAL, decode_base64, options};
use std::time::Duration;

#[test]
fn contained_worker_owns_the_play_pause_clock_without_emitting_test_audio() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = decode_base64(include_str!("../fixtures/media/test-1s.mp4.base64"));
    let mut session = MediaSession::launch(options()).expect("launch contained media worker");
    let decoded = session
        .decode_owned_fixture_frame(&fixture)
        .expect("decode owned fixture");
    let source_id = decoded.frame.metadata.source_id;

    let started = session
        .set_owned_fixture_playback(source_id, true, 0)
        .expect("start silent test playback");
    assert!(started.playing);
    assert!(!started.ended);
    std::thread::sleep(Duration::from_millis(120));
    let advanced = session
        .owned_fixture_playback_state(source_id)
        .expect("query worker clock");
    assert!(
        advanced.position_100ns >= 750_000,
        "worker clock did not advance: {advanced:?}"
    );

    let paused = session
        .set_owned_fixture_playback(source_id, false, 0)
        .expect("pause worker clock");
    assert!(!paused.playing);
    std::thread::sleep(Duration::from_millis(80));
    let still_paused = session
        .owned_fixture_playback_state(source_id)
        .expect("query paused worker clock");
    assert_eq!(still_paused.position_100ns, paused.position_100ns);
    session.shutdown().expect("clean media-worker shutdown");
}
