use super::support::*;
use better_web_browser::engine::{ControlKind, DisplayItem};
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentNodeId, FocusInput, InputModifiers, KeyPhase, KeyboardInput,
    NavigationCause, NavigationDisposition, PointerButton, PointerInput, PointerPhase,
    PresentationAcknowledgement, RendererPresentation, TextInput,
};
use std::time::{Duration, Instant};

const TEXT_FIXTURE: &str = include_str!("../fixtures/form-validation-text.html");
const CONTROLS_FIXTURE: &str = include_str!("../fixtures/form-validation-controls.html");
const NUMERIC_FIXTURE: &str = include_str!("../fixtures/form-validation-numeric.html");
const DENSE_FIXTURE: &str = include_str!("../fixtures/form-validation-dense.html");

fn acknowledge(session: &RendererSession, presentation: &RendererPresentation) {
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
/// runtime updates (which carry DOM-mutation counts from validation repair).
fn wait_for_presentation(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
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
fn assert_no_submit(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
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

fn wait_for_navigation_url(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
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

fn text_control(
    presentation: &RendererPresentation,
    name: &str,
) -> better_web_browser::engine::ControlSpec {
    presentation
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control)
                if control.kind == ControlKind::Text && control.name == name =>
            {
                Some((**control).clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("text control {name}"))
}

fn control_by_kind(
    presentation: &RendererPresentation,
    kind: ControlKind,
) -> better_web_browser::engine::ControlSpec {
    presentation
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == kind => Some((**control).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("control {kind:?}"))
}

fn node_id(wire: u128) -> DocumentNodeId {
    DocumentNodeId::new(wire).expect("control node id")
}

fn send_text(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
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

fn send_focus(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
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

fn send_enter(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
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

fn click(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
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

fn control_center(control: &better_web_browser::engine::ControlSpec) -> (f32, f32) {
    (
        control.rect.x + control.rect.width / 2.0,
        control.rect.y + control.rect.height / 2.0,
    )
}

/// Counts native diagnostic-selector matches (pure validity, no script).
fn diagnostic_matches(presentation: &RendererPresentation, selector: &str) -> u64 {
    presentation
        .page_diagnostics
        .selectors
        .iter()
        .find(|entry| entry.selector == selector)
        .map(|entry| entry.total_matches)
        .unwrap_or(0)
}

#[test]
fn scriptless_required_gate_blocks_then_submits() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        121,
        TEXT_FIXTURE,
        vec!["input:invalid".into(), "form:invalid".into()],
    );
    acknowledge(&session, &initial);

    let name = node_id(text_control(&initial, "name").node_id.to_wire());
    let email = node_id(text_control(&initial, "email").node_id.to_wire());
    // Pure validity through the native path: both inputs and their form.
    assert_eq!(diagnostic_matches(&initial, "input:invalid"), 2);
    assert_eq!(diagnostic_matches(&initial, "form:invalid"), 1);
    // Nothing is interactively reported yet, so no feedback flag.
    assert!(!text_control(&initial, "name").invalid);

    // Implicit submission of the empty form navigates nowhere but reports
    // the first invalid control into native feedback.
    send_focus(&session, initial.document, 1, name);
    send_enter(&session, initial.document, 2, name);
    let reported = wait_for_presentation(&session, initial.document, "reported feedback", |p| {
        text_control(p, "name").invalid
    });
    assert!(text_control(&reported, "name").invalid);
    assert!(!text_control(&reported, "email").invalid);
    assert_no_submit(&session, initial.document, Duration::from_millis(1500));

    send_text(&session, initial.document, 3, name, "ada");
    send_text(&session, initial.document, 4, email, "ada@intranet");
    let filled = wait_for_presentation(&session, initial.document, "filled controls", |p| {
        diagnostic_matches(p, "input:invalid") == 0 && diagnostic_matches(p, "form:invalid") == 0
    });
    assert_eq!(text_control(&filled, "name").value, "ada");

    send_enter(&session, initial.document, 5, name);
    let (url, disposition, cause) = wait_for_navigation_url(&session, initial.document);
    assert!(url.contains("/submit?"), "{url}");
    assert!(url.contains("name=ada"), "{url}");
    assert!(url.contains("email=ada%40intranet"), "{url}");
    assert_eq!(disposition, NavigationDisposition::CurrentTab);
    assert_eq!(cause, NavigationCause::UserActivation);
}

#[test]
fn scripted_invalid_order_focus_and_body() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        122,
        r#"<!doctype html><form id=f action="/submit" method="get">
        <input id=name name=name required><input id=email name=email type=email required>
        <button type=submit>Send</button></form><output></output><script>
            const form = document.getElementById('f');
            const invalids = [];
            let submits = 0;
            form.addEventListener('invalid', event => invalids.push(event.target.id), true);
            form.addEventListener('submit', event => {
                submits++;
                event.preventDefault();
                const data = new FormData(form);
                document.querySelector('output').textContent =
                    'valid:submits=' + submits + ':body=' + data.get('name') + ',' + data.get('email');
            });
            form.addEventListener('invalid', () => {
                if (submits === 0) {
                    document.querySelector('output').textContent = 'blocked:' + invalids.join(',') +
                        ':focus=' + document.activeElement.id;
                }
            }, true);
        </script>"#,
    );
    acknowledge(&session, &initial);
    let name = node_id(text_control(&initial, "name").node_id.to_wire());
    let email = node_id(text_control(&initial, "email").node_id.to_wire());

    send_focus(&session, initial.document, 1, name);
    send_enter(&session, initial.document, 2, name);
    let blocked = wait_for_presentation(&session, initial.document, "blocked trace", |p| {
        presentation_text(p).contains("blocked:name,email:focus=name")
    });
    assert!(presentation_text(&blocked).contains("blocked:name,email:focus=name"));

    send_text(&session, initial.document, 3, name, "ada");
    send_text(&session, initial.document, 4, email, "ada@intranet");
    send_enter(&session, initial.document, 5, name);
    let valid = wait_for_presentation(&session, initial.document, "valid trace", |p| {
        presentation_text(p).contains("valid:submits=1:body=ada,ada@intranet")
    });
    assert!(presentation_text(&valid).contains("valid:submits=1:body=ada,ada@intranet"));
}

fn presentation_text(presentation: &RendererPresentation) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn scriptless_select_checkbox_radio_reset_submit() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        123,
        CONTROLS_FIXTURE,
        vec!["input:checked".into()],
    );
    acknowledge(&session, &initial);

    // Native label clicks toggle checkbox/radio through hit testing and
    // label activation; find label text boxes in presentation geometry.
    let text_center = |presentation: &RendererPresentation, needle: &str| {
        presentation
            .layout
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text { rect, text, .. } if text.contains(needle) => {
                    Some((rect.x + rect.width / 2.0, rect.y + rect.height / 2.0))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("label text {needle}"))
    };
    let checked_count = |presentation: &RendererPresentation| {
        presentation
            .page_diagnostics
            .selectors
            .iter()
            .find(|selector| selector.selector == "input:checked")
            .map(|selector| selector.total_matches)
            .unwrap_or(0)
    };
    assert_eq!(checked_count(&initial), 0);

    // Native select pick by value through the text-input path.
    let select = control_by_kind(&initial, ControlKind::Select);
    let select_target = node_id(select.node_id.to_wire());
    send_text(&session, initial.document, 1, select_target, "b");
    let picked = wait_for_presentation(&session, initial.document, "picked Blue", |p| {
        control_by_kind(p, ControlKind::Select).selected_index == 2
    });
    assert_eq!(control_by_kind(&picked, ControlKind::Select).value, "b");

    // Native pointer clicks toggle checkbox and radio through hit testing.
    let (ax, ay) = text_center(&picked, "Agree");
    click(&session, initial.document, 2, ax, ay);
    let (mx, my) = text_center(&picked, "Medium");
    click(&session, initial.document, 4, mx, my);
    wait_for_presentation(&session, initial.document, "checked pair", |p| {
        checked_count(p) == 2
    });

    // Native click on the submit button navigates with live values.
    let submit = control_by_kind(&initial, ControlKind::Submit);
    let (x, y) = control_center(&submit);
    click(&session, initial.document, 6, x, y);
    let (url, _, _) = wait_for_navigation_url(&session, initial.document);
    assert!(url.contains("color=b"), "{url}");
    assert!(url.contains("agree=yes"), "{url}");
    assert!(url.contains("size=m"), "{url}");
}

#[test]
fn scriptless_pattern_numeric_gate() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        124,
        NUMERIC_FIXTURE,
        vec!["input:invalid".into(), "input:out-of-range".into()],
    );
    acknowledge(&session, &initial);

    let code = node_id(text_control(&initial, "code").node_id.to_wire());
    let age = node_id(text_control(&initial, "age").node_id.to_wire());
    assert_eq!(diagnostic_matches(&initial, "input:invalid"), 2);

    send_text(&session, initial.document, 1, code, "ABC");
    send_text(&session, initial.document, 2, age, "200");
    let bad = wait_for_presentation(&session, initial.document, "mismatched controls", |p| {
        diagnostic_matches(p, "input:invalid") == 2
            && diagnostic_matches(p, "input:out-of-range") == 1
    });
    assert_eq!(diagnostic_matches(&bad, "input:out-of-range"), 1);

    send_focus(&session, initial.document, 3, code);
    send_enter(&session, initial.document, 4, code);
    wait_for_presentation(
        &session,
        initial.document,
        "reported numeric feedback",
        |p| text_control(p, "code").invalid,
    );
    assert_no_submit(&session, initial.document, Duration::from_millis(1500));

    // Unconvertible native number text is badInput, not a silent value.
    send_text(&session, initial.document, 5, age, "abc");
    wait_for_presentation(&session, initial.document, "bad input", |p| {
        diagnostic_matches(p, "input:invalid") == 2
    });

    send_text(&session, initial.document, 6, code, "abc");
    send_text(&session, initial.document, 7, age, "30");
    wait_for_presentation(&session, initial.document, "corrected controls", |p| {
        diagnostic_matches(p, "input:invalid") == 0
    });
    send_enter(&session, initial.document, 8, code);
    let (url, _, _) = wait_for_navigation_url(&session, initial.document);
    assert!(url.contains("code=abc"), "{url}");
    assert!(url.contains("age=30"), "{url}");
    assert!(url.contains("level=5"), "{url}");
}

