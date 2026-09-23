use super::*;

#[test]
fn document_position_uses_tree_order_ancestry_and_stable_disconnected_order() {
    let (dom, outcome) = execute_html(
        r#"<main><section><b></b></section><aside></aside></main><output>no</output><script>
            const root = document.querySelector('main');
            const section = root.firstChild, nested = section.firstChild;
            const aside = root.lastChild;
            const detachedA = document.createElement('a');
            const detachedB = document.createElement('b');
            const p = Node;
            const aToB = detachedA.compareDocumentPosition(detachedB);
            const bToA = detachedB.compareDocumentPosition(detachedA);
            const disconnectedMask = p.DOCUMENT_POSITION_DISCONNECTED |
                p.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
            const checks = [root.compareDocumentPosition(root) === 0,
                root.compareDocumentPosition(nested) ===
                    (p.DOCUMENT_POSITION_FOLLOWING | p.DOCUMENT_POSITION_CONTAINED_BY),
                nested.compareDocumentPosition(root) ===
                    (p.DOCUMENT_POSITION_PRECEDING | p.DOCUMENT_POSITION_CONTAINS),
                section.compareDocumentPosition(aside) === p.DOCUMENT_POSITION_FOLLOWING,
                aside.compareDocumentPosition(section) === p.DOCUMENT_POSITION_PRECEDING,
                (aToB & disconnectedMask) === disconnectedMask,
                (bToA & disconnectedMask) === disconnectedMask,
                !!(aToB & p.DOCUMENT_POSITION_PRECEDING) !==
                    !!(bToA & p.DOCUMENT_POSITION_PRECEDING),
                p.prototype.DOCUMENT_POSITION_FOLLOWING === 4];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn document_position_handles_attributes_as_owner_adjacent_not_child_nodes() {
    let (dom, outcome) = execute_html(
        r#"<main id=x title=y><span></span></main><output>no</output><script>
            const root = document.querySelector('main');
            const id = root.getAttributeNode('id');
            const title = root.getAttributeNode('title');
            const child = root.firstChild;
            const p = Node;
            const checks = [root.compareDocumentPosition(id) === 20,
                id.compareDocumentPosition(root) === 10,
                id.compareDocumentPosition(child) === p.DOCUMENT_POSITION_FOLLOWING,
                child.compareDocumentPosition(id) === p.DOCUMENT_POSITION_PRECEDING,
                id.compareDocumentPosition(title) ===
                    (p.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC | p.DOCUMENT_POSITION_FOLLOWING),
                title.compareDocumentPosition(id) ===
                    (p.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC | p.DOCUMENT_POSITION_PRECEDING)];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn namespace_lookup_follows_declarations_and_default_namespace_shadowing() {
    let (dom, outcome) = execute_html(
        r#"<output>no</output><script>
            const ns = 'http://www.w3.org/2000/xmlns/';
            const outer = document.createElementNS('urn:root', 'r:root');
            outer.setAttributeNS(ns, 'xmlns:p', 'urn:paint');
            outer.setAttributeNS(ns, 'xmlns', 'urn:default');
            const inner = document.createElementNS('urn:inner', 'i:child');
            const text = document.createTextNode('text');
            outer.appendChild(inner); inner.appendChild(text);
            const checks = [outer.lookupPrefix('urn:root') === 'r',
                text.lookupPrefix('urn:paint') === 'p',
                text.lookupNamespaceURI('p') === 'urn:paint',
                text.lookupNamespaceURI('i') === 'urn:inner',
                text.lookupNamespaceURI('xml') === 'http://www.w3.org/XML/1998/namespace',
                text.lookupNamespaceURI(null) === 'urn:default',
                text.isDefaultNamespace('urn:default'),
                !text.isDefaultNamespace('urn:paint')];
            inner.setAttributeNS(ns, 'xmlns', '');
            checks.push(text.lookupNamespaceURI(null) === null,
                text.isDefaultNamespace(null), text.lookupPrefix('') === null);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn normalize_merges_exclusive_text_and_preserves_live_range_positions() {
    let (dom, outcome) = execute_html(
        r#"<main></main><output>no</output><script>
            const root = document.querySelector('main');
            const a = document.createTextNode('ab');
            const b = document.createTextNode('cd');
            const empty = document.createTextNode('');
            const c = document.createTextNode('ef');
            const element = document.createElement('span');
            element.appendChild(document.createTextNode('x'));
            element.appendChild(document.createTextNode('y'));
            root.append(a, b, empty, c, element);
            const within = document.createRange();
            within.setStart(b, 1); within.setEnd(c, 1);
            const between = document.createRange();
            between.setStart(root, 1); between.collapse(true);
            root.normalize();
            const checks = [root.childNodes.length === 2, root.firstChild === a,
                a.data === 'abcdef', element.firstChild.data === 'xy',
                element.childNodes.length === 1,
                within.startContainer === a, within.startOffset === 3,
                within.endContainer === a, within.endOffset === 5,
                between.startContainer === a, between.startOffset === 2,
                between.endContainer === a, between.endOffset === 2];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn normalize_removes_empty_text_but_never_merges_across_comments() {
    let (dom, outcome) = execute_html(
        r#"<main></main><output>no</output><script>
            const root = document.querySelector('main');
            const empty = document.createTextNode('');
            const first = document.createTextNode('a');
            const comment = document.createComment('boundary');
            const second = document.createTextNode('b');
            const third = document.createTextNode('c');
            root.append(empty, first, comment, second, third);
            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_TEXT);
            root.normalize();
            const checks = [empty.parentNode === null, root.childNodes.length === 3,
                root.firstChild === first, first.data === 'a',
                root.childNodes[1] === comment, second.data === 'bc',
                third.parentNode === null,
                iterator.nextNode() === first, iterator.nextNode() === second,
                iterator.nextNode() === null];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
