use super::*;
use crate::engine::script::{ScriptFetchAction, ScriptFetchEvent};

#[test]
fn srcdoc_navigation_keeps_window_proxy_and_replaces_the_document() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        const frame = document.createElement('iframe'); document.body.append(frame);
        const proxy = frame.contentWindow, oldDocument = frame.contentDocument;
        proxy.oldValue = 42;
        frame.src = '/must-not-be-requested';
        frame.srcdoc = '<p id=child>frame</p><script>document.body.dataset.ran = "yes"; parent.postMessage("ready", "/")<\/script>';
        addEventListener('message', e => {
            if (e.source !== proxy || proxy !== frame.contentWindow) throw Error('WindowProxy identity');
            if (proxy.document === oldDocument || proxy.oldValue !== undefined) throw Error('stale realm');
            if (proxy.document.URL !== 'about:srcdoc' || proxy.document.baseURI !== location.href) throw Error('URL/base');
            document.body.dataset.child = proxy.document.body.dataset.ran;
        });
    </script>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/page");
    let setup = runtime.execute_initial_before_document_completion(&script_inputs(&dom), None);
    assert!(setup.errors.is_empty(), "{:?}", setup.errors);
    for _ in 0..30 {
        let result = runtime.advance_time(Duration::ZERO, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(result.fetch_actions.is_empty(), "srcdoc must override src");
        assert!(
            !result.console.iter().any(|line| line.starts_with("error:")),
            "{:?}",
            result.console
        );
    }
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-child")
            .as_deref(),
        Some("yes")
    );
}

#[test]
fn url_frame_relay_runs_loaded_scripts_and_can_navigate_parent() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        const frame = document.createElement('iframe'); frame.src='/relay'; document.body.append(frame);
        const proxy = frame.contentWindow;
        addEventListener('message', e => {
            if (e.origin !== location.origin || e.source !== proxy || e.data !== 'ready') throw Error('relay source');
            proxy.postMessage('continue', location.origin);
        });
    </script>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(
        runtime
            .execute_initial_before_document_completion(&script_inputs(&dom), None)
            .errors
            .is_empty()
    );
    let start = runtime.advance_time(Duration::ZERO, 1);
    assert!(start.errors.is_empty(), "{:?}", start.errors);
    let (id, request) = start
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, request } => Some((id, request)),
            _ => None,
        })
        .expect("frame navigation request");
    assert_eq!(request.url.as_str(), "https://example.com/relay");
    let response = crate::fetch::FetchResponse {
        response_type: crate::fetch::ResponseType::Basic, url_list: vec![request.url], status: 200,
        headers: crate::fetch::HeaderList::new(), body: crate::fetch::Body::from_bytes(br#"<!doctype html><script>
            addEventListener('message', e => {
                if (e.origin === location.origin && e.source === parent && e.data === 'continue') parent.location.href='/destination';
            });
            parent.postMessage('ready', location.origin);
        </script>"#.to_vec()),
    };
    let result = runtime.complete_fetch_with_loader(id, Ok(response), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let mut navigation = None;
    for _ in 0..30 {
        let result = runtime.advance_time(Duration::ZERO, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(
            !result.console.iter().any(|line| line.starts_with("error:")),
            "{:?}",
            result.console
        );
        navigation = result.navigation_url.or(navigation);
    }
    assert_eq!(
        navigation.as_deref(),
        Some("https://example.com/destination")
    );
}

#[test]
fn posted_messages_use_the_incumbent_frame_and_clone_into_the_receiver() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        const frame = document.createElement('iframe'); document.body.append(frame);
        window.parentOnly = 7;
        let count = 0;
        addEventListener('message', e => {
            if (e.origin !== 'https://example.com' || e.source !== frame.contentWindow) throw new Error('sender');
            if (!(e.data.values instanceof Array) || !(e.data.bytes instanceof ArrayBuffer)) throw new Error('receiver realm');
            if (e.data.values[0] !== 1 || new Uint8Array(e.data.bytes)[0] !== 42 || !e.isTrusted) throw new Error('clone');
            document.body.dataset.messages = String(++count);
        });
        const send = frame.contentWindow.eval(`(() => {
            const bytes = new Uint8Array([42]).buffer;
            parent.postMessage({values: [1], bytes}, '/', [bytes]);
            if (bytes.byteLength !== 0) throw new Error('transfer');
        })`);
        send(); // Entrypoint is the parent, but the incumbent script belongs to the child.
        if (count) throw new Error('delivery must be asynchronous');
    </script>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let setup = runtime.execute_initial_before_document_completion(&script_inputs(&dom), None);
    assert!(setup.errors.is_empty(), "{:?}", setup.errors);
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    let result = runtime.advance_time(Duration::ZERO, 1);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        !result.console.iter().any(|line| line.starts_with("error:")),
        "{:?}",
        result.console
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-messages")
            .as_deref(),
        Some("1")
    );
}

