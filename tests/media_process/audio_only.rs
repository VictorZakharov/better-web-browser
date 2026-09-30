//! Silent contained-worker audio-only acceptance. No video frame may be fabricated.

use super::{MediaCodecFamily, MediaSession, SERIAL, decode_base64, decode_options, sha256};
use std::time::Duration;

#[path = "audio_only/containers.rs"]
mod containers;

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
    verify_playback(&wave_audio_fixture(), MediaCodecFamily::Pcm);
}

fn wave_audio_fixture() -> Vec<u8> {
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
    wave
}

#[test]
fn graph_pcm_stream_coexists_with_decoded_audio_playback() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut options = decode_options();
    options.silent_audio = true;
    let mut session = MediaSession::launch(options).expect("launch hidden contained media worker");
    let (source, _) = session
        .decode_owned_audio_fixture(&wave_audio_fixture())
        .expect("decode PCM wave fixture");
    session
        .set_owned_fixture_playback(source, true, 0)
        .expect("start decoded audio playback");
    let format = better_web_browser::media_protocol::GraphPcmFormat {
        sample_rate: 48_000,
        channels: 2,
    };
    assert!(
        session
            .queue_owned_graph_pcm_fixture(11, 13, format, vec![0; 512])
            .expect("queue graph PCM")
    );
    assert!(
        !session
            .queue_owned_graph_pcm_fixture(11, 14, format, vec![0; 512])
            .expect("reject competing graph context")
    );
    std::thread::sleep(Duration::from_millis(50));
    let state = session
        .owned_fixture_playback_state(source)
        .expect("query decoded audio after graph PCM");
    assert!(state.playing && state.position_100ns > 0, "{state:?}");
    assert!(!session.close_owned_graph_pcm_fixture(12, 13).unwrap());
    assert!(session.close_owned_graph_pcm_fixture(11, 13).unwrap());
    assert!(
        session
            .queue_owned_graph_pcm_fixture(11, 14, format, vec![0; 512])
            .expect("admit a new graph context after close")
    );
    let state = session
        .owned_fixture_playback_state(source)
        .expect("decoded playback remains after graph close");
    assert!(state.playing && state.position_100ns > 0, "{state:?}");
    session.shutdown().expect("clean media worker shutdown");
}

#[test]
fn aac_mp4_audio_only_play_pause_seek() {
    let bytes = decode_base64(include_str!(
        "../fixtures/media/test-1s-audio-fragmented.mp4.base64"
    ));
    verify_playback(&bytes, MediaCodecFamily::Aac);
}

#[test]
fn ordinary_m4a_aac_plays_without_media_foundation_codec() {
    let bytes = decode_base64(include_str!("../fixtures/media/test-0.4s-tone.m4a.base64"));
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut options = decode_options();
    options.silent_audio = true;
    let mut session = MediaSession::launch(options).expect("launch hidden contained media worker");
    let (source, report) = session
        .decode_owned_audio_fixture(&bytes)
        .expect("decode ordinary AAC-in-M4A without a host codec");
    assert_eq!(report.video_codec, MediaCodecFamily::None);
    assert_eq!(report.audio_codec, MediaCodecFamily::Aac);
    assert_eq!(report.audio_sample_rate, 44_100);
    assert_eq!(report.audio_channels, 1);
    assert_eq!(report.duration_100ns, 4_000_000);
    assert_eq!(report.audio_decoded_bytes, 17_640 * 2);
    session
        .set_owned_fixture_playback(source, true, 0)
        .expect("start silent M4A playback");
    let sought = session
        .seek_owned_fixture_playback(source, 2_000_000)
        .expect("seek ordinary M4A source");
    assert!((1_500_000..=2_500_000).contains(&sought.position_100ns));
    session.shutdown().expect("clean media worker shutdown");
}

#[test]
fn mp3_audio_only_play_pause_seek_without_media_foundation_codec() {
    let bytes = decode_base64(include_str!("../fixtures/media/test-1s-audio.mp3.base64"));
    verify_playback(&bytes, MediaCodecFamily::Mp3);
}

#[test]
fn adts_aac_audio_only_play_pause_seek() {
    let bytes = decode_base64(include_str!("../fixtures/media/test-1s-audio.aac.base64"));
    verify_playback(&bytes, MediaCodecFamily::AacLc);
}

#[test]
fn flac_audio_only_play_pause_seek_without_media_foundation_codec() {
    let bytes = decode_base64(include_str!("../fixtures/media/test-1s-audio.flac.base64"));
    assert_eq!(bytes.len(), 20_333);
    assert_eq!(
        sha256(&bytes),
        [
            0xdd, 0x80, 0x80, 0xcb, 0x04, 0xe2, 0x82, 0x22, 0xc5, 0x85, 0xc5, 0x52, 0xe6, 0xf7,
            0x6a, 0x66, 0x69, 0xc7, 0x89, 0xae, 0xb6, 0x21, 0xb8, 0x02, 0xec, 0xfe, 0x72, 0x89,
            0x49, 0xad, 0xc7, 0xad,
        ]
    );
    verify_playback(&bytes, MediaCodecFamily::Flac);
}

#[test]
fn ogg_vorbis_audio_only_play_pause_seek() {
    let bytes = decode_base64(include_str!("../fixtures/media/test-2s-audio.ogg.base64"));
    assert_eq!(bytes.len(), 6_675);
    assert_eq!(
        sha256(&bytes),
        [
            0x1e, 0x65, 0x83, 0x9c, 0x93, 0x5c, 0x43, 0xc4, 0x81, 0xf9, 0xe7, 0xa7, 0xdf, 0x38,
            0x88, 0xa2, 0xab, 0x2c, 0xe9, 0xe9, 0xfb, 0x05, 0xf5, 0xd0, 0xb8, 0x6c, 0x84, 0x6f,
            0x53, 0x6e, 0x4a, 0x7b,
        ]
    );
    verify_playback(&bytes, MediaCodecFamily::Vorbis);
}
