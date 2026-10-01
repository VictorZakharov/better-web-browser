use super::super::mux::Mux;
use super::fixture_builder::*;

#[test]
fn incremental_mux_preserves_exact_channels_tail_and_single_document() {
    for channels in [1, 2] {
        let document = Document::tone(channels);
        let expected = decode(document.bytes());
        for drain_each_packet in [false, true] {
            let mut mux = Mux::new(&document.head, 0).unwrap();
            let mut chunks = Vec::new();
            let end = 19_200 + 312;
            for (index, packet) in document.packets.iter().enumerate() {
                mux.packet(packet, (index + 1 == document.packets.len()).then_some(end))
                    .unwrap();
                if drain_each_packet {
                    let part = mux.drain().unwrap();
                    assert!(part.len() < 4_500);
                    assert!(mux.bytes().is_empty());
                    chunks.push(part);
                }
            }
            chunks.push(mux.drain().unwrap());
            assert!(mux.drain().unwrap().is_empty());
            let output = decode(chunks.concat());
            assert_eq!((output.0, output.1), (channels, 19_200));
            assert_eq!(output.2, expected.2);
            assert!(mux.packet(&document.packets[0], None).is_err());
        }
    }
}

#[test]
fn writer_refuses_malformed_headers_and_invalid_final_endpoints() {
    let document = Document::tone(1);
    for head in [vec![], b"OpusHead".to_vec(), vec![0; 19], vec![0; 257]] {
        assert!(Mux::new(&head, 1).is_err());
    }
    for final_granule in [0, 311, 961, u64::MAX] {
        let mut mux = Mux::new(&document.head, 1).unwrap();
        let before = mux.bytes().to_vec();
        assert!(
            mux.packet(&document.packets[0], Some(final_granule))
                .is_err()
        );
        assert_eq!(mux.bytes(), before);
        assert!(mux.packet(&document.packets[0], Some(960)).is_err());
    }
    let mut mux = Mux::new(&document.head, 1).unwrap();
    mux.packet(&document.packets[0], None).unwrap();
    let before = mux.bytes().to_vec();
    assert!(mux.packet(&document.packets[1], Some(959)).is_err());
    assert_eq!(mux.bytes(), before);
}

#[test]
fn invalid_packet_failure_is_terminal_and_never_serializes_a_partial_cluster() {
    let document = Document::tone(1);
    for packet in [vec![], vec![0; 4_001], vec![0xff]] {
        let mut mux = Mux::new(&document.head, 1).unwrap();
        let before = mux.bytes().to_vec();
        assert!(mux.packet(&packet, None).is_err());
        assert_eq!(mux.bytes(), before);
        assert!(mux.packet(&document.packets[0], None).is_err());
    }
    // A valid 10ms packet is not the recorder's configured 20ms packet shape.
    let mut encoder =
        opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Audio).unwrap();
    let mut packet = vec![0; 4_000];
    let len = encoder.encode(&[0; 480], &mut packet).unwrap();
    packet.truncate(len);
    let mut mux = Mux::new(&document.head, 1).unwrap();
    assert!(mux.packet(&packet, None).is_err());
}

#[test]
fn emitted_metadata_carries_real_encoder_delay_preroll_and_input_rate() {
    let document = Document::tone(2);
    let mut mux = Mux::new(&document.head, 71).unwrap();
    for (index, packet) in document.packets.iter().enumerate() {
        mux.packet(
            packet,
            (index + 1 == document.packets.len()).then_some(19_512),
        )
        .unwrap();
    }
    let bytes = mux.drain().unwrap();
    let metadata =
        super::super::metadata::inspect(&bytes, &mut crate::encoded_audio::Budget::new(None))
            .unwrap();
    assert_eq!(metadata.number, 1);
    assert_eq!(metadata.scale_ns, 1_000_000);
    assert!(metadata.unknown_segment);
    assert_eq!(
        (
            metadata.head.channels,
            metadata.head.pre_skip,
            metadata.head.input_rate,
            metadata.head.gain
        ),
        (2, 312, 48_000, 0)
    );
    assert_eq!(metadata.blocks.len(), document.packets.len());
    for (index, block) in metadata.blocks.iter().enumerate() {
        assert_eq!(block.frames, 1);
        assert_eq!(block.time_ticks, index as i128 * 20);
        assert_eq!(
            block.padding_ns,
            if index + 1 == document.packets.len() {
                13_500_000
            } else {
                0
            }
        );
    }
}
