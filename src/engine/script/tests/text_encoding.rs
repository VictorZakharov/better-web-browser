use super::*;

#[test]
fn decoder_accepts_standard_aliases_and_legacy_multibyte_encodings() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const western = new TextDecoder('  latin1\t');
            const sjis = new TextDecoder('shift_jis');
            const first = sjis.decode(new Uint8Array([0x82]), { stream: true });
            const second = sjis.decode(new Uint8Array([0xa0]));
            let unsupported = '';
            try { new TextDecoder('replacement'); } catch (error) { unsupported = error.name; }
            document.querySelector('output').textContent = [
                western.encoding, western.decode(new Uint8Array([0x80])),
                sjis.encoding, first, second, unsupported
            ].join('|');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "windows-1252|€|shift_jis||あ|RangeError"
    );
}

#[test]
fn decoder_handles_utf16_bom_fatal_errors_and_stream_reset() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const decoder = new TextDecoder('utf-16le');
            const prefix = decoder.decode(new Uint8Array([0xff]), {stream:true});
            const text = decoder.decode(new Uint8Array([0xfe, 0x41, 0x00]));
            const withBom = new TextDecoder('utf-8', {ignoreBOM:true})
                .decode(new Uint8Array([0xef, 0xbb, 0xbf, 0x41]));
            const afterReset = decoder.decode(new Uint8Array([0x42, 0x00]));
            let error = '';
            try { new TextDecoder('utf-16le', {fatal:true}).decode(new Uint8Array([0x41])); }
            catch (reason) { error = reason.name; }
            document.querySelector('output').textContent = [prefix, text, withBom.charCodeAt(0),
                withBom.slice(1), afterReset, error].join('|');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "|A|65279|A|B|TypeError"
    );
}

#[test]
fn encoder_never_partially_writes_an_astral_character() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const encoder = new TextEncoder();
            const short = new Uint8Array(3);
            const enough = new Uint8Array(4);
            const first = encoder.encodeInto('😀', short);
            const second = encoder.encodeInto('😀', enough);
            const lone = encoder.encode('\ud800');
            document.querySelector('output').textContent = [first.read, first.written,
                second.read, second.written, [...enough].join(','), [...lone].join(',')].join('|');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "0|0|2|4|240,159,152,128|239,191,189"
    );
}
