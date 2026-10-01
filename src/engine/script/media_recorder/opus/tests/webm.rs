use super::*;

#[test]
fn webm_native_rates_and_channel_counts_preserve_exact_encoder_tail() {
    for rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
        for channels in [1, 2] {
            for tail in [1, 17, rate / 50 - 1, rate / 50, rate / 50 + 1] {
                let mut session = Session::new_webm(71, 128_000, false);
                let mut bytes = Vec::new();
                for part in pcm(rate, channels, tail, 0).chunks(960 * channels * 2) {
                    bytes.extend(session.append(rate, channels, part).unwrap());
                }
                bytes.extend(session.finish().unwrap());
                assert!(crate::webm_opus::sniff(&bytes));
                let (actual_channels, frames, output) = decode(bytes);
                assert_eq!(actual_channels as usize, channels);
                assert_eq!(
                    frames,
                    (tail * (48_000 / rate)) as u64,
                    "{rate}/{channels}/{tail}"
                );
                assert!(output.iter().all(|value| value.is_finite()));
            }
        }
    }
}

#[test]
fn webm_output_chunks_do_not_retain_pcm_or_a_whole_segment() {
    let mut session = Session::new_webm(31, 96_000, false);
    let mut chunks = Vec::new();
    for offset in (0..9_600).step_by(960) {
        let bytes = session
            .append(48_000, 2, &pcm(48_000, 2, 960, offset))
            .unwrap();
        assert!(session.encoder.as_ref().unwrap().writer.inner().is_empty());
        assert!(bytes.len() < 4_500);
        assert!(session.encoder.as_ref().unwrap().pending.is_empty());
        chunks.push(bytes);
    }
    chunks.push(session.finish().unwrap());
    let (channels, frames, output) = decode(chunks.concat());
    assert_eq!((channels, frames), (2, 9_600));
    assert!(output.iter().any(|sample| sample.abs() > 0.1));
    assert!(
        output
            .chunks_exact(2)
            .any(|pair| (pair[0] - pair[1]).abs() > 0.1)
    );
}

#[test]
fn webm_and_ogg_recorders_share_predictive_pcm_not_just_capability_strings() {
    for channels in [1, 2] {
        for constant in [true, false] {
            let mut ogg = Session::new(9, 64_000, constant);
            let mut webm = Session::new_webm(9, 64_000, constant);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            for offset in (0..7_680).step_by(960) {
                let data = pcm(48_000, channels, 960, offset);
                a.extend(ogg.append(48_000, channels, &data).unwrap());
                b.extend(webm.append(48_000, channels, &data).unwrap());
            }
            a.extend(ogg.finish().unwrap());
            b.extend(webm.finish().unwrap());
            let a = decode(a);
            let b = decode(b);
            assert_eq!((a.0, a.1), (b.0, b.1));
            assert_eq!(a.2, b.2, "container must not change libopus output");
        }
    }
}

#[test]
fn webm_failure_and_format_change_are_terminal() {
    for (rate, channels, data) in [
        (44_100, 1, vec![0; 4]),
        (48_000, 3, vec![0; 6]),
        (48_000, 1, vec![0]),
        (48_000, 1, vec![]),
        (48_000, 2, vec![0; 3]),
    ] {
        let mut session = Session::new_webm(7, 64_000, false);
        assert!(session.append(rate, channels, &data).is_err());
        assert!(session.append(48_000, 1, &[0; 2]).is_err());
        assert!(session.finish().is_err());
    }
    let mut session = Session::new_webm(8, 64_000, false);
    session.append(48_000, 1, &[0; 2]).unwrap();
    assert!(!session.format_supported(16_000, 1));
    assert!(session.append(16_000, 1, &[0; 2]).is_err());
    assert!(session.finish().is_err());
    let mut empty = Session::new_webm(9, 64_000, false);
    assert!(empty.finish().unwrap().is_empty());
    assert!(empty.finish().is_err());
}

#[test]
fn webm_limits_reserve_final_discard_padding_without_mutating_failed_append() {
    for which in 0..3 {
        let mut session = Session::new_webm(7, 64_000, false);
        session.append(48_000, 1, &pcm(48_000, 1, 960, 0)).unwrap();
        let encoder = session.encoder.as_mut().unwrap();
        match which {
            0 => encoder.packets = MAX_PACKETS,
            1 => encoder.input_frames = MAX_DURATION_FRAMES,
            _ => encoder.encoded_bytes = crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES,
        }
        let before = (
            encoder.input_frames,
            encoder.pending.clone(),
            encoder.encoded_frames,
            encoder.packets,
        );
        assert!(
            session
                .append(48_000, 1, &pcm(48_000, 1, 960, 960))
                .is_err()
        );
        let encoder = session.encoder.as_ref().unwrap();
        assert_eq!(
            (
                encoder.input_frames,
                encoder.pending.clone(),
                encoder.encoded_frames,
                encoder.packets,
            ),
            before
        );
        assert!(session.blocked);
    }
}
