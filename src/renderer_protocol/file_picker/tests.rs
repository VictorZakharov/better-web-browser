use super::*;

fn document(value: u64) -> DocumentId {
    DocumentId::new(value).unwrap()
}

fn metadata(name: &str, size: u32) -> SelectedFileMetadata {
    SelectedFileMetadata {
        name: name.into(),
        mime_type: "text/plain".into(),
        last_modified: 123,
        size,
    }
}

#[test]
fn picker_intent_requires_a_bounded_hint_and_nonzero_identity() {
    let request = FilePickerRequest {
        document: document(7),
        request_id: 5,
        node: DocumentNodeId::new((3u128 << 64) | 1).unwrap(),
        client: RequestClient::default(),
        multiple: false,
        accept: "x".repeat(MAX_FILE_PICKER_ACCEPT_BYTES),
    };
    assert!(request.validate().is_ok());
    assert!(
        FilePickerRequest {
            request_id: 0,
            ..request.clone()
        }
        .validate()
        .is_err()
    );
    assert!(
        FilePickerRequest {
            accept: "x".repeat(MAX_FILE_PICKER_ACCEPT_BYTES + 1),
            ..request.clone()
        }
        .validate()
        .is_err()
    );
    assert!(
        FilePickerRequest {
            accept: "image/png\0".into(),
            ..request
        }
        .validate()
        .is_err()
    );
}

fn start(files: Vec<SelectedFileMetadata>) -> FilePickerUpdate {
    FilePickerUpdate::Start {
        document: document(7),
        request_id: 5,
        files,
    }
}

fn chunk(file_index: u8, offset: u32, bytes: &[u8]) -> FilePickerUpdate {
    FilePickerUpdate::Chunk {
        document: document(7),
        request_id: 5,
        file_index,
        offset,
        bytes: bytes.into(),
    }
}

#[test]
fn selected_metadata_is_bounded_and_cannot_carry_a_path() {
    assert!(
        metadata("a.txt", MAX_FILE_PICKER_BYTES as u32)
            .validate()
            .is_ok()
    );
    for name in [
        "",
        ".",
        "..",
        "C:\\Users\\secret.txt",
        "/tmp/secret",
        "C:secret",
        "a\n.txt",
    ] {
        assert!(metadata(name, 1).validate().is_err(), "{name:?}");
    }
    assert!(
        metadata(&"x".repeat(MAX_FILE_PICKER_NAME_BYTES + 1), 1)
            .validate()
            .is_err()
    );
    assert!(
        metadata("a", MAX_FILE_PICKER_BYTES as u32 + 1)
            .validate()
            .is_err()
    );
    for mime in [
        "TEXT/PLAIN",
        "text/plain; charset=utf-8",
        "C:\\Users\\secret",
        "text/",
        "/plain",
    ] {
        let mut file = metadata("a", 1);
        file.mime_type = mime.into();
        assert!(file.validate().is_err(), "{mime:?}");
    }
    let mut file = metadata("a", 1);
    file.mime_type.clear();
    assert!(file.validate().is_ok());
}

#[test]
fn stream_limits_apply_before_materializing_any_file() {
    assert!(start(vec![]).validate().is_err());
    assert!(
        start(vec![metadata("a", MAX_FILE_PICKER_BYTES as u32)])
            .validate()
            .is_ok()
    );
    assert!(
        start(vec![metadata("a", 1); MAX_FILE_PICKER_FILES + 1])
            .validate()
            .is_err()
    );
    assert!(
        start(vec![
            metadata("a", MAX_FILE_PICKER_BYTES as u32),
            metadata("b", 1)
        ])
        .validate()
        .is_err()
    );
    assert!(chunk(0, 0, &[]).validate().is_err());
    assert!(
        chunk(0, 0, &vec![0; MAX_FILE_PICKER_CHUNK_BYTES + 1])
            .validate()
            .is_err()
    );
    assert!(
        chunk(MAX_FILE_PICKER_FILES as u8, 0, &[1])
            .validate()
            .is_err()
    );
}

