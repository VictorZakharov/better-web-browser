use super::*;

#[test]
fn byob_reader_consumes_queued_bytes_on_element_boundaries() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const source = new Uint8Array([1, 2, 3, 4, 5]);
                const stream = new ReadableStream({ type: 'bytes', start(controller) {
                    controller.enqueue(source); controller.close();
                } });
                const reader = stream.getReader({ mode: 'byob' });
                const firstInput = new Uint16Array(2);
                const first = await reader.read(firstInput, { min: 2 });
                const second = await reader.read(new Uint8Array(2));
                const end = await reader.read(new Uint8Array(2));
                reader.releaseLock();
                document.querySelector('output').textContent = [
                    source.buffer.byteLength, firstInput.buffer.byteLength,
                    first.value.join(','), first.done,
                    second.value.join(','), second.done,
                    end.value.byteLength, end.done, stream.locked
                ].join('|');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "0|0|513,1027|false|5|false|0|true|false"
    );
}

#[test]
fn byob_request_responds_with_transferred_buffer() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                let requestView;
                const stream = new ReadableStream({ type: 'bytes', pull(controller) {
                    const request = controller.byobRequest;
                    requestView = request.view;
                    request.view.set([7, 8]);
                    request.respond(2);
                    controller.close();
                } });
                const reader = stream.getReader({ mode: 'byob' });
                const result = await reader.read(new Uint8Array(4));
                const end = await reader.read(new Uint8Array(1));
                document.querySelector('output').textContent = [
                    result.value.join(','), result.done,
                    requestView.buffer.byteLength, end.done
                ].join('|');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "7,8|false|0|true"
    );
}

#[test]
fn default_reader_uses_auto_allocated_byob_request() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const stream = new ReadableStream({
                    type: 'bytes', autoAllocateChunkSize: 8,
                    pull(controller) {
                        const request = controller.byobRequest;
                        request.view.set([9, 10, 11]);
                        request.respond(3);
                        controller.close();
                    }
                });
                const reader = stream.getReader();
                const first = await reader.read();
                const end = await reader.read();
                document.querySelector('output').textContent = [
                    first.value.join(','), first.done, end.done
                ].join('|');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "9,10,11|false|true"
    );
}

#[test]
fn byte_tee_provides_independent_buffers_to_both_branches() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const source = new ReadableStream({ type: 'bytes', start(controller) {
                    controller.enqueue(new Uint8Array([3, 4])); controller.close();
                } });
                const [left, right] = source.tee();
                const a = await left.getReader({ mode: 'byob' }).read(new Uint8Array(2));
                const b = await right.getReader({ mode: 'byob' }).read(new Uint8Array(2));
                a.value[0] = 99;
                document.querySelector('output').textContent = [
                    a.value.join(','), b.value.join(','), left.__byteStream, right.__byteStream
                ].join('|');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "99,4|3,4|true|true"
    );
}

#[test]
fn byte_stream_rejects_invalid_source_strategy_and_read_views() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const failures = [];
                try { new ReadableStream({ type: 'bytes' }, { size() { return 1; } }); }
                catch (error) { failures.push(error.name); }
                try { new ReadableStream().getReader({ mode: 'byob' }); }
                catch (error) { failures.push(error.name); }
                const stream = new ReadableStream({ type: 'bytes' });
                const reader = stream.getReader({ mode: 'byob' });
                try { await reader.read(new Uint8Array(0)); }
                catch (error) { failures.push(error.name); }
                try { await reader.read(new Uint8Array(2), { min: 3 }); }
                catch (error) { failures.push(error.name); }
                document.querySelector('output').textContent = failures.join(',');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "RangeError,TypeError,TypeError,RangeError"
    );
}

#[test]
fn byob_request_accepts_a_transferred_replacement_view() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                let stage = 'clone';
                const stream = new ReadableStream({ type: 'bytes', pull(controller) {
                    const request = controller.byobRequest;
                    const view = structuredClone(request.view, { transfer: [request.view.buffer] });
                    view[0] = 42;
                    stage = 'respond';
                    request.respondWithNewView(view);
                    controller.close();
                } });
                try {
                    const result = await stream.getReader({ mode: 'byob' }).read(new Uint8Array(1));
                    document.querySelector('output').textContent = result.value[0];
                } catch (error) {
                    document.querySelector('output').textContent = stage + ': ' + error.name + ': ' + error.message;
                }
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "42"
    );
}
