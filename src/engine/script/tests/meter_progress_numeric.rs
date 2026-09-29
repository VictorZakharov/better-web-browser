use super::*;

#[test]
fn meter_defaults_and_boundaries_follow_attribute_changes() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><meter id="meter"></meter><script>
            const meter = document.getElementById('meter');
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            assert(meter.min === 0 && meter.max === 1 && meter.value === 0 &&
                meter.low === 0 && meter.high === 1 && meter.optimum === .5, 'missing defaults');
            for (const attribute of ['min', 'max', 'value', 'low', 'high', 'optimum'])
                meter.setAttribute(attribute, '');
            assert(meter.min === 0 && meter.max === 1 && meter.value === 0 &&
                meter.low === 0 && meter.high === 1 && meter.optimum === .5, 'empty defaults');
            meter.min = 10;
            meter.max = 20;
            meter.value = 30;
            meter.low = 40;
            meter.high = -1;
            meter.optimum = -5;
            assert(meter.value === 20 && meter.low === 20 && meter.high === 20 &&
                meter.optimum === 10, 'ordered clamping');
            assert(meter.getAttribute('value') === '30' && meter.getAttribute('high') === '-1',
                'clamping must not rewrite content attributes');
            meter.removeAttribute('low');
            meter.removeAttribute('high');
            meter.removeAttribute('optimum');
            assert(meter.low === 10 && meter.high === 20 && meter.optimum === 15, 'live defaults');
            meter.max = 5;
            assert(meter.max === 10 && meter.value === 10 && meter.high === 10 &&
                meter.optimum === 10, 'inverted bounds');
            meter.min = 1e308;
            meter.max = 1.5e308;
            assert(meter.optimum === 1.25e308, 'finite midpoint must not overflow');
            document.getElementById('status').textContent = 'passed';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn meter_and_progress_parse_html_floating_prefixes() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const meter = document.createElement('meter');
            const progress = document.createElement('progress');
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            const cases = [
                ['\t\n\f\r +12.5e1suffix', 125], ['-.5ignored', -.5],
                ['2e+', 2], ['3E-', 3], ['4.e2rest', 400], ['0x10', 0],
                ['-0', 0], ['', 0], [' ', 0], ['.', 0], ['+', 0],
                ['NaN', 0], ['Infinity', 0], ['1e9999', 0], ['\u00a02', 0]
            ];
            for (const [text, expected] of cases) {
                meter.setAttribute('min', text);
                assert(Object.is(meter.min, expected), 'minimum parsing: ' + text);
            }
            for (const text of ['', ' ', 'NaN', 'Infinity', '1e9999', '\u00a02']) {
                meter.setAttribute('max', text);
                assert(meter.max === 1, 'maximum fallback: ' + text);
            }
            progress.setAttribute('max', '8suffix');
            progress.setAttribute('value', '2e+');
            assert(progress.max === 8 && progress.value === 2 && progress.position === .25,
                'progress prefix parsing');
            progress.setAttribute('value', '0x10');
            assert(progress.value === 0 && progress.position === 0, 'progress hex prefix');
            progress.setAttribute('max', '0x10');
            assert(progress.max === 1, 'nonpositive parsed maximum');
            document.getElementById('status').textContent = 'passed';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn progress_distinguishes_presence_and_ignores_nonpositive_idl_maximum() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const progress = document.createElement('progress');
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            assert(progress.value === 0 && progress.max === 1 && progress.position === -1,
                'indeterminate defaults');
            progress.value = progress.value;
            assert(progress.hasAttribute('value') && progress.position === 0, 'IDL determinate transition');
            progress.max = 10;
            for (const value of [0, -1, null, false, '']) {
                progress.max = value;
                assert(progress.max === 10 && progress.getAttribute('max') === '10',
                    'nonpositive assignment must leave attribute unchanged');
            }
            progress.value = 30;
            assert(progress.value === 10 && progress.position === 1, 'upper clamp');
            progress.setAttribute('max', '-1');
            assert(progress.max === 1 && progress.value === 1, 'content maximum fallback');
            progress.setAttribute('value', 'invalid');
            assert(progress.value === 0 && progress.position === 0, 'invalid but present remains determinate');
            progress.removeAttribute('value');
            assert(progress.position === -1, 'attribute removal becomes indeterminate');
            document.getElementById('status').textContent = 'passed';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn numeric_idl_setters_convert_double_without_partial_attribute_mutation() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            const controls = [
                [document.createElement('meter'), ['min', 'max', 'value', 'low', 'high', 'optimum']],
                [document.createElement('progress'), ['value', 'max']]
            ];
            for (const [element, attributes] of controls) for (const attribute of attributes) {
                element[attribute] = 7;
                for (const invalid of [NaN, Infinity, -Infinity, undefined, 1n, Symbol('number'),
                    { valueOf() { return 1n; } }]) {
                    let error = null;
                    try { element[attribute] = invalid; } catch (caught) { error = caught; }
                    assert(error instanceof TypeError, 'must reject non-double: ' + attribute);
                    assert(element.getAttribute(attribute) === '7', 'failure must not mutate attribute');
                }
                let calls = 0;
                element[attribute] = { [Symbol.toPrimitive](hint) {
                    assert(hint === 'number', 'conversion hint'); calls++; return '2.5';
                } };
                assert(calls === 1 && element.getAttribute(attribute) === '2.5', 'single numeric coercion');
                element[attribute] = true;
                assert(element.getAttribute(attribute) === '1', 'boolean numeric coercion');
                element[attribute] = '0x10';
                assert(element.getAttribute(attribute) === '16', 'IDL coercion is not content parsing');
            }
            document.getElementById('status').textContent = 'passed';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn numeric_reflection_uses_internal_attributes_and_preserves_mutation_records() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            const meter = document.createElement('meter');
            const progress = document.createElement('progress');
            const records = [];
            const observer = new MutationObserver(batch => records.push(...batch));
            for (const element of [meter, progress]) {
                observer.observe(element, { attributes: true, attributeOldValue: true });
                element.setAttribute = () => { throw new Error('author setAttribute'); };
            }
            meter.min = 10;
            meter.max = 30;
            meter.value = 25;
            meter.low = 15;
            meter.high = 20;
            progress.max = 10;
            progress.value = 4;
            for (const element of [meter, progress]) {
                for (const name of ['min', 'max', 'low', 'value'])
                    Object.defineProperty(element, name, { get() { throw new Error('author getter: ' + name); } });
                element.getAttribute = () => { throw new Error('author getAttribute'); };
                element.hasAttribute = () => { throw new Error('author hasAttribute'); };
            }
            const read = (prototype, name, element) =>
                Object.getOwnPropertyDescriptor(prototype, name).get.call(element);
            assert(read(HTMLMeterElement.prototype, 'min', meter) === 10, 'internal minimum');
            assert(read(HTMLMeterElement.prototype, 'max', meter) === 30, 'internal maximum');
            assert(read(HTMLMeterElement.prototype, 'value', meter) === 25, 'internal value');
            assert(read(HTMLMeterElement.prototype, 'low', meter) === 15, 'internal low');
            assert(read(HTMLMeterElement.prototype, 'high', meter) === 20, 'internal high');
            assert(read(HTMLMeterElement.prototype, 'optimum', meter) === 20, 'internal midpoint');
            assert(read(HTMLProgressElement.prototype, 'value', progress) === 4, 'internal progress value');
            assert(read(HTMLProgressElement.prototype, 'max', progress) === 10, 'internal progress maximum');
            assert(read(HTMLProgressElement.prototype, 'position', progress) === .4, 'internal position');
            queueMicrotask(() => {
                assert(records.length === 7, 'reflected mutations must be delivered');
                assert(records.every(record => record.attributeNamespace === null && record.oldValue === null),
                    'reflected records preserve namespace and old value');
                assert(JSON.stringify(records.map(record => record.attributeName)) ===
                    JSON.stringify(['min', 'max', 'value', 'low', 'high', 'max', 'value']),
                    'reflected mutations preserve setter order');
                document.getElementById('status').textContent = 'passed';
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "passed"
    );
}
