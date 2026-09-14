use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{DocumentId, DocumentState};
use better_web_browser::storage::{
    StorageAreaSnapshot, StorageEntry, StorageOperation, StorageString,
};
use std::time::Duration;

#[test]
fn large_lossless_storage_reaches_the_renderer_and_returns_as_a_mutation() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let key = StorageString::from_units(vec![0xd800, 0, 0xdfff]);
    let value: StorageString = "v".repeat(512 * 1024).into();
    let body = br#"<!doctype html><p>storage</p><script>
        const key = '\uD800\0\uDFFF';
        const value = localStorage.getItem(key);
        if (value !== 'v'.repeat(512 * 1024)) throw new Error('snapshot corrupted');
        localStorage.setItem(key, value + key);
    </script>"#
        .to_vec();
    session
        .load_document(
            document_start(DocumentId::new(951).unwrap(), body.len()),
            DocumentState {
                local_storage: StorageAreaSnapshot {
                    version: 1,
                    entries: vec![StorageEntry {
                        key: key.clone(),
                        value: value.clone(),
                    }],
                },
                cookie_version: 1,
                cookie_header: String::new(),
                session_storage: StorageAreaSnapshot::empty(),
            },
            body,
        )
        .unwrap();
    loop {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::StorageMutation(request) => {
                let StorageOperation::Set {
                    key: returned_key,
                    value: returned_value,
                } = request.mutation.operation
                else {
                    panic!("wrong operation");
                };
                assert_eq!(returned_key, key);
                let mut expected = value.units().to_vec();
                expected.extend_from_slice(key.units());
                assert_eq!(returned_value.units(), expected);
                break;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("missing lossless mutation: {event:?}"),
        }
    }
    session.shutdown().expect("clean shutdown");
}
