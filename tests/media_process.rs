#![cfg(target_os = "windows")]

use better_web_browser::media_process::{
    MediaLaunchOptions, MediaSession, MediaStartupFault, MediaWorkerState,
};
use better_web_browser::media_protocol::{MediaCodecFamily, MediaTestCommand};
use std::net::TcpListener;
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_SHA256_ALGORITHM, BCryptCloseAlgorithmProvider, BCryptHash,
    BCryptOpenAlgorithmProvider,
};

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
fn contained_worker_decodes_owned_h264_aac_mp4_to_nv12_and_pcm() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = decode_base64(include_str!("fixtures/media/test-1s.mp4.base64"));
    assert_eq!(fixture.len(), 13_932);
    assert_eq!(
        sha256(&fixture),
        [
            0xdc, 0x72, 0xb1, 0xb5, 0x59, 0x1b, 0xbc, 0x9e, 0x2d, 0x0d, 0x6b, 0x51, 0x1f, 0xa6,
            0xd5, 0x13, 0x4d, 0xd7, 0x8d, 0xca, 0x6c, 0xf3, 0x57, 0x24, 0x4d, 0x65, 0x62, 0x25,
            0xf6, 0x2a, 0x94, 0xb5,
        ],
        "vendored WPT fixture does not match its pinned SHA-256"
    );
    let mut session = MediaSession::launch(options()).expect("launch contained media worker");
    let report = session
        .decode_owned_fixture(&fixture)
        .expect("decode owned WPT MP4 fixture");

    assert_eq!(report.encoded_bytes, 13_932);
    assert_eq!(report.video_codec, MediaCodecFamily::H264);
    assert_eq!(report.audio_codec, MediaCodecFamily::AacLc);
    assert_eq!(report.source_reader_hresult, 0);
    assert_eq!(report.video_decode_hresult, 0);
    assert_eq!(report.audio_decode_hresult, 0);
    assert_eq!((report.video_width, report.video_height), (320, 240));
    assert_eq!(report.audio_sample_rate, 44_100);
    assert_eq!(report.audio_channels, 2);
    assert!(
        report.video_samples > 0,
        "no decoded video samples: {report:?}"
    );
    assert!(
        report.audio_samples > 0,
        "no decoded audio samples: {report:?}"
    );
    assert!(report.video_decoded_bytes > 0);
    assert!(report.audio_decoded_bytes > 0);
    assert!(report.video_first_timestamp_100ns <= report.video_last_timestamp_100ns);
    assert!(report.audio_first_timestamp_100ns <= report.audio_last_timestamp_100ns);
    assert!(
        (9_000_000..=12_000_000).contains(&report.duration_100ns),
        "unexpected fixture duration: {report:?}"
    );
    session.shutdown().expect("clean media-worker shutdown");
}

#[test]
fn production_session_rejects_browser_owned_media_admission() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = decode_base64(include_str!("fixtures/media/test-1s.mp4.base64"));
    let mut launch = options();
    launch.test_mode = false;
    let mut session = MediaSession::launch(launch).expect("launch production media worker");
    let error = session
        .decode_owned_fixture(&fixture)
        .expect_err("production byte admission must remain closed");
    assert!(error.contains("denied outside test mode"));
    session.shutdown().expect("clean media-worker shutdown");
}

#[test]
fn corrupt_truncated_and_oversized_media_fail_without_harming_a_sibling() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut fixture = decode_base64(include_str!("fixtures/media/test-1s.mp4.base64"));
    fixture.truncate(128);
    let mut sibling = MediaSession::launch(options()).expect("launch sibling media worker");
    let mut victim = MediaSession::launch(options()).expect("launch media decode victim");
    assert!(victim.decode_owned_fixture(&fixture).is_err());
    assert_eq!(victim.snapshot().state, MediaWorkerState::Exited);
    sibling.ping(17).expect("sibling remains responsive");

    let mut oversized = MediaSession::launch(options()).expect("launch oversized media victim");
    oversized
        .inject_oversized_source()
        .expect("reject oversized media frame");
    assert_eq!(oversized.snapshot().state, MediaWorkerState::Exited);
    sibling
        .ping(18)
        .expect("sibling survives oversized media frame");

    let mut replacement = MediaSession::launch(options()).expect("launch replacement media worker");
    replacement
        .ping(19)
        .expect("replacement remains responsive");
    replacement.shutdown().expect("shutdown replacement");
    sibling.shutdown().expect("shutdown sibling");
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

fn decode_base64(input: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    let mut quartet = [0_u8; 4];
    let mut count = 0;
    for byte in input.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        quartet[count] = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => panic!("invalid base64 fixture byte"),
        };
        count += 1;
        if count == 4 {
            output.push((quartet[0] << 2) | (quartet[1] >> 4));
            if quartet[2] != 64 {
                output.push((quartet[1] << 4) | (quartet[2] >> 2));
            }
            if quartet[3] != 64 {
                output.push((quartet[2] << 6) | quartet[3]);
            }
            count = 0;
        }
    }
    assert_eq!(count, 0, "truncated base64 fixture");
    output
}

fn sha256(input: &[u8]) -> [u8; 32] {
    let mut algorithm: BCRYPT_ALG_HANDLE = null_mut();
    let open_status =
        unsafe { BCryptOpenAlgorithmProvider(&mut algorithm, BCRYPT_SHA256_ALGORITHM, null(), 0) };
    assert!(
        open_status >= 0,
        "open SHA-256 provider: NTSTATUS {open_status:#x}"
    );

    let mut digest = [0_u8; 32];
    let hash_status = unsafe {
        BCryptHash(
            algorithm,
            null(),
            0,
            input.as_ptr(),
            input.len() as u32,
            digest.as_mut_ptr(),
            digest.len() as u32,
        )
    };
    let close_status = unsafe { BCryptCloseAlgorithmProvider(algorithm, 0) };
    assert!(hash_status >= 0, "hash fixture: NTSTATUS {hash_status:#x}");
    assert!(
        close_status >= 0,
        "close SHA-256 provider: NTSTATUS {close_status:#x}"
    );
    digest
}
