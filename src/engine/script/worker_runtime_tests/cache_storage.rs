use super::*;
use crate::cache_storage::{CacheCommand, CacheStorage};
use crate::engine::dom;
use crate::renderer_protocol::{DatabaseEvent, DocumentId};
use serde_json::{Value, json};

const ORIGIN: &str = "https://example.test";

fn loader() -> Arc<WorkerSourceLoader> {
    Arc::new(|url, _| Err(format!("unexpected worker import: {url}")))
}

fn start_worker(url: &str, source: &str) -> (WorkerRuntime, WorkerRuntimeOutcome) {
    let (worker, outcome) = WorkerRuntime::start(url, source, "", ScriptKind::Classic, loader());
    assert!(outcome.errors.is_empty(), "{url}: {:?}", outcome.errors);
    (worker.expect("worker realm"), outcome)
}

fn start_window(source: &str) -> (dom::Dom, ScriptRuntime, ScriptOutcome) {
    let html = format!("<body><script>{source}</script></body>");
    let dom = dom::parse_with_scripting(&html, true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), &format!("{ORIGIN}/page"));
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: format!("{ORIGIN}/page#inline"),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime, outcome)
}

fn cache_reply(model: &CacheStorage, origin: &str, payload: &str) -> String {
    let request: Value = serde_json::from_str(payload).unwrap();
    assert_eq!(request["kind"], "cache");
    assert!(
        request.get("origin").is_none(),
        "the renderer must not select a storage origin"
    );
    let command: CacheCommand = serde_json::from_value(request["command"].clone()).unwrap();
    let value = model
        .execute(origin, command)
        .expect("browser-owned cache operation");
    json!({"kind":"cache","value":value}).to_string()
}

fn drive_window(
    runtime: &mut ScriptRuntime,
    model: &CacheStorage,
    mut outcome: ScriptOutcome,
) -> ScriptOutcome {
    for _ in 0..16 {
        let actions = std::mem::take(&mut outcome.database_actions);
        if actions.is_empty() {
            return outcome;
        }
        assert_eq!(actions.len(), 1, "one serial Cache command at a time");
        let action = &actions[0];
        let reply = cache_reply(model, ORIGIN, &action.payload);
        outcome = runtime.deliver_database_event(DatabaseEvent {
            document: DocumentId::new(1).unwrap(),
            request_id: u64::from(action.id),
            payload: reply,
        });
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    panic!("Window Cache command chain did not settle");
}

fn drive_worker(
    runtime: &mut WorkerRuntime,
    model: &CacheStorage,
    mut outcome: WorkerRuntimeOutcome,
) -> WorkerRuntimeOutcome {
    for _ in 0..16 {
        let actions = std::mem::take(&mut outcome.database_actions);
        if actions.is_empty() {
            return outcome;
        }
        assert_eq!(actions.len(), 1, "one serial Cache command at a time");
        let action = &actions[0];
        let reply = cache_reply(model, ORIGIN, &action.payload);
        outcome = runtime.deliver_database_event(action.id, reply);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    panic!("Worker Cache command chain did not settle");
}

fn body_attribute(dom: &dom::Dom, name: &str) -> Option<String> {
    dom.elements_named("body").next().unwrap().attr(name)
}

#[test]
fn worker_caches_is_secure_context_only_and_same_object() {
    for (url, exposed) in [
        ("https://example.test/worker.js", true),
        ("http://127.0.0.1/worker.js", true),
        ("http://localhost/worker.js", true),
        ("http://example.test/worker.js", false),
        ("data:text/javascript,postMessage(1)", false),
    ] {
        let (worker, outcome) = start_worker(
            url,
            r#"const exposed = 'caches' in self;
               if (exposed) {
                   if (caches !== self.caches || !(caches instanceof CacheStorage))
                       throw Error('CacheStorage must be SameObject');
                   if (Object.prototype.toString.call(caches) !== '[object CacheStorage]')
                       throw Error('CacheStorage toStringTag');
                   for (const call of [
                       () => CacheStorage.prototype.keys.call({}),
                       () => Cache.prototype.keys.call({}),
                       () => caches.open(Symbol('invalid name'))
                   ]) {
                       try { call(); throw Error('missing Cache brand or conversion check'); }
                       catch (error) { if (!(error instanceof TypeError)) throw error; }
                   }
               }
               postMessage([exposed, 'CacheStorage' in self, 'Cache' in self].join(':'));"#,
        );
        assert!(
            outcome.database_actions.is_empty(),
            "exposure must not touch storage"
        );
        assert_eq!(
            outcome.messages,
            [format!("\"{exposed}:{exposed}:{exposed}\"")],
            "{url}"
        );
        drop(worker);
    }
}

#[test]
fn insecure_worker_cannot_select_an_origin_through_the_storage_host_call() {
    // The renderer's private host call is still callable by author JavaScript.
    // Exposure is not an authority boundary: the browser-owned model must reject
    // the worker client's insecure origin even if a command is forged directly.
    let (_, outcome) = start_worker(
        "http://example.test/worker.js",
        r#"__hostCall('databaseRequest', JSON.stringify({
               kind:'cache', command:{op:'open', name:'forged'}
           }));"#,
    );
    assert_eq!(outcome.database_actions.len(), 1);
    let request: Value = serde_json::from_str(&outcome.database_actions[0].payload).unwrap();
    assert_eq!(request["kind"], "cache");
    assert!(request.get("origin").is_none());
    let command: CacheCommand = serde_json::from_value(request["command"].clone()).unwrap();
    let model = CacheStorage::in_memory();
    assert!(model.execute("http://example.test", command).is_err());
    assert_eq!(
        model.execute(ORIGIN, CacheCommand::Names).unwrap(),
        json!([])
    );
}

