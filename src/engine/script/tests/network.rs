use super::*;
use crate::fetch::{
    Body, CredentialsMode, FetchResponse, FetchUrl, HeaderList, RedirectMode, Referrer,
    ReferrerPolicy, RequestCache, RequestMode, ResponseType,
};

#[test]
fn url_constructor_resolves_relative_inputs_and_rejects_invalid_urls() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            const resolved = new URL('/path?q=1').href;
            const request = new Request('/request');
            document.querySelector('div').textContent = [
                resolved, URL.canParse('/other'), URL.canParse('http://:invalid'), request.url
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "https://example.com/path?q=1|true|false|https://example.com/request"
    );
}

#[test]
fn url_components_and_search_params_remain_live_when_mutated() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            const url = new URL('https://user:pass@example.com:8443/path?old=1#before');
            const params = url.searchParams;
            url.search = '?b=2&a=1';
            params.sort();
            params.append('space', 'a b');
            url.pathname = '/next';
            url.hash = 'after';
            document.querySelector('div').textContent = [
                url.href,
                params === url.searchParams,
                params.get('old'),
                url.protocol,
                url.username,
                url.password,
                url.host,
                url.hostname,
                url.port,
                url.pathname,
                url.search,
                url.hash,
                url.origin
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "https://user:pass@example.com:8443/next?a=1&b=2&space=a+b#after|true||https:|user|pass|example.com:8443|example.com|8443|/next|?a=1&b=2&space=a+b|#after|https://example.com:8443"
    );
}

#[test]
fn fetch_translates_web_request_options_into_the_shared_policy_model() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            fetch('/api/items', {
                method: 'post',
                headers: [['Content-Type', 'application/json'], ['X-Client', 'Breeze']],
                body: '{"ready":true}',
                mode: 'same-origin',
                credentials: 'include',
                cache: 'no-cache',
                redirect: 'error',
                referrer: '',
                referrerPolicy: 'no-referrer'
            }).catch(error => document.querySelector('div').textContent = error.name + ':' + error.message);
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.fetch_actions.len(),
        1,
        "fetch rejection: {}",
        dom.elements_named("div").next().unwrap().text_content()
    );
    let ScriptFetchAction::Start { id, request } = &outcome.fetch_actions[0] else {
        panic!("expected a Fetch start action")
    };
    assert_eq!(*id, 1);
    assert_eq!(request.url.as_str(), "https://example.com/api/items");
    assert_eq!(request.method, "POST");
    assert_eq!(request.mode, RequestMode::SameOrigin);
    assert_eq!(request.credentials, CredentialsMode::Include);
    assert_eq!(request.cache, RequestCache::NoCache);
    assert_eq!(request.redirect, RedirectMode::Error);
    assert_eq!(request.referrer, Referrer::NoReferrer);
    assert_eq!(request.referrer_policy, ReferrerPolicy::NoReferrer);
    assert_eq!(
        request.headers.get("content-type"),
        Some("application/json")
    );
    assert_eq!(request.headers.get("x-client"), Some("Breeze"));
    assert_eq!(
        request.body.as_ref().map(Body::as_bytes),
        Some(&b"{\"ready\":true}"[..])
    );
    assert_eq!(dom.elements_named("div").next().unwrap().text_content(), "");
}

#[test]
fn abort_signal_rejects_before_a_request_is_dispatched() {
    let (dom, outcome) = execute_html(
        r#"<body><div>pending</div><script>
            const controller = new AbortController();
            let trusted = false;
            controller.signal.addEventListener('abort', event => trusted = event.isTrusted);
            fetch('/slow', { signal: controller.signal }).catch(error => {
                document.querySelector('div').textContent = error.name + '|' + trusted;
            });
            controller.abort();
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.fetch_actions.is_empty());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "AbortError|true"
    );
}

#[test]
fn retained_fetch_completion_resolves_response_body_and_headers() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"fetch('/data').then(async response => {
            const clone = response.clone();
            document.querySelector('div').textContent = [
                response.status, response.ok, response.redirected, response.type,
                response.headers.get('x-proof'), await clone.text(), response.bodyUsed
            ].join('|');
        });"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"hello")), None);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.render_requested);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "200|true|false|basic|passed|hello|false"
    );
}

