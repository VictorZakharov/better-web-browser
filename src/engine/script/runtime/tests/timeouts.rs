use super::*;

#[test]
fn timed_out_timer_preserves_partial_mutations_and_allows_later_tasks() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            let attempts = 0;
            let applicationState = 'initial';
            setTimeout(function interruptedUpdate() {
                attempts++;
                applicationState = 'partial';
                document.body.setAttribute('data-phase', applicationState);
                try { for (;;) {} }
                catch (_) { document.body.setAttribute('data-caught', 'yes'); }
                applicationState = 'complete';
                document.body.setAttribute('data-phase', applicationState);
            }, 100);
            setTimeout(function independentUpdate() {
                document.body.setAttribute('data-later', applicationState + '|' + attempts);
            }, 200);
        </script></body>"#,
        true,
    );
    let scripts = script_inputs(&dom);
    let body = dom.elements_named("body").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial_before_document_completion(&scripts, None);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(body.attr("data-phase"), None);

    let interrupted = runtime.advance_time(Duration::from_millis(100), 1);

    assert!(
        interrupted.errors.iter().any(|error| {
            error.contains("interruptedUpdate") && error.contains("execution time limit")
        }),
        "{:?}",
        interrupted.errors
    );
    assert!(interrupted.mutation_count > 0);
    assert!(!interrupted.runtime_stopped);
    assert!(runtime.is_active());
    assert_eq!(body.attr("data-phase").as_deref(), Some("partial"));
    assert_eq!(body.attr("data-caught"), None);
    assert_eq!(body.attr("data-later"), None);

    let continued = runtime.advance_time(Duration::from_millis(100), 1);

    assert!(continued.errors.is_empty(), "{:?}", continued.errors);
    assert!(!continued.runtime_stopped);
    assert_eq!(body.attr("data-later").as_deref(), Some("partial|1"));
    assert_eq!(body.attr("data-phase").as_deref(), Some("partial"));
    assert_eq!(runtime.next_timer_delay(), None);

    let idle = runtime.advance_time(Duration::from_secs(1), 1);
    assert!(idle.errors.is_empty(), "{:?}", idle.errors);
    assert_eq!(
        idle.mutation_count, 0,
        "the aborted one-shot timer was retried"
    );
}
