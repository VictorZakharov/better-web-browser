use super::*;
use crate::fetch::{HeaderList, csp::PolicyContainer};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};
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
          const nativeDispatch = EventTarget.prototype.dispatchEvent;
          EventTarget.prototype.dispatchEvent = function(event) {
            if (event.type === 'securitypolicyviolation') window.interceptedPolicyTarget = this;
            return nativeDispatch.call(this, event);
          };
          document.addEventListener('securitypolicyviolation', event => {
            document.querySelector('output').textContent = [event.isTrusted,
              event.target.localName, event.blockedURI, event.effectiveDirective,
              event.originalPolicy, event.sample.startsWith('window.neverRuns'),
              window.interceptedPolicyTarget === undefined].join('|');
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
        "true|script|inline|script-src-elem|script-src 'nonce-allowed' 'report-sample'|true|true"
    );
}

#[test]
fn inserted_inline_script_uses_its_hash_and_reports_a_blocked_sibling() {
    let approved = "document.querySelector('output').textContent += 'allowed';";
    let blocked = "document.querySelector('output').textContent += 'blocked';";
    let hash = STANDARD.encode(Sha256::digest(approved.as_bytes()));
    let html = format!(
        "<body><output></output><script nonce=root>\
         document.addEventListener('securitypolicyviolation', event => {{\
           document.querySelector('output').setAttribute('data-policy', event.originalPolicy);\
           document.querySelector('output').setAttribute('data-sample', event.sample);\
         }});\
         for (const code of [{approved:?}, {blocked:?}]) {{\
           const script = document.createElement('script');\
           script.text = code;\
           document.body.append(script);\
         }}\
         </script></body>"
    );
    let dom = dom::parse_with_scripting(&html, true);
    let mut headers = HeaderList::new();
    let serialized = format!("script-src 'nonce-root' 'sha256-{hash}' 'report-sample'");
    headers
        .append("content-security-policy", &serialized)
        .unwrap();
    let policy = PolicyContainer::from_headers("https://example.test/", &headers).unwrap();
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.test/".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.host.borrow_mut().policy = Arc::new(policy);
    let outcome = runtime.execute_initial(&scripts);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let output = dom.elements_named("output").next().unwrap();
    assert_eq!(output.text_content(), "allowed");
    let event = runtime.advance_time(Duration::from_millis(10), 8);
    assert!(event.errors.is_empty(), "{:?}", event.errors);
    assert_eq!(
        output.attr("data-policy").as_deref(),
        Some(serialized.as_str())
    );
    assert_eq!(output.attr("data-sample").as_deref(), Some(&blocked[..40]));
}

#[test]
fn parsed_inline_script_executes_only_when_its_hash_matches() {
    let approved = "document.querySelector('output').textContent = 'allowed';";
    let denied = "document.querySelector('output').textContent = 'blocked';";
    let hash = STANDARD.encode(Sha256::digest(approved.as_bytes()));
    let dom = dom::parse_with_scripting(
        &format!("<output></output><script>{approved}</script><script>{denied}</script>"),
        true,
    );
    let mut headers = HeaderList::new();
    headers
        .append(
            "content-security-policy",
            &format!("script-src-elem 'sha256-{hash}'"),
        )
        .unwrap();
    let policy = PolicyContainer::from_headers("https://example.test/", &headers).unwrap();
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.test/".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.host.borrow_mut().policy = Arc::new(policy);
    let outcome = runtime.execute_initial(&scripts);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "allowed"
    );
}

#[test]
fn strict_dynamic_allows_inserted_inline_script_but_not_parser_sibling() {
    let dynamic = "document.querySelector('output').textContent += 'dynamic';";
    let html = format!(
        "<body><output></output><script nonce=root>\
         const script = document.createElement('script');\
         script.textContent = {dynamic:?};\
         document.body.append(script);\
         </script><script>document.querySelector('output').textContent += 'parser';</script></body>"
    );
    let dom = dom::parse_with_scripting(&html, true);
    let mut headers = HeaderList::new();
    headers
        .append(
            "content-security-policy",
            "script-src 'nonce-root' 'strict-dynamic'",
        )
        .unwrap();
    let policy = PolicyContainer::from_headers("https://example.test/", &headers).unwrap();
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.test/".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.host.borrow_mut().policy = Arc::new(policy);
    let outcome = runtime.execute_initial(&scripts);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "dynamic"
    );
}

