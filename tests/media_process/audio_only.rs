//! Silent contained-worker audio-only acceptance. No video frame may be fabricated.

use super::{MediaCodecFamily, MediaSession, SERIAL, decode_base64, decode_options};
use std::time::Duration;

fn verify_playback(bytes: &[u8], codec: MediaCodecFamily) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut options = decode_options();
    options.silent_audio = true;
    let mut session = MediaSession::launch(options).expect("launch hidden contained media worker");
    let (source, report) = session
        .decode_owned_audio_fixture(bytes)
        .expect("decode audio without a video frame");
    assert_eq!(report.video_codec, MediaCodecFamily::None);
    assert_eq!(report.audio_codec, codec);
    assert_eq!(report.video_samples, 0);
    assert_eq!(report.video_decoded_bytes, 0);
    assert!(report.audio_decoded_bytes > 0);
    assert!(report.duration_100ns >= 8_000_000);

    let started = session
        .set_owned_fixture_playback(source, true, 0)
        .expect("start silent audio playback");
    assert!(started.playing);
    std::thread::sleep(Duration::from_millis(120));
    let advanced = session
        .owned_fixture_playback_state(source)
        .expect("query audio clock");
    assert!(
        advanced.position_100ns >= 750_000,
        "audio clock did not advance: {advanced:?}"
    );
    let paused = session
        .set_owned_fixture_playback(source, false, 0)
        .expect("pause audio playback");
    assert!(!paused.playing);
    std::thread::sleep(Duration::from_millis(80));
    let still_paused = session
        .owned_fixture_playback_state(source)
        .expect("query paused audio clock");
    assert_eq!(still_paused.position_100ns, paused.position_100ns);

    let sought = session
        .seek_owned_fixture_playback(source, 5_000_000)
        .expect("seek audio-only source");
    assert!(
        sought.position_100ns >= 4_000_000,
        "audio seek missed target: {sought:?}"
    );
    assert!(
        sought.position_100ns <= 6_000_000,
        "audio seek overshot target: {sought:?}"
    );
    session.shutdown().expect("clean media worker shutdown");
}

#[test]
fn pcm_wave_audio_only_play_pause_seek() {
    let pcm = vec![0_u8; 44_100 * 2];
    let mut wave = Vec::with_capacity(pcm.len() + 44);
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&u32::try_from(pcm.len() + 36).unwrap().to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16_u32.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&44_100_u32.to_le_bytes());
    wave.extend_from_slice(&88_200_u32.to_le_bytes());
    wave.extend_from_slice(&2_u16.to_le_bytes());
    wave.extend_from_slice(&16_u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&u32::try_from(pcm.len()).unwrap().to_le_bytes());
    wave.extend_from_slice(&pcm);
    verify_playback(&wave, MediaCodecFamily::Pcm);
}

#[test]
fn aac_mp4_audio_only_play_pause_seek() {
    let bytes = decode_base64(include_str!(
        "../fixtures/media/test-1s-audio-fragmented.mp4.base64"
    ));
    verify_playback(&bytes, MediaCodecFamily::Aac);
}

#[test]
fn mp3_audio_only_play_pause_seek() {
    let bytes = decode_base64(include_str!("../fixtures/media/test-1s-audio.mp3.base64"));
    verify_playback(&bytes, MediaCodecFamily::Mp3);
}

#[test]
fn adts_aac_audio_only_play_pause_seek() {
    let bytes = decode_base64(include_str!("../fixtures/media/test-1s-audio.aac.base64"));
    verify_playback(&bytes, MediaCodecFamily::Aac);
}
