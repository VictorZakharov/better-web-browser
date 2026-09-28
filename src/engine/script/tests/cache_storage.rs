use super::*;
use crate::cache_storage::{CacheCommand, CacheStorage};
use crate::renderer_protocol::{DatabaseEvent, DocumentId};
use serde_json::{Value, json};

fn run_at(url: &str, code: &str) -> (dom::Dom, ScriptRuntime, ScriptOutcome) {
    let html = format!("<body><script>{code}</script></body>");
    let dom = dom::parse_with_scripting(&html, true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), url);
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: format!("{url}#inline"),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    (dom, runtime, outcome)
}

fn body_attr(dom: &dom::Dom, name: &str) -> Option<String> {
    dom.elements_named("body").next().unwrap().attr(name)
}

fn complete_command(
    runtime: &mut ScriptRuntime,
    model: &CacheStorage,
    action: &crate::engine::script::network::ScriptDatabaseAction,
) -> ScriptOutcome {
    let payload: Value = serde_json::from_str(&action.payload).unwrap();
    assert_eq!(payload["kind"], "cache");
    let command: CacheCommand = serde_json::from_value(payload["command"].clone()).unwrap();
    let value = model.execute("https://example.test", command).unwrap();
    runtime.deliver_database_event(DatabaseEvent {
        document: DocumentId::new(1).unwrap(),
        request_id: u64::from(action.id),
        payload: json!({"kind":"cache","value":value}).to_string(),
    })
}

fn drive_commands(
    runtime: &mut ScriptRuntime,
    model: &CacheStorage,
    mut outcome: ScriptOutcome,
) -> ScriptOutcome {
    for _ in 0..12 {
        let actions = std::mem::take(&mut outcome.database_actions);
        if actions.is_empty() {
            return outcome;
        }
        assert_eq!(actions.len(), 1);
        outcome = complete_command(runtime, model, &actions[0]);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    panic!("Cache API command chain did not settle")
}

#[test]
fn window_cache_storage_is_secure_only_same_object_and_branded() {
    for (url, expected) in [
        ("https://example.test/", "true"),
        ("http://127.0.0.1/", "true"),
        ("http://example.test/", "false"),
    ] {
        let (dom, _, outcome) = run_at(
            url,
            r#"document.body.setAttribute('data-exposed', String('caches' in window));
               if ('caches' in window) {
                   if (caches !== window.caches) throw Error('SameObject');
                   if (!(caches instanceof CacheStorage)) throw Error('interface');
                   for (const call of [
                       () => CacheStorage.prototype.keys.call({}),
                       () => Cache.prototype.keys.call({}),
                       () => caches.open(Symbol('not a DOMString'))
                   ]) {
                       try { call(); throw Error('missing brand check'); }
                       catch (error) { if (!(error instanceof TypeError)) throw error; }
                   }
               }"#,
        );
        assert!(outcome.errors.is_empty(), "{url}: {:?}", outcome.errors);
        assert_eq!(body_attr(&dom, "data-exposed").as_deref(), Some(expected));
    }
}

#[test]
fn embedded_realm_fails_closed_until_ancestor_secure_contexts_are_tracked() {
    let (dom, _, outcome) = run_at(
        "https://example.test/",
        r#"const frame = document.createElement('iframe');
           document.body.append(frame);
           document.body.setAttribute('data-frame-cache', String('caches' in frame.contentWindow));"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        body_attr(&dom, "data-frame-cache").as_deref(),
        Some("false")
    );
}

#[test]
fn cache_put_and_match_roundtrip_through_browser_owned_protocol() {
    let (dom, mut runtime, initial) = run_at(
        "https://example.test/",
        r#"caches.open('assets').then(async cache => {
            const response = new Response('hello', {headers: {'Content-Type': 'text/plain'}});
            await cache.put('/item', response);
            if (await response.text() !== 'hello') throw Error('put consumed its Response');
            const match = await caches.match('/item');
            document.body.setAttribute('data-result', await match.text());
        });"#,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let model = CacheStorage::in_memory();
    drive_commands(&mut runtime, &model, initial);
    assert_eq!(body_attr(&dom, "data-result").as_deref(), Some("hello"));
}

#[test]
fn cache_storage_match_uses_creation_order_across_named_caches() {
    let (dom, mut runtime, initial) = run_at(
        "https://example.test/",
        r#"(async () => {
            const first = await caches.open('first');
            const second = await caches.open('second');
            await second.put('/shared', new Response('second'));
            await first.put('/shared', new Response('first'));
            const hit = await caches.match('/shared');
            document.body.setAttribute('data-result', await hit.text());
        })();"#,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    drive_commands(&mut runtime, &CacheStorage::in_memory(), initial);
    assert_eq!(body_attr(&dom, "data-result").as_deref(), Some("first"));
}

#[test]
fn named_cache_query_enumeration_and_deletion_are_functional() {
    let (dom, mut runtime, initial) = run_at(
        "https://example.test/",
        r#"(async () => {
            const cache = await caches.open('named');
            try { await cache.addAll(7); throw Error('non-iterable accepted'); }
            catch (error) { if (!(error instanceof TypeError)) throw error; }
            await cache.put('/one?q=1', new Response('one'));
            await cache.put('/two', new Response('two'));
            const keys = await cache.keys();
            const all = await cache.matchAll();
            const hit = await cache.match('/one?q=2', {ignoreSearch:true});
            const text = await hit.text();
            const removed = await cache.delete('/one?q=1');
            const missing = await cache.match('/one?q=1');
            const has = await caches.has('named');
            const names = await caches.keys();
            const dropped = await caches.delete('named');
            document.body.setAttribute('data-result', [
                keys.map(request => new URL(request.url).pathname).join(','),
                all.length, text, removed, missing === undefined,
                has, names.join(','), dropped
            ].join('|'));
        })();"#,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    drive_commands(&mut runtime, &CacheStorage::in_memory(), initial);
    assert_eq!(
        body_attr(&dom, "data-result").as_deref(),
        Some("/one,/two|2|one|true|true|true|named|true")
    );
}

