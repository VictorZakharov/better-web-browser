use super::*;

#[test]
fn embedding_policy_controls_child_self_navigation_and_redirects() {
    let (dom, mut runtime) = start("<body><script></script>");
    let mut headers = crate::fetch::HeaderList::new();
    headers
        .append("content-security-policy", "frame-src 'self'")
        .unwrap();
    runtime.host.borrow_mut().policy = std::sync::Arc::new(
        crate::fetch::csp::PolicyContainer::from_headers(
            "https://example.com/parent/index",
            &headers,
        )
        .unwrap(),
    );
    evaluate(
        &mut runtime,
        &dom,
        "const f=document.createElement('iframe'); f.src='/child'; document.body.append(f);",
    );
    let first = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    runtime.complete_fetch_with_loader(
        first,
        Ok(response("https://example.com/child", "<p>first</p>")),
        None,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "f.contentWindow.location.href='https://blocked.test/escape';",
    );
    let blocked = runtime.advance_time(Duration::ZERO, 1);
    assert!(blocked.fetch_actions.is_empty());
    assert!(
        blocked
            .diagnostics
            .iter()
            .any(|message| message.contains("frame-src"))
    );
    evaluate(
        &mut runtime,
        &dom,
        "f.contentWindow.location.href='/redirect';",
    );
    let second = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    let mut redirected = response("https://blocked.test/final", "<p>forbidden</p>");
    redirected.url_list.insert(
        0,
        crate::fetch::FetchUrl::parse("https://example.com/redirect").unwrap(),
    );
    let result = runtime.complete_fetch_with_loader(second, Ok(redirected), None);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|message| message.contains("frame-src"))
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(f.contentDocument.body.textContent!=='first') throw Error('redirect replaced document');",
    );
}
fn loaded(policy: &str, source: &str) -> (dom::Dom, ScriptRuntime) {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.messages=[];addEventListener('message',e=>messages.push(e.data));
        const f=document.createElement('iframe');f.src='/policy';document.body.append(f);
    </script>"#,
    );
    let result = runtime.advance_time(Duration::ZERO, 1);
    let id = result
        .fetch_actions
        .into_iter()
        .find_map(|a| match a {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    let mut response = response("https://example.com/policy", source);
    response
        .headers
        .append("content-security-policy", policy)
        .unwrap();
    assert!(
        runtime
            .complete_fetch_with_loader(id, Ok(response), None)
            .errors
            .is_empty()
    );
    drain(&mut runtime);
    (dom, runtime)
}
#[test]
fn policy_blocks_inline_parser_code_and_allows_explicit_inline_without_eval() {
    let (dom, mut runtime) = loaded(
        "default-src 'none'",
        "<script>parent.postMessage('BAD','*')</script>",
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(messages.length) throw Error('inline ran');",
    );
    let (dom, mut runtime) = loaded(
        "script-src 'unsafe-inline'; connect-src 'none'",
        r#"<script>
        const check=fn=>{try {fn();return false}catch(e){return e.name==='EvalError'}};
        parent.postMessage([check(()=>eval('1')),check(()=>new Function('return 1'))],'*');
    </script>"#,
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(messages.length!==1 || messages[0].some(v=>!v)) throw Error(JSON.stringify(messages));",
    );
}

#[test]
fn child_nonce_policy_admits_only_matching_parser_script() {
    let csp = "script-src 'report-sample' 'nonce-VGVzdA==' 'unsafe-inline' 'strict-dynamic' https:; object-src 'none'; base-uri 'self'; report-uri https://reports.example.test/csp";
    let (dom, mut runtime) = loaded(
        csp,
        "<script>parent.postMessage('BAD','*')</script><script nonce='VGVzdA=='>parent.postMessage('GOOD','*')</script>",
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(messages.join(',')!=='GOOD') throw Error(JSON.stringify(messages));",
    );
}
#[test]
fn policy_checks_ancestor_and_base_url_and_inherits_into_srcdoc() {
    let (dom, mut runtime) = loaded(
        "frame-ancestors https://other.test; script-src 'unsafe-inline'",
        "<script>parent.postMessage('BAD','*')</script>",
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(messages.length) throw Error('ancestor policy bypass');",
    );
    let (dom, mut runtime) = loaded(
        "script-src 'unsafe-inline'; base-uri 'self'",
        r#"<base href="https://other.test/"><body><script>
        parent.postMessage(document.baseURI,'*');
        const inner=document.createElement('iframe');
        inner.srcdoc='<script>let denied=false;try{eval("1")}catch(e){denied=e.name==="EvalError"}top.postMessage(denied,"*")<\/script>';
        document.body.append(inner);
    </script>"#,
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(messages.join(',')!=='https://example.com/policy,true') throw Error(JSON.stringify(messages));",
    );
}