#[test]
fn fetch_resolves_at_headers_and_streams_body_chunks() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"fetch('/data').then(async response => {
            document.querySelector('div').textContent = 'head:' + response.status;
            const reader = response.body.getReader();
            const first = await reader.read();
            document.querySelector('div').textContent += '|one:' + new TextDecoder().decode(first.value);
            const second = await reader.read();
            document.querySelector('div').textContent += '|two:' + new TextDecoder().decode(second.value);
            const end = await reader.read();
            document.querySelector('div').textContent += '|done:' + end.done;
        });"#,
    );

    let head = runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(test_response(b""))),
        None,
    );
    assert!(head.errors.is_empty(), "{:?}", head.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "head:200"
    );

    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"ab".to_vec()), None);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "head:200|one:ab"
    );
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"cd".to_vec()), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "head:200|one:ab|two:cd|done:true"
    );
}

#[test]
fn cancelled_document_discards_a_stale_fetch_completion() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"fetch('/data').then(() => {
            document.querySelector('div').textContent = 'stale completion';
        });"#,
    );
    runtime.cancel_document();

    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"ignored")), None);

    assert!(outcome.runtime_stopped);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "pending"
    );
}

#[test]
fn xhr_uses_fetch_and_dispatches_the_success_state_sequence() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const events = [];
        const xhr = new XMLHttpRequest();
        for (const name of ['readystatechange', 'loadstart', 'progress', 'load', 'loadend'])
            xhr.addEventListener(name, () => events.push(name + ':' + xhr.readyState));
        xhr.open('GET', '/xhr');
        xhr.responseType = 'json';
        xhr.onloadend = () => document.querySelector('div').textContent =
            xhr.status + '|' + xhr.response.answer + '|' + events.join(',');
        xhr.send();"#,
    );
    let outcome =
        runtime.complete_fetch_with_loader(id, Ok(test_response(br#"{"answer":42}"#)), None);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "200|42|readystatechange:1,loadstart:1,readystatechange:2,readystatechange:3,progress:3,readystatechange:4,load:4,loadend:4"
    );
}

#[test]
fn xhr_reports_monotonic_progress_for_incremental_chunks() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const samples = [];
        const xhr = new XMLHttpRequest();
        xhr.open('GET', '/xhr');
        xhr.onprogress = event => samples.push(
            event.loaded + '/' + event.total + '/' + event.lengthComputable + '/' + xhr.responseText
        );
        xhr.onloadend = () => document.querySelector('div').textContent =
            xhr.responseText + '|' + samples.join(',');
        xhr.send();"#,
    );
    let mut response = test_response(b"");
    response.headers.append("content-length", "6").unwrap();
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Head(Ok(response)), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"ab".to_vec()), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"cdef".to_vec()), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);

    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "abcdef|2/6/true/ab,6/6/true/abcdef"
    );
}

#[test]
fn xhr_reports_unknown_content_length_without_inventing_a_total() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const samples = [];
        const xhr = new XMLHttpRequest();
        xhr.open('GET', '/xhr');
        xhr.onprogress = event => samples.push(event.loaded + '/' + event.total + '/' + event.lengthComputable);
        xhr.onloadend = () => document.querySelector('div').textContent =
            xhr.responseText + '|' + samples.join(',');
        xhr.send();"#,
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(test_response(b""))),
        None,
    );
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"ab".to_vec()), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"cdef".to_vec()), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);

    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "abcdef|2/0/false,6/0/false"
    );
}

