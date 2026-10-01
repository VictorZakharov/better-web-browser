use super::*;
use base64::Engine;

fn fixtures() -> [(Vec<u8>, u16); 2] {
    [
        (
            include_str!("../../../../tests/fixtures/media/test-0.4s-opus.ogg.base64"),
            1,
        ),
        (
            include_str!("../../../../tests/fixtures/media/test-0.4s-opus-stereo.ogg.base64"),
            2,
        ),
    ]
    .map(|(encoded, channels)| {
        (
            base64::engine::general_purpose::STANDARD
                .decode(encoded.lines().collect::<String>())
                .unwrap(),
            channels,
        )
    })
}

fn supported_fixtures() -> Vec<(Vec<u8>, u16)> {
    let mut fixtures = fixtures().to_vec();
    fixtures.push((
        base64::engine::general_purpose::STANDARD
            .decode(
                include_str!("../../../../tests/fixtures/media/test-0.4s-opus.webm.base64")
                    .split_whitespace()
                    .collect::<String>(),
            )
            .unwrap(),
        1,
    ));
    fixtures
}

fn collect(decoder: &mut OpusDecoder) -> Vec<u8> {
    let mut result = Vec::new();
    while let Some(bytes) = decoder.next_sample().unwrap() {
        assert!(bytes.len() <= crate::limits::MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES);
        result.extend(bytes);
    }
    result
}

fn playback(bytes: &[u8], report: MediaDecodeReport) -> OpusDecoder {
    OpusDecoder::open(
        bytes,
        report.audio_samples,
        report.audio_sample_rate,
        report.audio_channels,
    )
    .unwrap()
}

fn long_owned_source(packet_count: u64) -> (Vec<u8>, u64) {
    use ogg::{PacketReader, PacketWriteEndInfo, PacketWriter};
    let (source, _) = fixtures().into_iter().next().unwrap();
    let mut reader = PacketReader::new(std::io::Cursor::new(source));
    let head = reader.read_packet().unwrap().unwrap();
    let tags = reader.read_packet().unwrap().unwrap();
    let audio = reader.read_packet().unwrap().unwrap();
    let serial = head.stream_serial();
    let pre_skip = u64::from(u16::from_le_bytes(head.data[10..12].try_into().unwrap()));
    let packet_frames = ::opus::packet::get_nb_samples(&audio.data, SAMPLE_RATE).unwrap() as u64;
    let total_frames = packet_frames.checked_mul(packet_count).unwrap();
    let mut writer = PacketWriter::new(Vec::new());
    writer
        .write_packet(
            head.data.into_boxed_slice(),
            serial,
            PacketWriteEndInfo::EndPage,
            0,
        )
        .unwrap();
    writer
        .write_packet(
            tags.data.into_boxed_slice(),
            serial,
            PacketWriteEndInfo::EndPage,
            0,
        )
        .unwrap();
    // Reuse only a complete, self-authored native-valid Opus packet. The Ogg
    // writer owns page lacing/CRC; libopus owns packet duration/decoding.
    // Repeated segments need not form a continuous sine at their joins.
    for index in 1..=packet_count {
        let end = if index == packet_count {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::EndPage
        };
        writer
            .write_packet(
                audio.data.clone().into_boxed_slice(),
                serial,
                end,
                index * packet_frames,
            )
            .unwrap();
    }
    (writer.into_inner(), total_frames - pre_skip)
}

#[test]
fn opus_media_streams_more_than_an_audio_buffer_budget_without_whole_file_pcm_allocation() {
    let (bytes, frames) = long_owned_source(5_000);
    assert!(bytes.len() < MAX_MEDIA_ENCODED_QUEUE_BYTES);
    assert!(frames * 4 > 16 * 1024 * 1024);
    let web_limits = crate::opus_audio::Limits {
        max_decoded_bytes: 16 * 1024 * 1024,
        ..crate::opus_audio::Limits::default()
    };
    assert!(
        crate::opus_audio::Stream::open(
            Arc::from(bytes.clone()),
            web_limits,
            None,
            Instant::now() + MEDIA_COMMAND_TIMEOUT,
        )
        .is_err()
    );
    let report = decode(&bytes, MediaLimits::default(), Instant::now())
        .unwrap()
        .report;
    assert_eq!(report.audio_decoded_bytes, frames * 2);
    assert_eq!(
        report.duration_100ns,
        stream::frames_to_100ns(frames).unwrap()
    );
    let mut decoder = playback(&bytes, report);
    let mut streamed_bytes = 0_u64;
    while let Some(chunk) = decoder.next_sample().unwrap() {
        assert!(chunk.len() <= 5_760 * 2);
        streamed_bytes += chunk.len() as u64;
    }
    assert_eq!(streamed_bytes, report.audio_decoded_bytes);
    decoder.seek(report.duration_100ns / 2).unwrap();
    assert!(decoder.next_sample().unwrap().is_some());
}

