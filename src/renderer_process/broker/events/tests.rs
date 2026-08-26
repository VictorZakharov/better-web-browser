use super::*;
use crate::document::Document;
use crate::renderer_protocol::{
    AccessibilityUpdate, DocumentId, DocumentNodeId, PageLoadReport, PresentedLayout,
    RendererPresentation, RendererRuntimeUpdate, RuntimeReport, StyleReport,
};

fn presentation(revision: u64) -> RendererEvent {
    let root = DocumentNodeId::new((1_u128 << 64) | 1).unwrap();
    let mut accessibility =
        AccessibilityUpdate::full_root(root, "revision 1", crate::engine::RectF::default());
    if revision > 1 {
        accessibility.full = false;
        accessibility.nodes[0].name = format!("revision {revision}");
    }
    RendererEvent::Presentation(Box::new(RendererPresentation {
        document: DocumentId::new(1).unwrap(),
        revision,
        title: String::new(),
        final_url: "https://example.test/".into(),
        status: 200,
        character_set: "utf-8".into(),
        reader: Document {
            title: String::new(),
            source_url: "https://example.test/".into(),
            blocks: Vec::new(),
            truncated: false,
        },
        layout: PresentedLayout::default(),
        images: Vec::new(),
        glyph_epoch: 0,
        glyphs: Vec::new(),
        runtime: RuntimeReport {
            scripts_executed: 1,
            console: vec![format!("revision {revision}")],
            render_requested: true,
            ..RuntimeReport::default()
        },
        style: StyleReport {
            total_styles: revision,
            ..StyleReport::default()
        },
        load: PageLoadReport::default(),
        page_diagnostics: Default::default(),
        accessibility,
        next_timer_micros: None,
    }))
}

fn runtime_update(scripts_executed: u64, console: &str) -> RendererEvent {
    RendererEvent::RuntimeUpdate(Box::new(RendererRuntimeUpdate {
        document: DocumentId::new(1).unwrap(),
        runtime: RuntimeReport {
            scripts_executed,
            console: vec![console.into()],
            runtime_active: true,
            ..RuntimeReport::default()
        },
        load: PageLoadReport::default(),
        next_timer_micros: Some(10_000),
    }))
}

#[test]
fn presentation_bursts_keep_only_the_newest_snapshot() {
    let (sender, receiver) = bounded();
    sender.try_send(presentation(1)).unwrap();
    sender
        .try_send(RendererEvent::Diagnostic {
            code: 7,
            text: "between revisions".into(),
        })
        .unwrap();
    sender.try_send(presentation(2)).unwrap();
    sender.try_send(presentation(3)).unwrap();

    assert!(matches!(
        receiver.try_recv().unwrap(),
        RendererEvent::Diagnostic { code: 7, .. }
    ));
    let RendererEvent::Presentation(presentation) = receiver.try_recv().unwrap() else {
        panic!("newest presentation was not retained");
    };
    assert_eq!(presentation.revision, 3);
    assert!(presentation.accessibility.full);
    assert_eq!(presentation.accessibility.nodes[0].name, "revision 3");
    assert_eq!(presentation.runtime.scripts_executed, 3);
    assert_eq!(
        presentation.runtime.console,
        ["revision 1", "revision 2", "revision 3"]
    );
    assert_eq!(presentation.style.total_styles, 6);
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

#[test]
fn adjacent_runtime_updates_coalesce_without_losing_ordered_output() {
    let (sender, receiver) = bounded();
    sender.try_send(runtime_update(1, "first")).unwrap();
    sender.try_send(runtime_update(2, "second")).unwrap();

    let RendererEvent::RuntimeUpdate(update) = receiver.try_recv().unwrap() else {
        panic!("coalesced runtime update was not retained");
    };
    assert_eq!(update.runtime.scripts_executed, 3);
    assert_eq!(update.runtime.console, ["first", "second"]);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn cursor_results_coalesce_to_the_newest_sequence_per_document() {
    let (sender, receiver) = bounded();
    let first = DocumentId::new(1).unwrap();
    let second = DocumentId::new(2).unwrap();
    let cursor = |document, sequence| {
        RendererEvent::PointerCursor(crate::renderer_protocol::PointerCursorResult {
            document,
            sequence,
            cursor: crate::renderer_protocol::PointerCursor::Pointer,
        })
    };

    sender.try_send(cursor(first, 1)).unwrap();
    sender.try_send(cursor(second, 1)).unwrap();
    sender.try_send(cursor(first, 3)).unwrap();
    sender.try_send(cursor(first, 2)).unwrap();

    let RendererEvent::PointerCursor(second_result) = receiver.try_recv().unwrap() else {
        panic!("expected second-document cursor");
    };
    assert_eq!(second_result.document, second);
    let RendererEvent::PointerCursor(first_result) = receiver.try_recv().unwrap() else {
        panic!("expected first-document cursor");
    };
    assert_eq!(first_result.document, first);
    assert_eq!(first_result.sequence, 3);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn consecutive_fetch_batches_wait_for_browser_drain_without_failing() {
    let (sender, receiver) = bounded();
    let document = DocumentId::new(1).unwrap();
    sender
        .send(RendererEvent::FetchBatch {
            document,
            requests: Vec::new(),
        })
        .unwrap();

    let (completed, completion) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        let result = sender.send(RendererEvent::FetchBatch {
            document,
            requests: Vec::new(),
        });
        completed.send(result).unwrap();
    });
    assert!(matches!(
        completion.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert!(matches!(
        receiver.try_recv().unwrap(),
        RendererEvent::FetchBatch {
            document: first,
            requests
        } if first == document && requests.is_empty()
    ));
    completion
        .recv_timeout(Duration::from_secs(1))
        .expect("Fetch producer resumed after the browser drained its slot")
        .unwrap();
    assert!(matches!(
        receiver.try_recv().unwrap(),
        RendererEvent::FetchBatch {
            document: second,
            requests
        } if second == document && requests.is_empty()
    ));
    producer.join().unwrap();
}

#[test]
fn cancelled_transactional_fetch_batches_are_discarded_and_reusable() {
    let (sender, receiver) = bounded();
    let replaced = DocumentId::new(1).unwrap();
    sender
        .send(RendererEvent::FetchBatch {
            document: replaced,
            requests: Vec::new(),
        })
        .unwrap();
    sender.discard_document(replaced);
    let replacement = DocumentId::new(2).unwrap();
    sender
        .send(RendererEvent::FetchBatch {
            document: replacement,
            requests: Vec::new(),
        })
        .unwrap();
    assert!(matches!(
        receiver.try_recv().unwrap(),
        RendererEvent::FetchBatch { document, requests }
            if document == replacement && requests.is_empty()
    ));
}
