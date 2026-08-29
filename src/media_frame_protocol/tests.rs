use super::codec::TEST_HEADER_LENGTH;
use super::*;
use crate::limits::MAX_MEDIA_DATA_CHUNK_BYTES;
use crate::media_protocol::{MEDIA_MAGIC, MediaSessionId, Nonce};
use std::io::Cursor;

fn session(value: u64) -> MediaSessionId {
    MediaSessionId::new(value).unwrap()
}

fn nonce(value: u8) -> Nonce {
    Nonce::new([value; 32])
}

fn metadata(width: u32, height: u32, stride: u32) -> MediaVideoFrameMetadata {
    MediaVideoFrameMetadata {
        source_id: 3,
        frame_id: 5,
        timestamp_100ns: 0,
        duration_100ns: 333_333,
        width,
        height,
        stride,
        format: MediaPixelFormat::Nv12,
        data_length: u64::from(stride) * u64::from(height + height / 2),
    }
}

#[test]
fn multi_chunk_frame_round_trips_with_independent_magic() {
    let metadata = metadata(512, 512, 512);
    let bytes = vec![0x80; metadata.data_length as usize];
    assert!(bytes.len() > MAX_MEDIA_DATA_CHUNK_BYTES);
    let mut wire = Vec::new();
    MediaFrameWriter::new(&mut wire, session(2), nonce(7))
        .send_frame(metadata, &bytes)
        .unwrap();
    assert_eq!(&wire[..4], b"BRV1");
    assert_ne!(&wire[..4], &MEDIA_MAGIC);
    let frame = MediaFrameReader::new(Cursor::new(wire), session(2), nonce(7))
        .read_frame(3, 5)
        .unwrap();
    assert_eq!(frame.metadata, metadata);
    assert_eq!(frame.nv12, bytes);
}

#[test]
fn stale_nonce_session_source_frame_and_offset_fail_closed() {
    let metadata = metadata(512, 512, 512);
    let bytes = vec![0x80; metadata.data_length as usize];
    let mut wire = Vec::new();
    MediaFrameWriter::new(&mut wire, session(2), nonce(7))
        .send_frame(metadata, &bytes)
        .unwrap();
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(wire.clone()), session(3), nonce(7)).read_frame(3, 5),
        Err(MediaFrameError::WrongSession { .. })
    ));
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(wire.clone()), session(2), nonce(8)).read_frame(3, 5),
        Err(MediaFrameError::WrongNonce)
    ));
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(wire.clone()), session(2), nonce(7)).read_frame(4, 5),
        Err(MediaFrameError::WrongSource { .. })
    ));
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(wire.clone()), session(2), nonce(7)).read_frame(3, 6),
        Err(MediaFrameError::WrongFrame { .. })
    ));
    let second = TEST_HEADER_LENGTH + MAX_MEDIA_DATA_CHUNK_BYTES;
    wire[second + 40..second + 48].copy_from_slice(&0_u64.to_le_bytes());
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(wire), session(2), nonce(7)).read_frame(3, 5),
        Err(MediaFrameError::WrongOffset { .. })
    ));
}

#[test]
fn invalid_metadata_is_rejected_before_allocation() {
    let mut invalid = metadata(320, 240, 320);
    invalid.width = 319;
    assert!(matches!(
        invalid.validate(),
        Err(MediaFrameError::InvalidDimensions)
    ));
    invalid = metadata(320, 240, 318);
    assert!(matches!(
        invalid.validate(),
        Err(MediaFrameError::InvalidStride(_))
    ));
    invalid = metadata(320, 240, 320);
    invalid.data_length += 1;
    assert!(matches!(
        invalid.validate(),
        Err(MediaFrameError::InvalidLength(_))
    ));
}

#[test]
fn truncation_and_bytes_after_end_never_join_another_frame() {
    let metadata = metadata(4, 2, 4);
    let bytes = vec![0x80; metadata.data_length as usize];
    let mut wire = Vec::new();
    MediaFrameWriter::new(&mut wire, session(2), nonce(7))
        .send_frame(metadata, &bytes)
        .unwrap();
    let mut truncated = wire.clone();
    truncated.truncate(truncated.len() - 1);
    assert!(
        MediaFrameReader::new(Cursor::new(truncated), session(2), nonce(7))
            .read_frame(3, 5)
            .is_err()
    );
    let stale = wire[..TEST_HEADER_LENGTH + bytes.len()].to_vec();
    wire.extend_from_slice(&stale);
    let mut reader = MediaFrameReader::new(Cursor::new(wire), session(2), nonce(7));
    reader.read_frame(3, 5).unwrap();
    assert!(matches!(
        reader.read_frame(4, 6),
        Err(MediaFrameError::WrongSource { .. })
    ));
}

#[test]
fn converts_limited_range_nv12_to_opaque_premultiplied_bgra() {
    let metadata = metadata(2, 2, 2);
    let black = nv12_to_bgra(metadata, &[16, 16, 16, 16, 128, 128]).unwrap();
    assert_eq!(black.bgra, [0, 0, 0, 255].repeat(4));
    let white = nv12_to_bgra(metadata, &[235, 235, 235, 235, 128, 128]).unwrap();
    assert_eq!(white.bgra, [255, 255, 255, 255].repeat(4));
}
