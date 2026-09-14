//! Input can beat the UI's next event drain after a renderer has already exited.
use super::support::*;
use better_web_browser::renderer_process::RendererSession;
use better_web_browser::renderer_protocol::{DocumentInput, ScrollInput, TestCommand};
use std::time::{Duration, Instant};

#[test]
fn input_after_an_undrained_exit_reports_the_original_crash_or_watchdog_reason() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (command, expected) in [
        (TestCommand::Crash, "crashed"),
        (TestCommand::Hang, "unresponsive-task budget"),
    ] {
        let session = RendererSession::launch(hung_task_options()).expect("hidden renderer");
        let presentation = load_inline_document(&session, 125);
        session.send_test_command(command).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        // Deliberately do not drain events: native input may arrive before the exit event.
        while session.snapshot().exit.is_none() {
            assert!(Instant::now() < deadline, "renderer did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
        let error = loop {
            let result = session.try_send_input_retained(DocumentInput::Scroll(ScrollInput {
                document: presentation.document,
                sequence: 1,
                x: 0.0,
                y: 100.0,
            }));
            if let Err(error) = result {
                break error;
            }
            assert!(Instant::now() < deadline, "broker did not disconnect");
            std::thread::sleep(Duration::from_millis(10));
        };
        let snapshot = session.snapshot();
        let exit = snapshot.exit.unwrap();
        assert!(error.contains(expected), "original reason lost: {error}");
        assert!(
            error.contains(&exit.process_id.to_string()),
            "PID lost: {error}"
        );
        assert!(
            error.contains(&format!("{:#x}", exit.code)),
            "exit code lost: {error}"
        );
        assert!(
            session.pending_events() > 0,
            "test must leave exit events undrained"
        );
    }
}
