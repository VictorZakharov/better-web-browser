use super::*;
use crate::fetch::{HeaderList, csp::PolicyContainer};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn synthetic_violation_event_has_csp3_defaults_and_readonly_fields() {
    let (dom, outcome) = execute_html(
        r#"<output></output><script>
        const event = new SecurityPolicyViolationEvent('securitypolicyviolation', {
            effectiveDirective: 'script-src-elem', blockedURI: 'inline',
            disposition: 'report', lineNumber: 7
        });
        const output = document.querySelector('output');
        output.textContent = [event instanceof Event, event.bubbles,
            event.effectiveDirective, event.blockedURI, event.disposition,
            event.lineNumber, event.statusCode, event.documentURI].join('|');
        try { new SecurityPolicyViolationEvent('securitypolicyviolation', { disposition: 'bad' }); }
        catch (error) { output.setAttribute('data-error', error.name); }
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let output = dom.elements_named("output").next().unwrap();
    assert_eq!(
        output.text_content(),
        "true|false|script-src-elem|inline|report|7|0|"
    );
    assert_eq!(output.attr("data-error").as_deref(), Some("TypeError"));
}

#[test]
fn blocked_inline_script_queues_trusted_policy_event_with_sample_and_policy() {
    let dom = dom::parse_with_scripting(
        r#"<body><output id=status></output>
        <script nonce=allowed>
          document.addEventListener('securitypolicyviolation', event => {
            document.querySelector('output').textContent = [event.isTrusted,
              event.target.localName, event.blockedURI, event.effectiveDirective,
              event.originalPolicy, event.sample.startsWith('window.neverRuns')].join('|');
          });
        </script>
        <script>window.neverRuns = true;</script></body>"#,
        true,
    );
    let mut headers = HeaderList::new();
    headers
        .append(
            "content-security-policy",
            "script-src 'nonce-allowed' 'report-sample'",
        )
        .unwrap();
    let policy =
        PolicyContainer::from_headers("https://example.com/page#secret", &headers).unwrap();
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/page#secret".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/page#secret");
    runtime.host.borrow_mut().policy = Arc::new(policy);
    let outcome = runtime.execute_initial(&scripts);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    let output = dom.elements_named("output").next().unwrap();
    let event = runtime.advance_time(Duration::from_millis(10), 8);
    assert!(event.errors.is_empty(), "{:?}", event.errors);
    assert_eq!(
        output.text_content(),
        "true|script|inline|script-src-elem|script-src 'nonce-allowed' 'report-sample'|true"
    );
}