#[test]
fn xhr_enforces_state_guards_and_abort_resets_to_unsent() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            const xhr = new XMLHttpRequest();
            const events = [];
            for (const name of ['readystatechange', 'loadstart', 'abort', 'loadend'])
                xhr.addEventListener(name, () => events.push(name + ':' + xhr.readyState));
            let invalidType = '', invalidText = '', credentials = '';
            try { xhr.responseType = 'invalid'; } catch (error) { invalidType = error.name; }
            xhr.open('POST', '/xhr');
            xhr.responseType = 'arraybuffer';
            try { void xhr.responseText; } catch (error) { invalidText = error.name; }
            xhr.withCredentials = true;
            xhr.send('payload');
            try { xhr.withCredentials = false; } catch (error) { credentials = error.name; }
            xhr.abort();
            document.querySelector('div').textContent = [
                invalidType, invalidText, credentials, xhr.readyState, xhr.status,
                xhr.response === null, events.join(',')
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.fetch_actions.is_empty());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "TypeError|InvalidStateError|InvalidStateError|0|0|true|readystatechange:1,loadstart:1,readystatechange:4,abort:4,loadend:4"
    );
}

#[test]
fn xhr_reports_upload_and_network_error_events() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const events = [];
        const xhr = new XMLHttpRequest();
        for (const name of ['loadstart', 'error', 'loadend'])
            xhr.upload.addEventListener(name, () => events.push('upload-' + name));
        for (const name of ['readystatechange', 'loadstart', 'error', 'loadend'])
            xhr.addEventListener(name, () => events.push(name + ':' + xhr.readyState));
        xhr.open('POST', '/xhr');
        xhr.onloadend = () => document.querySelector('div').textContent =
            xhr.status + '|' + (xhr.response === null) + '|' + events.join(',');
        xhr.send('payload');"#,
    );
    let outcome = runtime.complete_fetch_with_loader(
        id,
        Err(crate::fetch::FetchError::new(
            crate::fetch::FetchErrorKind::Network,
            "offline",
        )),
        None,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "0|false|readystatechange:1,loadstart:1,upload-loadstart,upload-error,upload-loadend,readystatechange:4,error:4,loadend:4"
    );
}

#[test]
fn xhr_arraybuffer_response_preserves_binary_bytes() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const xhr = new XMLHttpRequest();
        xhr.open('GET', '/binary');
        xhr.responseType = 'arraybuffer';
        xhr.onload = () => {
            const bytes = new Uint8Array(xhr.response);
            let textError = '';
            try { void xhr.responseText; } catch (error) { textError = error.name; }
            document.querySelector('div').textContent =
                bytes.length + '|' + bytes[0] + '|' + bytes[1] + '|' + textError;
        };
        xhr.send();"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(&[0, 255])), None);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "2|0|255|InvalidStateError"
    );
}

#[test]
fn xhr_blob_response_keeps_owned_chunks_until_bytes_are_read() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const xhr = new XMLHttpRequest();
        xhr.open('GET', '/binary');
        xhr.responseType = 'blob';
        xhr.onload = async () => {
            const chunksBeforeRead = xhr.response.__chunks.length;
            const bytes = new Uint8Array(await xhr.response.arrayBuffer());
            document.querySelector('div').textContent = [
                xhr.response.size, chunksBeforeRead, xhr.response.__chunks.length,
                [...bytes].join(',')
            ].join('|');
        };
        xhr.send();"#,
    );
    let mut response = test_response(b"");
    response.headers.append("content-length", "4").unwrap();
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Head(Ok(response)), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"ab".to_vec()), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"cd".to_vec()), None);
    let outcome = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "4|2|1|97,98,99,100"
    );
}

pub(super) fn pending_runtime(code: &str) -> (crate::engine::dom::Dom, ScriptRuntime, u32) {
    let html = format!("<body><div>pending</div><script>{code}</script></body>");
    let dom = crate::engine::dom::parse_with_scripting(&html, true);
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&scripts);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let Some(ScriptFetchAction::Start { id, .. }) = outcome.fetch_actions.first() else {
        panic!("expected a pending Fetch request")
    };
    (dom, runtime, *id)
}

pub(super) fn test_response(body: &[u8]) -> FetchResponse {
    let mut headers = HeaderList::new();
    headers.append("content-type", "application/json").unwrap();
    headers.append("x-proof", "passed").unwrap();
    FetchResponse {
        response_type: ResponseType::Basic,
        url_list: vec![FetchUrl::parse("https://example.com/data").unwrap()],
        status: 200,
        headers,
        body: Body::from_bytes(body.to_vec()),
    }
}
