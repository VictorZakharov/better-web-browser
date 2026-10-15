use super::*;

fn assert_sample(outcome: &WorkerRuntimeOutcome, expected: usize) {
    // Parallel native GPU tests may change real process pressure. Its single
    // aggregate is distinct from the unchanged task/heap sample contract.
    let pressure = outcome
        .diagnostics
        .iter()
        .filter(|row| row.starts_with("V8 process-pressure hints:"))
        .count();
    assert!(pressure <= usize::from(expected > 0));
    assert_eq!(
        outcome.diagnostics.len() - pressure,
        expected,
        "{:?}",
        outcome.diagnostics
    );
}

fn worker() -> WorkerRuntime {
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/worker.js",
        "onmessage=event=>postMessage(event.data);",
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(outcome.errors.is_empty());
    assert!(outcome.diagnostics.is_empty());
    runtime.unwrap()
}

#[test]
fn worker_execution_attribution_is_opt_in_bounded_and_contains_no_message_payload() {
    let mut runtime = worker();
    assert!(runtime.dispatch_message("123").diagnostics.is_empty());
    runtime.set_execution_diagnostics(true);
    for index in 0..12 {
        // Reapplying an unchanged settings value cannot replenish the budget.
        runtime.set_execution_diagnostics(true);
        let outcome = runtime.dispatch_message("\"private author payload\"");
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.messages, ["\"private author payload\""]);
        if index < 8 {
            assert_sample(&outcome, 2);
            assert!(
                outcome
                    .diagnostics
                    .iter()
                    .any(|row| row.contains("GC callback span"))
            );
            assert!(
                outcome
                    .diagnostics
                    .iter()
                    .any(|row| row.contains("V8 heap:"))
            );
            assert!(
                !outcome
                    .diagnostics
                    .join(" ")
                    .contains("private author payload")
            );
        } else {
            assert!(outcome.diagnostics.is_empty());
        }
    }
    runtime.set_execution_diagnostics(false);
    assert!(runtime.dispatch_message("456").diagnostics.is_empty());
    runtime.set_execution_diagnostics(true);
    assert_sample(&runtime.dispatch_message("789"), 2);
}

#[test]
fn repeated_failed_tasks_have_one_reserved_sample_after_success_budget_is_used() {
    let (runtime, started) = WorkerRuntime::start(
        "https://example.test/worker.js",
        "onmessage=event=>{if(event.data==='private author payload') {for(;;){}}postMessage(event.data);};",
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(started.errors.is_empty());
    let mut runtime = runtime.unwrap();
    runtime.set_execution_diagnostics(true);
    for _ in 0..8 {
        assert_sample(&runtime.dispatch_message("123"), 2);
    }
    fn failed_task(runtime: &mut WorkerRuntime) -> WorkerRuntimeOutcome {
        // This test checks six diagnostic-budget transitions, not deadline length.
        // Keep real watchdog failures, scoped to these known infinite callbacks;
        // normal bootstrap/tasks and the production-deadline tests remain unchanged.
        let original = runtime
            .context
            .replace_timeout_for_test(Duration::from_millis(100));
        let outcome = runtime.dispatch_message("\"private author payload\"");
        runtime.context.replace_timeout_for_test(original);
        assert!(
            outcome
                .errors
                .iter()
                .any(|error| error.contains("execution time limit"))
        );
        outcome
    }
    // Use a real watchdog failure: ordinary listener exceptions are reported
    // by event dispatch rather than returned as failed V8 execution tasks.
    for index in 0..4 {
        runtime.set_execution_diagnostics(true);
        let outcome = failed_task(&mut runtime);
        assert!(!outcome.errors.is_empty());
        assert_sample(&outcome, if index == 0 { 2 } else { 0 });
        assert!(
            !outcome
                .diagnostics
                .join(" ")
                .contains("private author payload")
        );
    }
    assert!(runtime.dispatch_message("456").diagnostics.is_empty());
    runtime.set_execution_diagnostics(false);
    assert!(failed_task(&mut runtime).diagnostics.is_empty());
    runtime.set_execution_diagnostics(true);
    assert_sample(&failed_task(&mut runtime), 2);
    assert_sample(&runtime.dispatch_message("789"), 2);
}
