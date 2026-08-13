use super::ScriptRuntime;
use crate::engine::dom;
use crate::engine::script::ScriptInput;
use std::rc::Rc;
use std::time::Duration;

#[test]
fn retained_runtime_executes_post_load_work_in_the_same_realm() {
    let dom = dom::parse_with_scripting(
        r#"<body><div id="status">waiting</div><script>
            window.retainedValue = 41;
            setTimeout(() => {
                document.getElementById('status').textContent = String(++window.retainedValue);
            }, 2000);
        </script></body>"#,
        true,
    );
    let scripts = script_inputs(&dom);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");

    let initial = runtime.execute_initial(&scripts);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(!initial.render_requested);
    assert_eq!(runtime.next_timer_delay(), Some(Duration::from_millis(500)));
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "waiting"
    );

    let post_load = runtime.advance_time(Duration::from_millis(500), 16);
    assert!(post_load.errors.is_empty(), "{:?}", post_load.errors);
    assert!(post_load.render_requested);
    assert_eq!(post_load.mutation_count, 1);
    assert_eq!(runtime.next_timer_delay(), None);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "42"
    );
}

#[test]
fn retained_runtime_executes_an_additional_script_in_the_same_realm() {
    let dom = dom::parse_with_scripting(
        r#"<body><div>waiting</div><script></script><script></script></body>"#,
        true,
    );
    let nodes = dom.elements_named("script").collect::<Vec<_>>();
    let initial = input(&nodes[0], "initial.js", "window.sharedValue = 41;", true);
    let additional = input(
        &nodes[1],
        "async.js",
        r#"document.querySelector('div').textContent = String(++sharedValue);"#,
        false,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");

    let first = runtime.execute_initial(&[initial]);
    let later = runtime.execute_additional_with_loader(&[additional], None);

    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(later.errors.is_empty(), "{:?}", later.errors);
    assert_eq!(later.executed, 1);
    assert!(later.render_requested);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "42"
    );
}

#[test]
fn a_failed_additional_script_does_not_poison_the_document_realm() {
    let dom = dom::parse_with_scripting(
        r#"<body><div>waiting</div><script></script><script></script>"#,
        true,
    );
    let nodes = dom.elements_named("script").collect::<Vec<_>>();
    let scripts = [
        input(
            &nodes[0],
            "fails.js",
            "throw new Error('expected async failure');",
            false,
        ),
        input(
            &nodes[1],
            "continues.js",
            "document.querySelector('div').textContent = 'continued';",
            false,
        ),
    ];
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&[]).errors.is_empty());

    let outcome = runtime.execute_additional_with_loader(&scripts, None);

    assert_eq!(outcome.executed, 1);
    assert!(
        outcome
            .errors
            .iter()
            .any(|error| error.contains("expected async failure"))
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "continued"
    );
}

#[test]
fn retained_document_realms_advance_independently() {
    let (first_dom, mut first_runtime) = retained_fixture("first");
    let (second_dom, mut second_runtime) = retained_fixture("second");
    first_runtime.advance_time(Duration::from_millis(500), 16);

    assert_eq!(
        first_dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "first"
    );
    assert_eq!(
        second_dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "waiting"
    );

    second_runtime.advance_time(Duration::from_millis(500), 16);
    assert_eq!(
        second_dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "second"
    );
}

#[test]
fn dropping_a_runtime_releases_its_document_owned_scheduler() {
    let dom = dom::parse_with_scripting("<body></body>", true);
    let runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let host = Rc::downgrade(&runtime.host);
    drop(runtime);
    assert!(host.upgrade().is_none());
}

#[test]
fn cancelling_a_document_prevents_its_retained_callbacks() {
    let dom = dom::parse_with_scripting(
        r#"<body><div>waiting</div><script>
            setTimeout(() => {
                document.querySelector('div').textContent = 'stale callback';
            }, 2000);
        </script></body>"#,
        true,
    );
    let scripts = script_inputs(&dom);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&scripts);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);

    runtime.cancel_document();
    assert!(!runtime.is_active());
    assert_eq!(runtime.next_timer_delay(), None);
    let cancelled = runtime.advance_time(Duration::from_secs(10), 128);

    assert!(cancelled.runtime_stopped);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "waiting"
    );
    let host = runtime.host.borrow();
    assert_eq!(host.timers.pending_task_count(), 0);
    assert_eq!(host.timers.pending_microtask_count(), 0);
}

fn retained_fixture(label: &str) -> (dom::Dom, ScriptRuntime) {
    let html = format!(
        r#"<body><div>waiting</div><script>
            window.realmLabel = '{label}';
            setTimeout(() => {{
                document.querySelector('div').textContent = window.realmLabel;
            }}, 2000);
        </script></body>"#
    );
    let dom = dom::parse_with_scripting(&html, true);
    let scripts = script_inputs(&dom);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&scripts);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

fn script_inputs(dom: &dom::Dom) -> Vec<ScriptInput> {
    dom.elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            finish_lifecycle: true,
        })
        .collect()
}

fn input(
    node: &crate::engine::dom::NodeRef,
    source: &str,
    code: &str,
    finish_lifecycle: bool,
) -> ScriptInput {
    ScriptInput {
        node: node.clone(),
        source_url: format!("https://example.com/{source}"),
        code: code.into(),
        finish_lifecycle,
    }
}
