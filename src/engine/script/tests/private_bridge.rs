use super::*;

#[test]
fn author_script_cannot_use_native_bridge_to_enumerate_closed_shadow_roots() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const host = document.createElement('div');
            document.body.append(host);
            (() => {
                const root = host.attachShadow({mode: 'closed'});
                root.innerHTML = '<span id="private">closed contents</span>';
            })();
            const denied = operation => {
                try { operation(); return false; }
                catch (error) { return error.name === 'ReferenceError'; }
            };
            const safe = host.shadowRoot === null &&
                !('__hostCall' in window) &&
                denied(() => __hostCall('nodeHandle', host)) &&
                denied(() => __hostCall('shadowRootForTraversal', 1)) &&
                denied(() => __hostCall('nodeMetadata', 1)) &&
                denied(() => __hostCall('innerHtmlGet', 1)) &&
                !('__trackModulePromise' in window) &&
                !('__moduleCompletionHandlers' in window) &&
                !('__parserDomChanged' in window) &&
                !('__constructParserElement' in window) &&
                !('__resumeDocumentStream' in window) &&
                !('__setCurrentScript' in window) &&
                !('__dispatchNodeEvent' in window) &&
                !('__queuePolicyViolation' in window) &&
                !('__setCurrentScript' in document) &&
                !('__dispatchNodeEvent' in document) &&
                !('__queuePolicyViolation' in document);
            document.body.setAttribute('data-result', safe ? 'pass' : 'fail');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-result").as_deref(), Some("pass"));
}

#[test]
fn window_module_settlement_does_not_read_author_promise_hooks() {
    for target in ["Promise.prototype,'constructor'", "Promise,Symbol.species"] {
        let dom = crate::engine::dom::parse_with_scripting(
            "<body><div>pending</div><script type=module></script></body>",
            true,
        );
        let input = ScriptInput {
            node: dom.elements_named("script").next().unwrap(),
            source_url: "https://example.test/module.js".into(),
            code: r#"
                let hooks = 0;
                await {get then() {
                    Object.defineProperty(TARGET, {configurable:true, get() {
                        hooks++; throw Error('author Promise hook called');
                    }});
                    return resolve => setTimeout(resolve,10);
                }};
                if(hooks) throw Error('settlement entered author code');
                document.querySelector('div').textContent='completed';
            "#
            .replace("TARGET", target),
            kind: ScriptKind::Module,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Module),
            finish_lifecycle: true,
        };
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
        let initial = runtime.execute_initial(&[input]);
        assert!(initial.errors.is_empty(), "{target}: {:?}", initial.errors);
        let settled = runtime.advance_time(std::time::Duration::from_millis(10), 8);
        assert!(settled.errors.is_empty(), "{target}: {:?}", settled.errors);
        // The standalone Window helper may already settle the short timer.
        // Completion must be reported exactly once, regardless of that turn.
        assert_eq!(initial.executed + settled.executed, 1);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "completed"
        );
    }
}

#[test]
fn unprintable_window_module_rejection_is_reported_without_stopping_the_realm() {
    let dom = crate::engine::dom::parse_with_scripting(
        "<body><div>pending</div><script type=module></script></body>",
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let module = ScriptInput {
        node: node.clone(),
        source_url: "https://example.test/reject.js".into(),
        code: r#"await new Promise((_, reject) => setTimeout(() => reject({
            toString() { throw Error('diagnostic conversion failed'); }
        }), 2000));"#
            .into(),
        kind: ScriptKind::Module,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Module),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let initial = runtime.execute_initial(&[module]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let settled = runtime.advance_time(std::time::Duration::from_millis(500), 8);
    assert_eq!(settled.executed, 0);
    assert_eq!(settled.errors.len(), 1, "{:?}", settled.errors);
    assert!(settled.errors[0].contains("Module evaluation rejected with an unprintable reason"));
    let classic = ScriptInput {
        node,
        source_url: "https://example.test/next.js".into(),
        code: "document.querySelector('div').textContent='still alive';".into(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    };
    let continued = runtime.execute_additional_with_loader(&[classic], None);
    assert!(continued.errors.is_empty(), "{:?}", continued.errors);
    assert_eq!(continued.executed, 1);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "still alive"
    );
}
