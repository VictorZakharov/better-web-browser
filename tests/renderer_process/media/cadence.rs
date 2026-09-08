use super::*;
use std::time::Instant;

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
    let encoded: String = include_str!("../../fixtures/media/test-1s.mp4.base64")
        .chars()
        .filter(|value| !value.is_ascii_whitespace())
        .collect();
    let html = format!(
        r#"<!doctype html><video id="movie" muted></video><script>
        const source = new MediaSource();
        source.addEventListener('sourceopen', () => {{
          const buffer = source.addSourceBuffer('video/mp4; codecs="avc1.42E01E,mp4a.40.2"');
          buffer.addEventListener('updateend', () => source.endOfStream(), {{once:true}});
          buffer.appendBuffer(Uint8Array.from(atob('{encoded}'), c => c.charCodeAt(0)));
        }}, {{once:true}});
        source.addEventListener('sourceended', () => movie.play().then(() => {{
          setTimeout(() => {{
            const start = performance.now();
            while (performance.now() - start < {busy_ms}) {{}}
            console.log('__BUSY_CALLBACK_COMPLETED__');
          }}, 200);
        }}));
        movie.src = URL.createObjectURL(source);
        </script>"#
    );
    let body = html.into_bytes();
    session
        .load_document(
            document_start(document, body.len()),
            empty_document_state(),
            body,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "video producer did not start");
        match session.wait_for_event(Duration::from_secs(2)).unwrap() {
            RendererEvent::VideoFrame(_) => break,
            RendererEvent::Presentation(presentation) => {
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                run_scheduled_renderer_timer(&session, document, Some(20_000));
            }
            RendererEvent::RuntimeUpdate(_) => {
                std::thread::sleep(Duration::from_millis(5));
                run_scheduled_renderer_timer(&session, document, Some(20_000));
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected startup event: {event:?}"),
        }
    }
    session
        .advance_time(document, Duration::from_millis(250), 1)
        .unwrap();
    let started = Instant::now();
    let mut frames_during_callback = 0;
    let mut completed = false;
    while started.elapsed() < Duration::from_secs(2) && !completed {
        match session.wait_for_event(Duration::from_secs(1)).unwrap() {
            RendererEvent::VideoFrame(_) if started.elapsed() < Duration::from_millis(350) => {
                frames_during_callback += 1;
            }
            RendererEvent::RuntimeUpdate(update) => {
                completed = update
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("__BUSY_CALLBACK_COMPLETED__"));
            }
            RendererEvent::Presentation(presentation) => {
                completed = presentation
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("__BUSY_CALLBACK_COMPLETED__"));
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::VideoFrame(_) => {}
            RendererEvent::Unresponsive if expect_timeout => {}
            RendererEvent::Exited(exit) if expect_timeout => {
                assert!(matches!(exit.reason,
                    better_web_browser::renderer_process::RendererExitReason::TaskBudgetExceeded(_)),
                    "unexpected exit: {exit:?}");
                assert!(frames_during_callback >= 3);
                return;
            }
            event => panic!("unexpected busy-callback event: {event:?}"),
        }
    }
    assert!(completed, "the long JavaScript callback did not execute");
    assert!(!expect_timeout, "video traffic hid the hung document");
    assert!(
        frames_during_callback >= 3,
        "video waited for JavaScript: {frames_during_callback} frames"
    );
    session.shutdown().unwrap();
}
