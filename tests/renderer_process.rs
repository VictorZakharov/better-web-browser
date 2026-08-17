#![cfg(target_os = "windows")]

use better_web_browser::renderer_process::{
    RendererEvent, RendererExitReason, RendererLaunchOptions, RendererSession, RendererState,
    StartupFault,
};
use better_web_browser::renderer_protocol::TestCommand;
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

static SERIAL: Mutex<()> = Mutex::new(());

fn options() -> RendererLaunchOptions {
    let mut options = RendererLaunchOptions::new(env!("CARGO_BIN_EXE_better-web-browser"));
    options.test_mode = true;
    options.heartbeat_interval = Duration::from_millis(50);
    options.unresponsive_timeout = Duration::from_secs(2);
    options.unresponsive_kill_timeout = Duration::from_millis(500);
    options
}

fn hung_task_options() -> RendererLaunchOptions {
    let mut options = options();
    options.unresponsive_timeout = Duration::from_millis(300);
    options.unresponsive_kill_timeout = Duration::from_millis(150);
    options
}

#[test]
fn hidden_contained_renderer_handshakes_pings_and_shuts_down() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // AppContainer and the Rust thread runtime each retain one process-wide cache handle on their
    // first use. Warm both once before checking that a renderer session itself is leak-free.
    for _ in 0..2 {
        let mut warmup = RendererSession::launch(options()).expect("warm renderer infrastructure");
        warmup.shutdown().expect("shutdown warmup renderer");
        drop(warmup);
    }
    let before = process_handle_count();
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    session.ping(Duration::from_secs(1)).expect("renderer pong");
    let snapshot = session.snapshot();
    assert_eq!(snapshot.state, RendererState::Running);
    assert_ne!(snapshot.process_id, 0);
    assert_ne!(snapshot.session_id, 0);
    assert!(snapshot.working_set > 0);
    assert!(snapshot.private_memory > 0);
    assert!(snapshot.peak_working_set >= snapshot.working_set);
    assert!(snapshot.handle_count > 0);
    let exit = session.shutdown().expect("clean renderer shutdown");
    assert_eq!(exit.reason, RendererExitReason::CleanShutdown);
    assert_eq!(exit.code, 0);
    drop(session);
    assert_handle_count_returns_to(before);
}

#[test]
fn crashed_tab_renderer_preserves_its_sibling_and_can_be_reloaded() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let first = RendererSession::launch(options()).expect("launch first tab renderer");
    let mut second = RendererSession::launch(options()).expect("launch second tab renderer");
    let first_snapshot = first.snapshot();
    let second_snapshot = second.snapshot();
    assert_ne!(first_snapshot.process_id, second_snapshot.process_id);
    assert_ne!(first_snapshot.session_id, second_snapshot.session_id);

    first.send_test_command(TestCommand::Crash).unwrap();
    let crash = first.wait_for_exit(Duration::from_secs(3)).unwrap();
    assert_eq!(crash.reason, RendererExitReason::Crash);
    let surface = crash
        .crash_surface()
        .expect("browser-owned crashed-tab surface");
    assert!(surface.can_reload);
    assert!(surface.detail.contains("crashed"));
    drop(first);

    second
        .ping(Duration::from_secs(1))
        .expect("sibling tab renderer remains responsive");
    assert_eq!(second.snapshot().state, RendererState::Running);

    let mut replacement = RendererSession::launch(options()).expect("reload crashed tab renderer");
    replacement
        .ping(Duration::from_secs(1))
        .expect("replacement renderer is responsive");
    assert_ne!(replacement.snapshot().process_id, first_snapshot.process_id);
    replacement
        .shutdown()
        .expect("shutdown replacement renderer");
    second.shutdown().expect("shutdown sibling tab renderer");
}

#[test]
fn app_container_denies_children_loopback_and_internet() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback probe");
    let port = listener.local_addr().unwrap().port();
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let report = session
        .probe_restrictions(port, Duration::from_secs(3))
        .expect("restriction report");
    assert!(
        report.child_launch_denied,
        "child launch was not denied: {report:?}"
    );
    assert!(
        report.loopback_denied,
        "loopback was not denied: {report:?}"
    );
    assert!(
        report.internet_denied,
        "Internet was not denied: {report:?}"
    );
    session
        .ping(Duration::from_secs(1))
        .expect("broker IPC remains available");
    session.shutdown().expect("shutdown renderer");
}

#[test]
fn malformed_frames_and_forced_abort_are_session_local_and_recoverable() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch malformed-frame renderer");
    session
        .send_test_command(TestCommand::WriteMalformedFrame)
        .unwrap();
    let malformed = session.wait_for_exit(Duration::from_secs(3)).unwrap();
    assert!(matches!(
        malformed.reason,
        RendererExitReason::ProtocolFailure(_)
    ));
    drop(session);

    let session = RendererSession::launch(options()).expect("launch replacement renderer");
    session.send_test_command(TestCommand::Crash).unwrap();
    let crash = session.wait_for_exit(Duration::from_secs(3)).unwrap();
    assert_eq!(crash.reason, RendererExitReason::Crash);
    let surface = crash
        .crash_surface()
        .expect("browser-owned recoverable crash surface");
    assert!(surface.can_reload);
    assert!(surface.detail.contains("crashed"));
    drop(session);

    let mut replacement = RendererSession::launch(options()).expect("recover renderer session");
    replacement.ping(Duration::from_secs(1)).unwrap();
    replacement.shutdown().unwrap();
}

#[test]
fn startup_faults_fail_closed_within_the_deadline() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut warmup = RendererSession::launch(options()).expect("warm renderer infrastructure");
    warmup.shutdown().expect("shutdown warmup renderer");
    drop(warmup);
    let before = process_handle_count();
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
    assert_handle_count_returns_to(before);
}

#[test]
fn hung_task_is_detected_and_terminated_without_blocking_the_browser() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(hung_task_options()).expect("launch renderer");
    session.send_test_command(TestCommand::Hang).unwrap();
    loop {
        if matches!(
            session.wait_for_event(Duration::from_secs(2)).unwrap(),
            RendererEvent::Unresponsive
        ) {
            break;
        }
    }
    let exit = session.wait_for_exit(Duration::from_secs(3)).unwrap();
    assert_eq!(exit.reason, RendererExitReason::TaskBudgetExceeded);
    let surface = exit
        .crash_surface()
        .expect("recoverable task-budget surface");
    assert!(surface.can_reload);
}

fn process_handle_count() -> u32 {
    let mut count = 0;
    assert_ne!(
        unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) },
        0
    );
    count
}

fn assert_handle_count_returns_to(before: u32) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let after = process_handle_count();
        if after <= before {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "renderer lifecycle leaked handles: before={before}, after={after}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
