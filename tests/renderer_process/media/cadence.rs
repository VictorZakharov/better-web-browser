use super::*;
use better_web_browser::renderer_protocol::{
    DocumentInput, InputModifiers, KeyPhase, KeyboardInput,
};
use std::time::Instant;

#[path = "cadence/setup.rs"]
mod setup;

#[test]
fn synthetic_click_does_not_allow_audible_autoplay() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let document = better_web_browser::renderer_protocol::DocumentId::new(194).unwrap();
    let html = br#"<!doctype html><video id="movie"></video><script>
        document.addEventListener('click', () => movie.play().catch(error => console.log(error.name)));
        document.dispatchEvent(new Event('click'));
        </script>"#.to_vec();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let mut rejected = false;
    for _ in 0..20 {
        let runtime = match session.wait_for_event(Duration::from_secs(2)).unwrap() {
            RendererEvent::Presentation(presentation) => presentation.runtime,
            RendererEvent::RuntimeUpdate(update) => update.runtime,
            RendererEvent::Diagnostic { .. } => continue,
            event => panic!("unexpected autoplay event: {event:?}"),
        };
        if runtime
            .console
            .iter()
            .any(|line| line.contains("NotAllowedError"))
        {
            rejected = true;
            break;
        }
        session
            .advance_time(document, Duration::from_millis(20), 8)
            .unwrap();
    }
    assert!(
        rejected,
        "synthetic click bypassed audible-playback permission"
    );
    session.shutdown().unwrap();
}

#[test]
fn video_pixels_advance_while_a_javascript_callback_owns_the_document_thread() {
    busy_callback(false);
}

#[test]
fn advancing_video_does_not_mask_a_document_watchdog_timeout() {
    busy_callback(true);
}

fn busy_callback(expect_timeout: bool) {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    if expect_timeout {
        launch.unresponsive_timeout = Duration::from_millis(500);
        launch.unresponsive_kill_timeout = Duration::from_millis(150);
    }
    let busy_ms = if expect_timeout { 3500 } else { 400 };
    let mut session = RendererSession::launch(launch).unwrap();
    let document = better_web_browser::renderer_protocol::DocumentId::new(193).unwrap();
    let mut entry_frame = setup::start_playback(&session, document, busy_ms);
    // Start the deliberate hang only after the video producer is running. Advancing
    // startup timers must not race the first decoded frame against a busy callback.
    session
        .send_input(DocumentInput::Keyboard(KeyboardInput {
            document,
            sequence: 1,
            phase: KeyPhase::Down,
            key: "a".into(),
            code: "KeyA".into(),
            repeat: false,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();
    let started = Instant::now();
    let mut frames_during_callback = 0;
    let mut input_task_seen = false;
    let mut completed = false;
    while started.elapsed() < Duration::from_secs(2) && !completed {
        match session.wait_for_event(Duration::from_secs(1)).unwrap() {
            RendererEvent::VideoFrame(frame) if frame.identity.document == document => {
                assert!(!frame.pixels.is_empty());
                let snapshot = session.snapshot();
                if snapshot.active_task.as_deref().is_some_and(|task| {
                    task.starts_with("dispatching input and rendering mutations for document 193 ")
                }) {
                    // The event queue retains one latest startup frame. Fence it on
                    // first observing the input task; only later producer identities
                    // prove pixel delivery while that task owns the document thread.
                    if !input_task_seen {
                        input_task_seen = true;
                        entry_frame = entry_frame.max(frame.identity.frame);
                    } else if frame.identity.frame > entry_frame
                        && snapshot
                            .active_task_elapsed
                            .is_some_and(|elapsed| elapsed < Duration::from_millis(350))
                    {
                        entry_frame = frame.identity.frame;
                        frames_during_callback += 1;
                    }
                }
            }
            RendererEvent::RuntimeUpdate(update) => {
                setup::assert_runtime_healthy(&update.runtime);
                completed = update
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("__BUSY_CALLBACK_COMPLETED__"));
            }
            RendererEvent::Presentation(presentation) => {
                setup::assert_runtime_healthy(&presentation.runtime);
                completed = presentation
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("__BUSY_CALLBACK_COMPLETED__"));
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::VideoFrame(_) => {}
            RendererEvent::Unresponsive if expect_timeout => {}
            RendererEvent::Exited(exit) if expect_timeout => {
                let timeout = exit
                    .reason
                    .task_timeout()
                    .expect("document task watchdog exit");
                assert!(
                    timeout
                        .task
                        .starts_with("dispatching input and rendering mutations for document 193 "),
                    "unexpected timed-out task: {timeout:?}"
                );
                assert!(timeout.elapsed >= Duration::from_millis(650), "{timeout:?}");
                assert!(
                    input_task_seen && frames_during_callback >= 3,
                    "video during input: {frames_during_callback} frames; {timeout:?}"
                );
                return;
            }
            event => panic!("unexpected busy-callback event: {event:?}"),
        }
    }
    assert!(completed, "the long JavaScript callback did not execute");
    assert!(!expect_timeout, "video traffic hid the hung document");
    assert!(
        input_task_seen && frames_during_callback >= 3,
        "video waited for JavaScript: {frames_during_callback} frames"
    );
    session.shutdown().unwrap();
}
