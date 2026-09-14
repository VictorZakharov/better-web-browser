use super::*;
use crate::storage::{
    StorageAreaKind, StorageEntry, StorageMutation, StorageOperation, StorageString,
};
use std::io::Cursor;

fn session() -> RendererSessionId {
    RendererSessionId::new(71).unwrap()
}

#[test]
fn storage_frames_round_trip_full_quota_and_isolated_surrogates() {
    let key = StorageString::from_units(vec![0xd800, 0, 0xdfff]);
    let value: StorageString = "x"
        .repeat(crate::limits::MAX_STORAGE_BYTES_PER_ORIGIN - key.byte_len())
        .into();
    let entry = StorageEntry { key, value };
    let browser = BrowserMessage::StorageSnapshotEntry(StorageSnapshotEntry {
        document: DocumentId::new(1).unwrap(),
        area: StorageAreaKind::Local,
        entry: entry.clone(),
    });
    let mut writer = FrameWriter::new(Vec::new(), session());
    writer.send_browser(&browser).unwrap();
    let bytes = writer.into_inner();
    assert!(bytes.len() > MAX_FRAME_PAYLOAD);
    assert_eq!(
        FrameReader::new(Cursor::new(bytes), session())
            .read_browser()
            .unwrap(),
        browser
    );
    let renderer = RendererMessage::StorageMutation(StorageMutationRequest {
        document: DocumentId::new(1).unwrap(),
        mutation: StorageMutation {
            area: StorageAreaKind::Local,
            expected_version: 1,
            operation: StorageOperation::Set {
                key: entry.key,
                value: entry.value,
            },
        },
    });
    let mut writer = FrameWriter::new(Vec::new(), session());
    writer.send_renderer(&renderer).unwrap();
    assert_eq!(
        FrameReader::new(Cursor::new(writer.into_inner()), session())
            .read_renderer()
            .unwrap(),
        renderer
    );
}

#[test]
fn storage_headers_reject_excess_before_reading_the_payload() {
    for (kind, limit) in [
        (0x0135_u16, crate::limits::MAX_STORAGE_FRAME_BYTES),
        (0x0133, MAX_CONTROL_PAYLOAD),
    ] {
        let mut header = [0_u8; HEADER_LENGTH];
        header[0..4].copy_from_slice(&MAGIC);
        header[4..6].copy_from_slice(&PROTOCOL_MAJOR.to_le_bytes());
        header[8..10].copy_from_slice(&kind.to_le_bytes());
        header[12..16].copy_from_slice(&((limit + 1) as u32).to_le_bytes());
        header[16..24].copy_from_slice(&session().get().to_le_bytes());
        header[24..32].copy_from_slice(&1_u64.to_le_bytes());
        assert!(matches!(
            FrameReader::new(Cursor::new(header), session()).read_browser(),
            Err(ProtocolError::PayloadTooLarge(_))
        ));
    }
}

#[test]
fn malformed_utf16_lengths_are_rejected_without_allocating() {
    for bytes in [vec![1, 0, 0, 0, 0], vec![4, 0, 0, 0, 0, 0]] {
        let mut reader = super::wire::WireReader::new(&bytes);
        assert!(reader.storage_string(1024).is_err());
    }
}
