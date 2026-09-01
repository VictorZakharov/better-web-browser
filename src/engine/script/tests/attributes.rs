use super::*;
use crate::engine::RectF;

#[test]
fn html_element_inner_text_has_a_string_contract() {
    let dom = crate::engine::dom::parse_with_scripting(
        r#"<body><div id="target"><span>alpha</span> beta</div><script>
            const target = document.getElementById('target');
            const initial = target.innerText;
            target.innerText = null;
            document.body.setAttribute('data-result', [
                typeof initial, initial, target.innerText, target.childElementCount
            ].join('|'));
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");

    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#inline".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("string|alpha beta||0")
    );
}

#[test]
fn repeated_attribute_mutations_do_not_serialize_attribute_collections() {
    let dom = crate::engine::dom::parse_with_scripting(
        r#"<body><div id="target"></div><script>
            const target = document.getElementById('target');
            for (let i = 0; i < 256; i++) {
                target.setAttribute('data-state', String(i));
                target.removeAttribute('data-state');
            }
        </script></body>"#,
        true,
    );
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_host_call_profiling(true);

    let outcome = runtime.execute_initial(&scripts);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|line| !line.starts_with("host call attrRecords:")),
        "setAttribute serialized the attribute collection: {:?}",
        outcome.diagnostics
    );
}

#[test]
fn cssom_view_geometry_uses_the_latest_layout_snapshot() {
    let dom = crate::engine::dom::parse_with_scripting(
        r#"<body><div id="target"></div><script>
            const target = document.getElementById('target');
            const rect = target.getBoundingClientRect();
            document.body.setAttribute('data-result',
                [target.clientWidth, target.clientHeight, target.offsetWidth,
                 target.offsetHeight, rect.x, rect.y, rect.right, rect.bottom].join(','));
        </script></body>"#,
        true,
    );
    let target = dom.elements_named("div").next().unwrap();
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_geometry(&HashMap::from([(
        target.id(),
        RectF {
            x: 12.5,
            y: 24.25,
            width: 320.0,
            height: 180.0,
        },
    )]));

    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#inline".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("320,180,320,180,12.5,24.25,332.5,204.25")
    );
}

#[test]
fn resize_observer_delivers_after_a_layout_snapshot_changes() {
    let dom = crate::engine::dom::parse_with_scripting(
        r#"<body><div id="target"></div><script>
            const target = document.getElementById('target');
            new ResizeObserver(entries => {
                document.body.setAttribute('data-width', String(entries[0].contentRect.width));
            }).observe(target);
        </script></body>"#,
        true,
    );
    let target = dom.elements_named("div").next().unwrap();
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#inline".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);

    runtime.set_layout_geometry(&HashMap::from([(
        target.id(),
        RectF {
            x: 0.0,
            y: 0.0,
            width: 640.0,
            height: 360.0,
        },
    )]));
    runtime.notify_layout_changed();
    let outcome = runtime.advance_time(Duration::from_millis(1), 8);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-width")
            .as_deref(),
        Some("640")
    );
}

#[test]
fn named_node_map_is_live_and_preserves_attribute_identity() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="target" data-state="one"></div><script>
            const check = (condition, message) => { if (!condition) throw new Error(message); };
            const element = document.getElementById('target');
            const attributes = element.attributes;
            const state = attributes.getNamedItem('DATA-STATE');

            check(attributes === element.attributes, 'attributes must be [SameObject]');
            check(attributes['data-state'] === state, 'named property access must return the Attr');
            check(Array.from({ length: attributes.length }, (_, index) => attributes[index]).includes(state),
                'indexed property access must expose the live attribute list');
            check(Object.getOwnPropertyNames(attributes).includes('data-state'),
                'named attributes must appear as unenumerable own properties');
            check(!Object.keys(attributes).includes('data-state'),
                'legacy named properties must not be enumerable');
            check(state instanceof Attr && state instanceof Node, 'Attr must inherit from Node');
            check(state.nodeType === 2 && state.nodeName === 'data-state', 'Attr node metadata is incorrect');
            check(state.ownerElement === element && state.ownerDocument === document, 'Attr ownership is incorrect');

            state.expando = 'jquery-support-probe';
            state.value = 'two';
            check(element.getAttribute('data-state') === 'two', 'Attr.value did not update the element');
            element.setAttribute('data-state', 'three');
            check(attributes.getNamedItem('data-state') === state, 'value changes replaced Attr identity');
            check(attributes['data-state'].expando === 'jquery-support-probe',
                'library expando state was lost from the stable Attr wrapper');
            check(state.nodeValue === 'three' && state.textContent === 'three', 'Attr value aliases are stale');

            element.removeAttribute('data-state');
            check(state.ownerElement === null && state.value === 'three', 'removed Attr did not detach');
            check(attributes.getNamedItem('data-state') === null, 'live collection retained removed Attr');
            element.setAttribute('data-state', 'four');
            check(attributes.getNamedItem('data-state') !== state, 're-added attribute reused removed identity');
            document.body.setAttribute('data-result', 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("pass")
    );
}

