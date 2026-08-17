use super::network::{pending_runtime, test_response};
use super::*;

#[test]
fn response_stream_locking_and_reads_follow_body_mixin_state() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"fetch('/stream').then(async response => {
            const before = response.bodyUsed;
            const reader = response.body.getReader();
            const afterLock = response.bodyUsed;
            const first = await reader.read();
            const afterRead = response.bodyUsed;
            const end = await reader.read();
            reader.releaseLock();
            let consumeError = '';
            try { await response.text(); } catch (error) { consumeError = error.name; }
            document.querySelector('div').textContent = [
                before, afterLock, afterRead, response.body.locked,
                new TextDecoder().decode(first.value), end.done, consumeError
            ].join('|');
        });"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"streamed")), None);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "false|false|true|false|streamed|true|TypeError"
    );
}

#[test]
fn readable_writable_and_transform_streams_preserve_order() {
    let (dom, outcome) = execute_html(
        r#"<body><div>pending</div><script>
            (async () => {
                const output = [];
                const destination = new WritableStream({ write(value) { output.push(value); } });
                const transform = new TransformStream({
                    transform(value, controller) { controller.enqueue(value * 2); }
                });
                await ReadableStream.from([1, 2, 3]).pipeThrough(transform).pipeTo(destination);
                document.querySelector('div').textContent = output.join(',');
            })();
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "2,4,6"
    );
}

#[test]
fn request_construction_transfers_bodies_without_breaking_clone() {
    let (dom, outcome) = execute_html(
        r#"<body><div>pending</div><script>
            (async () => {
                const original = new Request('/source', { method: 'POST', body: 'source' });
                const originalBody = original.body;
                const moved = new Request(original);
                const cloneSource = new Request('/clone', { method: 'POST', body: 'clone' });
                const clone = cloneSource.clone();
                const failedSource = new Request('/failed', { method: 'POST', body: 'kept' });
                let failed = '';
                try { new Request(failedSource, { method: 'GET' }); }
                catch (error) { failed = error.name; }
                const consumed = new Request('/consumed', { method: 'POST', body: 'old' });
                await consumed.text();
                const replacement = new Request(consumed, { method: 'POST', body: 'new' });
                const assignedHeaders = moved.headers;
                moved.headers = new Headers({ replaced: 'no' });
                document.querySelector('div').textContent = [
                    original.body === originalBody, original.bodyUsed, await moved.text(),
                    cloneSource.bodyUsed, await cloneSource.text(), await clone.text(),
                    failed, failedSource.bodyUsed, await replacement.text(),
                    moved.headers === assignedHeaders, moved.headers.has('replaced'),
                    moved.isReloadNavigation, moved.isHistoryNavigation
                ].join('|');
            })();
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true|true|source|false|clone|clone|TypeError|false|new|true|false|false|false"
    );
}

#[test]
fn response_initialization_guards_headers_and_status_text() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
            const response = new Response('body', {
                status: 201, statusText: String.fromCharCode(0x80),
                headers: { 'x-proof': 'yes', 'set-cookie': 'hidden=1' }
            });
            const headers = response.headers;
            response.headers = new Headers({ replaced: 'no' });
            let invalid = '', immutable = '';
            try { new Response('', { statusText: '\u0100' }); } catch (error) { invalid = error.name; }
            try { Response.error().headers.set('x', 'y'); } catch (error) { immutable = error.name; }
            document.querySelector('div').textContent = [
                response.status, response.ok, response.statusText.charCodeAt(0),
                response.headers === headers, headers.get('x-proof'), headers.get('set-cookie') === null,
                invalid, immutable, new Response(null, { status: 204 }).body === null
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "201|true|128|true|yes|true|TypeError|TypeError|true"
    );
}

#[test]
fn form_data_collects_successful_controls_and_keeps_file_metadata_readonly() {
    let (dom, outcome) = execute_html(
        r#"<body><form id=f>
            <input name=text value=one>
            <input name=skip value=no disabled>
            <input type=checkbox name=checked value=yes checked>
            <input type=checkbox name=unchecked value=no>
            <select name=choice><option value=first>First</option><option value=second selected>Second</option></select>
            <button id=submit name=action value=save>Save</button>
        </form><div></div><script>
            const form = document.getElementById('f');
            const withoutSubmitter = new FormData(form);
            const withSubmitter = new FormData(form, document.getElementById('submit'));
            const file = new File(['x'], 'original.txt', { type: 'TEXT/PLAIN', lastModified: 7 });
            file.name = 'changed.txt'; file.lastModified = 9; file.type = 'other/type';
            document.querySelector('div').textContent = [
                [...withoutSubmitter].map(entry => entry.join('=')).join(','),
                withSubmitter.get('action'), file.name, file.type, file.lastModified
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "text=one,checked=yes,choice=second|save|original.txt|text/plain|7"
    );
}

#[test]
fn multipart_form_data_serializes_empty_lists_and_rejects_missing_payloads() {
    let (dom, outcome) = execute_html(
        r#"<body><div>pending</div><script>
            (async () => {
                const response = new Response(new FormData());
                const contentType = response.headers.get('content-type');
                const serialized = await response.text();
                const parsed = await new Response(serialized, {
                    headers: { 'content-type': contentType }
                }).formData();
                let malformed = '';
                try {
                    await new Response(undefined, {
                        headers: { 'content-type': 'multipart/form-data; boundary=missing' }
                    }).formData();
                } catch (error) { malformed = error.name; }
                document.querySelector('div').textContent = [
                    serialized.length > 0, contentType.startsWith('multipart/form-data; boundary='),
                    [...parsed].length, malformed
                ].join('|');
            })();
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true|true|0|TypeError"
    );
}
