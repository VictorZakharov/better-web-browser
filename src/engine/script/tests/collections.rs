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
