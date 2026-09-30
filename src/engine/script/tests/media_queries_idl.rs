use super::*;

#[test]
fn media_queries_require_arguments_and_apply_domstring_conversion_once() {
    let (dom, outcome) = execute_html(
        r#"<body><audio></audio><output></output><script>
            const audio = document.querySelector('audio');
            const assert = (condition, message) => { if (!condition) throw Error(message); };
            const typeError = operation => {
                try { operation(); } catch (error) { return error.name === 'TypeError'; }
                return false;
            };
            assert(typeError(() => audio.canPlayType()), 'required playback argument');
            assert(typeError(() => MediaSource.isTypeSupported()), 'required MSE argument');
            for (const value of [Symbol('mime'), Object(Symbol('mime'))]) {
                assert(typeError(() => audio.canPlayType(value)), 'playback Symbol conversion');
                assert(typeError(() => MediaSource.isTypeSupported(value)), 'MSE Symbol conversion');
            }
            for (const value of [undefined, null, 1n]) {
                assert(audio.canPlayType(value) === '', 'explicit unsupported DOMString');
                assert(!MediaSource.isTypeSupported(value), 'explicit unsupported MSE DOMString');
            }
            let conversions = 0;
            const value = {
                [Symbol.toPrimitive](hint) {
                    assert(hint === 'string', 'ToString hint');
                    conversions++;
                    return 'audio/mp4;codecs=mp4a.40.2';
                }
            };
            assert(audio.canPlayType(value) === 'probably' && conversions === 1, 'one playback conversion');
            assert(MediaSource.isTypeSupported(value) && conversions === 2, 'one MSE conversion');
            const sentinel = {};
            try { audio.canPlayType({ toString() { throw sentinel; } }); }
            catch (error) { assert(error === sentinel, 'conversion exception identity'); conversions++; }
            assert(conversions === 3, 'throwing conversion must propagate');
            const method = HTMLMediaElement.prototype.canPlayType;
            for (const receiver of [null, {}, Object.create(HTMLMediaElement.prototype),
                document.createElement('div')]) {
                assert(typeError(() => method.call(receiver, 'audio/webm')), 'private media brand');
            }
            Object.setPrototypeOf(audio, null);
            assert(method.call(audio, 'audio/webm') === 'maybe', 'brand survives prototype change');
            document.querySelector('output').textContent = 'passed';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn sourcebuffer_type_arguments_are_converted_before_state_algorithms_and_only_once() {
    let (dom, _runtime, outcome) = super::media_source_segments::execute_media_source(
        r#"<video></video><output>pending</output><script>
            const source = new MediaSource();
            const assert = (condition, message) => { if (!condition) throw Error(message); };
            const typeError = operation => {
                try { operation(); } catch (error) { return error.name === 'TypeError'; }
                return false;
            };
            assert(typeError(() => source.addSourceBuffer()), 'required SourceBuffer type');
            assert(typeError(() => source.addSourceBuffer(Symbol())), 'conversion precedes closed state');
            source.addEventListener('sourceopen', () => {
                let count = 0;
                const type = { toString() {
                    count++;
                    return count === 1 ? 'audio/mp4;codecs=mp4a.40.2' : 'unsupported';
                }};
                const buffer = source.addSourceBuffer(type);
                assert(count === 1 && source.sourceBuffers.length === 1, 'one add conversion');
                assert(typeError(() => buffer.changeType()), 'required changeType argument');
                assert(typeError(() => buffer.changeType(Symbol())), 'changeType Symbol conversion');
                count = 0;
                buffer.changeType(type);
                assert(count === 1, 'one change conversion');
                document.querySelector('output').textContent = 'passed';
            });
            document.querySelector('video').src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}
