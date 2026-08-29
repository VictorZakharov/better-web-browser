#![cfg(target_os = "windows")]

use better_web_browser::media_process::{
    MediaLaunchOptions, MediaSession, MediaStartupFault, MediaWorkerState,
};
use better_web_browser::media_protocol::MediaTestCommand;
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());

fn options() -> MediaLaunchOptions {
    let mut options = MediaLaunchOptions::new(env!("CARGO_BIN_EXE_better-web-browser"));
    options.test_mode = true;
    options.startup_timeout = Duration::from_secs(3);
    options.command_timeout = Duration::from_millis(750);
    options.shutdown_timeout = Duration::from_secs(1);
    options
}

#[test]
fn contained_worker_proves_media_foundation_h264_and_aac_availability() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = MediaSession::launch(options()).expect("launch contained media worker");
    session.ping(42).expect("media worker pong");
    let report = session.probe().expect("probe Media Foundation");
    assert!(
        report.startup_hresult >= 0,
        "Media Foundation startup failed: {report:?}"
    );
    assert!(
        report.h264_hresult >= 0 && report.h264_decoders > 0,
        "H.264 decoder unavailable: {report:?}"
    );
    assert!(
        report.aac_hresult >= 0 && report.aac_decoders > 0,
        "AAC decoder unavailable: {report:?}"
    );
    let snapshot = session.snapshot();
    assert_eq!(snapshot.state, MediaWorkerState::Running);
    assert!(snapshot.containment.app_container);
    assert!(snapshot.containment.no_console_window);
    assert!(snapshot.containment.minimal_environment);
    assert_ne!(snapshot.process_id, 0);
    assert_ne!(snapshot.session_id, 0);
    assert!(snapshot.working_set > 0);
    assert!(snapshot.private_memory > 0);
    assert!(snapshot.handle_count > 0);
    assert!(snapshot.last_progress_age < Duration::from_secs(1));
    assert!(snapshot.limits.max_encoded_queue_bytes > 0);
    assert!(snapshot.limits.max_decoded_frames > 0);
    assert_eq!(snapshot.capability, Some(report));

    session.shutdown().expect("clean media-worker shutdown");
    let stopped = session.snapshot();
    assert_eq!(stopped.state, MediaWorkerState::Exited);
    assert_eq!(stopped.exit_code, Some(0));
}

#[test]
fn media_worker_cannot_spawn_children_or_open_network_connections() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback probe");
    let port = listener.local_addr().unwrap().port();
    let mut session = MediaSession::launch(options()).expect("launch contained media worker");
    let report = session
        .probe_restrictions(port)
        .expect("run worker restriction probe");
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
        "internet was not denied: {report:?}"
    );
    session.shutdown().expect("clean media-worker shutdown");
}

#[test]
fn malformed_crashed_and_hung_workers_fail_without_harming_a_sibling() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut sibling = MediaSession::launch(options()).expect("launch sibling media worker");

    for fault in [
        MediaTestCommand::Crash,
        MediaTestCommand::WriteMalformedFrame,
        MediaTestCommand::Hang,
    ] {
        let mut victim = MediaSession::launch(options()).expect("launch media fault victim");
        victim.inject_failure(fault).expect("contain media fault");
        assert_eq!(victim.snapshot().state, MediaWorkerState::Exited);
        sibling.ping(7).expect("sibling remains responsive");
    }

    let mut replacement = MediaSession::launch(options()).expect("launch replacement media worker");
    replacement.ping(9).expect("replacement is responsive");
    replacement.shutdown().expect("shutdown replacement");
    sibling.shutdown().expect("shutdown sibling");
}

#[test]
fn startup_faults_fail_closed_within_the_deadline() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for fault in [
        MediaStartupFault::Silent,
        MediaStartupFault::WrongNonce,
        MediaStartupFault::MalformedFrame,
        MediaStartupFault::OversizedFrame,
        MediaStartupFault::IncompatibleVersion,
    ] {
        let mut launch = options();
        launch.startup_timeout = Duration::from_millis(200);
        launch.startup_fault = Some(fault);
        let started = Instant::now();
        let result = MediaSession::launch(launch);
        assert!(result.is_err(), "startup fault was accepted: {fault:?}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "startup fault exceeded its deadline: {fault:?}"
        );
    }
}
