use super::*;

#[test]
fn part_is_a_live_same_object_token_list() {
    let (dom, outcome) = execute_html(
        r#"<body><output>no</output><script>
        const element = document.createElement('span');
        const list = element.part;
        const checks = [];
        const check = (name, condition) => checks.push(condition ? '' : name);
        check('identity', list === element.part && list instanceof DOMTokenList);
        check('initial', list.length === 0 && list.value === '');
        element.setAttribute('part', 'alpha\tbeta alpha');
        check('attribute-live', [...list].join() === 'alpha,beta' && list.length === 2);
        list.add('gamma', 'alpha');
        check('add', element.getAttribute('part') === 'alpha beta gamma');
        list.remove('beta');
        check('remove', element.getAttribute('part') === 'alpha gamma');
        check('toggle-boolean-conversion', list.toggle('gamma', 0) === false &&
            !list.contains('gamma') && list.toggle('delta', 1) === true && list.contains('delta'));
        list.replace('alpha', 'renamed');
        check('replace', element.getAttribute('part') === 'renamed delta');
        element.part = 'one   two';
        check('put-forwards', element.part === list && list.value === 'one   two' &&
            [...list].join() === 'one,two');
        check('forced-present-no-update', list.toggle('one', true) === true &&
            list.value === 'one   two');
        check('explicit-undefined-force', list.toggle('one', undefined) === false &&
            list.value === 'two');
        element.removeAttribute('part');
        check('remove-attribute-live', list.length === 0 && list.value === '');
        document.querySelector('output').textContent = checks.filter(Boolean).join(',') || 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn part_token_validation_is_atomic_and_parser_clone_attributes_round_trip() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="parsed" part="first second first" exportparts="inner: outer"></div>
        <output>no</output><script>
        const element = document.querySelector('#parsed');
        const list = element.part;
        const checks = [];
        const check = (name, condition) => checks.push(condition ? '' : name);
        check('parsed', [...list].join() === 'first,second');
        const before = element.getAttribute('part');
        for (const [operation, expected] of [
            [() => list.add('third', ''), 'SyntaxError'],
            [() => list.remove('first', 'bad token'), 'InvalidCharacterError'],
            [() => list.toggle('bad\ttoken'), 'InvalidCharacterError'],
            [() => list.replace('first', ''), 'SyntaxError'],
        ]) {
            let actual = 'no throw';
            try { operation(); } catch (error) { actual = error.name; }
            check('validation-' + expected, actual === expected && element.getAttribute('part') === before);
        }
        const clone = element.cloneNode(true);
        check('clone', clone.part !== list && [...clone.part].join() === 'first,second' &&
            clone.getAttribute('part') === before &&
            clone.getAttribute('exportparts') === 'inner: outer');
        clone.part.add('clone-only');
        check('clone-independent', !list.contains('clone-only') && clone.part.contains('clone-only'));
        check('no-unstandardized-export-parts-idl', !('exportParts' in element));
        document.querySelector('output').textContent = checks.filter(Boolean).join(',') || 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