#[test]
fn opus_report_and_stream_use_only_actual_mono_or_stereo_presentation_pcm() {
    for (bytes, channels) in supported_fixtures() {
        assert!(crate::opus_audio::sniff(&bytes));
        let report = super::super::decode(&bytes, MediaLimits::default())
            .unwrap()
            .report;
        assert_eq!(report.audio_codec, MediaCodecFamily::Opus);
        assert_eq!(report.video_codec, MediaCodecFamily::None);
        assert_eq!((report.video_samples, report.video_decoded_bytes), (0, 0));
        assert_eq!(
            (report.audio_sample_rate, report.audio_channels),
            (48_000, channels)
        );
        assert_eq!(report.duration_100ns, 4_000_000);
        assert_eq!(report.buffered.audio_end_100ns, report.duration_100ns);
        assert_eq!(report.audio_decoded_bytes, 19_200 * u64::from(channels) * 2);
        let pcm = collect(&mut playback(&bytes, report));
        assert_eq!(pcm.len() as u64, report.audio_decoded_bytes);
        assert!(pcm.iter().any(|byte| *byte != 0));
        for channel in 0..usize::from(channels) {
            assert!(pcm.chunks_exact(usize::from(channels) * 2).any(|frame| {
                i16::from_le_bytes(frame[channel * 2..channel * 2 + 2].try_into().unwrap())
                    .unsigned_abs()
                    > 100
            }));
        }
    }
}

#[test]
fn opus_exact_seek_returns_the_entire_original_pcm_suffix_including_after_consecutive_seeks() {
    for (bytes, channels) in supported_fixtures() {
        let report = decode(&bytes, MediaLimits::default(), Instant::now())
            .unwrap()
            .report;
        let mut decoder = playback(&bytes, report);
        let original = collect(&mut decoder);
        for position in [
            0, 1, 1_234_567, 2_000_000, 3_999_999, 4_000_000, 5_000_000, 0,
        ] {
            decoder.seek(position).unwrap();
            let offset = (position * u64::from(SAMPLE_RATE) / 10_000_000) as usize
                * usize::from(channels)
                * 2;
            assert_eq!(
                collect(&mut decoder),
                original[offset.min(original.len())..],
                "seek {position}"
            );
        }
        for position in [0, 0, 1_234_567, 0, 2_000_000] {
            decoder.seek(position).unwrap();
        }
        let offset = 9_600 * usize::from(channels) * 2;
        assert_eq!(collect(&mut decoder), original[offset..]);
        decoder.seek(0).unwrap();
        assert_eq!(collect(&mut decoder), original);
    }
}

#[test]
fn opus_errors_are_terminal_and_worker_source_report_and_time_limits_are_enforced() {
    for (bytes, _) in fixtures() {
        for end in [bytes.len() / 2, bytes.len() - 1] {
            assert!(super::super::decode(&bytes[..end], MediaLimits::default()).is_err());
        }
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(super::super::decode(&corrupt, MediaLimits::default()).is_err());
        let mut chained = bytes.clone();
        chained.extend_from_slice(&bytes);
        assert!(super::super::decode(&chained, MediaLimits::default()).is_err());
        assert!(
            decode(
                &bytes,
                MediaLimits {
                    max_encoded_bytes: 1,
                    ..MediaLimits::default()
                },
                Instant::now()
            )
            .is_err()
        );
        assert!(
            decode(
                &bytes,
                MediaLimits::default(),
                Instant::now() - MEDIA_COMMAND_TIMEOUT - std::time::Duration::from_millis(1)
            )
            .is_err()
        );
        let report = decode(&bytes, MediaLimits::default(), Instant::now())
            .unwrap()
            .report;
        assert!(OpusDecoder::open(&bytes, 0, 48_000, report.audio_channels).is_err());
        assert!(
            OpusDecoder::open(&bytes, report.audio_samples, 44_100, report.audio_channels).is_err()
        );
        assert!(OpusDecoder::open(&bytes, report.audio_samples, 48_000, 3).is_err());
        let mut wrong_count = OpusDecoder::open(
            &bytes,
            report.audio_samples + 1,
            48_000,
            report.audio_channels,
        )
        .unwrap();
        loop {
            match wrong_count.next_sample() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("Opus accepted an inflated report sample count"),
                Err(error) => {
                    assert!(error.contains("before decoded report"), "{error}");
                    break;
                }
            }
        }
        assert!(
            super::super::AudioDecoder::open(
                &bytes,
                MediaCodecFamily::Vorbis,
                report.audio_samples,
                48_000,
                report.audio_channels
            )
            .is_err()
        );
    }
}
