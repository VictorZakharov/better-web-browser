//! A native FLAC `<source>` must reach the contained decoder without fabricating video.

use super::*;
use better_web_browser::media_process::{MediaLaunchOptions, MediaSession};
use std::time::Instant;

fn host_has_flac_decoder() -> bool {
    let mut options = MediaLaunchOptions::new(env!("CARGO_BIN_EXE_better-web-browser"));
    options.test_mode = true;
    options.silent_audio = true;
    options.command_timeout = Duration::from_secs(3);
    let mut session = MediaSession::launch(options).expect("launch hidden codec probe");
    let report = session.probe().expect("probe native FLAC availability");
    assert!(
        report.startup_hresult >= 0 && report.flac_hresult >= 0,
        "FLAC decoder probe failed: {report:?}"
    );
    session.shutdown().expect("stop hidden codec probe");
    report.flac_decoders > 0
}

#[test]
fn native_flac_source_type_decodes_in_contained_renderer() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !host_has_flac_decoder() {
        eprintln!(
            "skipping renderer FLAC source: this Windows host has no Media Foundation FLAC decoder"
        );
        return;
    }
    let mut launch = options();
    launch.enable_media = true;
    launch.unresponsive_timeout = Duration::from_millis(500);
    let mut session = RendererSession::launch(launch).expect("launch hidden media renderer");
    let document = better_web_browser::renderer_protocol::DocumentId::new(203).unwrap();
    let html = r#"<!doctype html><title>Native FLAC</title>
        <audio id=sound muted><source src=/tone.flac type='audio/flac'></audio>
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
        assert!(Instant::now() < deadline, "FLAC source was not selected");
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
            event => panic!("unexpected event before FLAC request: {event:?}"),
        }
    };
    assert_eq!(request.head.destination, ResourceDestination::Audio);
    assert_eq!(request.head.url, "https://example.test/tone.flac");
    let bytes = decode_base64(include_str!(
        "../../fixtures/media/test-1s-audio.flac.base64"
    ));
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id: request.head.request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![request.head.url],
            status: 200,
            headers: vec![
                ("content-type".into(), "audio/flac".into()),
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
        assert!(Instant::now() < deadline, "FLAC decode did not settle");
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                loaded |= presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("decoded:https://example.test/tone.flac"))
                });
                if let Some(media) = presentation.runtime.media.as_ref() {
                    assert_eq!(media.audio_codec, "FLAC");
                    assert_eq!(media.video_codec, "none");
                    decoded = true;
                }
                assert!(
                    presentation
                        .images
                        .iter()
                        .all(|image| !image.url.starts_with("breeze-internal:media-frame:"))
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
            RendererEvent::VideoFrame(_) => panic!("FLAC audio produced a video frame"),
            event => panic!("unexpected FLAC decode event: {event:?}"),
        }
    }
    session.shutdown().expect("stop contained media renderer");
}
