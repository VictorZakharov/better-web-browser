use super::*;

#[test]
fn encoding_streams_preserve_split_unicode_and_bom_state() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const encoded = [];
                await ReadableStream.from(['A\uD83D', '\uDE00B'])
                    .pipeThrough(new TextEncoderStream())
                    .pipeTo(new WritableStream({ write(bytes) { encoded.push(...bytes); } }));
                const decoder = new TextDecoderStream();
                const decoded = [];
                await ReadableStream.from([
                    new Uint8Array([0xef, 0xbb]),
                    new Uint8Array([0xbf, 0x41, 0xf0, 0x9f]),
                    new Uint8Array([0x98, 0x80, 0x42])
                ]).pipeThrough(decoder)
                    .pipeTo(new WritableStream({ write(text) { decoded.push(text); } }));
                document.querySelector('output').textContent = [
                    encoded.join(','), decoded.join(''),
                    new TextEncoderStream().encoding,
                    decoder.encoding, decoder.fatal, decoder.ignoreBOM
                ].join('|');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "65,240,159,152,128,66|A😀B|utf-8|utf-8|false|false"
    );
}

#[test]
fn stream_strategies_validate_configuration_and_measure_chunks() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const failures = [];
            for (const ctor of [ByteLengthQueuingStrategy, CountQueuingStrategy]) {
                try { new ctor(); } catch (error) { failures.push(error.name); }
                try { new ReadableStream({}, new ctor({ highWaterMark: -1 })); }
                catch (error) { failures.push(error.name); }
            }
            const bytes = new ByteLengthQueuingStrategy({ highWaterMark: 4 });
            const count = new CountQueuingStrategy({ highWaterMark: 2 });
            const stream = new ReadableStream({ start(controller) {
                controller.enqueue(new Uint8Array(3));
            } }, bytes);
            document.querySelector('output').textContent = [
                bytes.highWaterMark, bytes.size(new Uint8Array(3)),
                count.highWaterMark, count.size(null),
                stream.__controller.desiredSize, failures.join(',')
            ].join('|');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "4|3|2|1|1|TypeError,RangeError,TypeError,RangeError"
    );
}

#[test]
fn fatal_decoder_stream_errors_on_invalid_utf8() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                let error = '';
                try {
                    await ReadableStream.from([new Uint8Array([0xc3]), new Uint8Array([0x28])])
                        .pipeThrough(new TextDecoderStream('utf-8', { fatal: true }))
                        .pipeTo(new WritableStream({ write() {} }));
                } catch (reason) { error = reason.name; }
                document.querySelector('output').textContent = error;
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "TypeError"
    );
}
