use super::*;
use crate::renderer_protocol::{
    ClipboardAction, ClipboardError, ClipboardUpdate, ClipboardValue, DocumentId,
    MAX_CLIPBOARD_TEXT_BYTES,
};

#[test]
fn clipboard_is_secure_same_object_and_read_write_settle_after_browser_reply() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const clipboard = navigator.clipboard;
            document.body.setAttribute('data-shape', String(
                clipboard === navigator.clipboard &&
                clipboard instanceof Clipboard &&
                typeof clipboard.readText === 'function' &&
                typeof clipboard.writeText === 'function' &&
                !('read' in clipboard) && !('write' in clipboard)));
            clipboard.writeText('hello 🦊').then(() =>
                document.body.setAttribute('data-written', 'yes'));
            clipboard.readText().then(text =>
                document.body.setAttribute('data-read', text));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-shape").as_deref(), Some("true"));
    assert_eq!(body.attr("data-written"), None);
    assert_eq!(initial.clipboard_actions.len(), 2);
    assert!(matches!(
        &initial.clipboard_actions[0].action,
        ClipboardAction::WriteText(text) if text == "hello 🦊"
    ));
    assert_eq!(
        initial.clipboard_actions[1].action,
        ClipboardAction::ReadText
    );
    let document = DocumentId::new(1).unwrap();
    let written = runtime.deliver_clipboard_update(ClipboardUpdate {
        document,
        request_id: initial.clipboard_actions[0].request_id,
        result: Ok(ClipboardValue::Written),
    });
    assert!(written.errors.is_empty(), "{:?}", written.errors);
    assert_eq!(body.attr("data-written").as_deref(), Some("yes"));
    let read = runtime.deliver_clipboard_update(ClipboardUpdate {
        document,
        request_id: initial.clipboard_actions[1].request_id,
        result: Ok(ClipboardValue::Text("OS clipboard text".into())),
    });
    assert!(read.errors.is_empty(), "{:?}", read.errors);
    assert_eq!(body.attr("data-read").as_deref(), Some("OS clipboard text"));
}

#[test]
fn denied_and_missing_clipboard_reject_with_distinct_dom_exceptions() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.clipboard.readText().catch(error =>
                document.body.setAttribute('data-denied', error.name));
            navigator.clipboard.readText().catch(error =>
                document.body.setAttribute('data-empty', error.name));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    for (request, error) in initial
        .clipboard_actions
        .iter()
        .zip([ClipboardError::NotAllowed, ClipboardError::NotFound])
    {
        let outcome = runtime.deliver_clipboard_update(ClipboardUpdate {
            document: DocumentId::new(1).unwrap(),
            request_id: request.request_id,
            result: Err(error),
        });
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-denied").as_deref(), Some("NotAllowedError"));
    assert_eq!(body.attr("data-empty").as_deref(), Some("NotFoundError"));
}

#[test]
fn clipboard_is_not_exposed_to_insecure_origins() {
    let dom = dom::parse_with_scripting(
        "<body><script>document.body.setAttribute('data-available', String('clipboard' in navigator));</script></body>",
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "http://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-available")
            .as_deref(),
        Some("false")
    );
}

#[test]
fn oversized_write_rejects_before_queuing_ipc() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.clipboard.writeText('x'.repeat(262145)).catch(error =>
                document.body.setAttribute('data-error', error.name));
        </script></body>"#,
        true,
    );
    assert_eq!(MAX_CLIPBOARD_TEXT_BYTES, 262144);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.clipboard_actions.is_empty());
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-error")
            .as_deref(),
        Some("RangeError")
    );
}
