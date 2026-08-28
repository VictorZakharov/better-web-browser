use super::*;

#[test]
fn executes_script_and_mutates_the_owned_dom() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="app"></main><script>
            const message = document.createElement('p');
            message.className = 'result';
            message.textContent = 'JavaScript works';
            document.getElementById('app').appendChild(message);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    let paragraph = dom.elements_named("p").next().unwrap();
    assert_eq!(paragraph.attr("class").as_deref(), Some("result"));
    assert_eq!(paragraph.text_content(), "JavaScript works");
}

#[test]
fn child_collections_keep_identity_and_refresh_after_tree_mutations() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><span></span></main><script>
            const parent = document.getElementById('parent');
            const childNodes = parent.childNodes;
            const children = parent.children;
            const sameBefore = childNodes === parent.childNodes && children === parent.children;
            parent.setAttribute('data-state', 'ready');
            const sameAfterAttribute = childNodes === parent.childNodes && children === parent.children;
            parent.appendChild(document.createTextNode('text'));
            parent.appendChild(document.createElement('strong'));
            const sameAfterInsertion = childNodes === parent.childNodes && children === parent.children;
            document.body.setAttribute('data-result', [
                sameBefore, sameAfterAttribute, sameAfterInsertion,
                childNodes.length, children.length
            ].join(':'));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-result"))
            .as_deref(),
        Some("true:true:true:3:2")
    );
}

#[test]
fn executes_classic_scripts_with_html_like_comments() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            <!--
            document.getElementById('status').textContent = 'ready';
            -->
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn document_write_moves_every_fragment_child_without_reborrowing_its_parent() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            document.write('<p id="first">one</p><p id="second">two</p>');
        </script></body>"#,
    );

    assert!(!outcome.runtime_stopped, "{:?}", outcome.errors);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(dom.elements_named("p").count(), 2);
    assert_eq!(
        dom.elements_named("p")
            .map(|node| node.text_content())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
}

#[test]
fn replace_with_accepts_nodes_and_strings_and_preserves_argument_order() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><i id="before"></i><b id="old"></b><i id="after"></i></main>
        <script>
            const old = document.getElementById('old');
            const replacement = document.createElement('span');
            replacement.id = 'replacement';
            old.replaceWith('left', replacement, 'right');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let parent = dom
        .elements_named("main")
        .find(|node| node.attr("id").as_deref() == Some("parent"))
        .unwrap();
    let children = parent.children.borrow();
    assert_eq!(
        children
            .iter()
            .map(|node| node.text_content())
            .collect::<String>(),
        "leftright"
    );
    assert_eq!(
        children
            .iter()
            .filter_map(|node| node.attr("id"))
            .collect::<Vec<_>>(),
        ["before", "replacement", "after"]
    );
}
