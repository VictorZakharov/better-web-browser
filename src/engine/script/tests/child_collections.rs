use super::*;

#[test]
fn insertion_indices_use_native_children_not_author_accessors() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const parent = document.createElement('div');
        const a = document.createTextNode('a'), b = document.createTextNode('b');
        parent.append(a, b);
        const nodes = parent.childNodes;
        const range = document.createRange(); range.setStart(parent, 1); range.setEnd(parent, 2);
        Object.defineProperty(parent, 'childNodes', { get() { throw new Error('author getter'); } });
        const c = document.createTextNode('c'); parent.appendChild(c);
        const checks = [nodes.length === 3, range.startOffset === 1, range.endOffset === 2];
        parent.insertBefore(c, a);
        checks.push(nodes[0] === c, range.startOffset === 2, range.endOffset === 3);
        parent.removeChild(a);
        checks.push(nodes.length === 2, nodes[1] === b, range.startOffset === 1, range.endOffset === 2);
        const other = document.createElement('div'); other.appendChild(b);
        checks.push(nodes.length === 1, nodes[0] === c, range.startOffset === 1, range.endOffset === 1);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn child_node_indexed_properties_follow_readonly_legacy_object_rules() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const parent = document.createElement('div'); parent.append('first', 'second');
        const nodes = parent.childNodes, first = nodes[0];
        const marker = Symbol('marker'); nodes.note = 'ordinary'; nodes[marker] = 1;
        const descriptor = Object.getOwnPropertyDescriptor(nodes, '0');
        const checks = [descriptor.value === first, descriptor.enumerable,
            descriptor.configurable, !descriptor.writable,
            !Reflect.set(nodes, '0', null), !Reflect.set(nodes, '10', null),
            !Reflect.defineProperty(nodes, '0', {value: null}),
            !Reflect.defineProperty(nodes, '10', {value: null}),
            !Reflect.defineProperty(nodes, '0', {get: () => null}),
            !Reflect.deleteProperty(nodes, '0'), Reflect.deleteProperty(nodes, '10'),
            !Reflect.preventExtensions(nodes), Object.isExtensible(nodes),
            nodes[0] === first, nodes[10] === undefined,
            Reflect.ownKeys(nodes).map(k => typeof k === 'symbol' ? 'symbol' : k).join() ===
                '0,1,note,symbol'];
        try { (() => {'use strict'; nodes[0] = null; })(); checks.push(false); }
        catch (error) { checks.push(error instanceof TypeError); }
        nodes['01'] = 'not an index'; nodes[4294967295] = 'not an index either';
        checks.push(nodes['01'] === 'not an index', nodes[4294967295] === 'not an index either');
        parent.replaceChildren();
        checks.push(Reflect.deleteProperty(nodes, '0'), Object.keys(nodes).join() ===
            'note,01,4294967295');
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn retained_child_nodes_are_live_without_reading_the_parent_getter_again() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const parent = document.createElement('div');
        const nodes = parent.childNodes;
        const checks = [nodes instanceof NodeList, !Array.isArray(nodes),
            Object.prototype.toString.call(nodes) === '[object NodeList]',
            nodes === parent.childNodes, nodes.length === 0];
        const a = document.createTextNode('a');
        const b = document.createElement('b');
        parent.appendChild(a);
        checks.push(nodes.length === 1, nodes[0] === a, nodes.item(0) === a);
        parent.insertBefore(b, a);
        checks.push(nodes.length === 2, nodes[0] === b, nodes[1] === a,
            [...nodes].map(n => n.nodeName).join() === 'B,#text');
        parent.removeChild(b);
        checks.push(nodes.length === 1, nodes[0] === a, nodes[1] === undefined,
            nodes.item(1) === null, !('1' in nodes));
        parent.textContent = 'replacement';
        checks.push(nodes.length === 1, nodes[0].data === 'replacement');
        parent.innerHTML = '<i></i><em></em>';
        checks.push(nodes.length === 2, nodes[0].localName === 'i', nodes[1].localName === 'em');
        parent.replaceChildren();
        checks.push(nodes.length === 0, nodes.item(0) === null);
        for (const invoke of [() => new NodeList(), () => nodes.item(),
            () => NodeList.prototype.item.call({}, 0)]) {
            try { invoke(); checks.push(false); } catch (e) { checks.push(e instanceof TypeError); }
        }
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn child_node_iterators_observe_mutations_and_foreach_captures_length() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const parent = document.createElement('div');
        parent.append('a', 'b');
        const nodes = parent.childNodes;
        const iterator = nodes.values();
        const checks = [iterator.next().value.data === 'a'];
        parent.append('c');
        checks.push(iterator.next().value.data === 'b', iterator.next().value.data === 'c',
            iterator.next().done);
        parent.append('d');
        checks.push(iterator.next().done);
        const visited = [];
        const receiver = {};
        nodes.forEach(function (node, index, collection) {
            visited.push(node.data);
            checks.push(this === receiver, collection === nodes, index === visited.length - 1);
            if (index === 0) parent.append('e');
        }, receiver);
        checks.push(visited.join() === 'a,b,c,d', nodes.length === 5,
            [...nodes.keys()].join() === '0,1,2,3,4',
            [...nodes.entries()].every(([index, node]) => node === nodes[index]));
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn moved_subtrees_invalidate_observer_ancestry_lazily() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const left = document.createElement('div');
        const right = document.createElement('div');
        const subtree = document.createElement('section');
        const child = document.createElement('span');
        subtree.appendChild(child); left.appendChild(subtree);
        const a = new MutationObserver(() => {}), b = new MutationObserver(() => {});
        a.observe(left, { attributes: true, subtree: true });
        b.observe(right, { attributes: true, subtree: true });
        child.setAttribute('data-step', 'first');
        const checks = [a.takeRecords().length === 1, b.takeRecords().length === 0];
        right.appendChild(subtree);
        child.setAttribute('data-step', 'second');
        checks.push(a.takeRecords().length === 0, b.takeRecords().length === 1);
        a.disconnect(); b.disconnect();
        left.appendChild(subtree);
        a.observe(left, { attributes: true, subtree: true });
        child.setAttribute('data-step', 'third');
        checks.push(a.takeRecords().length === 1);
        a.disconnect();
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
