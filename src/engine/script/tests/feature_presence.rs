use super::*;

#[test]
fn newly_created_html_elements_and_documents_expose_editing_and_drag_idl() {
    let (dom, outcome) = execute_html(
        r#"<output></output><script>
            const div = document.createElement('div');
            const elementProperties = ['draggable', 'contentEditable', 'isContentEditable',
                'ondrag', 'ondragstart', 'ondragenter', 'ondragover', 'ondragleave',
                'ondragend', 'ondrop'];
            const documentProperties = ['designMode', 'execCommand', 'queryCommandEnabled',
                'queryCommandIndeterm', 'queryCommandState', 'queryCommandSupported',
                'queryCommandValue'];
            const missing = elementProperties.filter(name => !(name in div)).concat(
                documentProperties.filter(name => !(name in document)));
            document.querySelector('output').textContent = missing.join(',') || 'complete';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "complete"
    );
}
