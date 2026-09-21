//! Review regressions exercised through hidden renderer input and presentation.
use super::support::*;
use better_web_browser::engine::{ControlKind, ControlSpec, DisplayItem};
use better_web_browser::renderer_process::RendererSession;
use better_web_browser::renderer_protocol::RendererPresentation;
use std::time::Duration;

fn input(presentation: &RendererPresentation) -> ControlSpec {
    presentation
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                Some((**control).clone())
            }
            _ => None,
        })
        .expect("editable control")
}

#[test]
fn canceled_invalid_event_does_not_bypass_native_enter_submission() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        161,
        r#"<form action='/accepted'><input name=q required><button>Send</button></form>
        <script>document.querySelector('input').addEventListener('invalid', e => e.preventDefault());</script>"#,
    );
    acknowledge(&session, &initial);
    let target = node_id(input(&initial).node_id.to_wire());
    send_focus(&session, initial.document, 1, target);
    send_enter(&session, initial.document, 2, target);
    assert_no_submit(&session, initial.document, Duration::from_millis(500));
    send_text(&session, initial.document, 3, target, "valid");
    let edited = wait_for_presentation(&session, initial.document, "corrected value", |p| {
        input(p).value == "valid"
    });
    acknowledge(&session, &edited);
    send_enter(&session, initial.document, 4, target);
    let (url, _, _) = wait_for_navigation_url(&session, initial.document);
    assert!(url.ends_with("/accepted?q=valid"), "{url}");
}

#[test]
fn incomplete_number_remains_visible_but_never_submits_stale_value() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (index, script) in ["", "<script>0</script>"].into_iter().enumerate() {
        let session = RendererSession::launch(options()).expect("hidden renderer");
        let initial = load_html_document_with_selectors(
            &session,
            162 + index as u64,
            &format!(
                "<form action='/accepted'><input type=number name=amount value=12 required><button>Send</button></form>{script}"
            ),
            vec!["input:invalid".into()],
        );
        acknowledge(&session, &initial);
        let target = node_id(input(&initial).node_id.to_wire());
        send_focus(&session, initial.document, 1, target);
        send_text(&session, initial.document, 2, target, "12e");
        let editing = wait_for_presentation(&session, initial.document, "raw numeric edit", |p| {
            input(p).value == "12e"
        });
        assert_eq!(editing.page_diagnostics.selectors[0].total_matches, 1);
        acknowledge(&session, &editing);
        send_enter(&session, initial.document, 3, target);
        assert_no_submit(&session, initial.document, Duration::from_millis(500));
        send_text(&session, initial.document, 4, target, "125");
        let corrected =
            wait_for_presentation(&session, initial.document, "corrected number", |p| {
                input(p).value == "125"
            });
        acknowledge(&session, &corrected);
        send_enter(&session, initial.document, 5, target);
        let (url, _, _) = wait_for_navigation_url(&session, initial.document);
        assert!(url.ends_with("/accepted?amount=125"), "{url}");
    }
}
