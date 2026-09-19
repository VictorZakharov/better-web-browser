use super::*;

#[test]
fn location_history_distinguishes_initial_redirect_assign_replace_and_reload() {
    let (dom, mut runtime) = start("<body><script></script>");
    let result = evaluate(&mut runtime, &dom, "location.assign('/initial');");
    assert!(result.navigation_options.replace_history);
    runtime.finish_document_lifecycle();
    drain(&mut runtime);
    assert!(runtime.document_load_finished());
    for (code, replace) in [
        ("location.assign('/next')", false),
        ("location.href='/href'", false),
        ("location.replace('/replace')", true),
        ("location.reload()", true),
    ] {
        let result = evaluate(&mut runtime, &dom, code);
        assert!(result.navigation_url.is_some(), "{code}");
        assert_eq!(result.navigation_options.replace_history, replace, "{code}");
    }
}

#[test]
fn loaded_parent_navigation_from_child_message_preserves_back_entry() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const frame = document.createElement('iframe');
        frame.srcdoc = '<script>onmessage=()=>parent.location.assign("/result")<\/script>';
        document.body.append(frame);
    </script>"#,
    );
    runtime.finish_document_lifecycle();
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "frame.contentWindow.postMessage('go', '*');",
    );
    let result = runtime.advance_time(Duration::ZERO, 20);
    assert_eq!(
        result.navigation_url.as_deref(),
        Some("https://example.com/result")
    );
    assert!(!result.navigation_options.replace_history);
}
