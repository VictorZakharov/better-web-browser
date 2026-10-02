//! Verify the compatibility adapter without allowing it to repair corrupt input.
use super::*;
use crate::engine::script::audio_codecs::test_packets::{flac_description, packets};

fn checksums(bytes: &mut [u8], header_end: usize) {
    let mut header = Crc8Ccitt::new(0);
    header.process_buf_bytes(&bytes[..header_end]);
    bytes[header_end] = header.crc();
    let end = bytes.len() - 2;
    let mut frame = Crc16Ansi::new(0);
    frame.process_buf_bytes(&bytes[..end]);
    bytes[end..].copy_from_slice(&frame.crc().to_be_bytes());
}

#[test]
fn streaminfo_bit_depth_fallback_preserves_every_integer_sample() {
    let description = flac_description();
    for packet in packets("flac") {
        let expected = decode(&description, &packet, -7).unwrap().remove(0);
        let header_end = validate_dimensions(&description, &packet).unwrap();
        let mut fallback = packet.clone();
        fallback[3] &= !14;
        checksums(&mut fallback, header_end);
        let actual = decode(&description, &fallback, -7).unwrap().remove(0);
        assert_eq!(actual.bytes, expected.bytes);
        assert_eq!(actual.frames, expected.frames);
        assert_eq!(actual.timestamp, -7);
        assert_eq!(actual.format, "s32-planar");
    }
}

#[test]
fn fallback_normalization_never_repairs_original_header_or_frame_crc() {
    let description = flac_description();
    let mut packet = packets("flac").remove(0);
    let end = validate_dimensions(&description, &packet).unwrap();
    packet[3] &= !14;
    checksums(&mut packet, end);
    for index in [4, end, end + 1, packet.len() - 2, packet.len() - 1] {
        let mut corrupt = packet.clone();
        corrupt[index] ^= 1;
        assert!(decode(&description, &corrupt, 0).is_err(), "byte {index}");
    }
}

#[test]
fn explicit_depth_frames_are_borrowed_and_do_not_get_checksum_rewritten() {
    let description = flac_description();
    let packet = packets("flac").remove(0);
    let end = validate_dimensions(&description, &packet).unwrap();
    let info = StreamInfo::read(&mut BufReader::new(&description[8..42])).unwrap();
    let code = match info.bits_per_sample {
        8 => 1,
        12 => 2,
        16 => 4,
        20 => 5,
        24 => 6,
        _ => unreachable!(),
    };
    let mut explicit = packet;
    explicit[3] = (explicit[3] & !14) | code << 1;
    checksums(&mut explicit, end);
    assert!(matches!(
        normalize_depth(&explicit, end, info.bits_per_sample).unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    let last = explicit.len() - 1;
    explicit[last] ^= 1;
    assert!(decode(&description, &explicit, 0).is_err());
}

#[test]
fn fallback_cannot_hide_crc_valid_concatenation_or_suffixes() {
    let description = flac_description();
    let mut packet = packets("flac").remove(0);
    let end = validate_dimensions(&description, &packet).unwrap();
    packet[3] &= !14;
    checksums(&mut packet, end);
    for extra in [vec![0], vec![0; 64], packet.clone()] {
        let mut oversized = packet.clone();
        oversized.extend(extra);
        assert!(decode(&description, &oversized, 0).is_err());
    }
}
