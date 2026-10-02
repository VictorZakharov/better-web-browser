use super::*;

#[test]
fn support_queries_do_not_claim_aac_profiles_the_backend_cannot_decode() {
    for codec in [
        "mp4a.40.5",
        "mp4a.40.05",
        "mp4a.40.29",
        "mp4a.40.42",
        "aac",
        "MP3",
        "mp4a.6b",
    ] {
        assert!(!supported(codec), "{codec}");
    }
    for description in [
        vec![],
        vec![0],
        vec![0x0a, 0x08],
        vec![0x1a, 0x08],
        vec![0x12, 0x38],
        vec![0x12, 0x00],
        vec![0x12, 0x0c],
        vec![0; 65],
    ] {
        assert!(aac::parameters(&description).is_err(), "{description:?}");
    }
}

#[test]
fn flac_description_requires_complete_single_initial_streaminfo() {
    let good = flac_description();
    assert!(flac::parameters(Some(&good)).is_ok());
    assert!(flac::parameters(None).is_err());
    for end in 0..good.len() {
        assert!(flac::parameters(Some(&good[..end])).is_err(), "{end}");
    }
    for index in [0, 4, 7] {
        let mut mutated = good.clone();
        mutated[index] ^= 1;
        assert!(flac::parameters(Some(&mutated)).is_err());
    }
    let mut trailing = good.clone();
    trailing.push(0);
    assert!(flac::parameters(Some(&trailing)).is_err());
    let mut duplicate = good.clone();
    duplicate[4] = 0;
    duplicate.extend_from_slice(&good[4..]);
    assert!(flac::parameters(Some(&duplicate)).is_err());
}

#[test]
fn optional_flac_metadata_is_bounded_and_not_decoded_as_frames() {
    let mut bytes = flac_description();
    bytes[4] = 0;
    bytes.extend_from_slice(&[0x81, 0, 0, 2, 0, 0]);
    assert!(flac::parameters(Some(&bytes)).is_ok());
    bytes[43..46].fill(255);
    assert!(flac::parameters(Some(&bytes)).is_err());
    let mut blocks = flac_description();
    blocks[4] = 0;
    for _ in 0..256 {
        blocks.extend_from_slice(&[1, 0, 0, 0]);
    }
    blocks.extend_from_slice(&[0x81, 0, 0, 0]);
    assert!(flac::parameters(Some(&blocks)).is_err());
}

#[test]
fn aac_framing_rejects_multiple_packets_protected_crc_and_channel_expansion() {
    let packet = adts_packets().remove(0);
    assert!(aac::adts(&packet).is_ok());
    let mut duplicate = packet.clone();
    duplicate.extend_from_slice(&packet);
    assert!(aac::adts(&duplicate).is_err());
    let mut protected = packet.clone();
    protected[1] &= !1;
    assert!(aac::adts(&protected).is_err());
    let mut surround = packet.clone();
    surround[2] |= 1;
    assert!(aac::adts(&surround).is_err());
}

#[test]
fn mp3_framing_rejects_reserved_and_other_mpeg_layers() {
    let packet = packets("mp3").remove(0);
    assert!(mp3::header(&packet).is_ok());
    for (index, mask, bits) in [
        (1, 6, 0),
        (1, 6, 4),
        (1, 6, 6),
        (1, 24, 8),
        (2, 0xf0, 0),
        (2, 0xf0, 0xf0),
        (2, 12, 12),
        (3, 3, 2),
    ] {
        let mut bad = packet.clone();
        bad[index] = (bad[index] & !mask) | bits;
        assert!(mp3::header(&bad).is_err(), "{index} {mask} {bits}");
    }
    let mut trailing = packet.clone();
    trailing.push(0);
    assert!(mp3::header(&trailing).is_err());
}

#[test]
fn initial_lame_delay_is_read_only_from_the_registered_xing_position() {
    let mut packet = packets("mp3").remove(0);
    let header = mp3::header(&packet).unwrap();
    let side = if header.mpeg1 {
        if header.mono { 17 } else { 32 }
    } else if header.mono {
        9
    } else {
        17
    };
    let offset = 4 + usize::from(header.protected) * 2 + side;
    packet[offset..].fill(0);
    packet[offset..offset + 4].copy_from_slice(b"Xing");
    packet[offset + 8..offset + 12].copy_from_slice(b"LAME");
    packet[offset + 29..offset + 32].copy_from_slice(&[0x24, 0x00, 0]);
    assert_eq!(mp3::priming(&packet, &header).unwrap(), 576);
    packet[offset..offset + 4].copy_from_slice(b"Info");
    assert_eq!(mp3::priming(&packet, &header).unwrap(), 576);
    packet[offset..offset + 4].copy_from_slice(b"Nope");
    assert_eq!(mp3::priming(&packet, &header).unwrap(), 0);
}
