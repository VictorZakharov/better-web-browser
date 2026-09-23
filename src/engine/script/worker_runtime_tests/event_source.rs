use super::*;

#[test]
fn dedicated_worker_event_source_decodes_split_utf8_streams() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"const source = new EventSource('/events');
           source.onopen = () => postMessage('open:' + source.readyState);
           source.onmessage = event => {
               postMessage([event.data, event.lastEventId, event.isTrusted].join('|'));
               source.close();
           };"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let ScriptFetchAction::Start { id, request } = &initial.fetch_actions[0] else {
        panic!("expected worker EventSource fetch");
    };
    assert_eq!(request.headers.get("accept"), Some("text/event-stream"));
    let id = *id;
    let mut runtime = runtime.expect("Worker failed to start");
    let mut headers = HeaderList::new();
    headers.append("content-type", "text/event-stream").unwrap();
    let response = FetchResponse {
        response_type: ResponseType::Basic,
        url_list: vec![FetchUrl::parse("https://example.com/events").unwrap()],
        status: 200,
        headers,
        body: Body::from_bytes(Vec::new()),
    };
    let head = runtime.deliver_fetch_event(id, ScriptFetchEvent::Head(Ok(response)));
    assert!(head.errors.is_empty(), "{:?}", head.errors);
    let first = runtime.deliver_fetch_event(
        id,
        ScriptFetchEvent::Chunk(b"\xef\xbb\xbfid: worker-1\ndata: caf\xc3".to_vec()),
    );
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    let second = runtime.deliver_fetch_event(id, ScriptFetchEvent::Chunk(b"\xa9\n\n".to_vec()));
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    let events = runtime.advance_time(Duration::ZERO, 8);
    assert!(events.errors.is_empty(), "{:?}", events.errors);
    assert_eq!(
        events.messages,
        ["\"open:1\"", "\"caf\u{e9}|worker-1|true\""]
    );
}

#[test]
fn worker_text_decoder_streaming_respects_bom_and_fatal_mode() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (_, result) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"const decoder = new TextDecoder();
           const first = decoder.decode(new Uint8Array([0xef, 0xbb]), {stream:true});
           const second = decoder.decode(new Uint8Array([0xbf, 0xc3]), {stream:true});
           const third = decoder.decode(new Uint8Array([0xa9]));
           let fatal = false;
           try { new TextDecoder('utf-8', {fatal:true}).decode(new Uint8Array([0xff])); }
           catch (error) { fatal = error.name === 'TypeError'; }
           postMessage([first, second, third, fatal].join('|'));"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.messages, ["\"||\u{e9}|true\""]);
}
