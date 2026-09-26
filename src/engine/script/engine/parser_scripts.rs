//! Nested classic evaluation shares the active isolate, watchdog deadline and global scope.
pub(super) fn run(
    scope: &mut v8::PinScope,
    arguments: v8::FunctionCallbackArguments,
    mut return_value: v8::ReturnValue,
) {
    let (Ok(code), Ok(url)) = (
        v8::Local::<v8::String>::try_from(arguments.get(1)),
        v8::Local::<v8::String>::try_from(arguments.get(2)),
    ) else {
        return;
    };
    let Some(node_id) = arguments.get(3).uint32_value(scope) else {
        return;
    };
    let text = code.to_rust_string_lossy(scope);
    let Some(host) = super::node_wrappers::host(scope.get_current_context()) else {
        return;
    };
    let state = host.borrow();
    if state.sandbox.scripts_blocked {
        return;
    }
    let Some(node) = state.node(node_id) else {
        return;
    };
    let nonce = node.attr("nonce");
    let parser_inserted = node
        .element()
        .is_some_and(|element| element.script_parser_inserted.get());
    let violations = state.policy.inline_script_violations_for_source(
        nonce.as_deref(),
        Some(&text),
        parser_inserted,
    );
    if !violations.is_empty() {
        let document_url =
            super::super::execution::csp_reporting::strip_report_url(&state.document_url);
        let report = serde_json::json!({
            "documentUrl": document_url,
            "violations": violations.iter().map(|violation| serde_json::json!({
                "originalPolicy": violation.original_policy,
                "sample": if violation.report_sample {
                    text.chars().take(40).collect::<String>()
                } else {
                    String::new()
                },
            })).collect::<Vec<_>>(),
        });
        if let Some(serialized) = v8::String::new(scope, &report.to_string()) {
            return_value.set(serialized.into());
        }
        return;
    }
    drop(state);
    let origin = v8::ScriptOrigin::new(
        scope,
        url.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    );
    if let Some(script) = v8::Script::compile(scope, code, Some(&origin)) {
        // Preserve pending exceptions for the binding's error-reporting algorithm. Do not
        // convert the completion value: script evaluation does not coerce it to a string.
        script.run(scope);
    }
    return_value.set(v8::Boolean::new(scope, true).into());
}
