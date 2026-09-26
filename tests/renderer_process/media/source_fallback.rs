use super::*;
use std::time::Instant;

#[test]
fn failed_audio_source_falls_back_and_resolves_early_play() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session =
        RendererSession::launch(launch).expect("launch hidden renderer and media worker");
    let document = better_web_browser::renderer_protocol::DocumentId::new(197).unwrap();
    let html = br#"<!doctype html><audio id='sound' muted>
        <source id='broken' src='/missing.mp3' type='audio/mpeg'>
        <source src='/good.mp3' type='audio/mpeg'></audio>
        <output id='state'>waiting</output><script>
        let sourceErrors = 0;
        broken.addEventListener('error', () => sourceErrors++);
        sound.addEventListener('error', () => state.textContent = 'media-error');
        sound.play().then(() => {
            state.textContent = 'playing:' + sourceErrors + ':' + sound.currentSrc;
        }).catch(error => state.textContent = 'play-error:' + error.name);
        </script>"#;
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html.to_vec(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let first = loop {
        assert!(
            Instant::now() < deadline,
            "first audio candidate was not requested"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::FetchBatch { requests, .. } => {
                break requests.into_iter().next().unwrap();
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
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before first audio candidate: {event:?}"),
        }
    };
    assert_eq!(first.head.url, "https://example.test/missing.mp3");
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id: first.head.request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![first.head.url],
            status: 404,
            headers: Vec::new(),
        },
    })
    .unwrap();
    sink.end(first.head.request_id, 0).unwrap();
    let fallback = loop {
        assert!(
            Instant::now() < deadline,
            "fallback audio candidate was not requested"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::FetchBatch { requests, .. } => {
                break requests.into_iter().next().unwrap();
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
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before fallback audio candidate: {event:?}"),
        }
    };
    assert_eq!(fallback.head.url, "https://example.test/good.mp3");
    let bytes = decode_base64(include_str!(
        "../../fixtures/media/test-1s-audio.mp3.base64"
    ));
    sink.start(FetchResponseHead {
        request_id: fallback.head.request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![fallback.head.url],
            status: 200,
            headers: vec![("content-type".into(), "audio/mpeg".into())],
        },
    })
    .unwrap();
    sink.chunk(TransferChunk {
        transfer_id: fallback.head.request_id,
        offset: 0,
        bytes: bytes.clone(),
    })
    .unwrap();
    sink.end(fallback.head.request_id, bytes.len() as u32)
        .unwrap();
    loop {
        assert!(
            Instant::now() < deadline,
            "fallback play promise did not settle"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                let settled = presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("playing:1:https://example.test/good.mp3"))
                });
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                if settled {
                    break;
                }
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event during fallback playback: {event:?}"),
        }
    }
    session
        .shutdown()
        .expect("shutdown contained playback pair");
}

#[test]
fn last_failed_child_source_rejects_early_play_without_media_element_error() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session =
        RendererSession::launch(launch).expect("launch hidden renderer and media worker");
    let document = better_web_browser::renderer_protocol::DocumentId::new(200).unwrap();
    let html = br#"<!doctype html><audio id='sound' muted>
        <source id='only' src='/missing.mp3' type='audio/mpeg'></audio>
        <output id='state'>waiting</output><script>
        let sourceErrors = 0, mediaErrors = 0;
        only.addEventListener('error', () => sourceErrors++);
        sound.addEventListener('error', () => mediaErrors++);
        sound.play().then(() => state.textContent = 'unexpected-play')
            .catch(error => state.textContent =
                'rejected:' + error.name + ':' + sound.networkState +
                ':' + sourceErrors + ':' + mediaErrors);
        </script>"#;
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html.to_vec(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let request = loop {
        assert!(Instant::now() < deadline, "child source was not requested");
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::FetchBatch { requests, .. } => {
                break requests.into_iter().next().unwrap();
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
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before source failure: {event:?}"),
        }
    };
    assert_eq!(request.head.url, "https://example.test/missing.mp3");
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id: request.head.request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![request.head.url],
            status: 404,
            headers: Vec::new(),
        },
    })
    .unwrap();
    sink.end(request.head.request_id, 0).unwrap();
    loop {
        assert!(Instant::now() < deadline, "early play was not rejected");
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                let rejected = presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("rejected:NotSupportedError:3:1:0"))
                });
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                if rejected {
                    break;
                }
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event after source failure: {event:?}"),
        }
    }
    session.shutdown().expect("shutdown contained media pair");
}
