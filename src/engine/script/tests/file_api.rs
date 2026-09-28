use super::*;

#[test]
fn blob_slices_cross_private_chunks_without_exposing_mutable_backing() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const input = new Uint8Array([10, 20, 30]);
                const original = new Blob(['ab', input, 'cd'], {type:'TEXT/PLAIN'});
                input[0] = 99;
                const middle = original.slice(1, 5, 'IMAGE/PNG');
                const end = original.slice(-4, -1);
                const rounded = original.slice(0.5, 3.5);
                const empty = original.slice(5, 2);
                const bytes = await Promise.all([
                    middle.bytes(), end.bytes(), rounded.bytes(), original.bytes()
                ]);
                bytes[0][0] = 0;
                const afterMutation = await middle.bytes();
                let symbol = '';
                try { original.slice(0, 1, Symbol()); }
                catch (error) { symbol = error.name; }
                document.querySelector('output').textContent = [
                    original.size, original.type, middle.size, middle.type,
                    [...bytes[1]], [...bytes[2]], [...bytes[3]],
                    [...afterMutation], empty.size, symbol
                ].join('|');
            })().catch(error => document.querySelector('output').textContent = error.name);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "7|text/plain|4|image/png|20,30,99|97,98,10,20|97,98,10,20,30,99,100|98,10,20,30|0|TypeError"
    );
}

#[test]
fn blob_stream_is_pull_driven_byte_stream_with_bounded_chunks_and_byob() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const input = new Uint8Array(150000);
                for (let i = 0; i < input.length; ++i) input[i] = i % 251;
                const blob = new Blob([input]);
                input[0] = 99;
                const reader = blob.stream().getReader();
                const a = await reader.read(), b = await reader.read();
                const c = await reader.read(), end = await reader.read();
                a.value[0] = 77;
                const immutable = (await blob.bytes())[0];

                const segmented = new Blob([new Uint8Array([1, 2, 3]),
                    new Uint8Array([4, 5, 6])]);
                const byob = segmented.stream().getReader({mode:'byob'});
                const first = await byob.read(new Uint8Array(5));
                const second = await byob.read(new Uint8Array(5));
                const closed = await byob.read(new Uint8Array(1));
                const empty = await new Blob().stream().getReader({mode:'byob'})
                    .read(new Uint8Array(1));
                document.querySelector('output').textContent = [
                    a.value.length, b.value.length, c.value.length, end.done,
                    immutable, first.value.join(','), second.value.join(','),
                    closed.done, empty.done
                ].join('|');
            })().catch(error => document.querySelector('output').textContent = error.name);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "65536|65536|18928|true|0|1,2,3,4,5|6|true|true"
    );
}

#[test]
fn synchronous_file_reader_is_not_exposed_on_window() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            document.querySelector('output').textContent =
                typeof FileReaderSync + '|' + typeof FileReader;
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "undefined|function"
    );
}

#[test]
fn blob_slice_clamps_extreme_offsets_and_preserves_nested_ranges() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const blob = new Blob(['abc', 'def']);
                const read = async value => new TextDecoder().decode(await value.bytes());
                const nested = blob.slice(1, -1).slice(1, -1);
                const cases = await Promise.all([
                    read(blob.slice(-Infinity, Infinity)),
                    read(blob.slice(NaN, 2.5)),
                    read(blob.slice(-2.5)),
                    read(blob.slice(100, 200)),
                    read(nested)
                ]);
                const nonAscii = blob.slice(0, 1, '\u0100').type;
                const nullType = blob.slice(0, 1, null).type;
                document.querySelector('output').textContent =
                    [...cases, nonAscii, nullType].join('|');
            })().catch(error => document.querySelector('output').textContent = error.name);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "abcdef|ab|ef||cd||null"
    );
}

#[test]
fn canceled_blob_stream_does_not_perturb_another_reader_or_its_source() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const bytes = new Uint8Array(70000);
                bytes[0] = 7; bytes[69999] = 9;
                const blob = new Blob([bytes]);
                const first = blob.stream().getReader();
                const peer = blob.stream().getReader({mode:'byob'});
                const initial = await first.read();
                initial.value[0] = 88;
                await first.cancel('not needed');
                const afterCancel = await first.read();
                const peerInitial = await peer.read(new Uint8Array(5));
                await peer.cancel();
                const fresh = blob.stream().getReader();
                const freshInitial = await fresh.read();
                const later = await fresh.read();
                document.querySelector('output').textContent = [
                    initial.value.length, afterCancel.done,
                    peerInitial.value[0], freshInitial.value[0],
                    later.value.length, later.value.at(-1)
                ].join('|');
            })().catch(error => document.querySelector('output').textContent = error.name);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "65536|true|7|7|4464|9"
    );
}
