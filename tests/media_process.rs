#![cfg(target_os = "windows")]

use better_web_browser::media_process::{
    MediaLaunchOptions, MediaSession, MediaStartupFault, MediaWorkerState,
};
use better_web_browser::media_protocol::{MediaCodecFamily, MediaPixelFormat, MediaTestCommand};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[path = "media_process/support.rs"]
mod support;

use support::{capture_frame_if_requested, decode_base64, sha256};

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
    let decoded = session
        .decode_owned_fixture_frame(&fixture)
        .expect("decode owned WPT MP4 fixture");
    let report = decoded.report;

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
    assert_eq!(decoded.frame.metadata.source_id, 1);
    assert_eq!(decoded.frame.metadata.frame_id, 1);
    assert_eq!(decoded.frame.metadata.format, MediaPixelFormat::Nv12);
    assert_eq!(
        (decoded.frame.metadata.width, decoded.frame.metadata.height),
        (320, 240)
    );
    assert_eq!(decoded.frame.metadata.stride, 320);
    assert_eq!(decoded.frame.metadata.data_length, 115_200);
    assert_eq!(
        decoded.frame.metadata.timestamp_100ns,
        report.video_first_timestamp_100ns
    );
    assert!(decoded.frame.metadata.duration_100ns > 0);
    assert_eq!(decoded.frame.nv12.len(), 115_200);
    assert_eq!(decoded.frame.bgra.len(), 320 * 240 * 4);
    assert!(
        decoded
            .frame
            .bgra
            .chunks_exact(4)
            .all(|pixel| pixel[3] == 255)
    );
    assert_eq!(
        sha256(&decoded.frame.nv12),
        [
            0xa2, 0x99, 0xed, 0x56, 0x3a, 0x5a, 0xf9, 0x6a, 0x8a, 0xae, 0x56, 0xd4, 0xf7, 0xd1,
            0xc5, 0xc9, 0x3d, 0x96, 0xd0, 0xc4, 0xde, 0x3a, 0x7e, 0x2c, 0xac, 0xbc, 0x82, 0x3d,
            0x42, 0xdb, 0xdd, 0x66,
        ],
        "pin first NV12 frame"
    );
    assert_eq!(
        sha256(&decoded.frame.bgra),
        [
            0x0b, 0x61, 0x93, 0x04, 0xb1, 0xfb, 0x98, 0x92, 0xb4, 0xd2, 0x8b, 0x69, 0x33, 0x77,
            0x57, 0xdc, 0x17, 0x2c, 0xb6, 0xad, 0x26, 0xd0, 0xd1, 0x24, 0xc7, 0x26, 0x7f, 0x82,
            0x42, 0x64, 0xbd, 0xa0,
        ],
        "pin converted BGRA frame"
    );
    capture_frame_if_requested(&decoded.frame);

    let repeated = session
        .decode_owned_fixture_frame(&fixture)
        .expect("repeat decode and frame acknowledgement");
    assert_eq!(repeated.frame.metadata.source_id, 2);
    assert_eq!(repeated.frame.metadata.frame_id, 2);
    assert_eq!(sha256(&repeated.frame.nv12), sha256(&decoded.frame.nv12));
    assert_eq!(sha256(&repeated.frame.bgra), sha256(&decoded.frame.bgra));
    session.shutdown().expect("clean media-worker shutdown");
}

#[test]
fn contained_worker_advances_an_acknowledged_video_frame_sequence() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = decode_base64(include_str!("fixtures/media/test-1s.mp4.base64"));
    let mut session = MediaSession::launch(options()).expect("launch contained media worker");
    let playback = session
        .decode_owned_fixture_frames(&fixture, 6)
        .expect("decode advancing video frame sequence");

    assert_eq!(playback.frames.len(), 6);
    assert!(
        playback
            .frames
            .windows(2)
            .all(|frames| frames[0].metadata.timestamp_100ns < frames[1].metadata.timestamp_100ns),
        "video presentation timestamps must strictly advance"
    );
    assert!(
        playback
            .frames
            .windows(2)
            .any(|frames| sha256(&frames[0].nv12) != sha256(&frames[1].nv12)),
        "playback returned only a repeated poster frame"
    );
    assert!(
        playback
            .frames
            .iter()
            .all(|frame| frame.metadata.source_id == 1),
        "all frames must belong to one media source"
    );
    assert_eq!(
        playback
            .frames
            .iter()
            .map(|frame| frame.metadata.frame_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    session.shutdown().expect("clean media-worker shutdown");
}

#[test]
fn stale_and_duplicate_frame_acknowledgements_fail_without_harming_a_sibling() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fixture = decode_base64(include_str!("fixtures/media/test-1s.mp4.base64"));
    let mut sibling = MediaSession::launch(options()).expect("launch sibling media worker");

    let mut stale = MediaSession::launch(options()).expect("launch stale acknowledgement victim");
    stale
        .inject_stale_frame_acknowledgement()
        .expect("reject acknowledgement without a frame");
    assert_eq!(stale.snapshot().state, MediaWorkerState::Exited);
    sibling
        .ping(31)
        .expect("sibling survives stale acknowledgement");

    let mut duplicate =
        MediaSession::launch(options()).expect("launch duplicate acknowledgement victim");
    duplicate
        .decode_owned_fixture_frame(&fixture)
        .expect("decode and acknowledge one frame");
    duplicate
        .inject_stale_frame_acknowledgement()
        .expect("reject duplicate acknowledgement");
    assert_eq!(duplicate.snapshot().state, MediaWorkerState::Exited);
    sibling
        .ping(32)
        .expect("sibling survives duplicate acknowledgement");
    sibling.shutdown().expect("shutdown sibling");
}

#[test]
fn malformed_truncated_and_oversized_frame_output_fail_without_harming_a_sibling() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut sibling = MediaSession::launch(options()).expect("launch sibling media worker");
    for (index, fault) in [
        MediaTestCommand::WriteMalformedDecodedFrame,
        MediaTestCommand::WriteTruncatedDecodedFrame,
        MediaTestCommand::WriteOversizedDecodedFrame,
    ]
    .into_iter()
    .enumerate()
    {
        let mut victim = MediaSession::launch(options()).expect("launch frame output victim");
        victim
            .inject_frame_failure(fault)
            .expect("contain invalid decoded frame output");
        assert_eq!(victim.snapshot().state, MediaWorkerState::Exited);
        sibling
            .ping(40 + index as u64)
            .expect("sibling survives invalid decoded frame output");
    }
    let mut replacement = MediaSession::launch(options()).expect("launch replacement media worker");
    replacement
        .ping(44)
        .expect("replacement survives invalid decoded frame output");
    replacement.shutdown().expect("shutdown replacement");
    sibling.shutdown().expect("shutdown sibling");
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
