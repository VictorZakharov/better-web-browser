//! HTMLAudioElement loads a real MP3 without ever receiving a synthetic video frame.

use super::*;
use std::time::Instant;

#[test]
fn contained_renderer_loads_plays_and_seeks_audio_only_mp3() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    launch.unresponsive_timeout = Duration::from_millis(500);
    let mut session =
        RendererSession::launch(launch).expect("launch hidden renderer and media worker");
    let document = better_web_browser::renderer_protocol::DocumentId::new(195).unwrap();
    let html = r#"<!doctype html><title>audio-only</title>
        <audio id="sound" src="/test.mp3" muted></audio>
        <output id="state">waiting</output><script>
            sound.addEventListener('loadeddata', () => {
                sound.play().then(() => {
                    sound.currentTime = 0.5;
                }).catch(error => state.textContent = 'play-error:' + error.name);
            });
            sound.addEventListener('seeked', () => {
                state.textContent = 'seeked:' + sound.currentTime.toFixed(1)
                    + ':' + sound.currentSrc;
            });
        </script>"#;
    let body = html.as_bytes().to_vec();
    session
        .load_document(
            document_start(document, body.len()),
            empty_document_state(),
            body,
        )
        .unwrap();

    let mut request = None;
    let deadline = Instant::now() + Duration::from_secs(15);
    while request.is_none() {
        assert!(
            Instant::now() < deadline,
            "audio request was not dispatched"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::FetchBatch {
                document: actual,
                requests,
            } if actual == document => {
                assert_eq!(requests.len(), 1);
                request = requests.into_iter().next();
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
            event => panic!("unexpected event before audio request: {event:?}"),
        }
    }
    let request = request.unwrap();
    let request_id = request.head.request_id;
    let bytes = decode_base64(include_str!(
        "../../fixtures/media/test-1s-audio.mp3.base64"
    ));
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![request.head.url],
            status: 200,
            headers: vec![
                ("content-type".into(), "audio/mpeg".into()),
                ("content-length".into(), bytes.len().to_string()),
            ],
        },
    })
    .unwrap();
    sink.chunk(TransferChunk {
        transfer_id: request_id,
        offset: 0,
        bytes: bytes.clone(),
    })
    .unwrap();
    sink.end(request_id, bytes.len() as u32).unwrap();

    let mut saw_seeked = false;
    let mut saw_audio_runtime = false;
    while !saw_seeked || !saw_audio_runtime {
        assert!(
            Instant::now() < deadline,
            "audio-only play/seek did not settle"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation
                        .images
                        .iter()
                        .all(|image| { !image.url.starts_with("breeze-internal:media-frame:") }),
                    "audio source fabricated a video frame"
                );
                saw_seeked |= presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("seeked:0.5:https://example.test/test.mp3"))
                });
                if let Some(media) = presentation.runtime.media.as_ref() {
                    assert_eq!((media.width, media.height), (0, 0));
                    assert_eq!(media.video_codec, "none");
                    assert_eq!(media.audio_codec, "MP3");
                    saw_audio_runtime = true;
                }
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
            RendererEvent::VideoFrame(_) => panic!("audio-only playback emitted a video frame"),
            event => panic!("unexpected audio-only event: {event:?}"),
        }
    }
    session
        .shutdown()
        .expect("shutdown contained renderer and media worker");
}

fn verify_audio_failure(status: u16, bytes: Vec<u8>, expected_code: u8) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session =
        RendererSession::launch(launch).expect("launch hidden renderer and media worker");
    let document = better_web_browser::renderer_protocol::DocumentId::new(196).unwrap();
    let html = r#"<!doctype html><audio id="sound" src="/broken.mp3"></audio>
        <output id="state">waiting</output><script>
        let code = 0, rejection = '';
        const update = () => state.textContent = 'error:' + code + ':' + rejection;
        sound.addEventListener('error', () => { code = sound.error.code; update(); });
        sound.play().catch(error => { rejection = error.name; update(); });
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
        assert!(
            Instant::now() < deadline,
            "failed audio request was not dispatched"
        );
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
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before failed audio response: {event:?}"),
        }
    };
    assert_eq!(request.head.destination, ResourceDestination::Audio);
    let request_id = request.head.request_id;
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![request.head.url],
            status,
            headers: vec![("content-type".into(), "audio/mpeg".into())],
        },
    })
    .unwrap();
    if !bytes.is_empty() {
        sink.chunk(TransferChunk {
            transfer_id: request_id,
            offset: 0,
            bytes: bytes.clone(),
        })
        .unwrap();
    }
    sink.end(request_id, bytes.len() as u32).unwrap();
    let expected = format!("error:{expected_code}:NotSupportedError");
    loop {
        assert!(
            Instant::now() < deadline,
            "audio failure event/promise did not settle"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                let settled = presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. } if text.contains(&expected))
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
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event while waiting for audio failure: {event:?}"),
        }
    }
    session
        .shutdown()
        .expect("shutdown contained playback pair");
}

#[test]
fn audio_404_reports_source_not_supported_and_rejects_pending_play() {
    verify_audio_failure(404, Vec::new(), 4);
}

#[test]
fn audio_invalid_bytes_report_source_not_supported_and_reject_pending_play() {
    verify_audio_failure(200, b"not a media stream".to_vec(), 4);
}
