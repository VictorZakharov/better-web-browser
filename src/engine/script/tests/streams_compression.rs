use super::*;

#[test]
fn compression_streams_round_trip_bytes_in_all_supported_formats() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const text = 'Hello 🌍 from Breeze. '.repeat(10);
                const input = new TextEncoder().encode(text);
                const results = [];
                for (const format of ['gzip', 'deflate', 'deflate-raw']) {
                    const output = [];
                    await ReadableStream.from([
                        input.subarray(0, 9), input.subarray(9, 26), input.subarray(26)
                    ]).pipeThrough(new CompressionStream(format))
                        .pipeThrough(new DecompressionStream(format))
                        .pipeTo(new WritableStream({ write(chunk) { output.push(...chunk); } }));
                    results.push(new TextDecoder().decode(new Uint8Array(output)) === text);
                }
                document.querySelector('output').textContent = results.join(',');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true"
    );
}

#[test]
fn decompression_stream_rejects_truncated_and_corrupt_input() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const compressed = [];
                await ReadableStream.from([new Uint8Array([1, 2, 3, 4])])
                    .pipeThrough(new CompressionStream('gzip'))
                    .pipeTo(new WritableStream({ write(bytes) { compressed.push(...bytes); } }));
                const cases = [new Uint8Array(compressed.slice(0, -1)),
                    new Uint8Array(compressed)];
                cases[1][cases[1].length - 8] ^= 1;
                const observed = [];
                for (const data of cases) {
                    try {
                        await ReadableStream.from([data])
                            .pipeThrough(new DecompressionStream('gzip'))
                            .pipeTo(new WritableStream({ write() {} }));
                    } catch (error) { observed.push(error.name); }
                }
                document.querySelector('output').textContent = observed.join(',');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "TypeError,TypeError"
    );
}

#[test]
fn compression_stream_validates_format_and_buffer_source_chunks() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const errors = [];
                for (const make of [
                    () => new CompressionStream('brotli'),
                    () => new DecompressionStream('unknown')
                ]) {
                    try { make(); } catch (error) { errors.push(error.name); }
                }
                try {
                    await ReadableStream.from(['not bytes'])
                        .pipeThrough(new CompressionStream('gzip'))
                        .pipeTo(new WritableStream({ write() {} }));
                } catch (error) { errors.push(error.name); }
                document.querySelector('output').textContent = errors.join(',');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "TypeError,TypeError,TypeError"
    );
}
