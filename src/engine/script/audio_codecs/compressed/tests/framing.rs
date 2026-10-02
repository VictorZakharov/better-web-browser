use super::*;

#[test]
fn every_aac_adts_prefix_and_suffix_is_rejected_as_an_incomplete_packet() {
    let packet = adts_packets().remove(1);
    for end in 0..packet.len() {
        assert!(aac::adts(&packet[..end]).is_err(), "prefix {end}");
    }
    for start in 1..packet.len() {
        assert!(aac::adts(&packet[start..]).is_err(), "suffix {start}");
    }
    for suffix in [vec![0], vec![255], vec![255, 241], vec![0; 100]] {
        let mut bad = packet.clone();
        bad.extend(suffix);
        assert!(aac::adts(&bad).is_err());
    }
}

#[test]
fn every_mp3_prefix_and_suffix_is_rejected_before_reservoir_mutation() {
    let packet = packets("mp3").remove(1);
    for end in 0..packet.len() {
        assert!(mp3::header(&packet[..end]).is_err(), "prefix {end}");
    }
    for start in 1..packet.len() {
        assert!(mp3::header(&packet[start..]).is_err(), "suffix {start}");
    }
    let mut duplicate = packet.clone();
    duplicate.extend_from_slice(&packet);
    assert!(mp3::header(&duplicate).is_err());
}

#[test]
fn empty_raw_aac_is_not_packet_loss_concealment_or_a_flush_token() {
    let mut decoder = Decoder::new(&config("mp4a.40.2", Some(vec![18, 8]))).unwrap();
    assert!(decoder.decode(&[], 0).is_err());
    for packet in [
        vec![255; 8],
        vec![0; 1],
        vec![255, 241, 80, 64, 1, 31, 252, 0],
    ] {
        let mut decoder = Decoder::new(&config("mp4a.40.2", Some(vec![18, 8]))).unwrap();
        assert!(decoder.decode(&packet, 0).is_err());
    }
}

#[test]
fn aac_description_does_not_switch_to_adts_when_raw_packet_decode_fails() {
    let packet = adts_packets().remove(0);
    let mut raw = Decoder::new(&config("mp4a.40.2", Some(vec![18, 8]))).unwrap();
    assert!(raw.decode(&packet, 0).is_err());
    let packet = packets("aac").remove(0);
    let mut adts = Decoder::new(&config("mp4a.40.2", None)).unwrap();
    assert!(adts.decode(&packet, 0).is_err());
}

#[test]
fn aac_midstream_dimension_changes_require_explicit_reconfiguration() {
    let packet = adts_packets().remove(0);
    let mut decoder = Decoder::new(&config("mp4a.40.2", None)).unwrap();
    decoder.decode(&packet, 0).unwrap();
    for (index, value) in [(2, 0x4c), (3, 0x80)] {
        let mut changed = packet.clone();
        changed[index] = value;
        let error = decoder.decode(&changed, 1000).unwrap_err();
        assert!(error.contains("dimensions changed"), "{error}");
    }
}

#[test]
fn aac_layer_sync_and_raw_block_count_are_not_best_effort_guessed() {
    let packet = adts_packets().remove(0);
    for (index, clear, set) in [
        (0, 255, 254),
        (1, 6, 2),
        (1, 6, 4),
        (1, 6, 6),
        (2, 60, 60),
        (2, 192, 0),
        (2, 192, 128),
        (2, 192, 192),
        (6, 3, 1),
        (6, 3, 2),
        (6, 3, 3),
    ] {
        let mut bad = packet.clone();
        bad[index] = (bad[index] & !clear) | set;
        assert!(aac::adts(&bad).is_err(), "{index} {clear} {set}");
    }
}

#[test]
fn flac_frame_dimensions_are_checked_before_entering_the_i32_decoder() {
    let description = flac_description();
    let good = packets("flac").remove(0);
    for code in [3, 7] {
        let mut packet = good.clone();
        packet[3] = (packet[3] & !14) | (code << 1);
        let error = flac::validate_packet(&description, &packet).unwrap_err();
        assert!(error.contains("bit depth"), "{error}");
    }
    for assignment in 1..=15 {
        let mut packet = good.clone();
        packet[3] = (packet[3] & 15) | (assignment << 4);
        let error = flac::validate_packet(&description, &packet).unwrap_err();
        assert!(
            error.contains("dimensions") || error.contains("channel assignment"),
            "{error}"
        );
    }
    let mut rate = good.clone();
    rate[2] = (rate[2] & 240) | 10;
    let error = flac::validate_packet(&description, &rate).unwrap_err();
    assert!(error.contains("sample rate disagrees"), "{error}");
}

#[test]
fn flac_parser_does_not_ignore_corrupted_trailing_bytes_after_a_valid_frame() {
    let description = flac_description();
    let good = packets("flac").remove(0);
    for suffix in [vec![0], vec![255], vec![255, 248], vec![0; 32]] {
        let mut packet = good.clone();
        packet.extend(suffix);
        assert!(flac::validate_packet(&description, &packet).is_err());
    }
}

#[test]
fn initial_xing_header_optional_fields_are_bounded_before_delay_access() {
    let mut bytes = packets("mp3").remove(0);
    let head = mp3::header(&bytes).unwrap();
    let offset = 4
        + usize::from(head.protected) * 2
        + if head.mpeg1 {
            if head.mono { 17 } else { 32 }
        } else if head.mono {
            9
        } else {
            17
        };
    bytes[offset..offset + 4].copy_from_slice(b"Xing");
    bytes[offset + 4..offset + 8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(
        mp3::priming(&bytes, &head)
            .unwrap_err()
            .contains("reserved bits")
    );
    bytes[offset + 4..offset + 8].copy_from_slice(&15_u32.to_be_bytes());
    for end in offset + 4..offset + 8 + 112 {
        assert!(mp3::priming(&bytes[..end], &head).is_err(), "{end}");
    }
}
