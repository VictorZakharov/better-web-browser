use super::*;

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
