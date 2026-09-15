use super::*;
use better_web_browser::renderer_protocol::{DocumentId, RendererFetchRequest};
use std::sync::mpsc;
use std::time::Instant;

#[path = "progressive/lifecycle.rs"]
mod lifecycle;

fn serve_worker(session: &RendererSession, document: DocumentId, source: &[u8]) {
    let entry = request(session, document).head.request_id;
    let sink = session.fetch_response_sink(document);
    let mut head = success_head(entry, source.len());
    if let FetchResponseResult::Success { headers, .. } = &mut head.result {
        headers.push(("content-type".into(), "application/javascript".into()));
    }
    sink.start(head).unwrap();
    sink.chunk(TransferChunk {
        transfer_id: entry,
        offset: 0,
        bytes: source.to_vec(),
    })
    .unwrap();
    sink.end(entry, source.len() as u32).unwrap();
}

fn next_event(session: &RendererSession, document: DocumentId, deadline: Instant) -> RendererEvent {
    loop {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for progressive Fetch"
        );
        session
            .advance_time(document, Duration::from_millis(10), 8)
            .unwrap();
        match session.wait_for_event(Duration::from_millis(30)) {
            Ok(event) => return event,
            Err(error) if error.contains("timed out") || error.contains("timeout") => {}
            Err(error) => panic!("progressive Fetch renderer: {error}"),
        }
    }
}

fn request(session: &RendererSession, document: DocumentId) -> RendererFetchRequest {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let RendererEvent::FetchBatch { mut requests, .. } =
            next_event(session, document, deadline)
        {
            assert_eq!(requests.len(), 1);
            return requests.pop().unwrap();
        }
    }
}

fn text(session: &RendererSession, document: DocumentId, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match next_event(session, document, deadline) {
            RendererEvent::Presentation(presentation) => {
                let value = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                if value.contains(expected) {
                    return;
                }
            }
            RendererEvent::RuntimeUpdate(update) => assert!(
                update.runtime.errors.is_empty(),
                "{:?}",
                update.runtime.errors
            ),
            RendererEvent::DocumentFailed { detail, .. } => panic!("{detail}"),
            RendererEvent::Exited(exit) => panic!("{exit:?}"),
            _ => {}
        }
    }
}

#[test]
fn document_fetch_exposes_headers_and_first_chunk_before_eof() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        182,
        r#"<div id="status">pending</div><script>
        fetch('/stream').then(async response => {
            statusNode = document.querySelector('#status'); statusNode.textContent = 'headers';
            const reader = response.body.getReader();
            const first = await reader.read(); statusNode.textContent = 'first:' + new TextDecoder().decode(first.value);
            const end = await reader.read(); statusNode.textContent = 'end:' + end.done;
        });</script>"#,
    );
    let id = request(&session, initial.document).head.request_id;
    let sink = session.fetch_response_sink(initial.document);
    sink.start(success_head(id, 2)).unwrap();
    text(&session, initial.document, "headers");
    sink.chunk(TransferChunk {
        transfer_id: id,
        offset: 0,
        bytes: b"ab".to_vec(),
    })
    .unwrap();
    text(&session, initial.document, "first:ab");
    sink.end(id, 2).unwrap();
    text(&session, initial.document, "end:true");
    session.shutdown().unwrap();
}

#[test]
fn worker_fetch_exposes_headers_chunks_and_timers_before_eof() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        183,
        r#"<div id="status">pending</div><script>
        const worker = new Worker('/worker.js');
        worker.onmessage = e => document.querySelector('#status').textContent = e.data;
        </script>"#,
    );
    let entry = request(&session, initial.document).head.request_id;
    let sink = session.fetch_response_sink(initial.document);
    let source = br#"fetch('/stream').then(async response => {
        postMessage('headers');
        const reader = response.body.getReader();
        const first = await reader.read();
        postMessage('first:' + new TextDecoder().decode(first.value));
        setTimeout(() => postMessage('timer while downloading'), 30);
        const end = await reader.read(); postMessage('end:' + end.done);
    });"#;
    let mut head = success_head(entry, source.len());
    if let FetchResponseResult::Success { headers, .. } = &mut head.result {
        headers.push(("content-type".into(), "application/javascript".into()));
    }
    sink.start(head).unwrap();
    sink.chunk(TransferChunk {
        transfer_id: entry,
        offset: 0,
        bytes: source.to_vec(),
    })
    .unwrap();
    sink.end(entry, source.len() as u32).unwrap();
    let id = request(&session, initial.document).head.request_id;
    sink.start(success_head(id, 2)).unwrap();
    text(&session, initial.document, "headers");
    sink.chunk(TransferChunk {
        transfer_id: id,
        offset: 0,
        bytes: b"ab".to_vec(),
    })
    .unwrap();
    text(&session, initial.document, "first:ab");
    text(&session, initial.document, "timer while downloading");
    sink.end(id, 2).unwrap();
    text(&session, initial.document, "end:true");
    session.shutdown().unwrap();
}

#[test]
fn idle_reader_applies_transport_backpressure_without_blocking_commands() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        184,
        r#"<div id="status">pending</div><script>
        fetch('/stream').then(response => {
            document.querySelector('#status').textContent = 'headers';
            setTimeout(async () => { const bytes = await response.arrayBuffer();
                document.querySelector('#status').textContent = 'bytes:' + bytes.byteLength;
            }, 2000);
        });</script>"#,
    );
    let id = request(&session, initial.document).head.request_id;
    let sink = session.fetch_response_sink(initial.document);
    sink.start(success_head(id, 512 * 1024)).unwrap();
    text(&session, initial.document, "headers");
    let (sent, progress) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        for chunk in 0..8 {
            sink.chunk(TransferChunk {
                transfer_id: id,
                offset: chunk * 65536,
                bytes: vec![b'x'; 65536],
            })?;
            sent.send(chunk).unwrap();
        }
        sink.end(id, 512 * 1024)
    });
    for chunk in 0..4 {
        assert_eq!(
            progress.recv_timeout(Duration::from_secs(2)).unwrap(),
            chunk
        );
    }
    assert!(
        progress.recv_timeout(Duration::from_millis(70)).is_err(),
        "idle JS reader drained beyond its window"
    );
    session.ping(Duration::from_secs(1)).unwrap();
    session
        .advance_time(initial.document, Duration::from_millis(2200), 32)
        .unwrap();
    text(&session, initial.document, "bytes:524288");
    producer.join().unwrap().unwrap();
    session.shutdown().unwrap();
}
