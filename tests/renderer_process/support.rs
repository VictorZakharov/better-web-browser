use better_web_browser::renderer_process::{RendererEvent, RendererLaunchOptions, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentInput, DocumentNodeId, DocumentStart, DocumentState, FocusInput,
    InputModifiers, KeyPhase, KeyboardInput, NavigationCause, NavigationDisposition, PointerButton,
    PointerInput, PointerPhase, PresentationAcknowledgement, PresentedViewport,
    RendererPresentation, TextInput,
};
use better_web_browser::storage::StorageAreaSnapshot;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) static SERIAL: Mutex<()> = Mutex::new(());

pub(super) fn options() -> RendererLaunchOptions {
    let mut options = RendererLaunchOptions::new(env!("CARGO_BIN_EXE_better-web-browser"));
    options.test_mode = true;
    options.silent_audio = true;
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

/// Acknowledges a presentation as shown with controls applied.
pub(super) fn acknowledge(session: &RendererSession, presentation: &RendererPresentation) {
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: presentation.document,
            revision: presentation.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
}

/// Waits for a presentation satisfying `wanted`, tolerating diagnostics and
/// runtime updates.
pub(super) fn wait_for_presentation(
    session: &RendererSession,
    document: DocumentId,
    wanted: &str,
    matches: impl Fn(&RendererPresentation) -> bool,
) -> RendererPresentation {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "no presentation: {wanted}");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if matches(&presentation) {
                    return *presentation;
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
            }
            event => panic!("unexpected event while waiting for {wanted}: {event:?}"),
        }
    }
}

/// Proves zero submissions: drains events, failing on any navigation signal.
pub(super) fn assert_no_submit(
    session: &RendererSession,
    document: DocumentId,
    duration: Duration,
) {
    let deadline = Instant::now() + duration;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match session.wait_for_event(remaining) {
            Ok(RendererEvent::NavigationRequested {
                document: navigated,
                url,
                ..
            }) if navigated == document => {
                panic!("blocked form navigated to {url}")
            }
            Ok(RendererEvent::Presentation(presentation)) if presentation.document == document => {
                assert!(
                    presentation.runtime.navigation_url.is_none(),
                    "blocked form navigated to {:?}",
                    presentation.runtime.navigation_url
                );
            }
            Ok(RendererEvent::RuntimeUpdate(update)) if update.document == document => {
                assert!(
                    update.runtime.navigation_url.is_none(),
                    "blocked form navigated to {:?}",
                    update.runtime.navigation_url
                );
            }
            Ok(RendererEvent::Diagnostic { .. }) => {}
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

pub(super) fn wait_for_navigation_url(
    session: &RendererSession,
    document: DocumentId,
) -> (String, NavigationDisposition, NavigationCause) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "form did not navigate");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::NavigationRequested {
                document: event_document,
                url,
                disposition,
                cause,
            } if event_document == document => return (url, disposition, cause),
            RendererEvent::Presentation(presentation) => {
                if let Some(url) = presentation.runtime.navigation_url {
                    return (
                        url,
                        NavigationDisposition::CurrentTab,
                        NavigationCause::UserActivation,
                    );
                }
                pump_ready_task(session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) => {
                if let Some(url) = update.runtime.navigation_url {
                    return (
                        url,
                        NavigationDisposition::CurrentTab,
                        NavigationCause::UserActivation,
                    );
                }
                pump_ready_task(session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event while waiting for navigation: {event:?}"),
        }
    }
}

pub(super) fn node_id(wire: u128) -> DocumentNodeId {
    DocumentNodeId::new(wire).expect("control node id")
}

pub(super) fn send_text(
    session: &RendererSession,
    document: DocumentId,
    sequence: u64,
    target: DocumentNodeId,
    value: &str,
) {
    session
        .send_input(DocumentInput::Text(TextInput {
            document,
            sequence,
            target,
            value: value.into(),
            selection_start: value.len() as u32,
            selection_end: value.len() as u32,
        }))
        .unwrap();
}

pub(super) fn send_focus(
    session: &RendererSession,
    document: DocumentId,
    sequence: u64,
    target: DocumentNodeId,
) {
    session
        .send_input(DocumentInput::Focus(FocusInput {
            document,
            sequence,
            focused: true,
            target: Some(target),
        }))
        .unwrap();
}

pub(super) fn send_enter(
    session: &RendererSession,
    document: DocumentId,
    sequence: u64,
    target: DocumentNodeId,
) {
    session
        .send_input(DocumentInput::Keyboard(KeyboardInput {
            document,
            sequence,
            phase: KeyPhase::Down,
            key: "Enter".into(),
            code: "Enter".into(),
            repeat: false,
            modifiers: InputModifiers::default(),
            target: Some(target),
        }))
        .unwrap();
}

pub(super) fn click(
    session: &RendererSession,
    document: DocumentId,
    sequence: u64,
    x: f32,
    y: f32,
) {
    for (offset, phase) in [(0, PointerPhase::Down), (1, PointerPhase::Up)] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document,
                sequence: sequence + offset,
                phase,
                button: PointerButton::Primary,
                buttons: if phase == PointerPhase::Down { 1 } else { 0 },
                x,
                y,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
    }
}

// Match the shell's immediate-work scheduling without fast-forwarding future timers. A clock
// command executes one task, so DCL/load may precede the timer a test is waiting to observe.
pub(super) fn pump_ready_task(session: &RendererSession, document: DocumentId, next: Option<u64>) {
    if next == Some(0) {
        session.advance_time(document, Duration::ZERO, 1).unwrap();
    }
}
