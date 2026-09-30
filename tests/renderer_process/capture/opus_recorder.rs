//! Real MediaRecorder output in the hidden, sandboxed renderer; no real device.

use super::*;

#[path = "opus_recorder/lifecycle.rs"]
mod lifecycle;
#[path = "opus_recorder/round_trip.rs"]
mod round_trip;

fn grant(session: &RendererSession, document: DocumentId, html: &str) -> u64 {
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html.as_bytes().to_vec(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let request = loop {
        assert!(
            Instant::now() < deadline,
            "no Opus microphone grant request"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::MediaCaptureRequest(request) if request.document == document => {
                break request;
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                acknowledge(session, &presentation);
                run_scheduled_renderer_timer(session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                run_scheduled_renderer_timer(session, document, update.next_timer_micros);
            }
            // Cancellation can leave already-serialized envelopes with their old
            // document owner; none may run in the newly granted realm.
            RendererEvent::Presentation(_)
            | RendererEvent::RuntimeUpdate(_)
            | RendererEvent::MediaCaptureRequest(_)
            | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event before Opus capture grant: {event:?}"),
        }
    };
    assert_eq!(
        request.action,
        MediaCaptureAction::Start {
            camera: false,
            microphone: true
        }
    );
    session
        .media_capture_sink(document)
        .try_send_update(MediaCaptureUpdate {
            document,
            request_id: request.request_id,
            event: MediaCaptureEvent::Started {
                camera: false,
                microphone: true,
            },
        })
        .unwrap();
    request.request_id
}

fn audio(
    session: &RendererSession,
    document: DocumentId,
    request: u64,
    sequence: u64,
    rate: u32,
    channels: u32,
    frames: u32,
) {
    let samples = (0..frames).flat_map(|frame| {
        (0..channels).map(move |channel| {
            let frequency = if channel == 0 { 440.0 } else { 660.0 };
            let index = (sequence - 1) * u64::from(rate / 50) + u64::from(frame);
            let phase = index as f64 * frequency * std::f64::consts::TAU / f64::from(rate);
            (phase.sin() * 16_000.0).round() as i16
        })
    });
    session
        .media_capture_sink(document)
        .try_send_frame(MediaCaptureFrame {
            document,
            request_id: request,
            track_id: 2,
            sequence,
            timestamp_100ns: (sequence - 1) * 200_000,
            kind: MediaCaptureFrameKind::AudioPcm16,
            width_or_rate: rate,
            height_or_frames: frames,
            stride_or_channels: channels,
            bytes: samples.flat_map(i16::to_le_bytes).collect(),
        })
        .unwrap();
    // A first pong can precede capture-queue draining. The next pipe checkpoint
    // makes the sample precede a subsequent trusted stop/requestData command.
    session.ping(Duration::from_secs(2)).unwrap();
    session.ping(Duration::from_secs(2)).unwrap();
}

fn wait(session: &RendererSession, document: DocumentId, expected: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "Opus recorder did not present {expected}"
        );
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                let text = presentation_text(&presentation);
                assert!(!text.contains("failed:"), "{text}");
                acknowledge(session, &presentation);
                if text.contains(expected) {
                    return text;
                }
                run_scheduled_renderer_timer(session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                run_scheduled_renderer_timer(session, document, update.next_timer_micros);
            }
            RendererEvent::MediaCaptureRequest(request) if request.document == document => {
                assert!(
                    matches!(request.action, MediaCaptureAction::Stop { .. }),
                    "{request:?}"
                );
            }
            RendererEvent::Diagnostic { .. } => {}
            event => {
                panic!("unexpected Opus recorder event while waiting for {expected}: {event:?}")
            }
        }
    }
}

fn click_recording_control(session: &RendererSession, document: DocumentId, sequence: u64) {
    click(session, document, sequence, 30.0, 20.0);
}