#[test]
fn ordered_chunks_and_empty_files_assemble_without_paths() {
    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assert_eq!(
        assembler
            .push(start(vec![metadata("empty", 0), metadata("data", 3)]))
            .unwrap(),
        None
    );
    assert_eq!(assembler.push(chunk(1, 0, b"ab")).unwrap(), None);
    assert_eq!(assembler.push(chunk(1, 2, b"c")).unwrap(), None);
    let selection = assembler
        .push(FilePickerUpdate::End {
            document: document(7),
            request_id: 5,
        })
        .unwrap()
        .unwrap();
    let FilePickerSelection::Selected(files) = selection else {
        panic!("expected files")
    };
    assert_eq!(files.len(), 2);
    assert!(files[0].bytes.is_empty());
    assert_eq!(files[1].bytes, b"abc");
    assert!(assembler.push(chunk(1, 3, b"d")).is_err());
}

#[test]
fn malformed_order_offsets_and_truncation_fail_closed() {
    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assert!(assembler.push(chunk(0, 0, b"a")).is_err());
    assert!(assembler.push(start(vec![metadata("a", 2)])).is_err());

    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assert!(
        assembler
            .push(FilePickerUpdate::End {
                document: document(7),
                request_id: 5,
            })
            .is_err()
    );

    let started = || {
        let mut assembler = FileSelectionAssembler::new(document(7), 5);
        assembler
            .push(start(vec![metadata("a", 2), metadata("b", 1)]))
            .unwrap();
        assembler
    };
    assert!(started().push(start(vec![metadata("a", 2)])).is_err());
    assert!(started().push(chunk(1, 0, b"b")).is_err());
    assert!(started().push(chunk(0, 1, b"a")).is_err());
    let mut assembler = started();
    assembler.push(chunk(0, 0, b"a")).unwrap();
    assert!(assembler.push(chunk(0, 0, b"a")).is_err());
    let mut assembler = started();
    assembler.push(chunk(0, 0, b"a")).unwrap();
    assert!(assembler.push(chunk(0, 1, b"bc")).is_err());
    let mut assembler = started();
    assembler.push(chunk(0, 0, b"a")).unwrap();
    assert!(
        assembler
            .push(FilePickerUpdate::End {
                document: document(7),
                request_id: 5,
            })
            .is_err()
    );
}

#[test]
fn wrong_document_or_request_and_late_chunks_are_rejected() {
    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assert!(
        assembler
            .push(FilePickerUpdate::Canceled {
                document: document(8),
                request_id: 5,
            })
            .is_err()
    );

    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assert!(
        assembler
            .push(FilePickerUpdate::Canceled {
                document: document(7),
                request_id: 6,
            })
            .is_err()
    );

    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assert_eq!(
        assembler.push(start(vec![metadata("empty", 0)])).unwrap(),
        None
    );
    assert!(assembler.push(chunk(1, 0, b"x")).is_err());
    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assembler.push(start(vec![metadata("empty", 0)])).unwrap();
    assert_eq!(
        assembler
            .push(FilePickerUpdate::End {
                document: document(7),
                request_id: 5,
            })
            .unwrap(),
        Some(FilePickerSelection::Selected(vec![SelectedFile {
            metadata: metadata("empty", 0),
            bytes: vec![],
        }]))
    );
    assert!(
        assembler
            .push(FilePickerUpdate::Canceled {
                document: document(7),
                request_id: 5,
            })
            .is_err()
    );
}

#[test]
fn failure_can_abort_a_started_stream_but_cancel_cannot() {
    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assembler.push(start(vec![metadata("a", 10)])).unwrap();
    assert!(
        assembler
            .push(FilePickerUpdate::Canceled {
                document: document(7),
                request_id: 5,
            })
            .is_err()
    );

    let mut assembler = FileSelectionAssembler::new(document(7), 5);
    assembler.push(start(vec![metadata("a", 10)])).unwrap();
    assert_eq!(
        assembler
            .push(FilePickerUpdate::Failed {
                document: document(7),
                request_id: 5,
            })
            .unwrap(),
        Some(FilePickerSelection::Failed)
    );
}
