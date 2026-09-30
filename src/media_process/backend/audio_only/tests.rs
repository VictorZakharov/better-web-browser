use super::*;
use base64::Engine as _;

fn decode_fixture(source: &str) -> Vec<u8> {
    let compact: String = source
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(compact)
        .unwrap()
}

fn pcm_wave() -> Vec<u8> {
    let pcm = vec![0_u8; 44_100 * 2];
    let mut wave = Vec::with_capacity(pcm.len() + 44);
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&u32::try_from(pcm.len() + 36).unwrap().to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16_u32.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    wave.extend_from_slice(&1_u16.to_le_bytes()); // mono
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
fn wave_pcm_is_decoded_as_audio_only_and_seekable() {
    let bytes = pcm_wave();
    let decoded = super::super::decode(&bytes, MediaLimits::default()).unwrap();
    assert!(decoded.playback.is_none());
    assert_eq!(decoded.report.video_codec, MediaCodecFamily::None);
    assert_eq!(decoded.report.audio_codec, MediaCodecFamily::Pcm);
    assert_eq!(decoded.report.audio_sample_rate, 44_100);
    assert_eq!(decoded.report.audio_channels, 1);
    assert_eq!(decoded.report.buffered.video_end_100ns, 0);
    assert!(decoded.report.duration_100ns >= 9_000_000);
    let mut playback = AudioDecoder::open(
        &bytes,
        decoded.report.audio_codec,
        decoded.report.audio_samples,
        decoded.report.audio_sample_rate,
        decoded.report.audio_channels,
    )
    .unwrap();
    assert!(playback.next_sample().unwrap().is_some());
    playback.seek(5_000_000).unwrap();
    assert!(playback.next_sample().unwrap().is_some());
}

#[test]
fn m4a_aac_is_decoded_without_a_synthetic_video_track() {
    let bytes = decode_fixture(include_str!(
        "../../../../tests/fixtures/media/test-1s-audio-fragmented.mp4.base64"
    ));
    let decoded = super::super::decode(&bytes, MediaLimits::default()).unwrap();
    assert!(decoded.playback.is_none());
    assert_eq!(decoded.report.video_codec, MediaCodecFamily::None);
    assert_eq!(decoded.report.audio_codec, MediaCodecFamily::Aac);
    assert_eq!(decoded.report.audio_sample_rate, 44_100);
    assert_eq!(decoded.report.audio_channels, 2);
    assert!(decoded.report.audio_samples > 0);
    let mut playback = AudioDecoder::open(
        &bytes,
        decoded.report.audio_codec,
        decoded.report.audio_samples,
        decoded.report.audio_sample_rate,
        decoded.report.audio_channels,
    )
    .unwrap();
    assert!(playback.next_sample().unwrap().is_some());
    playback.seek(5_000_000).unwrap();
    assert!(playback.next_sample().unwrap().is_some());
}

#[test]
fn mp3_is_decoded_as_audio_only_and_seekable() {
    let bytes = decode_fixture(include_str!(
        "../../../../tests/fixtures/media/test-1s-audio.mp3.base64"
    ));
    let decoded = super::super::decode(&bytes, MediaLimits::default()).unwrap();
    assert!(decoded.playback.is_none());
    assert_eq!(decoded.report.video_codec, MediaCodecFamily::None);
    assert_eq!(decoded.report.audio_codec, MediaCodecFamily::Mp3);
    assert!(decoded.report.audio_samples > 0);
    assert!(decoded.report.duration_100ns >= 8_000_000);
    let mut playback = AudioDecoder::open(
        &bytes,
        decoded.report.audio_codec,
        decoded.report.audio_samples,
        decoded.report.audio_sample_rate,
        decoded.report.audio_channels,
    )
    .unwrap();
    assert!(playback.next_sample().unwrap().is_some());
    playback.seek(5_000_000).unwrap();
    assert!(playback.next_sample().unwrap().is_some());
}

#[test]
fn adts_aac_is_decoded_as_audio_only_and_seekable() {
    let bytes = decode_fixture(include_str!(
        "../../../../tests/fixtures/media/test-1s-audio.aac.base64"
    ));
    let decoded = super::super::decode(&bytes, MediaLimits::default()).unwrap();
    assert!(decoded.playback.is_none());
    assert_eq!(decoded.report.video_codec, MediaCodecFamily::None);
    assert_eq!(decoded.report.audio_codec, MediaCodecFamily::AacLc);
    assert!(decoded.report.audio_samples > 0);
    let mut playback = AudioDecoder::open(
        &bytes,
        decoded.report.audio_codec,
        decoded.report.audio_samples,
        decoded.report.audio_sample_rate,
        decoded.report.audio_channels,
    )
    .unwrap();
    assert!(playback.next_sample().unwrap().is_some());
    playback.seek(5_000_000).unwrap();
    assert!(playback.next_sample().unwrap().is_some());
}

#[test]
fn flac_is_decoded_as_audio_only_and_seekable_without_media_foundation_codec() {
    let bytes = decode_fixture(include_str!(
        "../../../../tests/fixtures/media/test-1s-audio.flac.base64"
    ));
    let decoded = super::super::decode(&bytes, MediaLimits::default()).unwrap();
    assert!(decoded.playback.is_none());
    assert_eq!(decoded.report.video_codec, MediaCodecFamily::None);
    assert_eq!(decoded.report.audio_codec, MediaCodecFamily::Flac);
    assert_eq!(decoded.report.audio_sample_rate, 44_100);
    assert_eq!(decoded.report.audio_channels, 1);
    assert!(decoded.report.audio_samples > 0);
    assert!(decoded.report.duration_100ns >= 9_000_000);
    let mut playback = AudioDecoder::open(
        &bytes,
        decoded.report.audio_codec,
        decoded.report.audio_samples,
        decoded.report.audio_sample_rate,
        decoded.report.audio_channels,
    )
    .unwrap();
    assert!(playback.next_sample().unwrap().is_some());
    playback.seek(5_000_000).unwrap();
    assert!(playback.next_sample().unwrap().is_some());
}
