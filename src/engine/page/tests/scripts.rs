use super::*;

#[test]
fn discovers_external_scripts_and_executes_dom_mutations() {
    let mut page = Page::parse_scripted(
        r#"<body><main id="app"></main><script src="/library.js"></script>
            <script>const item = document.createElement('p');
            item.textContent = libraryMessage;
            document.getElementById('app').appendChild(item);</script>"#,
        "https://example.com/start",
    );
    let options = ScriptFetchOptions::for_kind(ScriptKind::Classic);
    assert!(page.resources.contains(&PageResource::Script {
        url: "https://example.com/library.js".into(),
        kind: ScriptKind::Classic,
        fetch_options: options,
    }));
    page.add_script(
        "https://example.com/library.js",
        ScriptKind::Classic,
        options,
        "const libraryMessage = 'loaded';".into(),
    );
    let outcome = page.execute_scripts();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 2);
    assert_eq!(
        page.dom.elements_named("p").next().unwrap().text_content(),
        "loaded"
    );
}

#[test]
fn discovers_module_script_fetch_policy() {
    let page = Page::parse_scripted(
        r#"<script type=module crossorigin=use-credentials
                   referrerpolicy=no-referrer src=/app.js></script>
            <script type=application/json>{"ignored":true}</script>"#,
        "https://example.com/start",
    );
    let options = ScriptFetchOptions::for_element(
        ScriptKind::Module,
        Some("use-credentials"),
        Some("no-referrer"),
    );
    assert_eq!(page.scripts.len(), 1);
    assert_eq!(page.scripts[0].kind, ScriptKind::Module);
    assert_eq!(page.scripts[0].fetch_options, options);
    assert!(page.scripts[0].blocks_first_paint);
    assert!(page.scripts[0].executes_after_parsing);
    assert!(page.resources.contains(&PageResource::Script {
        url: "https://example.com/app.js".into(),
        kind: ScriptKind::Module,
        fetch_options: options,
    }));
}

#[test]
fn keeps_distinct_fetch_policies_for_the_same_script_url() {
    let page = Page::parse_scripted(
        r#"<script async src=/shared.js></script>
            <script async crossorigin=anonymous src=/shared.js></script>"#,
        "https://example.com/start",
    );
    let resources = page
        .resources
        .iter()
        .filter(|resource| matches!(resource, PageResource::Script { .. }))
        .collect::<Vec<_>>();
    assert_eq!(resources.len(), 2);
    assert_ne!(resources[0], resources[1]);
}

#[test]
fn modules_execute_after_later_parser_blocking_classic_scripts() {
    let mut page = Page::parse_scripted(
        r#"<body><script>globalThis.order = ['first'];</script>
            <script type=module>
              order.push('module'); document.body.textContent = order.join(',');
            </script>
            <script>order.push('last');</script></body>"#,
        "https://example.com/",
    );
    let outcome = page.execute_scripts();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        page.dom
            .elements_named("body")
            .next()
            .unwrap()
            .text_content(),
        "first,last,module"
    );
}

#[test]
fn keeps_async_scripts_off_the_first_paint_path() {
    let mut page = Page::parse_scripted(
        r#"<body><div id=status>initial</div>
            <script async src=/analytics.js></script>
            <script src=/application.js></script>"#,
        "https://example.com/",
    );
    let options = ScriptFetchOptions::for_kind(ScriptKind::Classic);
    let resource = |name: &str| PageResource::Script {
        url: format!("https://example.com/{name}.js"),
        kind: ScriptKind::Classic,
        fetch_options: options,
    };
    assert!(!page.resource_blocks_first_paint(&resource("analytics")));
    assert!(page.resource_blocks_first_paint(&resource("application")));
    assert!(!page.resource_blocks_first_paint(&PageResource::Image {
        url: "https://example.com/hero.png".into(),
    }));

    page.add_script(
        "https://example.com/analytics.js",
        ScriptKind::Classic,
        options,
        "document.getElementById('status').textContent = 'analytics';".into(),
    );
    page.add_script(
        "https://example.com/application.js",
        ScriptKind::Classic,
        options,
        "document.getElementById('status').textContent = 'application';".into(),
    );
    let outcome = page.execute_first_paint_scripts();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    assert_eq!(
        page.dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "application"
    );
}

#[test]
fn retained_first_paint_runtime_mutates_the_same_page_after_load() {
    let mut page = Page::parse_scripted(
        r#"<body><div id=status>initial</div><script>
            globalThis.runtimeMarker = 41;
            setTimeout(() => {
                runtimeMarker += 1;
                document.getElementById('status').textContent = `updated ${runtimeMarker}`;
            }, 2000);</script>"#,
        "https://example.com/",
    );
    let mut unused_loader = |_: &str, _, _| Err("unexpected dynamic script".to_string());
    let (runtime, initial) = page.start_first_paint_script_runtime_with_loader(&mut unused_loader);
    let mut runtime = runtime.expect("a loaded script should retain its realm");
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        page.dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "initial"
    );
    assert_eq!(
        runtime.next_timer_delay(),
        Some(std::time::Duration::from_millis(500))
    );

    let post_load = runtime.advance_time(std::time::Duration::from_millis(500), 128);
    assert!(post_load.errors.is_empty(), "{:?}", post_load.errors);
    assert!(post_load.render_requested);
    assert_eq!(post_load.mutation_count, 1);
    assert_eq!(
        page.dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "updated 42"
    );
}

#[test]
fn async_only_page_retains_an_empty_realm_for_later_execution() {
    let mut page = Page::parse_scripted(
        r#"<body><div id=status>initial</div><script async src=later.js></script>"#,
        "https://example.com/",
    );
    let mut unused_loader = |_: &str, _, _| Err("unexpected dynamic script".to_string());
    let (runtime, initial) = page.start_first_paint_script_runtime_with_loader(&mut unused_loader);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.executed, 0);
    let mut runtime = runtime.expect("async-only page did not retain a realm");
    let script = page.scripts.first().expect("async script was discovered");
    let later = super::super::super::script::ScriptInput {
        node: script.node.clone(),
        source_url: script.source_url.clone(),
        code: "document.body.textContent = document.readyState;".into(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    };
    let outcome = runtime.execute_additional_with_loader(&[later], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        page.dom
            .elements_named("body")
            .next()
            .unwrap()
            .text_content(),
        "complete"
    );
}
