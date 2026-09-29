//! Capture frame event handlers can synchronously change the selected media source.

use super::*;

#[test]
fn detaching_src_object_during_a_frame_event_cannot_paint_the_old_frame() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let document = DocumentId::new(862).unwrap();
    let html = br#"<!doctype html><video id="preview" muted></video><p id="state">pending</p>
        <script>
          navigator.mediaDevices.getUserMedia({video: true}).then(stream => {
            preview.srcObject = stream;
            preview.addEventListener('timeupdate', () => {
              preview.srcObject = null;
              state.textContent = 'detached:' + (preview.srcObject === null);
            }, {once: true});
            state.textContent = 'ready';
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
    let sink = session.media_capture_sink(document);
    sink.try_send_update(MediaCaptureUpdate {
        document,
        request_id: request.request_id,
        event: MediaCaptureEvent::Started {
            camera: true,
            microphone: false,
        },
    })
    .unwrap();
    wait_for_text(&session, document, "ready");

    sink.try_send_frame(nv12_frame(document, request.request_id, 1, 235))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "frame listener did not detach preview"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                assert!(
                    presented_capture_bitmap(&presentation).is_none(),
                    "a frame was painted after the listener detached srcObject"
                );
                let detached = presentation_text(&presentation).contains("detached:true");
                acknowledge(&session, &presentation);
                if detached {
                    break;
                }
            }
            RendererEvent::VideoFrame(frame) if frame.identity.document == document => {
                panic!("a direct frame was painted after the listener detached srcObject");
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected capture detach event: {event:?}"),
        }
    }
    session.shutdown().unwrap();
}
