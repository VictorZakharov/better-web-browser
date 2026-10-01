use super::fixture_builder::*;
use crate::opus_audio::Limits;
use base64::Engine;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[test]
fn every_byte_truncation_of_a_finite_document_is_rejected() {
    let bytes = Document::tone(1).bytes();
    for length in 0..bytes.len() {
        assert!(
            open(bytes[..length].to_vec(), Limits::default()).is_err(),
            "length={length}"
        );
    }
    assert_eq!(decode(bytes).1, 19_200);
}

#[test]
fn unknown_segment_eof_is_only_allowed_after_a_complete_finite_block() {
    let mut document = Document::tone(1);
    document.unknown_segment = true;
    let bytes = document.bytes();
    // Header-only and the middle of the last finite Cluster/Block cannot be
    // mistaken for the demuxer's normal unknown-sized Segment EOF.
    for length in [
        0,
        1,
        4,
        64,
        bytes.len() - 1,
        bytes.len() - 8,
        bytes.len() - 20,
    ] {
        assert!(open(bytes[..length].to_vec(), Limits::default()).is_err());
    }
    assert_eq!(decode(bytes).1, 19_200);
}

#[test]
fn trailing_chained_documents_and_non_webm_headers_fail_closed() {
    let document = Document::tone(1);
    let bytes = document.bytes();
    for tail in [vec![0], vec![0xff], vec![0; 8], bytes.clone()] {
        let mut chained = bytes.clone();
        chained.extend(tail);
        assert!(open(chained, Limits::default()).is_err());
    }
    for doctype in [b"matroska".as_slice(), b"WEBM", b"webm\0", b""] {
        let mut document = document.clone();
        replace(&mut document.header, 0x4282, doctype);
        rejects(document.bytes(), "DocType");
    }
    for (id, value) in [(0x42f7, 2_u64), (0x42f2, 5), (0x42f3, 9), (0x4285, 3)] {
        let mut document = document.clone();
        replace(&mut document.header, id, value.to_be_bytes());
        rejects(document.bytes(), "unsupported");
    }
}

#[test]
fn leaf_size_encoded_source_and_work_limits_apply_before_libopus() {
    let mut document = Document::tone(1);
    document
        .extra_segment
        .push(Field::new(0x7ba9, vec![0; 256 * 1024 + 1]));
    rejects(document.bytes(), "allocation limit");
    let mut document = Document::tone(1);
    document.extra_segment = vec![Field::new(0xec, []); 65_537];
    rejects(document.bytes(), "complexity limit");
    let mut document = Document::tone(1);
    document.extra_segment.push(Field::new(
        0xec,
        vec![0; crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES],
    ));
    rejects(document.bytes(), "8 MiB");
}

#[test]
fn raw_predictive_work_and_presentation_byte_limits_are_independent() {
    for channels in [1, 2] {
        let mut document = Document::tone(channels);
        // Only one sample is presented, but all 21 predictive packets still
        // have to be decoded. Trimming must not hide their CPU/PCM work budget.
        document.pre_skip(19_511);
        let bytes = document.bytes();
        let required = 20_160 * usize::from(channels) * 4;
        assert_eq!(decode(bytes.clone()).1, 1);
        for limits in [
            Limits {
                max_decoded_bytes: required - 1,
                ..Default::default()
            },
            Limits {
                max_packets: 20,
                ..Default::default()
            },
            Limits {
                max_duration_frames: 20_159,
                ..Default::default()
            },
        ] {
            assert!(open(bytes.clone(), limits).is_err());
        }
        assert!(
            open(
                bytes,
                Limits {
                    max_decoded_bytes: required,
                    max_packets: 21,
                    max_duration_frames: 20_160
                }
            )
            .is_ok()
        );
    }
    let mut document = Document::tone(1);
    document.clusters = vec![cluster(3_600_001, &[block(&document.packets[0], 0, None)])];
    rejects(document.bytes(), "byte or duration limit");
}

#[test]
fn open_and_decode_cancel_or_expire_without_resumable_native_state() {
    let bytes = Document::tone(1).bytes();
    let cancelled = AtomicBool::new(true);
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(
        super::super::Stream::open(
            bytes.clone().into(),
            Limits::default(),
            Some(&cancelled),
            deadline
        )
        .is_err()
    );
    cancelled.store(false, Ordering::Relaxed);
    assert!(
        super::super::Stream::open(
            bytes.clone().into(),
            Limits::default(),
            Some(&cancelled),
            Instant::now()
        )
        .is_err()
    );
    for cancel in [true, false] {
        let mut stream = open(bytes.clone(), Limits::default()).unwrap();
        assert!(stream.next_pcm(None, deadline).unwrap().is_some());
        cancelled.store(cancel, Ordering::Relaxed);
        let operation_deadline = if cancel { deadline } else { Instant::now() };
        assert!(
            stream
                .next_pcm(Some(&cancelled), operation_deadline)
                .is_err()
        );
        cancelled.store(false, Ordering::Relaxed);
        assert!(stream.next_pcm(Some(&cancelled), deadline).is_err());
    }
    assert_eq!(decode(bytes).1, 19_200);
}

#[test]
fn arbitrary_codec_signatures_in_comments_never_select_the_opus_decoder() {
    let mut document = Document::tone(1);
    replace(&mut document.track, 0x86, b"A_VORBIS");
    document
        .extra_segment
        .push(Field::new(0x7ba9, b"A_OPUS OpusHead"));
    assert!(!super::super::sniff(&document.bytes()));
    let independent_vorbis = base64::engine::general_purpose::STANDARD
        .decode(
            include_str!("../../../tests/fixtures/media/test-0.4s-tone.webm.base64")
                .split_whitespace()
                .collect::<String>(),
        )
        .unwrap();
    assert!(!super::super::sniff(&independent_vorbis));
    for bytes in [
        b"OpusHead".as_slice(),
        b"A_OPUS",
        &[0x1a, 0x45, 0xdf, 0xa3],
        b"OggS arbitrary A_OPUS",
        b"RIFF OpusHead",
    ] {
        assert!(!super::super::sniff(bytes));
    }
}

#[test]
fn empty_foreign_or_invalid_packets_do_not_request_loss_concealment() {
    let mut document = Document::tone(1);
    for packet in [vec![], vec![0xff], vec![0xff; 61_441]] {
        document.clusters = vec![cluster(0, &[block(&packet, 0, None)])];
        assert!(open(document.bytes(), Limits::default()).is_err());
    }
    let mut data = payload(&document.packets[0], 0, 0x80);
    data[0] = 0x82; // Track 2 does not exist.
    document.clusters = vec![cluster(0, &[Field::new(0xa3, data)])];
    assert!(open(document.bytes(), Limits::default()).is_err());
    for id in [0xa4, 0x75a1] {
        document.clusters = vec![cluster(
            0,
            &[Field::master(
                0xa0,
                &[
                    Field::new(0xa1, payload(&document.packets[0], 0, 0)),
                    Field::new(id, []),
                ],
            )],
        )];
        rejects(document.bytes(), "additions/state");
    }
}
