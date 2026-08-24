use super::*;

#[test]
fn worker_constructor_and_messages_queue_document_owned_actions() {
    let (_, outcome) = execute_html(
        r#"<script>
            const worker = new Worker('/worker.js', { type: 'module', name: 'parser' });
            worker.postMessage({ value: 42 });
            worker.terminate();
        </script>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(matches!(
        outcome.worker_actions.as_slice(),
        [
            ScriptWorkerAction::Start { id: 1, url, kind: ScriptKind::Module, name, .. },
            ScriptWorkerAction::PostMessage { id: 1, serialized },
            ScriptWorkerAction::Terminate { id: 1 }
        ] if url == "https://example.com/worker.js"
            && name == "parser"
            && serialized == "{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"value\",42]]}"
    ));
}

#[test]
fn worker_constructor_validates_url_origin_and_options_synchronously() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            const errors = [];
            try { new Worker(); } catch (error) { errors.push(error.name); }
            try { new Worker('https://other.example/worker.js'); } catch (error) { errors.push(error.name); }
            try { new Worker('/worker.js', { type: 'shared' }); } catch (error) { errors.push(error.name); }
            try { new Worker('/worker.js', { credentials: 'always' }); } catch (error) { errors.push(error.name); }
            document.querySelector('div').textContent = errors.join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.worker_actions.is_empty());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "TypeError|SecurityError|TypeError|TypeError"
    );
}

#[test]
fn retained_worker_message_dispatches_a_message_event() {
    let dom = crate::engine::dom::parse_with_scripting(
        r#"<body><div>pending</div><script>
            const worker = new Worker('/worker.js');
            worker.onmessage = event => {
                document.querySelector('div').textContent = event.data.answer;
            };
        </script></body>"#,
        true,
    );
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&scripts);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);

    let outcome = runtime.complete_worker_event_with_loader(
        1,
        Ok("{\"t\":\"object\",\"id\":1,\"n\":false,\"v\":[[\"answer\",\"ready\"]]}".into()),
        None,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.render_requested);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn structured_clone_preserves_graph_types_and_detaches_transfers() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            const source = { date: new Date(123), map: new Map([['answer', 42]]) };
            source.self = source;
            const clone = structuredClone(source);
            const buffer = new Uint8Array([7, 8, 9]).buffer;
            const moved = structuredClone({ buffer }, { transfer: [buffer] });
            let duplicate = '';
            const other = new ArrayBuffer(1);
            try { structuredClone({}, { transfer: [other, other] }); }
            catch (error) { duplicate = error.name; }
            document.querySelector('div').textContent = [
                clone !== source, clone.self === clone, clone.date.getTime(),
                clone.map.get('answer'), buffer.byteLength, new Uint8Array(moved.buffer)[2], duplicate
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true|true|123|42|0|9|DataCloneError"
    );
}

#[test]
fn worker_post_message_serializes_before_detaching_transfer_buffers() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            const worker = new Worker('/worker.js');
            const buffer = new Uint8Array([1, 2, 3]).buffer;
            worker.postMessage({ buffer }, [buffer]);
            document.querySelector('div').textContent = buffer.byteLength;
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "0"
    );
    assert!(matches!(
        &outcome.worker_actions[1],
        ScriptWorkerAction::PostMessage { serialized, .. }
            if serialized.contains("AQID") && serialized.contains("buffer")
    ));
}
