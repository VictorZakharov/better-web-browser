use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentInput, DocumentNodeId, FetchResponseHead, FetchResponseResult,
    FetchResponseType, InputModifiers, NavigationCause, NavigationDisposition, PointerButton,
    PointerInput, PointerPhase, RendererFetchRequest, TestCommand, TextInput,
};
use std::time::{Duration, Instant};

#[test]
fn saturated_command_queue_recovers_and_delivers_final_input() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.unresponsive_timeout = Duration::from_secs(5);
    let mut session = RendererSession::launch(launch).expect("launch pressured renderer");
    let mut sibling = RendererSession::launch(options()).expect("launch sibling renderer");
    let initial = load_html_document(
        &session,
        150,
        r#"<!doctype html><input id="field"><p id="output">initial</p>
        <script>
            document.querySelector('#field').addEventListener('input', event => {
                document.querySelector('#output').textContent = event.target.value;
            });
        </script>"#,
    );
    let target = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) => DocumentNodeId::new(control.node_id.to_wire()).ok(),
            _ => None,
        })
        .expect("input control target");

    saturate_command_queue(&session);

    let expected_marker = "retained-final";
    let final_input = DocumentInput::Text(TextInput {
        document: initial.document,
        sequence: 1,
        target,
        value: expected_marker.into(),
        selection_start: expected_marker.len() as u32,
        selection_end: expected_marker.len() as u32,
    });
    let mut retained = session
        .try_send_input_retained(final_input)
        .expect("renderer command channel remains connected");

    let deadline = Instant::now() + Duration::from_secs(4);
    while let Some(input) = retained {
        match session
            .try_send_input_retained(input)
            .expect("renderer command channel remains connected while pressure clears")
        {
            None => retained = None,
            Some(input) if Instant::now() < deadline => {
                retained = Some(input);
                std::thread::sleep(Duration::from_millis(10));
            }
            Some(_) => panic!("renderer command pressure did not clear"),
        }
    }

    wait_for_text(&session, initial.document, expected_marker);
    session
        .ping(Duration::from_secs(1))
        .expect("pressured renderer remains responsive");
    sibling
        .ping(Duration::from_secs(1))
        .expect("sibling renderer remains responsive");
    assert_eq!(session.snapshot().state, RendererState::Running);
    assert_eq!(sibling.snapshot().state, RendererState::Running);

    session.shutdown().expect("shutdown pressured renderer");
    sibling.shutdown().expect("shutdown sibling renderer");
}

#[test]
fn duckduckgo_link_navigation_replaces_a_document_under_command_backpressure() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.unresponsive_timeout = Duration::from_secs(5);
    let mut session = RendererSession::launch(launch).expect("launch navigation renderer");
    let initial = load_html_document(
        &session,
        151,
        r#"<!doctype html><a href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FMain_Page&amp;rut=test">Wikipedia</a>"#,
    );
    let (rect, expected_url) = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text {
                rect,
                link: Some(url),
                ..
            } if url.contains("duckduckgo.com/l/") => Some((*rect, url.clone())),
            _ => None,
        })
        .expect("DuckDuckGo result link geometry");
    for (sequence, phase) in [(1, PointerPhase::Down), (2, PointerPhase::Up)] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence,
                phase,
                button: PointerButton::Primary,
                x: rect.x + rect.width / 2.0,
                y: rect.y + rect.height / 2.0,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .expect("activate DuckDuckGo result link");
    }
    let (url, disposition, cause) = wait_for_navigation(&session, initial.document);
    assert_eq!(url, expected_url);
    assert_eq!(
        url,
        "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FMain_Page&rut=test"
    );
    assert_eq!(disposition, NavigationDisposition::CurrentTab);
    assert_eq!(cause, NavigationCause::UserActivation);

    saturate_command_queue(&session);
    session
        .cancel_document(initial.document)
        .expect("queue lossless document cancellation");
    let replacement_document = better_web_browser::renderer_protocol::DocumentId::new(152).unwrap();
    let replacement = b"<!doctype html><p>Wikipedia replacement ready</p>".to_vec();
    session
        .load_document(
            document_start(replacement_document, replacement.len()),
            empty_document_state(),
            replacement,
        )
        .expect("queue lossless replacement document");

    wait_for_text(
        &session,
        replacement_document,
        "Wikipedia replacement ready",
    );
    session
        .ping(Duration::from_secs(1))
        .expect("replacement renderer remains responsive");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().expect("shutdown navigation renderer");
}

