//! Child navigation and external scripts use streamed IPC, not the page-resource queue.
use super::*;
use better_web_browser::renderer_protocol::{DocumentId, ResourceDestination};

#[test]
fn iframe_external_script_keeps_client_identity_and_delivers_message() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        182,
        r#"<!doctype html>
        <p id="status">pending</p><script>
        addEventListener('message',e=>document.getElementById('status').textContent=e.data);
        setTimeout(()=>{let f=document.createElement('iframe');f.src='/frame';document.body.append(f)},1);
    </script>"#,
    );
    session
        .advance_time(initial.document, Duration::from_millis(1), 1)
        .unwrap();
    let navigation = next_fetch(&session, initial.document);
    assert_eq!(navigation.head.initiator, FetchInitiator::ChildNavigation);
    assert_eq!(navigation.head.destination, ResourceDestination::Document);
    assert_eq!(navigation.head.client.id, 0);
    let client = navigation.head.resulting_client;
    assert_ne!(client.id, 0);
    complete(
        &session,
        initial.document,
        navigation.head.request_id,
        "https://child.test/redirected/frame",
        "text/html",
        "<script src='relay.js'></script>",
    );
    let script = next_fetch(&session, initial.document);
    assert_eq!(script.head.initiator, FetchInitiator::ChildResource);
    assert_eq!(script.head.client, client);
    assert_eq!(script.head.url, "https://child.test/redirected/relay.js");
    complete(
        &session,
        initial.document,
        script.head.request_id,
        &script.head.url,
        "application/javascript",
        "parent.postMessage('child stream relay passed','*');",
    );
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(p) => {
                let text = p
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                if text.contains("child stream relay passed") {
                    break;
                }
                pump_ready_task(&session, initial.document, p.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                pump_ready_task(&session, initial.document, update.next_timer_micros)
            }
            RendererEvent::Diagnostic { .. } => (),
            event => panic!("unexpected {event:?}"),
        }
    }
    session.shutdown().unwrap();
}

fn next_fetch(
    session: &RendererSession,
    document: DocumentId,
) -> better_web_browser::renderer_protocol::RendererFetchRequest {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::FetchBatch { mut requests, .. } => {
                assert_eq!(requests.len(), 1);
                return requests.pop().unwrap();
            }
            RendererEvent::RuntimeUpdate(update) => {
                if let Some(delay) = update.next_timer_micros {
                    session
                        .advance_time(document, Duration::from_micros(delay.min(1_000)), 1)
                        .unwrap();
                }
            }
            RendererEvent::Diagnostic { .. } => (),
            RendererEvent::Presentation(p) => {
                pump_ready_task(session, document, p.next_timer_micros)
            }
            event => panic!("unexpected {event:?}"),
        }
    }
}

fn complete(
    session: &RendererSession,
    document: DocumentId,
    id: u64,
    url: &str,
    mime: &str,
    body: &str,
) {
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id: id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![url.into()],
            status: 200,
            headers: vec![("content-type".into(), mime.into())],
        },
    })
    .unwrap();
    sink.chunk(TransferChunk {
        transfer_id: id,
        offset: 0,
        bytes: body.as_bytes().to_vec(),
    })
    .unwrap();
    sink.end(id, body.len() as u32).unwrap();
}
