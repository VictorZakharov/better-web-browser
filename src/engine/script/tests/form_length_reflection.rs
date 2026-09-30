use super::*;

#[test]
fn input_and_textarea_lengths_parse_nonnegative_integer_prefixes() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            for (const tag of ['input', 'textarea']) {
                const element = document.createElement(tag);
                assert(element.minLength === -1 && element.maxLength === -1, 'missing length: ' + tag);
                const cases = [
                    ['', -1], [' ', -1], ['invalid', -1], ['Infinity', -1], ['-1', -1],
                    ['2147483648', -1], ['9999999999999999999999999999999', -1],
                    ['\u00a02', -1], ['\t\n\f\r +12suffix', 12], ['2e2', 2],
                    ['3.9', 3], ['0x10', 0], ['-0', 0], ['2147483647', 2147483647]
                ];
                for (const [text, expected] of cases) {
                    element.setAttribute('minlength', text);
                    element.setAttribute('maxlength', text);
                    assert(Object.is(element.minLength, expected) && Object.is(element.maxLength, expected),
                        'content parsing: ' + tag + ':' + text);
                    assert(element.getAttribute('maxlength') === text, 'getter must not rewrite attributes');
                }
                element.removeAttribute('minlength');
                element.removeAttribute('maxlength');
                assert(element.minLength === -1 && element.maxLength === -1, 'removal defaults');
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
fn length_idl_setters_convert_signed_long_before_enforcing_nonnegative() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            for (const tag of ['input', 'textarea']) {
                const element = document.createElement(tag);
                for (const property of ['minLength', 'maxLength']) {
                    const attribute = property.toLowerCase();
                    const cases = [[3.9, 3], ['0x10', 16], [null, 0], [undefined, 0],
                        [NaN, 0], [Infinity, 0], [-Infinity, 0], [-.9, 0], [true, 1],
                        [4294967296, 0], [4294967297, 1], [-4294967295, 1]];
                    for (const [value, expected] of cases) {
                        element[property] = value;
                        assert(element[property] === expected && element.getAttribute(attribute) === String(expected),
                            'signed-long conversion: ' + tag + ':' + property);
                    }
                    element[property] = 7;
                    for (const value of [-1, -1.9, 2147483648, 4294967295]) {
                        let error = null;
                        try { element[property] = value; } catch (caught) { error = caught; }
                        assert(error instanceof DOMException && error.name === 'IndexSizeError', 'negative rejection');
                        assert(element.getAttribute(attribute) === '7', 'negative must not mutate attribute');
                    }
                    for (const value of [1n, Symbol('length'), { valueOf() { return 1n; } }]) {
                        let error = null;
                        try { element[property] = value; } catch (caught) { error = caught; }
                        assert(error instanceof TypeError, 'invalid numeric coercion');
                        assert(element.getAttribute(attribute) === '7', 'coercion failure must not mutate');
                    }
                    let calls = 0;
                    element[property] = { [Symbol.toPrimitive](hint) {
                        assert(hint === 'number', 'numeric coercion hint'); calls++; return 2.9;
                    } };
                    assert(calls === 1 && element[property] === 2, 'one coercion then truncation');
                }
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
fn reflected_lengths_bypass_author_methods_and_deliver_mutations() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            const records = [];
            const observer = new MutationObserver(batch => records.push(...batch));
            for (const [tag, prototype] of [['input', HTMLInputElement.prototype],
                ['textarea', HTMLTextAreaElement.prototype]]) {
                const element = document.createElement(tag);
                observer.observe(element, { attributes: true, attributeOldValue: true });
                element.getAttribute = element.setAttribute = () => { throw new Error('author attribute method'); };
                for (const property of ['minLength', 'maxLength']) {
                    const descriptor = Object.getOwnPropertyDescriptor(prototype, property);
                    Object.defineProperty(element, property, { get() { throw new Error('author length getter'); } });
                    descriptor.set.call(element, property === 'minLength' ? 2 : 7);
                    assert(descriptor.get.call(element) === (property === 'minLength' ? 2 : 7),
                        'internal numeric reflection');
                }
                Object.getOwnPropertyDescriptor(prototype, 'maxLength').set.call(element, 9);
            }
            queueMicrotask(() => {
                assert(records.length === 6 && records.every(record => record.attributeNamespace === null),
                    'six reflected attribute mutations');
                assert(JSON.stringify(records.map(record => [record.attributeName, record.oldValue])) ===
                    JSON.stringify([['minlength', null], ['maxlength', null], ['maxlength', '7'],
                        ['minlength', null], ['maxlength', null], ['maxlength', '7']]), 'ordered old values');
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

#[test]
fn reflected_lengths_preserve_script_values_and_utf16_selection_snapshots() {
    let (dom, outcome) = execute_html(
        r#"<body><input><textarea></textarea><output></output><script>
            const assert = (condition, message) => { if (!condition) throw new Error(message); };
            for (const element of [document.querySelector('input'), document.querySelector('textarea')]) {
                element.setAttribute('maxlength', '3tail');
                assert(element.maxLength === 3, 'integer-prefix reflection');
                element.value = 'A💡BC';
                element.setRangeText('XYZ', 1, 3, 'select');
                assert(element.value === 'AXYZBC', 'maxlength must not restrict script writes');
                assert(element.selectionStart === 1 && element.selectionEnd === 4,
                    'replacement retains UTF-16 selection');
                element.maxLength = 2;
                assert(element.value === 'AXYZBC' && element.selectionStart === 1 && element.selectionEnd === 4,
                    'length reflection must not change value or selection');
                element.setAttribute = () => { throw Error('author attribute method'); };
                element.minLength = 9;
                assert(element.minLength === 9 && element.value === 'AXYZBC', 'internal reflection');
            }
            document.querySelector('output').textContent = 'passed';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
    assert_eq!(outcome.selection_actions.len(), 2);
    for tag in ["input", "textarea"] {
        let node = dom.elements_named(tag).next().unwrap();
        let action = outcome
            .selection_actions
            .iter()
            .find(|action| action.node == node.id())
            .expect("the script's value and range must still mirror to the native control");
        assert_eq!(action.value, "AXYZBC");
        assert_eq!((action.selection_start, action.selection_end), (1, 4));
    }
}

#[test]
fn numeric_length_validity_and_native_selection_share_the_committed_edit() {
    use super::form_user_edits::{native_edit_at, run};
    use crate::renderer_protocol::TextSelectionDirection;

    let (dom, mut runtime) = run(
        r#"<input maxlength='3tail' value='A💡B'><output></output><script>
            const field = document.querySelector('input');
            field.addEventListener('beforeinput', () => {
                if (field.maxLength !== 3 || field.selectionStart !== 4)
                    throw Error('numeric reflection and pre-edit selection');
                field.setSelectionRange(4, 4);
            });
            field.addEventListener('input', () => {
                if (field.value !== 'A💡BX' || !field.validity.tooLong)
                    throw Error('native length validation must use the new UTF-16 value');
                field.setSelectionRange(1, 3, 'backward');
                document.querySelector('output').textContent = 'passed';
            });
        </script>"#,
    );
    let field = dom.elements_named("input").next().unwrap();
    let result = native_edit_at(
        &mut runtime,
        field.clone(),
        "A💡BX",
        "insertText",
        Some((4, 4)),
        (5, 5),
    );
    assert!(result.default_allowed);
    assert!(result.rejected_text.is_none());
    assert_eq!(field.input_value(), "A💡BX");
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
    assert_eq!(result.outcome.selection_actions.len(), 1);
    let selection = &result.outcome.selection_actions[0];
    assert_eq!(selection.node, field.id());
    assert_eq!(
        selection.value, "A💡BX",
        "the old beforeinput snapshot must be retired"
    );
    assert_eq!((selection.selection_start, selection.selection_end), (1, 3));
    assert_eq!(selection.direction, TextSelectionDirection::Backward);
}
