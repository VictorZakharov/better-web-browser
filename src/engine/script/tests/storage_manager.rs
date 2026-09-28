use super::*;
use crate::renderer_protocol::{DatabaseEvent, DocumentId};
use serde_json::{Value, json};

fn run_at(url: &str, source: &str) -> (dom::Dom, ScriptRuntime, ScriptOutcome) {
    let html = format!("<body><script>{source}</script></body>");
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

fn attr(dom: &dom::Dom, name: &str) -> Option<String> {
    dom.elements_named("body").next().unwrap().attr(name)
}

#[test]
fn navigator_storage_is_secure_same_object_and_not_a_fake_persistence_api() {
    for (url, exposed) in [
        ("https://example.test/", true),
        ("http://127.0.0.1/", true),
        ("http://example.test/", false),
    ] {
        let (dom, _, outcome) = run_at(
            url,
            r#"document.body.setAttribute('data-exposed', String('storage' in navigator));
               if ('storage' in navigator) {
                   if (navigator.storage !== navigator.storage) throw Error('SameObject');
                   if (!(navigator.storage instanceof StorageManager)) throw Error('interface');
                   if ('persist' in navigator.storage) throw Error('unsupported persistence');
                   if (Object.prototype.toString.call(navigator.storage) !== '[object StorageManager]')
                       throw Error('interface tag');
                   try { new StorageManager(); throw Error('constructible'); }
                   catch (error) { if (!(error instanceof TypeError)) throw error; }
                   try { StorageManager.prototype.estimate.call({}); throw Error('receiver'); }
                   catch (error) { if (!(error instanceof TypeError)) throw error; }
                   navigator.storage.persisted().then(value =>
                       document.body.setAttribute('data-persisted', String(value)));
               }"#,
        );
        assert!(outcome.errors.is_empty(), "{url}: {:?}", outcome.errors);
        assert_eq!(
            attr(&dom, "data-exposed").as_deref(),
            Some(exposed.to_string().as_str())
        );
        if exposed {
            assert_eq!(attr(&dom, "data-persisted").as_deref(), Some("false"));
        }
    }
}

#[test]
fn estimate_uses_async_browser_request_and_rejects_malformed_reply() {
    let (dom, mut runtime, outcome) = run_at(
        "https://example.test/",
        r#"navigator.storage.estimate().then(value =>
               document.body.setAttribute('data-estimate', value.usage + '/' + value.quota));"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.database_actions.len(), 1);
    let action = &outcome.database_actions[0];
    let payload: Value = serde_json::from_str(&action.payload).unwrap();
    assert_eq!(payload, json!({"kind":"storageEstimate"}));
    assert!(
        payload.get("origin").is_none(),
        "renderer cannot select an origin"
    );
    assert_eq!(attr(&dom, "data-estimate"), None);
    let complete = runtime.deliver_database_event(DatabaseEvent {
        document: DocumentId::new(1).unwrap(),
        request_id: u64::from(action.id),
        payload: json!({"kind":"storageEstimate","value":{"usage":123,"quota":456}}).to_string(),
    });
    assert!(complete.errors.is_empty(), "{:?}", complete.errors);
    assert_eq!(attr(&dom, "data-estimate").as_deref(), Some("123/456"));

    let (dom, mut runtime, outcome) = run_at(
        "https://example.test/",
        r#"navigator.storage.estimate().catch(error =>
               document.body.setAttribute('data-error', error.name));"#,
    );
    let action = &outcome.database_actions[0];
    let complete = runtime.deliver_database_event(DatabaseEvent {
        document: DocumentId::new(1).unwrap(),
        request_id: u64::from(action.id),
        payload: json!({"kind":"storageEstimate","value":{"usage":-1,"quota":456}}).to_string(),
    });
    assert!(complete.errors.is_empty());
    assert_eq!(attr(&dom, "data-error").as_deref(), Some("UnknownError"));
}
