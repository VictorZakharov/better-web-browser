use super::*;
use crate::renderer_protocol::ProtocolHandlerAction;

#[test]
fn secure_document_validates_and_queues_registration_without_self_approval() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const result = navigator.registerProtocolHandler('WEB+Soup', '/h?uri=%s');
            document.body.setAttribute('data-return', String(result));
            for (const [scheme, url] of [
                ['http', '/h?uri=%s'], ['web+soup', '/h'],
                ['web+soup', 'https://other.test/h?uri=%s']
            ]) {
                try { navigator.registerProtocolHandler(scheme, url); }
                catch (error) {
                    document.body.setAttribute('data-error-' + scheme + '-' + url.length, error.name);
                }
            }
            navigator.unregisterProtocolHandler('web+soup', '/h?uri=%s');
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/page");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-return").as_deref(), Some("undefined"));
    assert_eq!(
        body.attr("data-error-http-9").as_deref(),
        Some("SecurityError")
    );
    assert_eq!(
        body.attr("data-error-web+soup-2").as_deref(),
        Some("SyntaxError")
    );
    assert_eq!(
        body.attr("data-error-web+soup-27").as_deref(),
        Some("SecurityError")
    );
    assert_eq!(outcome.protocol_handler_actions.len(), 2);
    assert_eq!(outcome.protocol_handler_actions[0].scheme, "web+soup");
    assert_eq!(
        outcome.protocol_handler_actions[0].template,
        "https://example.test/h?uri=%s"
    );
    assert_eq!(
        outcome.protocol_handler_actions[0].action,
        ProtocolHandlerAction::Register
    );
    assert_eq!(
        outcome.protocol_handler_actions[1].action,
        ProtocolHandlerAction::Unregister
    );
}

#[test]
fn insecure_document_does_not_expose_handler_methods() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-exposed', String('registerProtocolHandler' in navigator));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "http://example.test/page");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-exposed")
            .as_deref(),
        Some("false")
    );
    assert!(outcome.protocol_handler_actions.is_empty());
}
