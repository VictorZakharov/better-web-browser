use super::*;
use crate::storage::{StorageAreaKind, StorageChange, StorageUpdate};

fn setup(code: &str) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(
        "<body><p id='result'>waiting</p><script></script></body>",
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/recipient");
    let outcome = runtime.execute_initial(&[input(&node, "setup.js", code, true)]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    while runtime.has_ready_document_task() {
        let outcome = runtime.advance_time(Duration::ZERO, 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    }
    (dom, runtime)
}

#[test]
fn storage_and_document_ready_events_preserve_enqueue_order() {
    for predecessor in ["none", "dcl", "load"] {
        let dom = dom::parse_with_scripting("<body><p></p><script></script></body>", true);
        let node = dom.elements_named("script").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
        let code = r#"
            const log = [];
            const record = v => { log.push(v); document.querySelector('p').textContent = log.join('|'); };
            onstorage = () => record('storage');
            document.addEventListener('DOMContentLoaded', () => record('dcl'));
            onload = () => record('load');
        "#;
        let initial = runtime.execute_initial_before_document_completion(
            &[input(&node, "order.js", code, true)],
            None,
        );
        assert!(initial.errors.is_empty());
        if predecessor != "none" {
            assert!(runtime.finish_document_lifecycle().errors.is_empty());
        }
        if predecessor == "load" {
            assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
            assert_eq!(text(&dom), "dcl");
        }
        runtime
            .synchronize_storage(update(2, Some("key"), None, Some("value")))
            .unwrap();
        if predecessor == "none" {
            assert!(runtime.finish_document_lifecycle().errors.is_empty());
        }
        for _ in 0..3 {
            assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
        }
        let expected = match predecessor {
            "none" => "storage|dcl|load",
            "dcl" => "dcl|storage|load",
            _ => "dcl|load|storage",
        };
        assert_eq!(text(&dom), expected);
    }
}

fn update(version: u64, key: Option<&str>, old: Option<&str>, new: Option<&str>) -> StorageUpdate {
    StorageUpdate {
        area: StorageAreaKind::Local,
        version,
        acknowledgement: 0,
        change: Some(StorageChange {
            version,
            key: key.map(Into::into),
            old_value: old.map(Into::into),
            new_value: new.map(Into::into),
        }),
        source_url: "https://example.com/source?q=1#fragment".into(),
    }
}

fn text(dom: &dom::Dom) -> String {
    dom.elements_named("p").next().unwrap().text_content()
}

#[test]
fn storage_events_are_trusted_queued_tasks_with_microtasks_and_recipient_area() {
    let (dom, mut runtime) = setup(
        r#"
        const area = localStorage;
        const log = [];
        onstorage = event => {
            log.push([event.key, event.oldValue, event.newValue, event.url,
                event.storageArea === area, event instanceof StorageEvent,
                event.isTrusted, event.bubbles, event.cancelable, event.composed,
                event.target === window, event.currentTarget === window,
                area.getItem(event.key)].join(','));
            Promise.resolve().then(() => {
                log.push('microtask'); document.getElementById('result').textContent = log.join('|');
            });
        };
        if (typeof __dispatchStorageEvent !== 'undefined') throw Error('trusted hook exposed');
    "#,
    );
    assert!(
        runtime
            .synchronize_storage(update(2, Some("key"), None, Some("value")))
            .unwrap()
    );
    assert_eq!(
        text(&dom),
        "waiting",
        "IPC application must not dispatch reentrantly"
    );
    assert_eq!(
        runtime
            .host
            .borrow()
            .local_storage
            .view()
            .get(&"key".into()),
        Some(&"value".into())
    );
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.storage_event_receipts,
        [(StorageAreaKind::Local, 2)]
    );
    assert_eq!(
        text(&dom),
        "key,,value,https://example.com/source?q=1#fragment,true,true,true,false,false,false,true,true,value|microtask"
    );
    assert!(
        runtime
            .advance_time(Duration::ZERO, 1)
            .storage_event_receipts
            .is_empty()
    );
}

#[test]
fn handlers_can_write_reentrantly_and_captured_intrinsics_resist_page_overrides() {
    let (dom, mut runtime) = setup(
        r#"
        const area = localStorage;
        onstorage = event => {
            if (event.storageArea !== area || !event.isTrusted) throw Error('event payload');
            area.setItem('reply', event.newValue);
            document.getElementById('result').textContent = event.key + ':' + event.newValue;
        };
        window.addEventListener('load', () => {
        window.__dispatchStorageEvent = () => { throw Error('forged callback'); };
        window.dispatchEvent = () => { throw Error('overridden dispatcher'); };
        window.StorageEvent = function() { throw Error('overridden constructor'); };
        Object.defineProperty(window, 'localStorage', { value: null });
        });
    "#,
    );
    runtime
        .synchronize_storage(update(2, Some("hello"), None, Some("world")))
        .unwrap();
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(text(&dom), "hello:world");
    assert_eq!(outcome.storage_updates.len(), 1);
    assert_eq!(outcome.storage_updates[0].sequence, 1);
    assert_eq!(
        outcome.storage_updates[0].source_url,
        "https://example.com/recipient"
    );
}

#[test]
fn nullable_and_lossless_payloads_survive_dispatch_and_navigation_discards_tasks() {
    let (dom, mut runtime) = setup(
        r#"
        let count = 0;
        onstorage = event => {
            count++;
            const key = '\uD800\0\uDFFF';
            if (count === 1 && (event.key !== key || event.oldValue !== null || event.newValue !== '')) throw Error('UTF16');
            if (count === 2 && (event.key !== key || event.oldValue !== '' || event.newValue !== null)) throw Error('remove');
            document.getElementById('result').textContent = String(count);
        };
    "#,
    );
    let key = crate::storage::StorageString::from_units(vec![0xd800, 0, 0xdfff]);
    let mut first = update(2, Some("unused"), None, Some(""));
    first.change.as_mut().unwrap().key = Some(key.clone());
    runtime.synchronize_storage(first).unwrap();
    assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
    let mut remove = update(3, Some("unused"), Some(""), None);
    remove.change.as_mut().unwrap().key = Some(key);
    runtime.synchronize_storage(remove).unwrap();
    assert!(runtime.advance_time(Duration::ZERO, 1).errors.is_empty());
    assert_eq!(text(&dom), "2");
    runtime
        .synchronize_storage(update(4, Some("later"), None, Some("never")))
        .unwrap();
    runtime.cancel_document();
    assert_eq!(runtime.next_timer_delay(), None);
    assert_eq!(text(&dom), "2");
}
