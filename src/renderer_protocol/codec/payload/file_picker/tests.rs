use super::*;
use crate::limits::MAX_CONTROL_PAYLOAD;
use crate::renderer_protocol::{
    BrowserMessage, FrameReader, FrameWriter, RendererMessage, RendererSessionId,
};
use std::io::Cursor;

fn request() -> FilePickerRequest {
    FilePickerRequest {
        document: DocumentId::new(7).unwrap(),
        request_id: 42,
        node: DocumentNodeId::new((9u128 << 64) | 3).unwrap(),
        client: RequestClient {
            id: 11,
            opaque: false,
        },
        multiple: true,
        accept: ".png,image/*".into(),
    }
}

fn metadata(name: &str, size: u32) -> SelectedFileMetadata {
    SelectedFileMetadata {
        name: name.into(),
        mime_type: "image/png".into(),
        last_modified: 1_700_000_000_000,
        size,
    }
}

fn update() -> FilePickerUpdate {
    FilePickerUpdate::Start {
        document: DocumentId::new(7).unwrap(),
        request_id: 42,
        files: vec![metadata("a.png", MAX_FILE_PICKER_CHUNK_BYTES as u32)],
    }
}

#[test]
fn request_and_every_update_variant_round_trip() {
    let request = request();
    assert_eq!(
        decode_request(&encode_request(&request).unwrap()).unwrap(),
        request
    );
    let document = request.document;
    for update in [
        update(),
        FilePickerUpdate::Chunk {
            document,
            request_id: 42,
            file_index: 0,
            offset: 0,
            bytes: vec![0xa5; MAX_FILE_PICKER_CHUNK_BYTES],
        },
        FilePickerUpdate::End {
            document,
            request_id: 42,
        },
        FilePickerUpdate::Canceled {
            document,
            request_id: 42,
        },
        FilePickerUpdate::Failed {
            document,
            request_id: 42,
        },
    ] {
        assert_eq!(
            decode_update(&encode_update(&update).unwrap()).unwrap(),
            update
        );
    }
}

#[test]
fn largest_chunks_fit_the_control_frame_and_direction_is_enforced() {
    let session = RendererSessionId::new(91).unwrap();
    let request = RendererMessage::FilePickerRequest(FilePickerRequest {
        accept: "x".repeat(MAX_FILE_PICKER_ACCEPT_BYTES),
        ..request()
    });
    let mut bytes = Vec::new();
    FrameWriter::new(&mut bytes, session)
        .send_renderer(&request)
        .unwrap();
    assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 0x0240);
    assert_eq!(
        FrameReader::new(Cursor::new(bytes.clone()), session)
            .read_renderer()
            .unwrap(),
        request
    );
    assert!(
        FrameReader::new(Cursor::new(bytes), session)
            .read_browser()
            .is_err()
    );

    let update = BrowserMessage::FilePickerUpdate(FilePickerUpdate::Chunk {
        document: DocumentId::new(7).unwrap(),
        request_id: 42,
        file_index: 0,
        offset: 0,
        bytes: vec![0x5a; MAX_FILE_PICKER_CHUNK_BYTES],
    });
    let mut bytes = Vec::new();
    FrameWriter::new(&mut bytes, session)
        .send_browser(&update)
        .unwrap();
    assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 0x0241);
    assert!(bytes.len() < MAX_CONTROL_PAYLOAD);
    assert_eq!(
        FrameReader::new(Cursor::new(bytes.clone()), session)
            .read_browser()
            .unwrap(),
        update
    );
    assert!(
        FrameReader::new(Cursor::new(bytes), session)
            .read_renderer()
            .is_err()
    );
}

#[test]
fn malformed_request_fields_and_unbounded_strings_are_rejected() {
    let mut bytes = encode_request(&request()).unwrap();
    bytes.pop();
    assert!(decode_request(&bytes).is_err());

    let mut bytes = encode_request(&request()).unwrap();
    bytes[40] = 2; // opaque must be a wire boolean
    assert!(decode_request(&bytes).is_err());

    let mut bytes = encode_request(&request()).unwrap();
    bytes[42..46].copy_from_slice(&((MAX_FILE_PICKER_ACCEPT_BYTES + 1) as u32).to_le_bytes());
    assert!(decode_request(&bytes).is_err());

    let mut bytes = encode_request(&FilePickerRequest {
        accept: "x".into(),
        ..request()
    })
    .unwrap();
    bytes[46] = 0xff;
    assert!(decode_request(&bytes).is_err());

    let mut bytes = encode_request(&request()).unwrap();
    bytes.extend_from_slice(b"trailing");
    assert!(decode_request(&bytes).is_err());
}

#[test]
fn malformed_update_counts_lengths_and_kinds_are_rejected() {
    let mut bytes = encode_update(&update()).unwrap();
    bytes[17] = (MAX_FILE_PICKER_FILES + 1) as u8;
    assert!(decode_update(&bytes).is_err());

    let mut bytes = encode_update(&update()).unwrap();
    bytes[18..22].copy_from_slice(&((MAX_FILE_PICKER_NAME_BYTES + 1) as u32).to_le_bytes());
    assert!(decode_update(&bytes).is_err());

    let mut bytes = encode_update(&update()).unwrap();
    bytes[22] = 0xff; // first byte of basename
    assert!(decode_update(&bytes).is_err());

    let mut bytes = encode_update(&update()).unwrap();
    bytes.pop();
    assert!(decode_update(&bytes).is_err());

    let mut bytes = encode_update(&update()).unwrap();
    bytes[16] = 99;
    assert!(decode_update(&bytes).is_err());

    let mut bytes = encode_update(&FilePickerUpdate::Chunk {
        document: DocumentId::new(7).unwrap(),
        request_id: 42,
        file_index: 0,
        offset: 0,
        bytes: vec![1],
    })
    .unwrap();
    bytes[22..26].copy_from_slice(&((MAX_FILE_PICKER_CHUNK_BYTES + 1) as u32).to_le_bytes());
    assert!(decode_update(&bytes).is_err());
}

#[test]
fn file_bytes_are_not_in_debug_diagnostics() {
    let update = FilePickerUpdate::Chunk {
        document: DocumentId::new(7).unwrap(),
        request_id: 42,
        file_index: 0,
        offset: 0,
        bytes: b"private file contents".to_vec(),
    };
    assert!(!format!("{update:?}").contains("private file contents"));
}
