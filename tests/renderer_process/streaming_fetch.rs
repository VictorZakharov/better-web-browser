use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::{
    FetchInitiator, FetchResponseHead, FetchResponseResult, FetchResponseType, TransferChunk,
};
use std::time::Duration;

#[test]
fn xhr_receives_incremental_progress_without_blocking_renderer_commands() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch streaming renderer");
    let initial = load_html_document(
        &session,
        180,
        r#"<!doctype html><div id="status">pending</div><script>
            const samples = [];
            const xhr = new XMLHttpRequest();
            xhr.open('GET', '/stream');
            xhr.onprogress = event => {
                samples.push(event.loaded);
                document.querySelector('#status').textContent = event.loaded + '/' + event.total;
            };
            xhr.onloadend = () => document.querySelector('#status').textContent =
                'done:' + xhr.responseText + '|' + samples.join(',');
            xhr.send();
        </script>"#,
    );
    session
        .advance_time(initial.document, Duration::from_millis(1), 1)
        .expect("start script Fetch");
    let request = wait_for_fetch(&session, initial.document);
    assert_eq!(request.head.initiator, FetchInitiator::ScriptApi);
    let request_id = request.head.request_id;
    let sink = session.fetch_response_sink(initial.document);
    sink.start(success_head(request_id, 6)).unwrap();
    sink.chunk(TransferChunk {
        transfer_id: request_id,
        offset: 0,
        bytes: b"ab".to_vec(),
    })
    .unwrap();
    wait_for_text(&session, initial.document, "2/6");
    session
        .ping(Duration::from_secs(1))
        .expect("renderer accepts commands between response chunks");
    std::thread::sleep(Duration::from_millis(60));
    sink.chunk(TransferChunk {
        transfer_id: request_id,
        offset: 2,
        bytes: b"cdef".to_vec(),
    })
    .unwrap();
    wait_for_text(&session, initial.document, "6/6");
    sink.end(request_id, 6).unwrap();
    wait_for_text(&session, initial.document, "done:abcdef|2,6");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().unwrap();
}

#[test]
fn xhr_abort_emits_a_request_scoped_browser_event() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch abort renderer");
    let initial = load_html_document(
        &session,
        181,
        r#"<!doctype html><div id="status">pending</div><script>
            const xhr = new XMLHttpRequest();
            xhr.open('GET', '/stream');
            xhr.onprogress = () => xhr.abort();
            xhr.onabort = () => document.querySelector('#status').textContent = 'aborted';
            xhr.send();
        </script>"#,
    );
    session
        .advance_time(initial.document, Duration::from_millis(1), 1)
        .expect("start abortable Fetch");
    let request = wait_for_fetch(&session, initial.document);
    let request_id = request.head.request_id;
    let sink = session.fetch_response_sink(initial.document);
    sink.start(success_head(request_id, 1024)).unwrap();
    sink.chunk(TransferChunk {
        transfer_id: request_id,
        offset: 0,
        bytes: b"ab".to_vec(),
    })
    .unwrap();

    loop {
        match session
            .wait_for_event(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("waiting for Fetch abort: {error}"))
        {
            RendererEvent::FetchAbort {
                document,
                request_id: aborted,
            } if document == initial.document && aborted == request_id => break,
            RendererEvent::Presentation(presentation) => {
                assert_eq!(presentation.document, initial.document);
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected event while waiting for Fetch abort: {event:?}"),
        }
    }
    wait_for_text(&session, initial.document, "aborted");
    session.shutdown().unwrap();
}

fn success_head(request_id: u64, length: usize) -> FetchResponseHead {
    FetchResponseHead {
        request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec!["https://example.test/stream".into()],
            status: 200,
            headers: vec![("content-length".into(), length.to_string())],
        },
    }
}

fn wait_for_fetch(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
) -> better_web_browser::renderer_protocol::RendererFetchRequest {
    loop {
        match session
            .wait_for_event(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("waiting for script Fetch: {error}"))
        {
            RendererEvent::FetchBatch {
                document: owner,
                mut requests,
            } if owner == document => {
                assert_eq!(requests.len(), 1);
                return requests.pop().unwrap();
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected event while waiting for script Fetch: {event:?}"),
        }
    }
}

fn wait_for_text(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    expected: &str,
) {
    loop {
        match session
            .wait_for_event(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("waiting for {expected:?}: {error}"))
        {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                let text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                if text.contains(expected) {
                    return;
                }
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected event while waiting for {expected:?}: {event:?}"),
        }
    }
}
