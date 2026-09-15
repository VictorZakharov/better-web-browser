//! Two real hidden AppContainer renderers sharing the production storage authority.
use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    BrowsingContextId, DocumentId, PresentationAcknowledgement,
};
use better_web_browser::storage::{
    LocalStorage, SessionStorage, StorageCoordinator, StorageSubscription, StorageWrite,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "storage_sync/lifecycle.rs"]
mod lifecycle;

struct Tab {
    session: RendererSession,
    document: DocumentId,
    subscription: StorageSubscription,
    storage: SessionStorage,
    console: Vec<String>,
    presented: bool,
}

impl Tab {
    fn load(hub: &StorageCoordinator, id: u64, html: &str) -> Self {
        Self::load_at(hub, id, &format!("https://example.test/{id}"), html)
    }

    fn load_at(hub: &StorageCoordinator, id: u64, url: &str, html: &str) -> Self {
        let document = DocumentId::new(id).unwrap();
        let mut launch = options();
        launch.browsing_context = BrowsingContextId::new(id).unwrap();
        let session = RendererSession::launch(launch).expect("launch hidden renderer");
        let mut start = document_start(document, html.len());
        start.url = url.into();
        let (snapshot, subscription) = hub.subscribe(&start.url).unwrap();
        let mut state = empty_document_state();
        state.local_storage = snapshot;
        session
            .load_document(start, state, html.as_bytes().to_vec())
            .unwrap();
        Self {
            session,
            document,
            subscription,
            storage: SessionStorage::default(),
            console: Vec::new(),
            presented: false,
        }
    }

    fn pump(&mut self, hub: &StorageCoordinator) {
        self.subscription.take_if(|update| {
            self.session
                .try_synchronize_storage(self.document, update.clone())
                .unwrap()
        });
        for _ in 0..32 {
            let Some(event) = self.session.try_event().unwrap() else {
                break;
            };
            match event {
                RendererEvent::StorageMutation(request) => {
                    assert_eq!(request.document, self.document);
                    assert!(
                        hub.apply(
                            &self.subscription,
                            &[StorageWrite {
                                sequence: request.sequence,
                                source_url: request.source_url,
                                mutation: request.mutation,
                            }],
                            &mut self.storage
                        )
                        .unwrap()
                    );
                }
                RendererEvent::Presentation(p) => {
                    assert!(p.runtime.errors.is_empty(), "{:?}", p.runtime.errors);
                    self.console.extend(p.runtime.console.clone());
                    self.session
                        .acknowledge_presentation(PresentationAcknowledgement {
                            document: p.document,
                            revision: p.revision,
                            presented: true,
                            controls_applied: true,
                        })
                        .unwrap();
                    pump_ready_task(&self.session, self.document, p.next_timer_micros);
                    self.presented = true;
                }
                RendererEvent::RuntimeUpdate(update) => {
                    assert!(
                        update.runtime.errors.is_empty(),
                        "{:?}",
                        update.runtime.errors
                    );
                    self.console.extend(update.runtime.console.clone());
                    pump_ready_task(&self.session, self.document, update.next_timer_micros);
                }
                RendererEvent::Diagnostic { .. } => {}
                other => panic!("unexpected storage renderer event: {other:?}"),
            }
        }
        self.session.finish_event_drain();
    }
}

#[test]
fn original_chrome_comparison_fixture_passes_in_two_isolated_renderers() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let hub = StorageCoordinator::new(Arc::new(LocalStorage::in_memory()));
    let html = include_str!("../../benchmarks/alpha/fixtures/storage-sync.html");
    let mut reader = Tab::load_at(&hub, 1211, "https://example.test/storage-sync.html", html);
    let mut writer = Tab::load_at(
        &hub,
        1212,
        "https://example.test/storage-sync.html?role=writer",
        html,
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        reader.pump(&hub);
        writer.pump(&hub);
        for tab in [&reader, &writer] {
            assert!(
                !tab.console
                    .iter()
                    .any(|line| line.contains("storage-sync:FAIL")),
                "{:?}",
                tab.console
            );
        }
        if [&reader, &writer].iter().all(|tab| {
            tab.console
                .iter()
                .any(|line| line.contains("storage-sync:PASS:8"))
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "comparison fixture timed out: reader {:?}; writer {:?}",
            reader.console,
            writer.console
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    reader.session.shutdown().unwrap();
    writer.session.shutdown().unwrap();
}

#[test]
fn isolated_tabs_deliver_ordered_storage_events_without_echo_and_allow_reentrant_writes() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let local = Arc::new(LocalStorage::in_memory());
    let hub = StorageCoordinator::new(Arc::clone(&local));
    let mut source = Tab::load(
        &hub,
        1201,
        r#"<!doctype html><p>source</p><script>
        onstorage = e => {
            if (!e.isTrusted || e.key !== 'reply' || e.newValue !== 'received'
                || e.url !== 'https://example.test/1202' || e.storageArea !== localStorage) throw Error('source echo or payload');
            console.log('source-reply');
        };
        setTimeout(() => {
            localStorage.setItem('key','one'); localStorage.setItem('key','one');
            localStorage.setItem('key','two'); localStorage.removeItem('key'); localStorage.removeItem('key');
            localStorage.setItem('empty',''); localStorage.clear(); localStorage.clear();
            sessionStorage.setItem('private','source only');
            console.log('writes-submitted');
        }, 10000);
    </script>"#,
    );
    let mut recipient = Tab::load(
        &hub,
        1202,
        r#"<!doctype html><p>recipient</p><script>
        const expected = [['key',null,'one'],['key','one','two'],['key','two',null],['empty',null,''],[null,null,null]];
        let count = 0;
        onstorage = e => {
            if (!e.isTrusted || e.bubbles || e.cancelable || e.composed || e.target !== window
                || e.storageArea !== localStorage || e.url !== 'https://example.test/1201') throw Error('recipient payload');
            if (JSON.stringify([e.key,e.oldValue,e.newValue]) !== JSON.stringify(expected[count++])) throw Error('event order');
            if (sessionStorage.getItem('private') !== null) throw Error('session scope');
            if (count === 5) { localStorage.setItem('reply','received'); console.log('recipient-five'); }
        };
    </script>"#,
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while !source.presented || !recipient.presented {
        source.pump(&hub);
        recipient.pump(&hub);
        assert!(Instant::now() < deadline, "initial presentation timeout");
        std::thread::sleep(Duration::from_millis(5));
    }
    source
        .session
        .advance_time(source.document, Duration::from_secs(10), 1)
        .unwrap();
    loop {
        source.pump(&hub);
        recipient.pump(&hub);
        if source
            .console
            .iter()
            .any(|line| line.contains("source-reply"))
            && recipient
                .console
                .iter()
                .any(|line| line.contains("recipient-five"))
            && !source.subscription.has_pending()
            && !recipient.subscription.has_pending()
            && source.session.snapshot().pending_state_updates == 0
            && recipient.session.snapshot().pending_state_updates == 0
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "storage delivery timed out: source {:?} {:?}; recipient {:?} {:?}",
            source.console,
            source.session.snapshot(),
            recipient.console,
            recipient.session.snapshot()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        source
            .console
            .iter()
            .filter(|line| line.contains("source-reply"))
            .count(),
        1
    );
    let final_map = local.snapshot("https://example.test/").unwrap();
    assert_eq!(final_map.entries.len(), 1);
    assert_eq!(final_map.entries[0].key, "reply");
    assert_eq!(final_map.entries[0].value, "received");
    source.session.shutdown().unwrap();
    recipient.session.shutdown().unwrap();
}