#[test]
fn attribute_node_mutation_methods_follow_dom_ownership_rules() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="first"></div><div id="second"></div><script>
            const check = (condition, message) => { if (!condition) throw new Error(message); };
            const first = document.getElementById('first');
            const second = document.getElementById('second');
            const attribute = document.createAttribute('DATA-PROBE');
            attribute.value = 'one';

            check(attribute.name === 'data-probe', 'HTML createAttribute must ASCII-lowercase');
            check(first.setAttributeNode(attribute) === null, 'initial attachment replaced an attribute');
            check(first.attributes['data-probe'] === attribute, 'attached identity was not preserved');
            let inUse = false;
            try { second.setAttributeNode(attribute); }
            catch (error) { inUse = error.name === 'InUseAttributeError'; }
            check(inUse, 'attaching an in-use Attr must throw InUseAttributeError');

            const replacement = document.createAttribute('data-probe');
            replacement.value = 'two';
            const replaced = first.attributes.setNamedItem(replacement);
            check(replaced === attribute && replaced.ownerElement === null, 'replacement did not detach old Attr');
            check(first.getAttributeNode('data-probe') === replacement, 'replacement identity is not live');
            const removed = first.attributes.removeNamedItem('DATA-PROBE');
            check(removed === replacement && removed.ownerElement === null, 'removeNamedItem returned the wrong Attr');

            let notFound = false;
            try { first.attributes.removeNamedItem('data-probe'); }
            catch (error) { notFound = error.name === 'NotFoundError'; }
            check(notFound, 'removing a missing Attr must throw NotFoundError');
            check(replacement.cloneNode() instanceof Attr && replacement.cloneNode().ownerElement === null,
                'cloned Attr must be detached');
            let illegalConstructor = false;
            try { new Attr(); } catch (error) { illegalConstructor = error instanceof TypeError; }
            check(illegalConstructor, 'Attr must not be directly constructible');
            document.body.setAttribute('data-result', 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("pass")
    );
}

#[test]
fn namespaced_attributes_are_live_and_prefix_stable() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="target"></div><script>
            const check = (condition, message) => { if (!condition) throw new Error(message); };
            const element = document.getElementById('target');
            const attribute = document.createAttributeNS('urn:example', 'x:state');
            attribute.value = 'one';
            element.attributes.setNamedItemNS(attribute);

            check(element.getAttributeNS('urn:example', 'state') === 'one', 'namespace lookup failed');
            check(element.attributes.getNamedItemNS('urn:example', 'state') === attribute,
                'namespace lookup lost Attr identity');
            check(element.attributes['x:state'] === attribute, 'qualified named property lookup failed');
            check(attribute.prefix === 'x' && attribute.localName === 'state', 'namespace metadata is incorrect');

            element.setAttributeNS('urn:example', 'y:state', 'two');
            check(attribute.value === 'two', 'setAttributeNS did not update the existing Attr');
            check(attribute.name === 'x:state', 'setAttributeNS incorrectly replaced the existing prefix');
            element.removeAttributeNS('urn:example', 'state');
            check(attribute.ownerElement === null && attribute.value === 'two', 'namespace removal did not detach Attr');

            let namespaceError = false;
            try { document.createAttributeNS(null, 'x:state'); }
            catch (error) { namespaceError = error.name === 'NamespaceError'; }
            check(namespaceError, 'a prefix without a namespace must throw NamespaceError');
            document.body.setAttribute('data-result', 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("pass")
    );
}

#[test]
fn attribute_mutations_queue_filtered_records_with_old_values() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="target"></div><script>
            const target = document.getElementById('target');
            const seen = [];
            new MutationObserver(records => seen.push(...records)).observe(target, {
                attributes: true,
                attributeOldValue: true,
                attributeFilter: ['data-state']
            });
            target.setAttribute('data-state', 'one');
            target.getAttributeNode('data-state').value = 'two';
            const replacement = document.createAttribute('data-state');
            replacement.value = 'three';
            target.setAttributeNode(replacement);
            target.removeAttributeNode(replacement);
            target.setAttribute('data-ignored', 'ignored');
            queueMicrotask(() => {
                const valid = seen.length === 4 &&
                    seen.every(record => record.target === target && record.attributeName === 'data-state' &&
                        record.attributeNamespace === null) &&
                    JSON.stringify(seen.map(record => record.oldValue)) === JSON.stringify([null, 'one', 'two', 'three']);
                document.body.setAttribute('data-result', valid ? 'pass' : 'fail:' + JSON.stringify(seen));
            });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("pass")
    );
}