#[test]
fn worker_from_non_secure_owner_does_not_gain_caches_from_https_entry() {
    let (_, outcome) = WorkerRuntime::start_with_creator_context(
        &format!("{ORIGIN}/worker.js"),
        "postMessage(['caches' in self, 'CacheStorage' in self, typeof crypto.randomUUID].join(':'));",
        "",
        ScriptKind::Classic,
        loader(),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
        false,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"false:false:undefined\""]);
}

#[test]
fn dedicated_worker_and_window_share_the_browser_owned_origin_cache() {
    let model = CacheStorage::in_memory();
    let (window_dom, mut window, initial) = start_window(
        r#"caches.open('shared').then(cache =>
               cache.put('/from-window', new Response('written by window')))
               .then(() => document.body.setAttribute('data-written', 'yes'));"#,
    );
    drive_window(&mut window, &model, initial);
    assert_eq!(
        body_attribute(&window_dom, "data-written").as_deref(),
        Some("yes")
    );

    let (mut worker, initial) = start_worker(
        &format!("{ORIGIN}/worker.js"),
        r#"(async () => {
               const cache = await caches.open('shared');
               const hit = await cache.match('/from-window');
               const fromWindow = await hit.text();
               await cache.put('/from-worker', new Response('written by worker'));
               postMessage(fromWindow);
           })().catch(error => postMessage('error:' + error));"#,
    );
    let complete = drive_worker(&mut worker, &model, initial);
    assert_eq!(complete.messages, ["\"written by window\""]);

    let (window_dom, mut fresh_window, initial) = start_window(
        r#"caches.match('/from-worker').then(response => response.text())
               .then(text => document.body.setAttribute('data-worker-value', text));"#,
    );
    drive_window(&mut fresh_window, &model, initial);
    assert_eq!(
        body_attribute(&window_dom, "data-worker-value").as_deref(),
        Some("written by worker")
    );
    assert_eq!(
        model
            .execute("https://other.test", CacheCommand::Names)
            .unwrap(),
        json!([]),
        "another origin cannot see the worker/window cache"
    );
}

#[test]
fn cancelled_worker_drops_pending_cache_completion() {
    let (mut worker, initial) = start_worker(
        &format!("{ORIGIN}/worker.js"),
        r#"caches.open('pending').then(() => postMessage('unexpected'));"#,
    );
    assert_eq!(initial.database_actions.len(), 1);
    let id = initial.database_actions[0].id;
    worker.cancel();
    let ignored =
        worker.deliver_database_event(id, json!({"kind":"cache","value":null}).to_string());
    assert!(ignored.errors.is_empty());
    assert!(ignored.messages.is_empty());
    assert!(ignored.database_actions.is_empty());
}
