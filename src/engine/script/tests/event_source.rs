use super::*;
use crate::fetch::{Body, FetchResponse, FetchUrl, HeaderList, ResponseType};

fn event_response(content_type: &str, status: u16, url: &str) -> FetchResponse {
    let mut headers = HeaderList::new();
    headers.append("content-type", content_type).unwrap();
    FetchResponse {
        response_type: ResponseType::Basic,
        url_list: vec![FetchUrl::parse(url).unwrap()],
        status,
        headers,
        body: Body::from_bytes(Vec::new()),
    }
}

fn result(dom: &crate::engine::dom::Dom) -> String {
    dom.elements_named("div").next().unwrap().text_content()
}

#[test]
fn event_source_constructor_exposes_a_real_event_target_and_fetches_with_stream_headers() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>const source = new EventSource('/updates', { withCredentials: true });
        document.querySelector('div').textContent = [
            source instanceof EventTarget, source.url, source.withCredentials,
            source.readyState, source.CONNECTING, EventSource.OPEN, EventSource.CLOSED,
            Object.prototype.toString.call(source)
        ].join('|');</script></body>"#,
    );
    assert_eq!(
        result(&dom),
        "true|https://example.com/updates|true|0|0|1|2|[object EventSource]"
    );
    let ScriptFetchAction::Start { request, .. } = &outcome.fetch_actions[0] else {
        panic!("expected EventSource Fetch start");
    };
    assert_eq!(request.method, "GET");
    assert_eq!(request.headers.get("accept"), Some("text/event-stream"));
    assert_eq!(request.cache, crate::fetch::RequestCache::NoStore);
    assert_eq!(request.mode, crate::fetch::RequestMode::Cors);
    assert_eq!(request.credentials, crate::fetch::CredentialsMode::Include);
}

#[test]
fn event_source_parses_utf8_crlf_fields_and_redirect_origin_across_chunks() {
    let (dom, mut runtime, id) = network::pending_runtime(
        r#"const source = new EventSource('/updates');
        const seen = [];
        source.onopen = event => seen.push('open:' + event.isTrusted + ':' + source.readyState);
        source.addEventListener('notice', event => seen.push([
            event.type, event.data, event.lastEventId, event.origin, event.isTrusted
        ].join(':')));
        source.onmessage = event => {
            seen.push('message:' + event.data + ':' + event.lastEventId);
            source.close();
            document.querySelector('div').textContent = seen.join('|') + ':' + source.readyState;
        };"#,
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(event_response(
            "text/event-stream; charset=utf-8",
            200,
            "https://example.com/final",
        ))),
        None,
    );
    let first = runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Chunk(
            b"\xef\xbb\xbf: heartbeat\r\nid: 7\r\nevent: notice\r\ndata: caf\xc3".to_vec(),
        ),
        None,
    );
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(result(&dom), "pending");
    let second = runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Chunk(b"\xa9\r\ndata: next\r\n\r\ndata: last\n\n".to_vec()),
        None,
    );
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    let timers = runtime.advance_time(std::time::Duration::ZERO, 10);
    assert!(
        timers.errors.is_empty(),
        "{:?}; console={:?}",
        timers.errors,
        timers.console
    );
    assert_eq!(
        result(&dom),
        "open:true:1|notice:caf\u{e9}\nnext:7:https://example.com:true|message:last:7:2"
    );
}

#[test]
fn event_source_rejects_invalid_urls_and_terminal_responses() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            let invalid = '';
            try { new EventSource('http://:invalid'); } catch (error) { invalid = error.name; }
            document.querySelector('div').textContent = invalid;
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "SyntaxError");

    let (dom, mut runtime, id) = network::pending_runtime(
        r#"const source = new EventSource('/updates');
        source.onerror = () => document.querySelector('div').textContent =
            'error:' + source.readyState;"#,
    );
    let outcome = runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(event_response(
            "text/html",
            200,
            "https://example.com/updates",
        ))),
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let timers = runtime.advance_time(std::time::Duration::ZERO, 10);
    assert!(
        timers.errors.is_empty(),
        "{:?}; console={:?}",
        timers.errors,
        timers.console
    );
    assert_eq!(result(&dom), "error:2");
}

#[test]
fn event_source_close_aborts_pending_fetch_without_error_event() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            const source = new EventSource('/updates');
            source.onerror = () => document.querySelector('div').textContent = 'unexpected';
            source.close();
            document.querySelector('div').textContent = String(source.readyState);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "2");
    assert!(outcome.fetch_actions.is_empty());
}

#[test]
fn event_source_reconnects_with_last_event_id_and_honors_retry() {
    let (dom, mut runtime, id) = network::pending_runtime(
        r#"const source = new EventSource('/updates');
        const seen = [];
        source.onopen = () => seen.push('open:' + source.readyState);
        source.onmessage = event => seen.push('message:' + event.data + ':' + event.lastEventId);
        source.onerror = () => {
            seen.push('error:' + source.readyState);
            document.querySelector('div').textContent = seen.join('|');
        };"#,
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(event_response(
            "text/event-stream",
            200,
            "https://example.com/updates",
        ))),
        None,
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Chunk(b"retry: 7\nid: seq-1\ndata: first\n\n".to_vec()),
        None,
    );
    let done = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    let events = runtime.advance_time(std::time::Duration::ZERO, 10);
    assert!(events.errors.is_empty(), "{:?}", events.errors);
    assert_eq!(result(&dom), "open:1|message:first:seq-1|error:0");
    let early = runtime.advance_time(std::time::Duration::from_millis(6), 10);
    assert!(early.fetch_actions.is_empty(), "{:?}", early.fetch_actions);
    let next = runtime.advance_time(std::time::Duration::from_millis(1), 10);
    let ScriptFetchAction::Start { request, .. } = &next.fetch_actions[0] else {
        panic!("expected reconnect request: {:?}", next.fetch_actions);
    };
    assert_eq!(request.headers.get("last-event-id"), Some("seq-1"));
}

#[test]
fn event_source_ignores_incomplete_events_at_eof() {
    let (dom, mut runtime, id) = network::pending_runtime(
        r#"const source = new EventSource('/updates');
        source.onmessage = event => document.querySelector('div').textContent = event.data;
        globalThis.source = source;"#,
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(event_response(
            "text/event-stream",
            200,
            "https://example.com/updates",
        ))),
        None,
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Chunk(b"data: complete\n\ndata: partial".to_vec()),
        None,
    );
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    // EOF must never turn the unterminated second block into a message.
    let events = runtime.advance_time(std::time::Duration::ZERO, 10);
    assert!(events.errors.is_empty(), "{:?}", events.errors);
    assert_eq!(result(&dom), "complete");
}
