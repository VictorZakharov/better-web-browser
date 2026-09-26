use super::*;

#[test]
fn send_beacon_queues_one_way_post_with_body_snapshot() {
    let (dom, outcome) = execute_html(
        r#"<output></output><script>
            const sent = navigator.sendBeacon('/collect', 'hello');
            document.querySelector('output').textContent =
                typeof sent + ':' + sent + ':' + navigator.sendBeacon.length;
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "boolean:true:1"
    );
    let [ScriptFetchAction::Beacon { request }] = outcome.fetch_actions.as_slice() else {
        panic!(
            "expected one one-way Beacon action: {:?}",
            outcome.fetch_actions
        );
    };
    assert_eq!(request.url.as_str(), "https://example.com/collect");
    assert_eq!(request.method, "POST");
    assert_eq!(request.body.as_ref().unwrap().as_bytes(), b"hello");
    assert_eq!(
        request.headers.get("content-type"),
        Some("text/plain;charset=UTF-8")
    );
    assert_eq!(request.mode, crate::fetch::RequestMode::NoCors);
    assert_eq!(request.credentials, crate::fetch::CredentialsMode::Include);
}

#[test]
fn send_beacon_uses_cors_for_non_safelisted_blob_type() {
    let (dom, outcome) = execute_html(
        r#"<output></output><script>
            const body = new Blob(['{"ready":true}'], { type: 'application/json' });
            document.querySelector('output').textContent =
                String(navigator.sendBeacon('/collect', body));
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
    let [ScriptFetchAction::Beacon { request }] = outcome.fetch_actions.as_slice() else {
        panic!("expected a Beacon action");
    };
    assert_eq!(
        request.headers.get("content-type"),
        Some("application/json")
    );
    assert_eq!(request.mode, crate::fetch::RequestMode::Cors);
}

#[test]
fn send_beacon_serializes_common_bodyinit_types() {
    let (_, outcome) = execute_html(
        r#"<script>
            const params = new URLSearchParams([['q', 'a b']]);
            const form = new FormData(); form.append('field', 'value');
            navigator.sendBeacon('/params', params);
            navigator.sendBeacon('/form', form);
            navigator.sendBeacon('/bytes', new Uint8Array([1, 2, 3]));
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let [
        ScriptFetchAction::Beacon { request: params },
        ScriptFetchAction::Beacon { request: form },
        ScriptFetchAction::Beacon { request: bytes },
    ] = outcome.fetch_actions.as_slice()
    else {
        panic!("expected three Beacon actions");
    };
    assert_eq!(params.body.as_ref().unwrap().as_bytes(), b"q=a+b");
    assert_eq!(
        params.headers.get("content-type"),
        Some("application/x-www-form-urlencoded;charset=UTF-8")
    );
    assert!(
        form.headers
            .get("content-type")
            .unwrap()
            .starts_with("multipart/form-data;")
    );
    assert!(
        form.body
            .as_ref()
            .unwrap()
            .as_bytes()
            .windows(5)
            .any(|part| part == b"value")
    );
    assert_eq!(bytes.body.as_ref().unwrap().as_bytes(), &[1, 2, 3]);
    assert_eq!(bytes.headers.get("content-type"), None);
}

#[test]
fn send_beacon_rejects_invalid_urls_and_bounded_queue_overflow() {
    let (dom, outcome) = execute_html(
        r#"<output></output><script>
            const names = [];
            for (const url of ['data:text/plain,x', 'https://user:pass@example.com/', 'http://:bad']) {
                try { navigator.sendBeacon(url, 'x'); names.push('accepted'); }
                catch (error) { names.push(error.name); }
            }
            const first = navigator.sendBeacon('/first', 'a'.repeat(40000));
            const second = navigator.sendBeacon('/second', 'b'.repeat(30000));
            const oversized = navigator.sendBeacon('/oversized', 'c'.repeat(65537));
            document.querySelector('output').textContent =
                names.join(',') + '|' + [first, second, oversized].join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "TypeError,TypeError,TypeError|true,false,false"
    );
    assert_eq!(outcome.fetch_actions.len(), 1);
}

#[test]
fn send_beacon_budget_reopens_after_previous_script_batch_is_drained() {
    let dom = crate::engine::dom::parse_with_scripting(
        r#"<script>window.firstBeacon = navigator.sendBeacon('/first', 'a'.repeat(40000));</script>
           <script>window.secondBeacon = navigator.sendBeacon('/second', 'b'.repeat(40000));</script>"#,
        true,
    );
    let nodes = dom.elements_named("script").collect::<Vec<_>>();
    let input = |index: usize| ScriptInput {
        source_url: format!("https://example.com/#{index}"),
        code: nodes[index].text_content(),
        node: nodes[index].clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let first = runtime.execute_initial(&[input(0)]);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(first.fetch_actions.len(), 1);
    let second = runtime.execute_additional_with_loader(&[input(1)], None);
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    assert_eq!(second.fetch_actions.len(), 1);
}