#[test]
fn navigation_discards_a_queued_fetch_batch_from_the_replaced_document() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch navigation renderer");
    let replaced_document = DocumentId::new(153).unwrap();
    let replaced = br#"<!doctype html><script src="/stale.js"></script><p>old page</p>"#.to_vec();
    session
        .load_document(
            document_start(replaced_document, replaced.len()),
            empty_document_state(),
            replaced,
        )
        .expect("load document with an outstanding Fetch batch");
    session
        .ping(Duration::from_secs(1))
        .expect("old Fetch batch reached the broker before navigation");

    session
        .cancel_document(replaced_document)
        .expect("cancel document with a queued Fetch batch");
    let replacement_document = DocumentId::new(154).unwrap();
    let replacement =
        br#"<!doctype html><script src="/current.js"></script><p>replacement ready</p>"#.to_vec();
    session
        .load_document(
            document_start(replacement_document, replacement.len()),
            empty_document_state(),
            replacement,
        )
        .expect("load replacement document with its own Fetch batch");
    session
        .ping(Duration::from_secs(1))
        .expect("replacement Fetch batch does not kill the renderer");

    let requests = loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::FetchBatch { document, requests }
                if document == replacement_document =>
            {
                break requests;
            }
            RendererEvent::FetchBatch { document, requests } => {
                panic!("stale Fetch batch for {document:?} survived navigation: {requests:?}")
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::TimeAdvanced { .. } => {}
            event => panic!("unexpected replacement event: {event:?}"),
        }
    };
    respond_with_empty_scripts(&session, replacement_document, requests);
    wait_for_text(&session, replacement_document, "replacement ready");
    session
        .ping(Duration::from_secs(1))
        .expect("replacement renderer remains responsive after Fetch completion");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().expect("shutdown navigation renderer");
}

fn respond_with_empty_scripts(
    session: &RendererSession,
    document: DocumentId,
    requests: Vec<RendererFetchRequest>,
) {
    let sink = session.fetch_response_sink(document);
    for request in requests {
        let request_id = request.head.request_id;
        sink.start(FetchResponseHead {
            request_id,
            result: FetchResponseResult::Success {
                response_type: FetchResponseType::Basic,
                urls: vec![request.head.url],
                status: 200,
                headers: vec![("content-type".into(), "text/javascript".into())],
            },
        })
        .expect("start replacement script response");
        sink.end(request_id, 0)
            .expect("finish replacement script response");
    }
}

fn saturate_command_queue(session: &RendererSession) {
    session
        .send_test_command(TestCommand::DelayCommandRead { millis: 1_200 })
        .expect("start finite renderer command-read stall");
    let mut saturated = false;
    for _ in 0..256 {
        match session.send_test_command(TestCommand::Padding { bytes: 60 * 1024 }) {
            Ok(()) => {}
            Err(error) if error.contains("command queue is full") => {
                saturated = true;
                break;
            }
            Err(error) => panic!("unexpected command enqueue failure: {error}"),
        }
    }
    assert!(
        saturated,
        "finite child stall did not apply bounded command backpressure"
    );
}

fn wait_for_navigation(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
) -> (String, NavigationDisposition, NavigationCause) {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::NavigationRequested {
                document: event_document,
                url,
                disposition,
                cause,
            } if event_document == document => return (url, disposition, cause),
            RendererEvent::Diagnostic { .. } | RendererEvent::TimeAdvanced { .. } => {}
            event => panic!("unexpected navigation event: {event:?}"),
        }
    }
}

fn wait_for_text(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    expected: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match session.wait_for_event(Duration::from_millis(500)) {
            Ok(RendererEvent::Presentation(presentation)) if presentation.document == document => {
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
            Ok(RendererEvent::Diagnostic { .. } | RendererEvent::TimeAdvanced { .. }) | Err(_) => {}
            Ok(event) => panic!("unexpected backpressure event: {event:?}"),
        }
    }
    panic!("retained final input was not presented");
}
