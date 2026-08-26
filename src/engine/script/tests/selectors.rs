use super::*;

#[test]
fn element_matches_and_closest_use_selector_semantics() {
    let (dom, outcome) = execute_html(
        r#"<body><main class="shell"><section><a id="target" class="link">link</a></section></main><div id="status">no</div><script>
            const target = document.getElementById('target');
            const shell = target.closest('.shell');
            if (
                target.matches('main .link, button') &&
                !target.matches('button, .missing') &&
                target.closest('.link') === target &&
                shell && shell.localName === 'main' &&
                target.closest('.missing') === null
            ) document.getElementById('status').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}
