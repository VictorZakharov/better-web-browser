//! An M4A MIME alias must select a real AAC source and reach the contained decoder.

use super::*;
use std::time::Instant;

#[test]
fn m4a_source_type_and_response_alias_decode_real_aac_audio() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    launch.unresponsive_timeout = Duration::from_millis(500);
    let mut session = RendererSession::launch(launch).expect("launch hidden media renderer");
    let document = better_web_browser::renderer_protocol::DocumentId::new(202).unwrap();
    let html = r#"<!doctype html><title>M4A alias</title>
        <audio id=sound muted><source src=/alias.m4a type='audio/x-m4a; codecs="mp4a.40.2"'></audio>
        <output id=state>waiting</output><script>
            sound.addEventListener('loadeddata', () => {
                state.textContent = 'decoded:' + sound.currentSrc;
            });
            sound.play().catch(error => state.textContent = 'play-error:' + error.name);
        </script>"#;
    let body = html.as_bytes().to_vec();
    session
        .load_document(
            document_start(document, body.len()),
            empty_document_state(),
            body,
        )
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(15);
    let request = loop {
        assert!(Instant::now() < deadline, "M4A source was not selected");
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::FetchBatch {
                document: actual,
                requests,
            } if actual == document => {
                assert_eq!(requests.len(), 1);
                break requests.into_iter().next().unwrap();
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
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
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before M4A request: {event:?}"),
        }
    };
    assert_eq!(request.head.destination, ResourceDestination::Audio);
    assert_eq!(request.head.url, "https://example.test/alias.m4a");
    let bytes = decode_base64(include_str!(
        "../../fixtures/media/test-1s-audio-fragmented.mp4.base64"
    ));
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id: request.head.request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![request.head.url],
            status: 200,
            headers: vec![
                ("content-type".into(), "audio/x-m4a".into()),
                ("content-length".into(), bytes.len().to_string()),
            ],
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

    let mut loaded = false;
    let mut decoded = false;
    while !loaded || !decoded {
        assert!(Instant::now() < deadline, "M4A decode did not settle");
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                loaded |= presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("decoded:https://example.test/alias.m4a"))
                });
                if let Some(media) = presentation.runtime.media.as_ref() {
                    // Media Foundation reports its AAC family, not the parsed profile.
                    assert_eq!(media.audio_codec, "AAC");
                    assert_eq!(media.video_codec, "none");
                    decoded = true;
                }
                assert!(
                    presentation
                        .images
                        .iter()
                        .all(|image| { !image.url.starts_with("breeze-internal:media-frame:") })
                );
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
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::VideoFrame(_) => panic!("M4A audio produced a video frame"),
            event => panic!("unexpected M4A decode event: {event:?}"),
        }
    }
    session
        .shutdown()
        .expect("shutdown contained media renderer");
}
