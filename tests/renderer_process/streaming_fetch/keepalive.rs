use super::*;
use better_web_browser::renderer_protocol::DocumentId;

#[test]
fn keepalive_fetch_crosses_the_renderer_boundary_and_keeps_its_response_promise() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch keepalive renderer");
    let document = DocumentId::new(189).unwrap();
    let body = r#"<!doctype html><div id="status">pending</div><script>
            fetch('/collect', {method:'POST', body:'proof', keepalive:true})
                .then(response => response.text())
                .then(text => document.querySelector('#status').textContent = text);
        </script>"#
        .as_bytes()
        .to_vec();
    // A survivable Fetch can cross the broker before the first presentation.
    // Do not use the generic document loader, which expects presentation first.
    session
        .load_document(
            document_start(document, body.len()),
            empty_document_state(),
            body,
        )
        .unwrap();
    session
        .advance_time(document, Duration::from_millis(1), 1)
        .expect("start keepalive Fetch");
    let request = wait_for_fetch(&session, document);
    assert_eq!(request.head.initiator, FetchInitiator::ScriptApi);
    assert!(request.head.keepalive);
    assert_eq!(request.head.method, "POST");
    assert_eq!(request.body, b"proof");
    let sink = session.fetch_response_sink(document);
    sink.start(success_head(request.head.request_id, 2))
        .unwrap();
    sink.chunk(TransferChunk {
        transfer_id: request.head.request_id,
        offset: 0,
        bytes: b"ok".to_vec(),
    })
    .unwrap();
    sink.end(request.head.request_id, 2).unwrap();
    wait_for_text(&session, document, "ok");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().unwrap();
}
