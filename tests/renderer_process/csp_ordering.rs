use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, FetchResponseHead, FetchResponseResult, FetchResponseType,
};
use std::time::{Duration, Instant};

#[test]
fn fetch_callback_sends_inserted_meta_policy_before_followup_fetch() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        196,
        r#"<script>
            fetch('/trigger').then(() => {
                const meta = document.createElement('meta');
                meta.setAttribute('http-equiv', 'Content-Security-Policy');
                meta.setAttribute('content', "connect-src 'none'");
                document.head.appendChild(meta);
                fetch('/blocked');
            });
        </script>"#,
    );
    session
        .advance_time(initial.document, Duration::from_millis(1), 8)
        .unwrap();
    let trigger = next_fetch(&session, initial.document, "/trigger");
    let id = trigger.head.request_id;
    let sink = session.fetch_response_sink(initial.document);
    sink.start(FetchResponseHead {
        request_id: id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![trigger.head.url],
            status: 200,
            headers: Vec::new(),
        },
    })
    .unwrap();
    sink.end(id, 0).unwrap();
    assert_policy_before_fetch(&session, initial.document, "/blocked");
    session.shutdown().unwrap();
}

#[test]
fn timer_sends_inserted_meta_policy_before_followup_fetch() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        197,
        r#"<script>
            setTimeout(() => {
                const meta = document.createElement('meta');
                meta.setAttribute('http-equiv', 'Content-Security-Policy');
                meta.setAttribute('content', "connect-src 'none'");
                document.head.appendChild(meta);
                fetch('/blocked');
            }, 10);
        </script>"#,
    );
    acknowledge(&session, &initial);
    assert_timer_policy_before_fetch(&session, initial.document, "/blocked");
    session.shutdown().unwrap();
}

#[test]
fn resource_error_sends_inserted_meta_policy_before_followup_fetch() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let document = DocumentId::new(198).unwrap();
    let html = r#"<!doctype html><body><img id=first src=/first.png><script>
        document.getElementById('first').onerror = () => {
            const meta = document.createElement('meta');
            meta.setAttribute('http-equiv', 'Content-Security-Policy');
            meta.setAttribute('content', "img-src 'none'");
            document.head.appendChild(meta);
            const image = document.createElement('img');
            image.src = '/blocked.png';
            document.body.appendChild(image);
        };
    </script>"#;
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html.as_bytes().to_vec(),
        )
        .unwrap();
    let first = next_fetch(&session, document, "/first.png");
    let id = first.head.request_id;
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id: id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![first.head.url],
            status: 404,
            headers: Vec::new(),
        },
    })
    .unwrap();
    sink.end(id, 0).unwrap();
    assert_policy_before_fetch_with_policy(
        &session,
        document,
        "/blocked.png",
        "img-src 'none'",
        true,
    );
    session.shutdown().unwrap();
}

fn next_fetch(
    session: &RendererSession,
    document: DocumentId,
    path: &str,
) -> better_web_browser::renderer_protocol::RendererFetchRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "no initial Fetch for {path}");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::FetchBatch {
                document: owner,
                mut requests,
            } if owner == document => {
                assert_eq!(requests.len(), 1);
                let request = requests.pop().unwrap();
                assert!(request.head.url.ends_with(path));
                return request;
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                acknowledge(session, &presentation);
            }
            event => panic!("unexpected event before initial Fetch: {event:?}"),
        }
    }
}

fn assert_policy_before_fetch(session: &RendererSession, document: DocumentId, path: &str) {
    assert_policy_before_fetch_with_policy(session, document, path, "connect-src 'none'", false);
}

fn assert_policy_before_fetch_with_policy(
    session: &RendererSession,
    document: DocumentId,
    path: &str,
    policy: &str,
    advance_on_runtime_update: bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_policy = false;
    loop {
        assert!(Instant::now() < deadline, "no followup Fetch for {path}");
        match session
            .wait_for_event(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("{error}; saw policy: {saw_policy}"))
        {
            RendererEvent::PolicyMutation(update) if update.document == document => {
                assert_eq!(update.serialized, policy);
                saw_policy = true;
            }
            RendererEvent::FetchBatch {
                document: owner,
                requests,
            } if owner == document => {
                assert!(saw_policy, "FetchBatch preceded the browser policy update");
                assert_eq!(requests.len(), 1);
                assert!(requests[0].head.url.ends_with(path));
                return;
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::Presentation(_) => {}
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                if advance_on_runtime_update && saw_policy {
                    session
                        .advance_time(document, Duration::from_millis(1), 8)
                        .unwrap();
                }
            }
            event => panic!("unexpected event before followup Fetch: {event:?}"),
        }
    }
}

fn assert_timer_policy_before_fetch(session: &RendererSession, document: DocumentId, path: &str) {
    let mut saw_policy = false;
    for _ in 0..6 {
        session
            .advance_time(document, Duration::from_secs(2), 1)
            .unwrap();
        loop {
            match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::PolicyMutation(update) if update.document == document => {
                    assert_eq!(update.serialized, "connect-src 'none'");
                    saw_policy = true;
                }
                RendererEvent::FetchBatch {
                    document: owner,
                    requests,
                } if owner == document => {
                    assert!(saw_policy, "FetchBatch preceded the browser policy update");
                    assert_eq!(requests.len(), 1);
                    assert!(requests[0].head.url.ends_with(path));
                    return;
                }
                RendererEvent::RuntimeUpdate(update) if update.document == document => {
                    assert!(
                        update.runtime.errors.is_empty(),
                        "{:?}",
                        update.runtime.errors
                    );
                    break;
                }
                RendererEvent::Presentation(presentation) if presentation.document == document => {
                    break;
                }
                RendererEvent::Diagnostic { .. } => {}
                event => panic!("unexpected event while driving timer: {event:?}"),
            }
        }
    }
    panic!("timer produced no followup Fetch for {path}");
}
