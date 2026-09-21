use super::support::*;
use better_web_browser::engine::{ControlKind, DisplayItem};
use better_web_browser::renderer_process::RendererSession;
use better_web_browser::renderer_protocol::{
    NavigationCause, NavigationDisposition, RendererPresentation,
};
use std::time::Duration;

const TEXT_FIXTURE: &str = include_str!("../fixtures/form-validation-text.html");
const CONTROLS_FIXTURE: &str = include_str!("../fixtures/form-validation-controls.html");
const NUMERIC_FIXTURE: &str = include_str!("../fixtures/form-validation-numeric.html");

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
