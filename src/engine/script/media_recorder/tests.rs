use super::*;
use std::io::Cursor;

fn pcm(samples: &[i16]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

fn decoded(bytes: Vec<u8>) -> Vec<i32> {
    claxon::FlacReader::new(Cursor::new(bytes))
        .expect("FLAC header and frame must decode")
        .samples()
        .map(|sample| sample.expect("lossless PCM sample"))
        .collect()
}

#[test]
fn chunks_form_one_lossless_variable_block_stream() {
    let mut recorders = MediaRecorders::default();
    let id = recorders.open("flac", 128_000, false).unwrap();
    let first: Vec<i16> = (0..160).map(|n| (n * 127 - 8000) as i16).collect();
    let second: Vec<i16> = (0..160).map(|n| (8000 - n * 83) as i16).collect();
    let mut output = recorders.append(id, 8_000, 1, &pcm(&first)).unwrap();
    assert_eq!(&output[..4], b"fLaC");
    output.extend(recorders.append(id, 8_000, 1, &pcm(&second)).unwrap());
    output.extend(recorders.finish(id).unwrap());
    let expected = first
        .into_iter()
        .chain(second)
        .map(i32::from)
        .collect::<Vec<_>>();
    assert_eq!(decoded(output), expected);
}

#[test]
fn last_frame_preserves_one_to_fifteen_samples_without_padding() {
    for tail in 1..16 {
        let mut recorders = MediaRecorders::default();
        let id = recorders.open("flac", 128_000, false).unwrap();
        let first = vec![4096_i16; 32];
        let last = (0..tail).map(|n| -3000 + n as i16).collect::<Vec<_>>();
        let mut output = recorders.append(id, 8_000, 1, &pcm(&first)).unwrap();
        output.extend(recorders.append(id, 8_000, 1, &pcm(&last)).unwrap());
        output.extend(recorders.finish(id).unwrap());
        let expected = first
            .into_iter()
            .chain(last)
            .map(i32::from)
            .collect::<Vec<_>>();
        assert_eq!(decoded(output), expected, "tail length {tail}");
    }
}

#[test]
fn a_single_short_capture_packet_is_decodable() {
    let mut recorders = MediaRecorders::default();
    let id = recorders.open("flac", 128_000, false).unwrap();
    let mut output = recorders
        .append(id, 48_000, 2, &pcm(&[1, -2, 3, -4, 5, -6]))
        .unwrap();
    output.extend(recorders.finish(id).unwrap());
    assert_eq!(decoded(output), [1, -2, 3, -4, 5, -6]);
}

#[test]
fn capture_input_and_session_count_are_bounded() {
    let mut recorders = MediaRecorders::default();
    let ids = (0..MAX_RECORDERS)
        .map(|_| recorders.open("flac", 128_000, false).unwrap())
        .collect::<Vec<_>>();
    assert!(recorders.open("opus", 128_000, false).is_err());
    let id = ids[0];
    assert!(recorders.append(id, 8_000, 1, &vec![0; 961 * 2]).is_err());
    recorders.append(id, 8_000, 1, &pcm(&[1, 2, 3])).unwrap();
    assert!(recorders.append(id, 48_000, 1, &pcm(&[4])).is_err());
    recorders.cancel(id);
    assert!(recorders.append(id, 8_000, 1, &pcm(&[5])).is_err());
    assert!(!recorders.format_supported(id, 48_000, 1));
    let opus = recorders.open("opus", 128_000, false).unwrap();
    assert!(recorders.format_supported(opus, 48_000, 2));
    assert!(!recorders.format_supported(opus, 44_100, 1));
    recorders.finish(opus).unwrap();
    assert!(recorders.finish(opus).is_err());
    assert!(recorders.open("unsupported", 128_000, false).is_err());
}

#[test]
fn codec_target_bitrate_is_real_bounded_and_flac_default_remains_unchanged() {
    assert_eq!(target_bitrate("opus", 0), 500);
    assert_eq!(target_bitrate("opus", u32::MAX), 512_000);
    assert_eq!(target_bitrate("opus", 96_000), 96_000);
    assert_eq!(target_bitrate("flac", 0), 0);
    assert_eq!(target_bitrate("flac", 128_000), 128_000);
}