#[test]
fn fresh_window_reads_persisted_cache_after_store_reopen() {
    let path = std::env::temp_dir().join(format!(
        "breeze-cache-script-test-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let model = CacheStorage::open(&path).unwrap();
        let (dom, mut runtime, initial) = run_at(
            "https://example.test/",
            r#"caches.open('assets').then(cache =>
                cache.put('/persist', new Response('durable')).then(() =>
                    document.body.setAttribute('data-written', 'yes')));"#,
        );
        assert!(initial.errors.is_empty(), "{:?}", initial.errors);
        drive_commands(&mut runtime, &model, initial);
        assert_eq!(body_attr(&dom, "data-written").as_deref(), Some("yes"));
    }
    {
        let model = CacheStorage::open(&path).unwrap();
        let (dom, mut runtime, initial) = run_at(
            "https://example.test/",
            r#"caches.open('assets').then(async cache => {
                const hit = await cache.match('/persist');
                document.body.setAttribute('data-reloaded', await hit.text());
            });"#,
        );
        assert!(initial.errors.is_empty(), "{:?}", initial.errors);
        drive_commands(&mut runtime, &model, initial);
        assert_eq!(body_attr(&dom, "data-reloaded").as_deref(), Some("durable"));
    }
    std::fs::remove_file(&path).unwrap();
    let backup = path.with_extension("cache-bak");
    if backup.exists() {
        std::fs::remove_file(backup).unwrap();
    }
}

#[test]
fn add_all_does_not_write_any_entry_when_second_fetch_fails() {
    use crate::engine::script::ScriptFetchAction;
    use crate::fetch::{FetchError, FetchErrorKind};
    let (dom, mut runtime, initial) = run_at(
        "https://example.test/",
        r#"caches.open('assets').then(async cache => {
            let failure = '';
            try { await cache.addAll(['/first', '/second']); }
            catch (error) { failure = error.name; }
            document.body.setAttribute('data-result', failure + ':' + (await cache.keys()).length);
        });"#,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let model = CacheStorage::in_memory();
    let open = complete_command(&mut runtime, &model, &initial.database_actions[0]);
    assert!(open.errors.is_empty(), "{:?}", open.errors);
    assert_eq!(open.fetch_actions.len(), 2);
    let ids = open
        .fetch_actions
        .iter()
        .map(|action| match action {
            ScriptFetchAction::Start { id, .. } => *id,
            _ => panic!("expected Fetch start"),
        })
        .collect::<Vec<_>>();
    let first = runtime.complete_fetch_with_loader(
        ids[0],
        Ok(super::network::test_response(b"first")),
        None,
    );
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(
        first.database_actions.is_empty(),
        "first success cannot commit addAll"
    );
    let failed = runtime.complete_fetch_with_loader(
        ids[1],
        Err(FetchError::new(FetchErrorKind::Network, "offline")),
        None,
    );
    assert!(failed.errors.is_empty(), "{:?}", failed.errors);
    let settled = drive_commands(&mut runtime, &model, failed);
    assert!(settled.database_actions.is_empty());
    assert_eq!(
        body_attr(&dom, "data-result").as_deref(),
        Some("TypeError:0")
    );
    assert_eq!(
        model
            .execute("https://example.test", CacheCommand::Names)
            .unwrap(),
        json!(["assets"])
    );
}

#[test]
fn add_all_fetches_through_policy_and_commits_one_batch() {
    use crate::engine::script::ScriptFetchAction;
    let (dom, mut runtime, initial) = run_at(
        "https://example.test/",
        r#"caches.open('assets').then(async cache => {
            window.Request = function () { throw Error('replaced Request'); };
            window.Response = function () { throw Error('replaced Response'); };
            window.fetch = () => Promise.reject(Error('replaced fetch'));
            await cache.addAll(['/first', '/second']);
            document.body.setAttribute('data-count', String((await cache.keys()).length));
        });"#,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let model = CacheStorage::in_memory();
    let open = complete_command(&mut runtime, &model, &initial.database_actions[0]);
    assert!(open.errors.is_empty(), "{:?}", open.errors);
    assert_eq!(open.fetch_actions.len(), 2);
    let ids = open
        .fetch_actions
        .iter()
        .map(|action| match action {
            ScriptFetchAction::Start { id, request } => {
                assert_eq!(request.method, "GET");
                assert_eq!(request.url.origin().serialize(), "https://example.test");
                *id
            }
            _ => panic!("expected policy-mediated Fetch start"),
        })
        .collect::<Vec<_>>();
    let first = runtime.complete_fetch_with_loader(
        ids[0],
        Ok(super::network::test_response(b"first")),
        None,
    );
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(first.database_actions.is_empty());
    let second = runtime.complete_fetch_with_loader(
        ids[1],
        Ok(super::network::test_response(b"second")),
        None,
    );
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    assert_eq!(second.database_actions.len(), 1, "one batch per addAll");
    let payload: Value = serde_json::from_str(&second.database_actions[0].payload).unwrap();
    assert_eq!(payload["command"]["op"], "put");
    assert_eq!(payload["command"]["entries"].as_array().unwrap().len(), 2);
    drive_commands(&mut runtime, &model, second);
    assert_eq!(body_attr(&dom, "data-count").as_deref(), Some("2"));
}
