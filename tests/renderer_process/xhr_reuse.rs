use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::{
    DocumentId, FetchResponseHead, FetchResponseResult, FetchResponseType, TransferChunk,
};
use std::time::{Duration, Instant};

#[test]
fn xhr_reuse_from_progress_cancels_only_the_old_stream() {
    run_reuse("xhr.onprogress", "true");
}

#[test]
fn xhr_reuse_from_headers_keeps_replacement_bytes_and_completion() {
    run_reuse("xhr.onreadystatechange", "xhr.readyState === 2");
}

fn run_reuse(handler: &str, condition: &str) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let html = format!(
        r#"<!doctype html><title>pending</title><script>
        const xhr = new XMLHttpRequest();
        let replaced = false;
        const errors = [];
        xhr.onabort = () => errors.push('abort');
        xhr.onerror = () => errors.push('error');
        {handler} = () => {{
            if (!replaced && ({condition})) {{
                replaced = true;
                xhr.open('GET', '/new'); xhr.send();
            }}
        }};
        xhr.onload = () => document.title = xhr.responseText + '|' + errors.join(',');
        xhr.open('GET', '/old'); xhr.send();
        </script>"#
    );
    let presentation = load_html_document(&session, 185, &html);
    let document = presentation.document;
    session
        .advance_time(document, Duration::from_millis(1), 1)
        .unwrap();
    let mut aborted = Vec::new();
    let old = wait_for_fetch(&session, document, "/old", &mut aborted);
    let sink = session.fetch_response_sink(document);
    sink.start(head(old, "/old", 1024)).unwrap();
    sink.chunk(TransferChunk {
        transfer_id: old,
        offset: 0,
        bytes: b"old".to_vec(),
    })
    .unwrap();
    let new = wait_for_fetch(&session, document, "/new", &mut aborted);
    assert_ne!(old, new);
    sink.start(head(new, "/new", 3)).unwrap();
    sink.chunk(TransferChunk {
        transfer_id: new,
        offset: 0,
        bytes: b"new".to_vec(),
    })
    .unwrap();
    sink.end(new, 3).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut completed = false;
    while !completed || !aborted.contains(&old) {
        assert!(
            Instant::now() < deadline,
            "replacement did not complete cleanly"
        );
        match session.wait_for_event(Duration::from_secs(2)).unwrap() {
            RendererEvent::FetchAbort {
                document: owner,
                request_id,
            } if owner == document => aborted.push(request_id),
            RendererEvent::Presentation(value) if value.document == document => {
                completed |= value.title == "new|";
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected event during replacement: {event:?}"),
        }
    }
    assert!(
        !aborted.contains(&new),
        "old request's failure aborted its replacement"
    );
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().unwrap();
}

fn head(request_id: u64, path: &str, length: usize) -> FetchResponseHead {
    FetchResponseHead {
        request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![format!("https://example.test{path}")],
            status: 200,
            headers: vec![("content-length".into(), length.to_string())],
        },
    }
}

fn wait_for_fetch(
    session: &RendererSession,
    document: DocumentId,
    path: &str,
    aborted: &mut Vec<u64>,
) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(
            Instant::now() < deadline,
            "waiting for replacement fetch {path}"
        );
        match session.wait_for_event(Duration::from_secs(2)).unwrap() {
            RendererEvent::FetchBatch {
                document: owner,
                requests,
            } if owner == document => {
                assert_eq!(requests.len(), 1);
                assert!(requests[0].head.url.ends_with(path));
                return requests[0].head.request_id;
            }
            RendererEvent::FetchAbort {
                document: owner,
                request_id,
            } if owner == document => aborted.push(request_id),
            RendererEvent::Presentation(_)
            | RendererEvent::Diagnostic { .. }
            | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected event waiting for fetch: {event:?}"),
        }
    }
}
