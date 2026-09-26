//! A typed URL-backed H.264-only source reaches the contained decoder and video clock.

use super::*;
use std::time::Instant;

#[test]
fn contained_renderer_plays_url_backed_h264_without_audio() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    launch.unresponsive_timeout = Duration::from_millis(500);
    let mut session =
        RendererSession::launch(launch).expect("launch hidden renderer and media worker");
    let document = better_web_browser::renderer_protocol::DocumentId::new(199).unwrap();
    let html = br#"<!doctype html><video id='movie' muted width='320' height='240'>
        <source src='/silent.mp4' type='video/mp4; codecs="avc1.42E01E"'>
        </video><output id='state'>waiting</output><script>
        const phases = [];
        const publish = value => { phases.push(value); state.textContent = phases.join('|'); };
        movie.addEventListener('loadeddata', async () => {
          try {
            publish('loaded:' + movie.audioTracks.length + ':' + movie.currentSrc);
            await movie.play();
            publish('playing');
            movie.pause();
            publish('paused');
            movie.currentTime = 0.5;
          } catch (error) { state.textContent = 'error:' + error.name; }
        });
        movie.addEventListener('seeked', () => {
          publish('seeked:' + movie.currentTime.toFixed(1));
          movie.play().catch(error => state.textContent = 'replay-error:' + error.name);
        });
        movie.addEventListener('ended', () => {
          publish('ended');
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
    let request = loop {
        assert!(
            Instant::now() < deadline,
            "H.264-only source was not requested"
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
                acknowledge(&session, &presentation);
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected H.264-only source event: {event:?}"),
        }
    };
    assert_eq!(request.head.destination, ResourceDestination::Video);
    assert_eq!(request.head.url, "https://example.test/silent.mp4");
    let bytes = decode_base64(include_str!(
        "../../fixtures/media/test-1s-video-fragmented.mp4.base64"
    ));
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id: request.head.request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![request.head.url],
            status: 200,
            headers: vec![("content-type".into(), "video/mp4".into())],
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

    let mut saw_frame = false;
    let mut saw_codec = false;
    let mut saw_ended = false;
    let mut last_state = String::new();
    while !(saw_frame && saw_codec && saw_ended) {
        assert!(
            Instant::now() < deadline,
            "H.264-only playback did not finish"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap_or_else(|error| {
            panic!("H.264-only playback event: {error}; frame={saw_frame} codec={saw_codec} ended={saw_ended} state={last_state}")
        }) {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                saw_frame |= presentation.images.iter().any(|image| {
                    image.url.starts_with("breeze-internal:media-frame:")
                        && (image.image.width, image.image.height) == (320, 240)
                });
                saw_ended |= presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, .. }
                        if text.contains("loaded:0:https://example.test/silent.mp4|playing|paused|seeked:0.5|ended"))
                });
                if let Some(text) = presentation.layout.items.iter().find_map(|item| {
                    match item {
                        DisplayItem::Text { text, .. } if text.contains("loaded:")
                            || text.contains("error:") => Some(text.as_str()),
                        _ => None,
                    }
                }) {
                    last_state = text.into();
                }
                if let Some(media) = presentation.runtime.media.as_ref() {
                    saw_codec |= media.video_codec == "H.264" && media.audio_codec == "none";
                }
                acknowledge(&session, &presentation);
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::VideoFrame(update) if update.identity.document == document => {
                saw_frame |= (update.identity.width, update.identity.height) == (320, 240);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected H.264-only playback event: {event:?}"),
        }
    }
    session
        .shutdown()
        .expect("stop hidden renderer and media worker");
}
