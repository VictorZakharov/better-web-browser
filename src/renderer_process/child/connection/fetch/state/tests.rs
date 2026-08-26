use super::*;
use crate::renderer_protocol::{FetchResponseResult, FetchResponseType, TransferChunk};

fn success_head(request_id: u64) -> FetchResponseHead {
    FetchResponseHead {
        request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec!["https://example.test/stream".into()],
            status: 200,
            headers: vec![("content-length".into(), (25 * 1024 * 1024).to_string())],
        },
    }
}

#[test]
fn script_response_streams_beyond_the_buffered_response_limit() {
    let document = DocumentId::new(1).unwrap();
    let mut state = FetchState::default();
    const CHUNK: usize = 1024 * 1024;
    for cycle in 0..3_u64 {
        let request_id = 7 + cycle;
        state
            .register(document, 3 + cycle, &[(request_id, true)])
            .unwrap();
        assert!(matches!(
            state
                .handle(BrowserMessage::FetchResponseStart(success_head(request_id)))
                .unwrap(),
            Some(ScriptFetchDelivery::Head { .. })
        ));
        for index in 0..25_u32 {
            let delivery = state
                .handle(BrowserMessage::FetchResponseChunk(TransferChunk {
                    transfer_id: request_id,
                    offset: index * CHUNK as u32,
                    bytes: vec![b'x'; CHUNK],
                }))
                .unwrap();
            assert!(matches!(
                delivery,
                Some(ScriptFetchDelivery::Chunk { bytes, .. }) if bytes.len() == CHUNK
            ));
        }
        assert!(matches!(
            state
                .handle(BrowserMessage::FetchResponseEnd(
                    crate::renderer_protocol::FetchResponseEnd {
                        request_id,
                        total_length: (25 * CHUNK) as u32,
                    },
                ))
                .unwrap(),
            Some(ScriptFetchDelivery::End { request_id: completed, .. })
                if completed == request_id
        ));
        assert!(state.requests.is_empty());
        assert!(state.streaming.is_empty());
    }
}

#[test]
fn script_response_rejects_non_monotonic_offsets() {
    let document = DocumentId::new(1).unwrap();
    let mut state = FetchState::default();
    state.register(document, 3, &[(7, true)]).unwrap();
    state
        .handle(BrowserMessage::FetchResponseStart(success_head(7)))
        .unwrap();
    let error = state
        .handle(BrowserMessage::FetchResponseChunk(TransferChunk {
            transfer_id: 7,
            offset: 1,
            bytes: vec![b'x'],
        }))
        .unwrap_err();
    assert_eq!(error, "Fetch stream response offset mismatch");
}
