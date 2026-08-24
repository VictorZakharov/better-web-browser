use super::ScriptRuntime;
use crate::engine::dom;
use crate::engine::script::ScriptInput;
use crate::storage::{StorageAreaKind, StorageAreaSnapshot, StorageEntry};
use std::rc::Rc;
use std::time::Duration;

#[test]
fn preserves_the_network_cookie_jars_order_and_duplicate_names() {
    let dom = dom::parse_with_scripting(
        r#"<body><div></div><script>
            document.querySelector('div').textContent = document.cookie;
        </script></body>"#,
        true,
    );
    let scripts = script_inputs(&dom);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_document_cookie_header("theme=narrow; session=visible; theme=wide");

    let outcome = runtime.execute_initial(&scripts);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "theme=narrow; session=visible; theme=wide"
    );
}

#[test]
fn browser_snapshots_replace_optimistic_cookie_and_storage_state() {
    let dom = dom::parse_with_scripting(
        "<body><div></div><script></script><script></script></body>",
        true,
    );
    let nodes = dom.elements_named("script").collect::<Vec<_>>();
    let initial = input(
        &nodes[0],
        "initial.js",
        r#"
            document.cookie = 'theme=optimistic; Path=/';
            localStorage.setItem('seed', 'optimistic');
            sessionStorage.setItem('draft', 'optimistic');
        "#,
        true,
    );
    let observe = input(
        &nodes[1],
        "observe.js",
        r#"
            document.querySelector('div').textContent = [
                document.cookie,
                localStorage.getItem('seed'),
                sessionStorage.getItem('draft')
            ].join('|');
        "#,
        false,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime
        .set_document_state(
            2,
            "theme=initial",
            snapshot(3, "seed", "initial"),
            snapshot(4, "draft", "initial"),
        )
        .unwrap();

    let outcome = runtime.execute_initial(&[initial]);
    assert_eq!(outcome.cookie_updates.len(), 1);
    assert_eq!(outcome.storage_updates.len(), 2);
    runtime.replace_cookie_snapshot(9, "theme=authoritative");
    runtime
        .replace_storage_snapshot(
            StorageAreaKind::Local,
            snapshot(10, "seed", "authoritative"),
        )
        .unwrap();
    runtime
        .replace_storage_snapshot(
            StorageAreaKind::Session,
            snapshot(11, "draft", "authoritative"),
        )
        .unwrap();

    let observed = runtime.execute_additional_with_loader(&[observe], None);
    assert!(observed.errors.is_empty(), "{:?}", observed.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "theme=authoritative|authoritative|authoritative"
    );
}

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
fn elapsed_time_makes_a_timer_ready_without_running_its_task() {
    let dom = dom::parse_with_scripting(
        r#"<body><div>waiting</div><script>
            setTimeout(() => document.querySelector('div').textContent = 'done', 2000);
        </script></body>"#,
        true,
    );
    let scripts = script_inputs(&dom);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&scripts).errors.is_empty());

    runtime.elapse_time(Duration::from_millis(500));

    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "waiting"
    );
    assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "done"
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

fn snapshot(version: u64, key: &str, value: &str) -> StorageAreaSnapshot {
    StorageAreaSnapshot {
        version,
        entries: vec![StorageEntry {
            key: key.into(),
            value: value.into(),
        }],
    }
}

fn script_inputs(dom: &dom::Dom) -> Vec<ScriptInput> {
    dom.elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: crate::engine::script::ScriptKind::Classic,
            fetch_options: crate::engine::script::ScriptFetchOptions::for_kind(
                crate::engine::script::ScriptKind::Classic,
            ),
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
        kind: crate::engine::script::ScriptKind::Classic,
        fetch_options: crate::engine::script::ScriptFetchOptions::for_kind(
            crate::engine::script::ScriptKind::Classic,
        ),
        finish_lifecycle,
    }
}
