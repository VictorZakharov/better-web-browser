use super::*;

#[test]
fn indexed_collection_iterators_are_live_array_values() {
    let (dom, outcome) = execute_html(
        r#"<body><output>no</output><script>
        const checks = [];
        for (const kind of ['attributes', 'children', 'tokens']) {
            const element = document.createElement('div');
            const set = names => {
                if (kind === 'attributes') {
                    for (const attribute of Array.from(element.attributes)) element.removeAttributeNode(attribute);
                    for (const name of names) element.setAttribute(name, name);
                } else if (kind === 'children') {
                    element.replaceChildren(...names.map(name => {
                        const child = document.createElement('span'); child.id = name; return child;
                    }));
                } else element.className = names.join(' ');
            };
            const list = kind === 'attributes' ? element.attributes :
                kind === 'children' ? element.children : element.classList;
            const name = item => typeof item === 'string' ? item : item.name || item.id;
            set(['a', 'b']);
            const iterator = list[Symbol.iterator]();
            checks.push(name(iterator.next().value) === 'a');
            set(['x', 'y', 'z']);
            checks.push(name(iterator.next().value) === 'y', name(iterator.next().value) === 'z');
            set([]);
            checks.push(iterator.next().done);
            set(['new']);
            checks.push(iterator.next().done, [...list].map(name).join() === 'new');
            const proto = Object.getPrototypeOf(list);
            const descriptor = Object.getOwnPropertyDescriptor(proto, Symbol.iterator);
            checks.push(descriptor.value === Array.prototype.values, descriptor.writable,
                descriptor.configurable, !descriptor.enumerable);
            const borrowed = proto[Symbol.iterator].call({0: 'generic', length: 1});
            checks.push(borrowed.next().value === 'generic', borrowed.next().done);
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
fn token_list_value_iterable_methods_follow_array_mutation_rules() {
    let (dom, outcome) = execute_html(
        r#"<body><output>no</output><script>
        const element = document.createElement('div');
        element.className = 'a b';
        const tokens = element.classList;
        const checks = ['entries', 'keys', 'values', 'forEach'].map(method =>
            DOMTokenList.prototype[method] === Array.prototype[method]);
        const entries = tokens.entries(), keys = tokens.keys();
        checks.push(JSON.stringify(entries.next().value) === '[0,"a"]', keys.next().value === 0);
        element.className = 'c d e';
        checks.push(JSON.stringify(entries.next().value) === '[1,"d"]', keys.next().value === 1);
        const visited = [], receiver = {};
        tokens.forEach(function(value, index, collection) {
            checks.push(this === receiver, collection === tokens);
            visited.push(value);
            if (index === 0) element.className = 'c replaced e appended';
        }, receiver);
        checks.push(visited.join() === 'c,replaced,e');
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
fn collection_indexed_properties_do_not_use_item_null_or_integer_conversion() {
    let (dom, outcome) = execute_html(
        r#"<body><div><span></span><span></span></div><output>no</output><script>
        const elements = document.querySelector('div').getElementsByTagName('*');
        const checks = [];
        checks.push(elements.length === 2, elements[2] === undefined,
            elements.item(2) === null, !(2 in elements),
            Object.getOwnPropertyDescriptor(elements, '2') === undefined,
            elements[4294967296] === undefined, !(4294967296 in elements),
            elements.item(4294967296) === elements[0]);
        let visited = 0;
        for (let i = 0; elements[i] !== undefined && i < 4; i++) {
            if (elements[i].nodeType === 1) visited++;
        }
        checks.push(visited === 2);
        HTMLCollection.prototype[2] = 'inherited';
        checks.push(elements[2] === 'inherited', 2 in elements,
            Object.getOwnPropertyDescriptor(elements, '2') === undefined);
        delete HTMLCollection.prototype[2];
        elements[0].remove();
        checks.push(elements.length === 1, elements[1] === undefined);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : 'no';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn retained_element_children_refresh_after_every_child_list_change() {
    // `children` reuses its resolved list until the parent's child list changes,
    // so each script mutation path must invalidate it.
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><b></b>text</main><script>
            const parent = document.getElementById('parent');
            const children = parent.children;
            const lengths = [children.length];
            parent.appendChild(document.createElement('i'));
            lengths.push(children.length);
            parent.firstChild.remove();
            lengths.push(children.length);
            parent.innerHTML = '<p></p><p></p><p></p>';
            lengths.push(children.length, children[2].localName);
            parent.replaceChildren(document.createElement('u'));
            lengths.push(children.length, children[0].localName);
            parent.insertAdjacentHTML('beforeend', '<s></s>');
            lengths.push(children.length);
            parent.textContent = 'only text';
            lengths.push(children.length);
            const nested = document.createElement('div');
            parent.append(nested);
            const nestedChildren = nested.children;
            nestedChildren.length;
            nested.insertAdjacentHTML('afterbegin', '<q></q>');
            lengths.push(children.length, nestedChildren.length);
            document.querySelector('output').textContent = lengths.join(':');
        </script><output></output></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "1:2:1:3:p:1:u:2:0:1:1"
    );
}

#[test]
fn selector_collections_follow_attribute_and_tree_changes_between_reads() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="root"><a id="first" class="item">one</a><a id="second">two</a></div><script>
            const root = document.getElementById('root');
            const items = root.getElementsByClassName('item');
            const links = document.links;
            const steps = [items.length, links.length, items[0] === items[0]];
            document.getElementById('second').className = 'item';
            steps.push(items.length, items[1].id);
            document.getElementById('first').href = '#one';
            steps.push(links.length, links[0].id);
            const seen = [];
            for (const item of items) {
                seen.push(item.id);
                if (item.id === 'first') root.append(Object.assign(document.createElement('b'), {className: 'item', id: 'third'}));
            }
            steps.push(seen.join('+'));
            document.getElementById('first').removeAttribute('class');
            steps.push(items.length, items[0].id);
            document.querySelector('output').textContent = steps.join(':');
        </script><output></output></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "1:0:true:2:second:1:first:first+second+third:2:second"
    );
}
