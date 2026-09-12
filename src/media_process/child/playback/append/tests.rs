use super::*;
use crate::media_data_protocol::MediaDataWriter;
use crate::media_protocol::{MediaSessionId, Nonce};
use std::io::Cursor;

const INVALID_MEDIA: &[u8] = b"not an MP4 segment";

#[test]
fn frame_decode_failure_retires_only_the_source_and_consumes_the_request() {
    let mut bytes = fixture(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/media/test-1s-video-fragmented.mp4.base64"
    )));
    let mdat = bytes.windows(4).position(|value| value == b"mdat").unwrap();
    // Preserve container metadata so rejection occurs in the production frame decoder.
    bytes[mdat + 4..].fill(0);
    let decoder = backend::decode_append(&bytes, &[], MediaLimits::default())
        .unwrap()
        .video
        .unwrap();
    let mut playback = Playback {
        active: Some((1, decoder)),
        last_source_id: 1,
        ..Playback::new(true)
    };
    let session = MediaSessionId::new(1).unwrap();
    let nonce = Nonce::new([7; 32]);
    let mut pixels = Vec::new();
    let mut control = Vec::new();
    playback
        .request_frame(
            1,
            1,
            &mut DecodedFrameWriter::new(&mut pixels, session, nonce),
            &mut MediaFrameWriter::new(&mut control, session),
        )
        .expect("codec error is not a protocol exit");
    assert!(pixels.is_empty());
    assert!(playback.active.is_none());
    assert!(playback.audio.is_none());
    assert_eq!(playback.last_frame_id, 1);
    let mut reader = crate::media_protocol::MediaFrameReader::new(Cursor::new(control), session);
    assert!(matches!(
        reader.read_worker().unwrap(),
        WorkerMediaMessage::DecodeFailed { request_id: 1, .. }
    ));

    // A later valid source continues the same protocol session and frame sequence.
    playback.active = self::playback().active.map(|(_, decoder)| (2, decoder));
    let mut control = Vec::new();
    playback
        .request_frame(
            2,
            2,
            &mut DecodedFrameWriter::new(&mut pixels, session, nonce),
            &mut MediaFrameWriter::new(&mut control, session),
        )
        .unwrap();
    assert!(!pixels.is_empty());
    assert_eq!(playback.pending.as_ref().unwrap().0.frame_id, 2);
    assert!(
        playback
            .request_frame(
                2,
                3,
                &mut DecodedFrameWriter::new(Vec::new(), session, nonce),
                &mut MediaFrameWriter::new(Vec::new(), session)
            )
            .unwrap_err()
            .contains("before acknowledging")
    );
}

#[test]
fn rejected_append_payload_consumes_transfer_ids_without_replacing_playback() {
    let mut playback = playback();
    for video_source_id in [2, 4] {
        let mut reader = transfer(video_source_id, 7);
        let error = playback
            .append_tracks(
                1,
                video_source_id,
                video_source_id + 1,
                INVALID_MEDIA.len() as u64,
                0,
                &mut reader,
                MediaLimits::default(),
            )
            .unwrap_err();
        assert!(!error.contains("stale"), "{error}");
        assert!(!error.contains("read appended"), "{error}");
        assert_eq!(playback.last_source_id, video_source_id + 1);
        assert_eq!(playback.active.as_ref().unwrap().0, 1);
        assert_eq!(playback.encoded_bytes, 10);
    }
}

#[test]
fn stale_append_or_invalid_transfer_does_not_consume_source_generations() {
    let mut playback = playback();
    let mut reader = transfer(2, 7);
    let stale = playback
        .append_tracks(
            1,
            3,
            4,
            INVALID_MEDIA.len() as u64,
            0,
            &mut reader,
            MediaLimits::default(),
        )
        .unwrap_err();
    assert!(stale.contains("stale adaptive append generation"));
    assert_eq!(playback.last_source_id, 1);

    // The rejected command must not have consumed any data; its valid successor can
    // still read the source and reach the recoverable media-payload rejection.
    let rejected = playback
        .append_tracks(
            1,
            2,
            3,
            INVALID_MEDIA.len() as u64,
            0,
            &mut reader,
            MediaLimits::default(),
        )
        .unwrap_err();
    assert!(!rejected.contains("read appended"), "{rejected}");
    assert_eq!(playback.last_source_id, 3);

    let mut invalid_nonce = transfer(4, 8);
    let malformed = playback
        .append_tracks(
            1,
            4,
            5,
            INVALID_MEDIA.len() as u64,
            0,
            &mut invalid_nonce,
            MediaLimits::default(),
        )
        .unwrap_err();
    assert!(malformed.starts_with("read appended video source:"));
    assert_eq!(playback.last_source_id, 3);
    assert_eq!(playback.active.as_ref().unwrap().0, 1);
}

fn transfer(source_id: u64, nonce: u8) -> MediaDataReader<Cursor<Vec<u8>>> {
    let session = MediaSessionId::new(1).unwrap();
    let mut wire = Vec::new();
    MediaDataWriter::new(&mut wire, session, Nonce::new([nonce; 32]))
        .send_source(MediaSourceId::new(source_id).unwrap(), INVALID_MEDIA)
        .unwrap();
    MediaDataReader::new(Cursor::new(wire), session, Nonce::new([7; 32]))
}

fn playback() -> Playback {
    let video = fixture(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/media/test-1s-video-fragmented.mp4.base64"
    )));
    // Fragmented decoders open lazily. No frame is decoded, no worker is launched,
    // and this fixture never creates an audio device or starts playback.
    let active_video = backend::decode_append(&video, &[], MediaLimits::default())
        .unwrap()
        .video
        .unwrap();
    Playback {
        last_source_id: 1,
        active: Some((1, active_video)),
        encoded_bytes: 10,
        ..Playback::new(true)
    }
}

fn fixture(encoded: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let (mut accumulator, mut bits) = (0_u32, 0_u32);
    for byte in encoded.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => panic!("invalid owned fixture encoding"),
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
        }
    }
    output
}
