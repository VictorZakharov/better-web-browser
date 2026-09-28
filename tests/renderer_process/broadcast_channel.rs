//! Real hidden AppContainer renderers exchanging a browser-brokered BroadcastChannel message.

use super::support::*;
use better_web_browser::broadcast_channel::BroadcastRegistry;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{BrowsingContextId, DocumentId};
use std::time::{Duration, Instant};

fn launch(id: u64) -> RendererSession {
    let mut launch = options();
    launch.browsing_context = BrowsingContextId::new(id).unwrap();
    RendererSession::launch(launch).expect("launch hidden BroadcastChannel renderer")
}

fn load(session: &RendererSession, id: u64, html: &str) {
    let document = DocumentId::new(id).unwrap();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html.as_bytes().to_vec(),
        )
        .unwrap();
}

#[test]
fn two_isolated_renderers_exchange_a_structured_clone_through_browser_registry() {
    let _serial = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut reader = launch(821);
    let mut writer = launch(822);
    let mut registry = BroadcastRegistry::default();
    load(
        &reader,
        821,
        "<!doctype html><p>pending</p><script>window.channel = new BroadcastChannel('room'); channel.onmessage = event => document.querySelector('p').textContent = [event.data.answer, event.origin, event.isTrusted].join('|');</script>",
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "reader did not open BroadcastChannel"
        );
        match reader.wait_for_event(Duration::from_secs(2)).unwrap() {
            RendererEvent::BroadcastCommand(command) => {
                assert!(
                    registry
                        .apply(821, "https://example.test", &command)
                        .unwrap()
                );
                break;
            }
            RendererEvent::Presentation(presentation) => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected reader event: {event:?}"),
        }
    }
    load(
        &writer,
        822,
        "<!doctype html><script>window.channel = new BroadcastChannel('room'); channel.postMessage({answer: 42});</script>",
    );
    let mut posted = false;
    while !posted {
        assert!(
            Instant::now() < deadline,
            "writer did not post BroadcastChannel message"
        );
        match writer.wait_for_event(Duration::from_secs(2)).unwrap() {
            RendererEvent::BroadcastCommand(command) => {
                posted = matches!(
                    command.operation,
                    better_web_browser::renderer_protocol::BroadcastOperation::Post { .. }
                );
                assert!(
                    registry
                        .apply(822, "https://example.test", &command)
                        .unwrap()
                );
            }
            RendererEvent::Presentation(presentation) => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected writer event: {event:?}"),
        }
    }
    assert!(!registry.has_pending(822));
    assert!(registry.take_if(821, |delivery| {
        reader
            .try_send_broadcast_delivery(delivery.clone())
            .unwrap()
    }));
    let expected = "42|https://example.test|true";
    loop {
        assert!(
            Instant::now() < deadline,
            "reader did not render BroadcastChannel delivery"
        );
        match reader.wait_for_event(Duration::from_secs(2)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
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
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected reader event: {event:?}"),
        }
    }
    writer.shutdown().unwrap();
    reader.shutdown().unwrap();
}