#[test]
fn blocked_external_script_reports_origin_without_cross_origin_path_or_sample() {
    let dom = dom::parse_with_scripting(
        r#"<body><output></output><script nonce=allowed>
        document.addEventListener('securitypolicyviolation', event => {
            document.querySelector('output').textContent = [event.isTrusted,
                event.target.localName, event.blockedURI,
                event.effectiveDirective, event.sample, event.sourceFile].join('|');
        });</script><script src='https://cdn.example.test/private/app.js?token=secret'></script></body>"#,
        true,
    );
    let mut headers = HeaderList::new();
    headers
        .append("content-security-policy", "script-src 'nonce-allowed'")
        .unwrap();
    let policy = PolicyContainer::from_headers("https://example.com/", &headers).unwrap();
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: node
                .attr("src")
                .unwrap_or_else(|| "https://example.com/".into()),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.host.borrow_mut().policy = Arc::new(policy);
    let outcome = runtime.execute_initial(&scripts);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    let event = runtime.advance_time(Duration::from_millis(10), 8);
    assert!(event.errors.is_empty(), "{:?}", event.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|script|https://cdn.example.test|script-src-elem||"
    );
}

#[test]
fn dynamic_meta_csp_takes_effect_within_the_same_task_without_attribute_reprocessing() {
    let (_dom, outcome) = execute_html(
        r#"<body><script>
        const meta = document.createElement('meta');
        meta.setAttribute('http-equiv', 'Content-Security-Policy');
        meta.setAttribute('content', "connect-src 'none'");
        document.head.appendChild(meta);
        fetch('/after');
        meta.setAttribute('content', 'connect-src *');
        fetch('/still-blocked');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.policy_updates.len(), 1);
    assert_eq!(outcome.policy_updates[0].serialized, "connect-src 'none'");
    assert_eq!(outcome.fetch_actions.len(), 2);
    for action in &outcome.fetch_actions {
        let ScriptFetchAction::Start { request, .. } = action else {
            panic!("expected a Fetch start action");
        };
        assert!(
            request
                .policy
                .check_request(
                    crate::fetch::RequestDestination::Fetch,
                    request.url.as_str(),
                    0
                )
                .is_err(),
            "fetch escaped the inserted CSP"
        );
    }
}

#[test]
fn parser_meta_csp_blocks_eval_but_allows_authorized_inline_script() {
    let dom = dom::parse_with_scripting(
        r#"<head><meta http-equiv="Content-Security-Policy"
            content="script-src 'unsafe-inline'"></head>
            <body><output></output><script>
            const output = document.querySelector('output');
            try { eval('true'); output.textContent = 'eval-allowed'; }
            catch (error) { output.textContent = error.name; }
            </script></body>"#,
        true,
    );
    let meta = dom.elements_named("meta").next().unwrap();
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.process_parser_csp_meta(&meta).unwrap();
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "EvalError"
    );
}

#[test]
fn response_csp_blocks_eval_after_realm_creation() {
    let dom = dom::parse_with_scripting(
        r#"<body><output></output><script>
        try { Function('return true')(); document.querySelector('output').textContent = 'allowed'; }
        catch (error) { document.querySelector('output').textContent = error.name; }
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let mut headers = HeaderList::new();
    headers
        .append("content-security-policy", "script-src 'unsafe-inline'")
        .unwrap();
    runtime.set_document_policy(Arc::new(
        PolicyContainer::from_headers("https://example.com/", &headers).unwrap(),
    ));
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "EvalError"
    );
}

#[test]
fn dynamically_inserted_meta_csp_blocks_eval_in_the_same_script() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const meta = document.createElement('meta');
        meta.setAttribute('http-equiv', 'Content-Security-Policy');
        meta.setAttribute('content', "script-src 'unsafe-inline'");
        document.head.appendChild(meta);
        const output = document.querySelector('output');
        try { eval('true'); output.textContent = 'eval-allowed'; }
        catch (error) { output.textContent = error.name; }
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "EvalError"
    );
}

#[test]
fn response_csp_with_unsafe_eval_keeps_string_generation_available() {
    let dom = dom::parse_with_scripting(
        r#"<body><output></output><script>
        const generated = Function('return 42')();
        document.querySelector('output').textContent = String(generated);
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let mut headers = HeaderList::new();
    headers
        .append(
            "content-security-policy",
            "script-src 'unsafe-inline' 'unsafe-eval'",
        )
        .unwrap();
    runtime.set_document_policy(Arc::new(
        PolicyContainer::from_headers("https://example.com/", &headers).unwrap(),
    ));
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "42"
    );
}
