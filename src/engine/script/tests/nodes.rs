use super::*;

#[test]
fn exposes_web_idl_node_type_constants_on_constructor_and_prototype() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            const expected = {
                ELEMENT_NODE: 1,
                ATTRIBUTE_NODE: 2,
                TEXT_NODE: 3,
                CDATA_SECTION_NODE: 4,
                ENTITY_REFERENCE_NODE: 5,
                ENTITY_NODE: 6,
                PROCESSING_INSTRUCTION_NODE: 7,
                COMMENT_NODE: 8,
                DOCUMENT_NODE: 9,
                DOCUMENT_TYPE_NODE: 10,
                DOCUMENT_FRAGMENT_NODE: 11,
                NOTATION_NODE: 12
            };
            const valid = Object.entries(expected).every(([name, value]) =>
                Node[name] === value && Node.prototype[name] === value &&
                Object.getOwnPropertyDescriptor(Node, name).writable === false &&
                Object.getOwnPropertyDescriptor(Node.prototype, name).writable === false);
            if (valid) document.getElementById('status').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}
