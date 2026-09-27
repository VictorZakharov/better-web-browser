use super::*;

fn format() -> GraphPcmFormat {
    GraphPcmFormat {
        sample_rate: 48_000,
        channels: 2,
    }
}

fn message(pcm: Vec<u8>) -> BrowserMediaMessage {
    BrowserMediaMessage::QueueGraphPcm {
        request_id: 7,
        document_id: 11,
        context_id: 13,
        format: format(),
        pcm,
    }
}

#[test]
fn graph_pcm_and_decode_messages_share_a_sequenced_control_pipe() {
    let pcm = vec![0, 0, 255, 127, 0, 128, 0, 0];
    let mut bytes = Vec::new();
    let mut writer = MediaFrameWriter::new(&mut bytes, session(3));
    writer.send_browser(&message(pcm.clone())).unwrap();
    writer
        .send_browser(&BrowserMediaMessage::DecodeSource {
            request_id: 8,
            source_id: 1,
            frame_id: 1,
            encoded_length: 4,
        })
        .unwrap();
    writer
        .send_browser(&BrowserMediaMessage::CloseGraphPcm {
            request_id: 9,
            document_id: 11,
            context_id: 13,
        })
        .unwrap();
    let mut reader = MediaFrameReader::new(Cursor::new(bytes), session(3));
    assert_eq!(reader.read_browser().unwrap(), message(pcm));
    assert_eq!(
        reader.read_browser().unwrap(),
        BrowserMediaMessage::DecodeSource {
            request_id: 8,
            source_id: 1,
            frame_id: 1,
            encoded_length: 4,
        }
    );
    assert_eq!(
        reader.read_browser().unwrap(),
        BrowserMediaMessage::CloseGraphPcm {
            request_id: 9,
            document_id: 11,
            context_id: 13,
        }
    );

    let mut bytes = Vec::new();
    MediaFrameWriter::new(&mut bytes, session(3))
        .send_worker(&WorkerMediaMessage::GraphPcmStatus {
            request_id: 7,
            status: GraphPcmStatus::Backpressure,
        })
        .unwrap();
    assert_eq!(
        MediaFrameReader::new(Cursor::new(bytes), session(3))
            .read_worker()
            .unwrap(),
        WorkerMediaMessage::GraphPcmStatus {
            request_id: 7,
            status: GraphPcmStatus::Backpressure,
        }
    );
}

#[test]
fn graph_pcm_rejects_invalid_format_and_byte_counts() {
    let mut writer = MediaFrameWriter::new(Vec::new(), session(3));
    writer.send_browser(&message(vec![0; 3_584])).unwrap();
    for pcm in [vec![], vec![0; 3], vec![0; 3_588]] {
        assert!(matches!(
            writer.send_browser(&message(pcm)),
            Err(MediaProtocolError::InvalidPayload(_))
        ));
    }
    for (sample_rate, channels) in [(7_999, 2), (192_001, 2), (48_000, 0), (48_000, 3)] {
        let mut invalid = message(vec![0; 4]);
        if let BrowserMediaMessage::QueueGraphPcm { format, .. } = &mut invalid {
            *format = GraphPcmFormat {
                sample_rate,
                channels,
            };
        }
        assert!(matches!(
            writer.send_browser(&invalid),
            Err(MediaProtocolError::InvalidPayload(_))
        ));
    }
    for (sample_rate, channels, bytes) in [(8_000, 1, 1_792), (192_000, 2, 3_584)] {
        GraphPcmFormat {
            sample_rate,
            channels,
        }
        .validate(bytes)
        .unwrap();
    }
    assert_eq!(
        MediaFrameReader::new(Cursor::new(writer.into_inner()), session(3))
            .read_browser()
            .unwrap(),
        message(vec![0; 3_584])
    );
}

#[test]
fn graph_pcm_reader_rejects_malformed_format_and_declared_length() {
    let mut valid = Vec::new();
    MediaFrameWriter::new(&mut valid, session(3))
        .send_browser(&message(vec![0; 4]))
        .unwrap();
    // Header(32), request/document/context(24), rate(4), channels(2), byte length(2).
    for (offset, replacement) in [
        (56, 0_u32.to_le_bytes().to_vec()),
        (60, 0_u16.to_le_bytes().to_vec()),
        (62, 3_u16.to_le_bytes().to_vec()),
    ] {
        let mut malformed = valid.clone();
        malformed[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(matches!(
            MediaFrameReader::new(Cursor::new(malformed), session(3)).read_browser(),
            Err(MediaProtocolError::InvalidPayload(_))
        ));
    }
}

#[test]
fn graph_pcm_reader_rejects_unknown_worker_status() {
    let mut bytes = Vec::new();
    MediaFrameWriter::new(&mut bytes, session(3))
        .send_worker(&WorkerMediaMessage::GraphPcmStatus {
            request_id: 7,
            status: GraphPcmStatus::Backpressure,
        })
        .unwrap();
    bytes[40] = 3;
    assert!(matches!(
        MediaFrameReader::new(Cursor::new(bytes), session(3)).read_worker(),
        Err(MediaProtocolError::InvalidPayload("graph PCM status"))
    ));
}
