//! Real document handoff preserves bounded, opt-in engine attribution.
use super::*;

fn engine_rows(outcome: &crate::engine::script::ScriptOutcome) -> Vec<&str> {
    outcome
        .diagnostics
        .iter()
        .filter(|line| line.starts_with("document engine task"))
        .map(String::as_str)
        .collect()
}

fn failed_rows(outcome: &crate::engine::script::ScriptOutcome) -> Vec<&str> {
    engine_rows(outcome)
        .into_iter()
        .filter(|line| line.starts_with("document engine task (failed)"))
        .collect()
}

#[test]
fn document_engine_failure_sampling_is_private_drained_and_not_replenished_by_enable() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_host_call_profiling(true);
    let initial = input(
        &node,
        "private-source.js",
        "throw Error('private author data')",
        true,
    );
    let first = runtime.execute_initial(std::slice::from_ref(&initial));
    assert!(!first.errors.is_empty());
    let rows = failed_rows(&first);
    assert_eq!(rows.len(), 1, "{:?}", first.diagnostics);
    assert!(rows[0].contains("failed"));
    assert!(rows[0].contains("owner-thread CPU including native work"));
    assert!(
        engine_rows(&first)
            .iter()
            .all(|line| !line.contains("private"))
    );
    // Same-state settings updates and outcome drains retain the Agent budget.
    runtime.set_host_call_profiling(true);
    let repeated = runtime.execute_additional_with_loader(std::slice::from_ref(&initial), None);
    assert!(!repeated.errors.is_empty());
    assert!(failed_rows(&repeated).is_empty());
    runtime.set_host_call_profiling(false);
    let disabled = runtime.execute_additional_with_loader(std::slice::from_ref(&initial), None);
    assert!(engine_rows(&disabled).is_empty());
    runtime.set_host_call_profiling(true);
    let reenabled = runtime.execute_additional_with_loader(&[initial], None);
    assert_eq!(failed_rows(&reenabled).len(), 1);
    assert!(runtime.is_active());
}

#[test]
fn ordinary_document_execution_has_no_engine_profile_or_author_console_noise() {
    let dom = dom::parse_with_scripting(
        "<body><script>throw Error('expected')</script></body>",
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(!outcome.errors.is_empty());
    assert!(engine_rows(&outcome).is_empty());
    assert!(outcome.console.is_empty());
}
