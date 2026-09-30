//! Browser-supplied capture grant and samples in the hidden AppContainer renderer.
//! These tests never enumerate devices or start a real camera or microphone.

use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, MediaCaptureAction, MediaCaptureEvent, MediaCaptureFrame, MediaCaptureFrameKind,
    MediaCaptureRequest, MediaCaptureUpdate, RendererPresentation,
};
use std::time::{Duration, Instant};

#[path = "capture/frame_delivery.rs"]
mod frame_delivery;
#[path = "capture/opus_recorder.rs"]
mod opus_recorder;

#[test]
fn capture_grant_presents_nv12_preview_and_ignores_frame_after_video_track_ended() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(860).unwrap();
    let html = br#"<!doctype html><video id="preview" muted></video><p id="state">pending</p>
        <script>
          navigator.mediaDevices.getUserMedia({video: true, audio: true}).then(stream => {
            const video = stream.getVideoTracks()[0];
            preview.srcObject = stream;
            state.textContent = 'granted:' + stream.getVideoTracks().length + ':' +
              stream.getAudioTracks().length + ':' + (preview.srcObject === stream) + ':' +
              video.readyState;
            preview.addEventListener('loadeddata', () => {
              state.textContent = 'frame:' + preview.videoWidth + 'x' + preview.videoHeight + ':' +
                video.getSettings().width + 'x' + video.getSettings().height;
            });
            video.addEventListener('ended', () => {
              state.textContent = 'ended:' + video.readyState + ':' + stream.active;
            });
          }, error => state.textContent = 'error:' + error.name);
        </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let request = wait_for_capture_start(&session, document);
    assert_eq!(
        request.action,
        MediaCaptureAction::Start {
            camera: true,
            microphone: true
        }
    );
    assert_eq!(request.client.id, 0);
    assert!(!request.client.opaque);
    let sink = session.media_capture_sink(document);
    sink.try_send_update(MediaCaptureUpdate {
        document,
        request_id: request.request_id,
        event: MediaCaptureEvent::Started {
            camera: true,
            microphone: true,
        },
    })
    .unwrap();
    wait_for_text(&session, document, "granted:1:1:true:live");

    sink.try_send_frame(nv12_frame(document, request.request_id, 1, 16))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_dimensions = false;
    let mut saw_black_bitmap = false;
    while !(saw_dimensions && saw_black_bitmap) {
        assert!(Instant::now() < deadline, "capture preview did not render");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                saw_dimensions |= presentation_text(&presentation).contains("frame:2x2:2x2");
                saw_black_bitmap |= presented_capture_bitmap(&presentation)
                    .is_some_and(|pixels| pixels == [0, 0, 0, 255].repeat(4));
                acknowledge(&session, &presentation);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected capture preview event: {event:?}"),
        }
    }

    sink.try_send_update(MediaCaptureUpdate {
        document,
        request_id: request.request_id,
        event: MediaCaptureEvent::TrackEnded { track_id: 1 },
    })
    .unwrap();
    wait_for_text(&session, document, "ended:ended:true");

    // A late bright frame must not replace the black preview after the camera track ended.
    sink.try_send_frame(nv12_frame(document, request.request_id, 2, 235))
        .unwrap();
    session.ping(Duration::from_secs(2)).unwrap();
    session.ping(Duration::from_secs(2)).unwrap();
    // The second pong follows the sample on the renderer pipe. A correct renderer emits no
    // presentation for that stale sample, so inspect any queued events with a bounded wait.
    let deadline = Instant::now() + Duration::from_millis(250);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match session.wait_for_event(remaining) {
            Ok(RendererEvent::Presentation(presentation)) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                assert!(
                    !presentation.images.iter().any(|image| {
                        image.url.starts_with("breeze-internal:media-frame:")
                            && image.image.bgra.as_ref() == [255, 255, 255, 255].repeat(4)
                    }),
                    "late frame repainted the ended camera track"
                );
                acknowledge(&session, &presentation);
            }
            Ok(RendererEvent::VideoFrame(frame)) if frame.identity.document == document => {
                assert_ne!(frame.identity.frame, 2, "late frame reached the preview");
            }
            Ok(RendererEvent::RuntimeUpdate(update)) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            Ok(RendererEvent::Diagnostic { .. }) => {}
            Err(error) if error == "renderer event timed out" => break,
            event => panic!("unexpected post-end capture event: {event:?}"),
        }
    }
    session.ping(Duration::from_secs(2)).unwrap();
    session.shutdown().unwrap();
}

