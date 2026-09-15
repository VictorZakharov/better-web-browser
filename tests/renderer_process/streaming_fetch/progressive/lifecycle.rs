use super::*;

fn aborted(session: &RendererSession, document: DocumentId, id: u64) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match next_event(session, document, deadline) {
            RendererEvent::FetchAbort {
                document: owner,
                request_id,
            } => {
                assert_eq!((owner, request_id), (document, id));
                return;
            }
            RendererEvent::DocumentFailed { detail, .. } => panic!("{detail}"),
            RendererEvent::Exited(exit) => panic!("{exit:?}"),
            _ => {}
        }
    }
}

#[test]
fn progressive_abort_errors_both_clone_readers_with_the_exact_reason_in_each_realm() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    for worker in [false, true] {
        let mut session = RendererSession::launch(options()).unwrap();
        let source = r#"
            const report = value => typeof document === 'undefined' ? postMessage(value)
                : document.querySelector('#status').textContent = value;
            const control = new AbortController(); const reason = { stopped: true };
            fetch('/stream', { signal: control.signal }).then(async response => {
                const cloned = response.clone().text().catch(error => error === reason);
                const reader = response.body.getReader();
                await reader.read();
                const pending = reader.read().catch(error => error === reason);
                control.abort(reason);
                report('aborted:' + await pending + ':' + await cloned);
            });
        "#;
        let html = if worker {
            "<div id='status'>pending</div><script>const w=new Worker('/worker.js');
             w.onmessage=e=>document.querySelector('#status').textContent=e.data;</script>"
                .into()
        } else {
            format!("<div id='status'>pending</div><script>{source}</script>")
        };
        let initial = load_html_document(&session, 185, &html);
        if worker {
            serve_worker(&session, initial.document, source.as_bytes());
        }
        let id = request(&session, initial.document).head.request_id;
        let sink = session.fetch_response_sink(initial.document);
        sink.start(success_head(id, 100)).unwrap();
        sink.chunk(TransferChunk {
            transfer_id: id,
            offset: 0,
            bytes: b"first".to_vec(),
        })
        .unwrap();
        aborted(&session, initial.document, id);
        text(&session, initial.document, "aborted:true:true");
        // The terminal network response can arrive after script cancellation.
        sink.abort(
            id,
            better_web_browser::renderer_protocol::BrowserFetchError {
                kind: better_web_browser::renderer_protocol::BrowserFetchErrorKind::Network,
                message: "cancelled".into(),
            },
        )
        .unwrap();
        session.ping(Duration::from_secs(1)).unwrap();
        session.shutdown().unwrap();
    }
}

#[test]
fn worker_termination_cancels_streaming_bodies_and_pending_entry_scripts() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    for entry_pending in [true, false] {
        let mut session = RendererSession::launch(options()).unwrap();
        let initial = load_html_document(
            &session,
            186,
            "<div id='status'>pending</div><script>const w = new Worker('/worker.js');
             w.onmessage=e=>{ document.querySelector('#status').textContent=e.data; };
             setTimeout(()=>w.terminate(), 2000);</script>",
        );
        if !entry_pending {
            serve_worker(
                &session,
                initial.document,
                b"fetch('/stream').then(r => postMessage('headers'));",
            );
        }
        let id = request(&session, initial.document).head.request_id;
        let sink = session.fetch_response_sink(initial.document);
        sink.start(success_head(id, 512 * 1024)).unwrap();
        if !entry_pending {
            text(&session, initial.document, "headers");
        }
        session
            .advance_time(initial.document, Duration::from_millis(2200), 32)
            .unwrap();
        aborted(&session, initial.document, id);
        assert!(
            sink.chunk(TransferChunk {
                transfer_id: id,
                offset: 0,
                bytes: vec![0; 65536]
            })
            .is_err()
        );
        sink.abort(
            id,
            better_web_browser::renderer_protocol::BrowserFetchError {
                kind: better_web_browser::renderer_protocol::BrowserFetchErrorKind::Network,
                message: "terminated".into(),
            },
        )
        .unwrap();
        session.ping(Duration::from_secs(1)).unwrap();
        session.shutdown().unwrap();
    }
}

#[test]
fn navigation_wakes_a_credit_blocked_fetch_producer() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(&session, 187, "<script>fetch('/stream')</script>");
    let id = request(&session, initial.document).head.request_id;
    let sink = session.fetch_response_sink(initial.document);
    sink.start(success_head(id, 512 * 1024)).unwrap();
    let (sent, received) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        for chunk in 0..8 {
            let result = sink.chunk(TransferChunk {
                transfer_id: id,
                offset: chunk * 65536,
                bytes: vec![0; 65536],
            });
            if result.is_err() {
                return;
            }
            sent.send(chunk).unwrap();
        }
        panic!("idle response escaped its credit window");
    });
    for chunk in 0..4 {
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap(),
            chunk
        );
    }
    assert!(received.recv_timeout(Duration::from_millis(50)).is_err());
    let replacement = DocumentId::new(188).unwrap();
    session.cancel_document(initial.document).unwrap();
    let body = b"<h1>new document</h1>".to_vec();
    session
        .load_document(
            document_start(replacement, body.len()),
            empty_document_state(),
            body,
        )
        .unwrap();
    text(&session, replacement, "new document");
    producer.join().unwrap();
    session.ping(Duration::from_secs(1)).unwrap();
    session.shutdown().unwrap();
}
