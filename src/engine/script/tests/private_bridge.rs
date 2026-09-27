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
