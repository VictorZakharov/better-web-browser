use super::*;
use better_web_browser::storage::{StorageAreaKind, StorageMutation, StorageOperation};

const URL: &str = "https://example.test/writer#mutation-time";

fn write(sequence: u64, value: &str) -> StorageWrite {
    StorageWrite {
        sequence,
        source_url: URL.into(),
        mutation: StorageMutation {
            area: StorageAreaKind::Local,
            expected_version: 1,
            operation: StorageOperation::Set {
                key: "key".into(),
                value: value.into(),
            },
        },
    }
}

fn until(tab: &mut Tab, hub: &StorageCoordinator, done: impl Fn(&Tab) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        tab.pump(hub);
        if done(tab) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "storage lifecycle timeout: {:?}",
            tab.console
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn navigation_retires_pending_delivery_and_new_document_receives_only_new_events() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let hub = StorageCoordinator::new(Arc::new(LocalStorage::in_memory()));
    let (_, writer) = hub.subscribe(URL).unwrap();
    let mut session = SessionStorage::default();
    let mut recipient = Tab::load(&hub, 1221, "<!doctype html><p>old</p>");
    until(&mut recipient, &hub, |tab| tab.presented);
    assert!(
        hub.apply(&writer, &[write(1, "before")], &mut session)
            .unwrap()
    );
    recipient.subscription.take_if(|update| {
        recipient
            .session
            .try_synchronize_storage(recipient.document, update.clone())
            .unwrap()
    });
    // Race a previously admitted update/receipt against replacing its document.
    recipient
        .session
        .cancel_document(recipient.document)
        .unwrap();
    let html = r#"<!doctype html><p>new</p><script>
        if (localStorage.getItem('key') !== 'before') throw Error('replacement snapshot');
        onstorage = e => {
            if (e.oldValue !== 'before' || e.newValue !== 'after') throw Error('retired event leaked');
            console.log('new-event');
        };
        console.log('new-ready');
    </script>"#;
    recipient.document = DocumentId::new(1222).unwrap();
    let mut start = document_start(recipient.document, html.len());
    start.url = "https://example.test/replacement".into();
    let (snapshot, subscription) = hub.subscribe(&start.url).unwrap();
    recipient.subscription = subscription;
    let mut state = empty_document_state();
    state.local_storage = snapshot;
    recipient.presented = false;
    recipient
        .session
        .load_document(start, state, html.as_bytes().to_vec())
        .unwrap();
    until(&mut recipient, &hub, |tab| {
        tab.console.iter().any(|line| line.contains("new-ready"))
    });
    assert!(
        hub.apply(&writer, &[write(2, "after")], &mut session)
            .unwrap()
    );
    until(&mut recipient, &hub, |tab| {
        tab.console.iter().any(|line| line.contains("new-event"))
            && !tab.subscription.has_pending()
            && tab.session.snapshot().pending_state_updates == 0
    });
    assert_eq!(
        recipient
            .console
            .iter()
            .filter(|line| line.contains("new-event"))
            .count(),
        1
    );
    recipient.session.shutdown().unwrap();
}

#[test]
fn full_quota_old_and_new_values_cross_the_real_pipe_without_truncation() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let hub = StorageCoordinator::new(Arc::new(LocalStorage::in_memory()));
    let (_, writer) = hub.subscribe(URL).unwrap();
    let mut session = SessionStorage::default();
    let mut recipient = Tab::load(
        &hub,
        1231,
        r#"<!doctype html><p>large values</p><script>
        let count = 0; const length = 5 * 1024 * 1024 - 3;
        onstorage = e => {
            if (e.key !== 'key' || !e.isTrusted || e.url !== 'https://example.test/writer#mutation-time') throw Error('large event metadata');
            if (e.newValue.length !== length || e.newValue !== (count ? 'b' : 'a').repeat(length)) throw Error('truncated new value');
            if (count === 0 ? e.oldValue !== null : e.oldValue !== 'a'.repeat(length)) throw Error('truncated old value');
            if (localStorage.getItem('key') !== e.newValue) throw Error('map not synchronized before event');
            if (++count === 2) console.log('large-passed');
        };
    </script>"#,
    );
    until(&mut recipient, &hub, |tab| tab.presented);
    let length = 5 * 1024 * 1024 - 3;
    assert!(
        hub.apply(
            &writer,
            &[write(1, &"a".repeat(length)), write(2, &"b".repeat(length))],
            &mut session
        )
        .unwrap()
    );
    until(&mut recipient, &hub, |tab| {
        tab.console.iter().any(|line| line.contains("large-passed"))
            && !tab.subscription.has_pending()
            && tab.session.snapshot().pending_state_updates == 0
    });
    recipient.session.shutdown().unwrap();
}
