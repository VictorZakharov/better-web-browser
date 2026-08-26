use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    CookieStateSnapshot, DocumentId, DocumentState, RendererPresentation, StorageMutationRequest,
};
use better_web_browser::storage::{
    StorageAreaKind, StorageAreaSnapshot, StorageEntry, StorageOperation,
};
use std::time::Duration;

#[test]
fn typed_state_snapshots_and_mutations_cross_the_renderer_boundary() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let document = DocumentId::new(92).unwrap();
    let html = r#"<!doctype html><p>waiting</p><script>
        const visibleState = () => [
            document.cookie,
            localStorage.getItem('seed'),
            sessionStorage.getItem('draft')
        ].join('|');
        document.querySelector('p').textContent = visibleState();
        document.cookie = 'theme=changed; Path=/';
        localStorage.setItem('seed', localStorage.getItem('seed') + '!');
        sessionStorage.removeItem('draft');
        setTimeout(() => document.querySelector('p').textContent = visibleState(), 10000);
    </script>"#;
    let body = html.as_bytes().to_vec();
    session
        .load_document(
            document_start(document, body.len()),
            DocumentState {
                cookie_version: 3,
                cookie_header: "theme=dark".into(),
                local_storage: state_snapshot(5, "seed", "ready"),
                session_storage: state_snapshot(7, "draft", "pending"),
            },
            body,
        )
        .unwrap();

    let mut cookie = None;
    let mut storage = Vec::new();
    let initial = loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::CookieMutation(mutation) if mutation.document == document => {
                cookie = Some(mutation.assignment);
            }
            RendererEvent::StorageMutation(mutation) if mutation.document == document => {
                storage.push(mutation);
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                break *presentation;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer state event: {event:?}"),
        }
    };
    assert_eq!(cookie.as_deref(), Some("theme=changed; Path=/"));
    assert_eq!(storage.len(), 2);
    assert_storage_set(&storage[0], StorageAreaKind::Local, 5, "seed", "ready!");
    assert_storage_remove(&storage[1], StorageAreaKind::Session, 7, "draft");
    assert!(presentation_text(&initial).contains("theme=dark|ready|pending"));

    session
        .update_cookie_snapshot(CookieStateSnapshot {
            document,
            version: 9,
            header: "theme=accepted".into(),
        })
        .unwrap();
    session
        .update_storage_snapshot(
            document,
            StorageAreaKind::Local,
            state_snapshot(10, "seed", "browser"),
        )
        .unwrap();
    session
        .update_storage_snapshot(
            document,
            StorageAreaKind::Session,
            state_snapshot(11, "draft", "restored"),
        )
        .unwrap();
    session
        .advance_time(document, Duration::from_secs(10), 1)
        .unwrap();

    let corrected = loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation)
                if presentation.document == document
                    && presentation.revision > initial.revision =>
            {
                break *presentation;
            }
            RendererEvent::RuntimeUpdate(_) | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer correction event: {event:?}"),
        }
    };
    assert!(
        presentation_text(&corrected).contains("theme=accepted|browser|restored"),
        "{}",
        presentation_text(&corrected)
    );
    session.shutdown().expect("shutdown renderer");
}

fn state_snapshot(version: u64, key: &str, value: &str) -> StorageAreaSnapshot {
    StorageAreaSnapshot {
        version,
        entries: vec![StorageEntry {
            key: key.into(),
            value: value.into(),
        }],
    }
}

fn presentation_text(presentation: &RendererPresentation) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn assert_storage_set(
    request: &StorageMutationRequest,
    area: StorageAreaKind,
    version: u64,
    key: &str,
    value: &str,
) {
    assert_eq!(request.mutation.area, area);
    assert_eq!(request.mutation.expected_version, version);
    assert_eq!(
        request.mutation.operation,
        StorageOperation::Set {
            key: key.into(),
            value: value.into(),
        }
    );
}

fn assert_storage_remove(
    request: &StorageMutationRequest,
    area: StorageAreaKind,
    version: u64,
    key: &str,
) {
    assert_eq!(request.mutation.area, area);
    assert_eq!(request.mutation.expected_version, version);
    assert_eq!(
        request.mutation.operation,
        StorageOperation::Remove { key: key.into() }
    );
}
