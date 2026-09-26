//! A replaced URL response must not install after the new audio source is selected.

use super::*;
use std::time::Instant;

fn next_audio_request(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    deadline: Instant,
    drive_timers: bool,
) -> better_web_browser::renderer_protocol::RendererFetchRequest {
    loop {
        assert!(Instant::now() < deadline, "audio fetch was not dispatched");
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::FetchBatch { requests, .. } => {
                assert_eq!(requests.len(), 1);
                return requests.into_iter().next().unwrap();
            }
            RendererEvent::Presentation(presentation) => {
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                if drive_timers {
                    run_scheduled_renderer_timer(session, document, presentation.next_timer_micros);
                }
            }
            RendererEvent::RuntimeUpdate(update) => {
                if drive_timers {
                    run_scheduled_renderer_timer(session, document, update.next_timer_micros);
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before audio fetch: {event:?}"),
        }
    }
}

#[test]
fn old_audio_response_does_not_install_after_src_replacement() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session =
        RendererSession::launch(launch).expect("launch hidden renderer and media worker");
    let document = better_web_browser::renderer_protocol::DocumentId::new(198).unwrap();
    let html = br#"<!doctype html><audio id='sound' src='/old.mp3' muted></audio>
        <output id='state'>waiting</output><script>
        const loaded = [];
        sound.addEventListener('loadedmetadata', () => {
            loaded.push(sound.currentSrc);
            state.textContent = 'loaded:' + loaded.join('|');
        });
        state.textContent = 'script-ready';
        setTimeout(() => { sound.src = '/new.mp3'; }, 100);
        </script>"#;
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html.to_vec(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let old = next_audio_request(&session, document, deadline, false);
    assert_eq!(old.head.url, "https://example.test/old.mp3");
    loop {
        assert!(Instant::now() < deadline, "parser script did not run");
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                let ready = presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. } if text.contains("script-ready"))
                });
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                if ready {
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(_) | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before parser script: {event:?}"),
        }
    }
    session
        .advance_time(document, Duration::from_millis(100), 64)
        .expect("replace audio src before old response completes");
    let replacement = next_audio_request(&session, document, deadline, true);
    assert_eq!(replacement.head.url, "https://example.test/new.mp3");
    let bytes = decode_base64(include_str!(
        "../../fixtures/media/test-1s-audio.mp3.base64"
    ));
    let sink = session.fetch_response_sink(document);
    for request in [old, replacement] {
        sink.start(FetchResponseHead {
            request_id: request.head.request_id,
            result: FetchResponseResult::Success {
                response_type: FetchResponseType::Basic,
                urls: vec![request.head.url],
                status: 200,
                headers: vec![("content-type".into(), "audio/mpeg".into())],
            },
        })
        .unwrap();
        sink.chunk(TransferChunk {
            transfer_id: request.head.request_id,
            offset: 0,
            bytes: bytes.clone(),
        })
        .unwrap();
        sink.end(request.head.request_id, bytes.len() as u32)
            .unwrap();
    }
    loop {
        assert!(
            Instant::now() < deadline,
            "replacement audio metadata did not load"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                let loaded = presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("loaded:https://example.test/new.mp3"))
                });
                let stale = presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("old.mp3"))
                });
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                assert!(!stale, "old response dispatched loadedmetadata");
                if loaded {
                    break;
                }
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected replacement audio event: {event:?}"),
        }
    }
    session
        .shutdown()
        .expect("shutdown contained playback pair");
}

#[test]
fn load_refetches_unchanged_audio_src_after_decoding() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session =
        RendererSession::launch(launch).expect("launch hidden renderer and media worker");
    let document = better_web_browser::renderer_protocol::DocumentId::new(201).unwrap();
    let html = br#"<!doctype html><audio id='sound' src='/same.mp3' muted></audio>
        <output id='state'>waiting</output><script>
        let loads = 0;
        sound.addEventListener('loadedmetadata', () => {
            loads++;
            state.textContent = 'loaded:' + loads + ':' + sound.currentSrc;
            if (loads === 1) setTimeout(() => sound.load(), 100);
        });
        </script>"#;
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html.to_vec(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let first = next_audio_request(&session, document, deadline, true);
    assert_eq!(first.head.url, "https://example.test/same.mp3");
    let bytes = decode_base64(include_str!(
        "../../fixtures/media/test-1s-audio.mp3.base64"
    ));
    let sink = session.fetch_response_sink(document);
    let respond = |request: better_web_browser::renderer_protocol::RendererFetchRequest| {
        sink.start(FetchResponseHead {
            request_id: request.head.request_id,
            result: FetchResponseResult::Success {
                response_type: FetchResponseType::Basic,
                urls: vec![request.head.url],
                status: 200,
                headers: vec![("content-type".into(), "audio/mpeg".into())],
            },
        })
        .unwrap();
        sink.chunk(TransferChunk {
            transfer_id: request.head.request_id,
            offset: 0,
            bytes: bytes.clone(),
        })
        .unwrap();
        sink.end(request.head.request_id, bytes.len() as u32)
            .unwrap();
    };
    let first_id = first.head.request_id;
    respond(first);
    let second = next_audio_request(&session, document, deadline, true);
    assert_eq!(second.head.url, "https://example.test/same.mp3");
    assert_ne!(second.head.request_id, first_id);
    respond(second);
    loop {
        assert!(
            Instant::now() < deadline,
            "reloaded audio metadata did not fire"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                let loaded_twice = presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("loaded:2:https://example.test/same.mp3"))
                });
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                if loaded_twice {
                    break;
                }
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected reloaded audio event: {event:?}"),
        }
    }
    session.shutdown().expect("shutdown contained media pair");
}