#[test]
fn child_timers_run_in_their_realm_and_stop_when_removed() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        const frame = document.createElement('iframe'); document.body.append(frame);
        frame.contentWindow.eval(`window.childOnly = 1; setTimeout(() => {
            parent.document.body.dataset.child = String(window.childOnly);
        }, 20);`);
    </script>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let setup = runtime.execute_initial_before_document_completion(&script_inputs(&dom), None);
    assert!(setup.errors.is_empty(), "{:?}", setup.errors);
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO)); // Initial iframe load task.
    assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
    assert_eq!(runtime.next_timer_delay(), Some(Duration::from_millis(20)));
    let body = dom.elements_named("body").next().unwrap();
    for _ in 0..4 {
        assert!(
            runtime
                .advance_time(Duration::from_millis(10), 1)
                .errors
                .is_empty()
        );
    }
    assert_eq!(body.attr("data-child").as_deref(), Some("1"));
    let script = dom.elements_named("script").next().unwrap();
    let queued = runtime.execute_additional_with_loader(&[input(&script, "cancel.js",
        "frame.contentWindow.setTimeout(() => document.body.dataset.late = 'bad', 5); frame.remove();", false)], None);
    assert!(queued.errors.is_empty(), "{:?}", queued.errors);
    for _ in 0..4 {
        assert!(
            runtime
                .advance_time(Duration::from_millis(10), 1)
                .errors
                .is_empty()
        );
    }
    assert_eq!(body.attr("data-late"), None);
}

#[test]
fn child_fetches_have_unique_ids_and_complete_in_the_owning_realm() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
        const frame = document.createElement('iframe'); document.body.append(frame);
        fetch('/root').then(r => r.text()).then(t => document.body.dataset.root = t);
        frame.contentWindow.eval(`fetch('/child').then(r => r.text()).then(t => {
            document.body.dataset.result = t;
            parent.document.body.dataset.child = t;
        });`);
    </script>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let setup = runtime.execute_initial_before_document_completion(&script_inputs(&dom), None);
    assert!(setup.errors.is_empty(), "{:?}", setup.errors);
    let starts: Vec<_> = setup
        .fetch_actions
        .into_iter()
        .filter_map(|action| match action {
            ScriptFetchAction::Start { id, request } => Some((id, request)),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 2);
    assert_ne!(starts[0].0, starts[1].0);
    for (id, request) in starts {
        assert_eq!(request.origin.unwrap().serialize(), "https://example.com");
        let response = crate::fetch::FetchResponse {
            response_type: crate::fetch::ResponseType::Basic,
            url_list: vec![request.url.clone()],
            status: 200,
            headers: crate::fetch::HeaderList::new(),
            body: crate::fetch::Body::empty(1024),
        };
        let head =
            runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Head(Ok(response)), None);
        assert!(head.errors.is_empty(), "{:?}", head.errors);
        let chunk = runtime.deliver_fetch_event_with_loader(
            id,
            ScriptFetchEvent::Chunk(b"ok".to_vec()),
            None,
        );
        assert!(chunk.errors.is_empty(), "{:?}", chunk.errors);
        let end = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
        assert!(end.errors.is_empty(), "{:?}", end.errors);
    }
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-root").as_deref(), Some("ok"));
    assert_eq!(body.attr("data-child").as_deref(), Some("ok"));
}
