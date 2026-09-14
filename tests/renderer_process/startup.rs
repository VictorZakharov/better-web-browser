use super::{SERIAL, assert_handle_count_returns_to, options, process_handle_count};
use better_web_browser::renderer_process::{RendererSession, StartupFault};
use std::time::{Duration, Instant};

#[test]
fn startup_faults_fail_closed_within_the_deadline() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut warmup = RendererSession::launch(options()).expect("warm renderer infrastructure");
    warmup.shutdown().expect("shutdown warmup renderer");
    drop(warmup);
    // Successful startup alone does not warm the OS failure/wait path. On Windows,
    // the first timed-out startup retained one WaitCompletionPacket; 15 subsequent
    // failed starts retained no additional handles. Warm both paths before the
    // steady-state leak check, rather than allowing a growing handle-count margin.
    exercise_faults();
    let before = process_handle_count();
    for _ in 0..3 {
        exercise_faults();
        assert_handle_count_returns_to(before);
    }
}

fn exercise_faults() {
    for fault in [
        StartupFault::Silent,
        StartupFault::WrongNonce,
        StartupFault::MalformedFrame,
        StartupFault::OversizedFrame,
        StartupFault::IncompatibleVersion,
    ] {
        let mut launch = options();
        launch.startup_timeout = Duration::from_millis(200);
        launch.startup_fault = Some(fault);
        let started = Instant::now();
        let result = RendererSession::launch(launch);
        assert!(result.is_err(), "startup fault was accepted: {fault:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
