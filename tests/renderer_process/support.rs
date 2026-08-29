use better_web_browser::renderer_process::{RendererLaunchOptions, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentStart, DocumentState, PresentedViewport, RendererPresentation,
};
use better_web_browser::storage::StorageAreaSnapshot;
use std::sync::Mutex;
use std::time::Duration;

pub(super) static SERIAL: Mutex<()> = Mutex::new(());

pub(super) fn options() -> RendererLaunchOptions {
    let mut options = RendererLaunchOptions::new(env!("CARGO_BIN_EXE_better-web-browser"));
    options.test_mode = true;
    options.heartbeat_interval = Duration::from_millis(50);
    options.unresponsive_timeout = Duration::from_secs(2);
    options.unresponsive_kill_timeout = Duration::from_millis(500);
    options
}

pub(super) fn hung_task_options() -> RendererLaunchOptions {
    let mut options = options();
    options.unresponsive_timeout = Duration::from_millis(300);
    options.unresponsive_kill_timeout = Duration::from_millis(150);
    options
}

pub(super) fn load_inline_document(session: &RendererSession, value: u64) -> RendererPresentation {
    load_html_document(
        session,
        value,
        "<!doctype html><title>isolated</title><p>renderer owns this document</p>",
    )
}

pub(super) fn load_html_document(
    session: &RendererSession,
    value: u64,
    html: &str,
) -> RendererPresentation {
    load_html_document_with_selectors(session, value, html, Vec::new())
}

pub(super) fn load_html_document_with_selectors(
    session: &RendererSession,
    value: u64,
    html: &str,
    diagnostic_selectors: Vec<String>,
) -> RendererPresentation {
    let document = DocumentId::new(value).unwrap();
    let body = html.as_bytes().to_vec();
    let mut start = document_start(document, body.len());
    start.diagnostic_selectors = diagnostic_selectors;
    session
        .load_document(start, empty_document_state(), body)
        .unwrap();
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            better_web_browser::renderer_process::RendererEvent::Presentation(presentation)
                if presentation.document == document =>
            {
                return *presentation;
            }
            better_web_browser::renderer_process::RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer event while loading document: {event:?}"),
        }
    }
}

pub(super) fn document_start(document: DocumentId, body_length: usize) -> DocumentStart {
    DocumentStart {
        document,
        url: format!("https://example.test/{}", document.get()),
        status: 200,
        content_type: "text/html; charset=utf-8".into(),
        diagnostic_selectors: Vec::new(),
        body_length: body_length as u32,
        viewport: PresentedViewport {
            width: 800.0,
            height: 600.0,
            style_width: 800.0,
            dpi: 96,
            prefers_dark_color_scheme: false,
        },
        prefers_dark_color_scheme: false,
    }
}

pub(super) fn empty_document_state() -> DocumentState {
    DocumentState {
        cookie_version: 1,
        cookie_header: String::new(),
        local_storage: StorageAreaSnapshot::empty(),
        session_storage: StorageAreaSnapshot::empty(),
    }
}
