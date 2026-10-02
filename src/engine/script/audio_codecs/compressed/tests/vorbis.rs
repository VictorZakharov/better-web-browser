//! Vorbis metadata reuses the established setup allocation preflight.
use super::super::super::test_packets::vorbis_description;
use super::*;
use std::io::Cursor;

#[test]
fn vorbis_decodes_actual_packets_with_one_warmup_packet_and_real_output_dimensions() {
    let cfg = config("vorbis", Some(vorbis_description()));
    assert_tone(cfg, &packets("ogg"), 88200);
}

#[test]
fn vorbis_requires_xiph_laced_description_not_an_ogg_file_or_concatenated_headers() {
    assert!(super::super::vorbis::parameters(None).is_err());
    for bytes in [
        vec![],
        vec![2],
        vec![2, 255],
        vec![2, 255, 255],
        fixture("ogg"),
        vec![0; 65_537],
    ] {
        assert!(super::super::vorbis::parameters(Some(&bytes)).is_err());
    }
    let good = vorbis_description();
    assert!(super::super::vorbis::parameters(Some(&good)).is_ok());
    for end in 0..16 {
        assert!(super::super::vorbis::parameters(Some(&good[..end])).is_err());
    }
}

#[test]
fn vorbis_header_packets_cannot_be_used_as_encoded_audio_chunks() {
    let mut packets = ogg::reading::PacketReader::new(Cursor::new(fixture("ogg")));
    let cfg = config("vorbis", Some(vorbis_description()));
    for _ in 0..3 {
        let header = packets.read_packet().unwrap().unwrap().data;
        let error = Decoder::new(&cfg).unwrap().decode(&header, 0).unwrap_err();
        assert!(error.contains("not a header packet"), "{error}");
    }
}

#[test]
fn vorbis_identification_dimensions_are_checked_before_allocating_setup_codebooks() {
    let good = vorbis_description();
    // Owned identification/comment lengths are both below 255, so the first
    // header begins at byte three. Check this independently before mutation.
    assert_eq!(&good[3..10], b"\x01vorbis");
    for channels in [0, 9, 255] {
        let mut description = good.clone();
        description[14] = channels;
        assert!(
            super::super::vorbis::parameters(Some(&description)).is_err(),
            "{channels}"
        );
    }
    for rate in [0_u32, 7999, 192001, u32::MAX] {
        let mut description = good.clone();
        description[15..19].copy_from_slice(&rate.to_le_bytes());
        assert!(
            super::super::vorbis::parameters(Some(&description)).is_err(),
            "{rate}"
        );
    }
}

#[test]
fn malformed_vorbis_packets_and_container_bytes_never_return_placeholder_samples() {
    let cfg = config("vorbis", Some(vorbis_description()));
    for bytes in [vec![], vec![255], fixture("ogg")] {
        assert!(Decoder::new(&cfg).unwrap().decode(&bytes, 0).is_err());
    }
}

#[test]
fn silent_vorbis_warmup_packet_does_not_invent_output_samples() {
    let cfg = config("vorbis", Some(vorbis_description()));
    // A valid short-mode packet can establish overlap state without output.
    // Lack of output on the first packet is not a decoding failure.
    assert!(
        Decoder::new(&cfg)
            .unwrap()
            .decode(&[2], 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn independent_vorbis_decoders_do_not_share_overlap_history() {
    let cfg = config("vorbis", Some(vorbis_description()));
    let source = packets("ogg");
    let mut first = Decoder::new(&cfg).unwrap();
    let mut second = Decoder::new(&cfg).unwrap();
    assert!(first.decode(&source[0], 0).unwrap().is_empty());
    let first_pcm = first.decode(&source[1], 1000).unwrap().remove(0);
    assert!(second.decode(&source[0], 0).unwrap().is_empty());
    let second_pcm = second.decode(&source[1], 1000).unwrap().remove(0);
    assert_eq!(first_pcm.bytes, second_pcm.bytes);
    assert_eq!(first_pcm.frames, second_pcm.frames);
    assert_eq!(first_pcm.sample_rate, second_pcm.sample_rate);
}
