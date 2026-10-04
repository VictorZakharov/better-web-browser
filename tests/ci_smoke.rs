//! Deliberately small CI target: production startup/rendering, not standards conformance.
//! Do not import the full renderer test tree; selecting tests after compilation
//! would retain its compilation and linking cost.
#![cfg(target_os = "windows")]

use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{
    RendererEvent, RendererExitReason, RendererLaunchOptions, RendererSession, RendererState,
};
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentStart, DocumentState, NotificationPermission, PresentedViewport,
    ScrollRestorationMode,
};
use better_web_browser::storage::StorageAreaSnapshot;
use std::net::TcpListener;
use std::time::{Duration, Instant};

fn options() -> RendererLaunchOptions {
    // The production launcher uses --renderer-process and CREATE_NO_WINDOW,
    // before the browser shell can create a window. Never launch the GUI here.
    let mut options = RendererLaunchOptions::new(env!("CARGO_BIN_EXE_better-web-browser"));
    options.test_mode = true;
    options.silent_audio = true;
    options
}

#[test]
fn contained_renderer_starts_pings_and_shuts_down() {
    let mut session = RendererSession::launch(options()).expect("hidden renderer startup");
    session.ping(Duration::from_secs(3)).expect("renderer pong");
    let snapshot = session.snapshot();
    assert_eq!(snapshot.state, RendererState::Running);
    assert_ne!(snapshot.process_id, 0);
    assert!(snapshot.working_set > 0);
    let exit = session.shutdown().expect("clean shutdown");
    assert_eq!(exit.reason, RendererExitReason::CleanShutdown);
    assert_eq!(exit.code, 0);
}

#[test]
fn contained_renderer_cannot_launch_children_or_open_network_sockets() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback restriction probe");
    let mut session = RendererSession::launch(options()).expect("hidden renderer startup");
    let report = session
        .probe_restrictions(
            listener.local_addr().unwrap().port(),
            Duration::from_secs(3),
        )
        .expect("containment report");
    assert!(report.child_launch_denied, "{report:?}");
    assert!(report.loopback_denied, "{report:?}");
    assert!(report.internet_denied, "{report:?}");
    session
        .ping(Duration::from_secs(3))
        .expect("IPC still works");
    session.shutdown().expect("clean shutdown");
}

#[test]
fn html_css_and_javascript_produce_a_real_renderer_presentation() {
    let mut session = RendererSession::launch(options()).expect("hidden renderer startup");
    let document = DocumentId::new(1).unwrap();
    let html = br#"<!doctype html><title>initial</title>
        <style>p { color: rgb(1, 2, 3); }</style><p>smoke content</p>
        <script>
        const gl = document.createElement('canvas').getContext('webgl2');
        if (!gl) throw new Error('native graphics startup failed');
        gl.clearColor(0, 1, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT);
        const pixel = new Uint8Array(4);
        gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
        if (pixel[0] !== 0 || pixel[1] !== 255 || pixel[2] !== 0 || pixel[3] !== 255)
            throw new Error('native graphics smoke did not produce green');
        document.title = 'smoke-ready';
        </script>"#;
    session
        .load_document(
            DocumentStart {
                document,
                url: "https://example.test/ci-smoke".into(),
                status: 200,
                content_type: "text/html; charset=utf-8".into(),
                csp_policies: Vec::new(),
                diagnostic_selectors: Vec::new(),
                body_length: html.len() as u32,
                history_length: 1,
                history_index: 0,
                history_state: None,
                scroll_restoration: ScrollRestorationMode::Auto,
                viewport: PresentedViewport {
                    width: 640.0,
                    height: 480.0,
                    style_width: 640.0,
                    dpi: 96,
                    prefers_dark_color_scheme: false,
                },
                prefers_dark_color_scheme: false,
                notification_permission: NotificationPermission::Default,
            },
            DocumentState {
                cookie_version: 1,
                cookie_header: String::new(),
                local_storage: StorageAreaSnapshot::empty(),
                session_storage: StorageAreaSnapshot::empty(),
            },
            html.to_vec(),
        )
        .expect("deliver document");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("no completed smoke presentation within five seconds");
        match session.wait_for_event(remaining).expect("renderer event") {
            RendererEvent::Presentation(presentation)
                if presentation.document == document && presentation.title == "smoke-ready" =>
            {
                assert_eq!(presentation.status, 200);
                assert!(presentation.layout.items.iter().any(|item| {
                    matches!(item, DisplayItem::Text { text, color, .. }
                        if text.contains("smoke") && color.red == 1 && color.green == 2 && color.blue == 3)
                }), "HTML text and CSS color missing: {:?}", presentation.layout.items);
                break;
            }
            RendererEvent::Presentation(_) | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event in renderer smoke: {event:?}"),
        }
    }
    session.shutdown().expect("clean shutdown");
}
