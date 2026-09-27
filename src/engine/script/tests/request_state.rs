use super::*;

#[test]
fn copying_request_retains_body_headers_and_abort_signal_but_honors_init_overrides() {
    let (dom, outcome) = execute_html(
        r#"<div></div><script>
        (async () => {
            const controller = new AbortController();
            const original = new Request('/payload', {method:'POST', body:'payload',
                headers:{'x-test':'original'}, signal:controller.signal});
            for (const name of ['url','method','headers','body','signal'])
                Object.defineProperty(original,name,{get(){throw new Error('public '+name);}});
            const cloned = original.clone();
            const copied = new Request(original);
            const changed = new Request(cloned,{method:'PATCH',body:'replacement',headers:{'x-test':'changed'}});
            controller.abort('canceled');
            document.querySelector('div').textContent = [copied.method, copied.headers.get('x-test'),
                await copied.text(), copied.signal.aborted, changed.method, changed.headers.get('x-test'),
                await changed.text()].join('|');
        })();
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "POST|original|payload|true|PATCH|changed|replacement"
    );
}

#[test]
fn request_copy_clone_and_fetch_ignore_shadowed_public_attributes() {
    let (dom, outcome) = execute_html(
        r#"<div></div><script>
        const request = new Request('data:text/plain,original');
        for (const name of ['url', 'method', 'headers', 'mode', 'credentials', 'cache',
            'redirect', 'referrer', 'referrerPolicy', 'integrity', 'keepalive', 'signal', 'body'])
            Object.defineProperty(request, name, {get() { throw new Error('read public ' + name); }});
        const copied = new Request(request);
        const cloned = request.clone();
        document.querySelector('div').textContent = [copied.url, copied.method,
            cloned.url, cloned.method, copied.body === null, cloned.body === null].join('|');
        fetch(request);
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "data:text/plain,original|GET|data:text/plain,original|GET|true|true"
    );
    let request = outcome
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { request, .. } => Some(request),
            _ => None,
        })
        .expect("fetch must retain the internal request");
    assert_eq!(request.url.as_str(), "data:text/plain,original");
    assert_eq!(request.method, "GET");
}

#[test]
fn keepalive_is_preserved_through_request_copy_and_crosses_the_fetch_host_boundary() {
    let (dom, outcome) = execute_html(
        r#"<div></div><script>
        const original = new Request('/collect', {method:'POST', body:'proof', keepalive:true});
        const clone = original.clone();
        const copy = new Request(original);
        let oversized = false;
        try { new Request('/large', {method:'POST', keepalive:true,
            body:new Uint8Array(65537)}); } catch (error) { oversized = error instanceof TypeError; }
        document.querySelector('div').textContent = [original.keepalive, copy.keepalive,
            clone.keepalive, oversized].join('|');
        fetch(copy);
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true|true|true|true"
    );
    let request = outcome
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { request, .. } => Some(request),
            _ => None,
        })
        .expect("keepalive fetch action");
    assert!(request.keepalive);
    assert_eq!(request.method, "POST");
    assert_eq!(request.body.unwrap().as_bytes(), b"proof");
}