#[test]
fn capture_stream_stops_video_and_audio_tracks_independently() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(861).unwrap();
    let html = br#"<!doctype html><p id="state">pending</p><script>
        navigator.mediaDevices.getUserMedia({video: true, audio: true}).then(stream => {
          const video = stream.getVideoTracks()[0];
          const audio = stream.getAudioTracks()[0];
          video.stop();
          video.stop();
          const audioRemainedLive = video.readyState === 'ended' &&
            audio.readyState === 'live' && stream.active;
          audio.stop();
          audio.stop();
          state.textContent = 'stopped:' + audioRemainedLive + ':' +
            (audio.readyState === 'ended' && !stream.active);
        }, error => state.textContent = 'error:' + error.name);
        </script>"#
        .to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let request = wait_for_capture_start(&session, document);
    session
        .media_capture_sink(document)
        .try_send_update(MediaCaptureUpdate {
            document,
            request_id: request.request_id,
            event: MediaCaptureEvent::Started {
                camera: true,
                microphone: true,
            },
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stops = Vec::new();
    let mut saw_state = false;
    while stops.len() < 2 || !saw_state {
        assert!(
            Instant::now() < deadline,
            "capture tracks did not stop independently"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::MediaCaptureRequest(stop) if stop.document == document => {
                assert_eq!(stop.request_id, request.request_id);
                let MediaCaptureAction::Stop { track_id } = stop.action else {
                    panic!("unexpected second capture start: {stop:?}");
                };
                stops.push(track_id);
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                saw_state |= presentation_text(&presentation).contains("stopped:true:true");
                acknowledge(&session, &presentation);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected capture stop event: {event:?}"),
        }
    }
    assert_eq!(stops, [1, 2]);
    session.shutdown().unwrap();
}

fn nv12_frame(document: DocumentId, request_id: u64, sequence: u64, luma: u8) -> MediaCaptureFrame {
    MediaCaptureFrame {
        document,
        request_id,
        track_id: 1,
        sequence,
        timestamp_100ns: sequence * 1_000_000,
        kind: MediaCaptureFrameKind::VideoNv12,
        width_or_rate: 2,
        height_or_frames: 2,
        stride_or_channels: 2,
        bytes: vec![luma, luma, luma, luma, 128, 128],
    }
}

fn wait_for_capture_start(session: &RendererSession, document: DocumentId) -> MediaCaptureRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "no browser-authorized capture request"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::MediaCaptureRequest(request) if request.document == document => {
                return request;
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                acknowledge(session, &presentation);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before capture request: {event:?}"),
        }
    }
}

fn wait_for_text(session: &RendererSession, document: DocumentId, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "capture result did not render {expected}"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                let found = presentation_text(&presentation).contains(expected);
                acknowledge(session, &presentation);
                if found {
                    return;
                }
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected capture result event: {event:?}"),
        }
    }
}

fn presentation_text(presentation: &RendererPresentation) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn presented_capture_bitmap(presentation: &RendererPresentation) -> Option<&[u8]> {
    let key = presentation
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image { url, .. } if url.starts_with("breeze-internal:media-frame:") => {
                Some(url.as_str())
            }
            _ => None,
        })?;
    presentation
        .images
        .iter()
        .find(|image| image.url == key)
        .map(|image| image.image.bgra.as_ref())
}
