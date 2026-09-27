use super::*;

fn result(dom: &super::super::super::dom::Dom) -> Option<String> {
    dom.elements_named("body")
        .next()
        .and_then(|body| body.attr("data-result"))
}

#[test]
fn shallow_and_deep_clones_copy_clonable_shadow_roots_independently() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const host = document.createElement('div');
            const light = document.createElement('p');
            light.textContent = 'light';
            host.append(light);
            const root = host.attachShadow({mode:'open', clonable:true,
                delegatesFocus:true, serializable:true, slotAssignment:'manual'});
            root.innerHTML = '<span id="shadow">shadow</span>';
            const shallow = host.cloneNode(false);
            const deep = host.cloneNode(true);
            const copy = shallow.shadowRoot;
            const valid = shallow !== host && shallow.children.length === 0 &&
                copy instanceof ShadowRoot && copy !== root && copy.host === shallow &&
                copy.mode === 'open' && copy.clonable && copy.delegatesFocus &&
                copy.serializable && copy.slotAssignment === 'manual' &&
                copy.querySelector('#shadow').textContent === 'shadow' &&
                deep.firstChild.textContent === 'light' &&
                deep.shadowRoot.querySelector('#shadow').textContent === 'shadow';
            copy.querySelector('#shadow').textContent = 'changed';
            document.body.setAttribute('data-result', valid &&
                root.querySelector('#shadow').textContent === 'shadow' ? 'pass' : 'fail');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn nested_and_nonclonable_roots_follow_each_hosts_setting() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const host = document.createElement('div');
            const root = host.attachShadow({mode:'open', clonable:true});
            root.innerHTML = '<div id="copy"></div><div id="skip"></div>';
            root.querySelector('#copy').attachShadow({mode:'open', clonable:true})
                .innerHTML = '<b>copied</b>';
            root.querySelector('#skip').attachShadow({mode:'open'})
                .innerHTML = '<i>skipped</i>';
            const clone = host.cloneNode(false);
            const clonedRoot = clone.shadowRoot;
            const valid = clonedRoot.querySelector('#copy').shadowRoot.textContent === 'copied' &&
                clonedRoot.querySelector('#skip').shadowRoot === null &&
                root.querySelector('#copy').shadowRoot !== clonedRoot.querySelector('#copy').shadowRoot;
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn import_node_uses_destination_document_for_clonable_shadow_descendants() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const foreign = document.implementation.createHTMLDocument('');
            const host = foreign.createElement('div');
            host.attachShadow({mode:'open', clonable:true}).innerHTML = '<em>foreign</em>';
            const imported = document.importNode(host, false);
            const root = imported.shadowRoot;
            const valid = imported.ownerDocument === document && root.host === imported &&
                root.ownerDocument === document && root.firstChild.ownerDocument === document &&
                root.textContent === 'foreign' && host.shadowRoot.firstChild !== root.firstChild;
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn a_shadow_root_cannot_itself_be_cloned_or_imported() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const root = document.createElement('div').attachShadow({mode:'open', clonable:true});
            const rejected = operation => {
                try { operation(); return false; }
                catch (error) { return error.name === 'NotSupportedError'; }
            };
            document.body.setAttribute('data-result',
                rejected(() => root.cloneNode(true)) &&
                rejected(() => document.importNode(root, true)) ? 'pass' : 'fail');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