#[test]
fn validation_style_repaints_natively() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for script in ["", "<script>document.body.dataset.ready='yes'</script>"] {
        let session = RendererSession::launch(options()).expect("hidden renderer");
        let initial = load_html_document(
            &session,
            125,
            &format!(
                r#"<!doctype html><style>body{{margin:0}}
                input{{display:block;width:200px;height:30px}}
                span{{display:block;width:100px;height:30px;background:rgb(0,128,0)}}
                input:invalid+span{{background:rgb(255,0,0)}}</style>
                <input required><span>flag</span>{script}"#
            ),
        );
        acknowledge(&session, &initial);
        let red = |presentation: &RendererPresentation| {
            presentation.layout.items.iter().any(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } => {
                    rect.width == 100.0 && color.red == 255
                }
                _ => false,
            })
        };
        let green = |presentation: &RendererPresentation| {
            presentation.layout.items.iter().any(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } => {
                    rect.width == 100.0 && color.green == 128
                }
                _ => false,
            })
        };
        assert!(red(&initial), "invalid flag paints, script={script:?}");
        let target = initial
            .layout
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                    DocumentNodeId::new(control.node_id.to_wire()).ok()
                }
                _ => None,
            })
            .expect("input control target");
        send_text(&session, initial.document, 1, target, "ok");
        let painted = wait_for_presentation(&session, initial.document, "valid repaint", |p| {
            green(p) && !red(p)
        });
        assert!(green(&painted) && !red(&painted), "script={script:?}");
    }
}

#[test]
fn dense_form_native_edit_presents() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(&session, 126, DENSE_FIXTURE);
    acknowledge(&session, &initial);
    let controls = initial
        .layout
        .items
        .iter()
        .filter(|item| {
            matches!(item, DisplayItem::Control(control) if control.kind == ControlKind::Text)
        })
        .count();
    assert!(controls >= 400, "dense fixture controls: {controls}");
    let first = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                DocumentNodeId::new(control.node_id.to_wire()).ok()
            }
            _ => None,
        })
        .expect("dense input target");
    let started = Instant::now();
    send_text(&session, initial.document, 1, first, "edited!");
    let updated = wait_for_presentation(&session, initial.document, "dense edit", |p| {
        p.layout.items.iter().any(|item| {
            matches!(item, DisplayItem::Control(control)
                if control.kind == ControlKind::Text && control.value == "edited!")
        })
    });
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "dense native edit presented in {elapsed:?}"
    );
    assert!(
        updated
            .layout
            .items
            .iter()
            .filter(|item| matches!(
                item,
                DisplayItem::Control(control) if control.kind == ControlKind::Text
            ))
            .count()
            >= 400
    );
}
