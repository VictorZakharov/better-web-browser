//! The API loading path must obey font policy just like a CSS resource request.
use super::*;
use crate::fetch::{HeaderList, RequestDestination, csp::PolicyContainer};

fn policy(text: &str) -> std::sync::Arc<PolicyContainer> {
    let mut headers = HeaderList::new();
    headers.append("content-security-policy", text).unwrap();
    std::sync::Arc::new(PolicyContainer::from_headers("https://example.test/", &headers).unwrap())
}

fn worker(
    policy_text: &str,
) -> (
    WorkerRuntime,
    super::super::worker_runtime::WorkerRuntimeOutcome,
) {
    let (runtime, outcome) = WorkerRuntime::start_with_policy(
        "https://example.test/font-policy.js",
        r#"if('__fontFaceFetch' in self)throw Error('private font fetch leaked');
        const face=new FontFace('PolicyFont','url(/font.ttf)');fonts.add(face);
        face.load().catch(error=>postMessage('font:'+error.name+':'+face.status));
        fetch('/ordinary').catch(()=>postMessage('ordinary-denied'));"#,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected import {url}"))),
        policy(policy_text),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (runtime.unwrap(), outcome)
}

#[test]
fn allowed_worker_font_is_not_blocked_by_connect_src_and_keeps_redirect_policy() {
    let (_runtime, outcome) = worker("font-src 'self'; connect-src 'none'");
    assert_eq!(outcome.fetch_actions.len(), 1);
    let ScriptFetchAction::Start { request, .. } = &outcome.fetch_actions[0] else {
        panic!("expected font request");
    };
    assert_eq!(request.destination, RequestDestination::Font);
    assert_eq!(request.url.as_str(), "https://example.test/font.ttf");
    assert!(
        request
            .policy
            .check_request(RequestDestination::Font, "https://foreign.test/font.ttf", 1)
            .is_err()
    );
    assert!(outcome.messages.contains(&"\"ordinary-denied\"".into()));
}

#[test]
fn disallowed_worker_font_does_not_escape_through_allowed_connect_src() {
    let (mut runtime, initial) = worker("font-src 'none'; connect-src 'self'");
    assert_eq!(initial.fetch_actions.len(), 1);
    let ScriptFetchAction::Start { request, .. } = &initial.fetch_actions[0] else {
        panic!("expected ordinary request");
    };
    assert_eq!(request.destination, RequestDestination::Fetch);
    assert_eq!(request.url.as_str(), "https://example.test/ordinary");
    let mut messages = initial.messages;
    for _ in 0..8 {
        let turn = runtime.advance_time(Duration::ZERO, 8);
        assert!(turn.errors.is_empty(), "{:?}", turn.errors);
        assert!(turn.fetch_actions.is_empty());
        messages.extend(turn.messages);
    }
    assert!(
        messages.contains(&"\"font:NetworkError:error\"".into()),
        "{messages:?}"
    );
}

#[test]
fn canvas_font_fetch_uses_private_captured_loading_and_request_serialization() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        if('__fontFaceFetch' in window)throw Error('private font fetch leaked');
        const face=new FontFace('Captured','url(/captured.ttf)');document.fonts.add(face);
        const fail=()=>{throw Error('author replacement called');};
        globalThis.fetch=fail;Request.prototype.__serialize=fail;FontFace.prototype.load=fail;
        const ctx=document.createElement('canvas').getContext('2d');ctx.font='20px Captured';
        ctx.measureText('AAAA');
        document.querySelector('output').textContent=face.status;
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "loading"
    );
    assert_eq!(outcome.fetch_actions.len(), 1);
    let ScriptFetchAction::Start { request, .. } = &outcome.fetch_actions[0] else {
        panic!("expected captured font request");
    };
    assert_eq!(request.destination, RequestDestination::Font);
    assert_eq!(request.url.as_str(), "https://example.com/captured.ttf");
}

#[test]
fn author_fetch_cannot_request_a_font_destination_via_unknown_options() {
    let (_, outcome) = execute_html(
        "<script>fetch('/ordinary',{destination:'font',fontFetchStart:true});</script>",
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let ScriptFetchAction::Start { request, .. } = &outcome.fetch_actions[0] else {
        panic!("expected author request");
    };
    assert_eq!(request.destination, RequestDestination::Fetch);
}
