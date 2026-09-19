use super::*;

#[test]
fn adoption_between_frame_realms_preserves_nodes_and_document_identity() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output></output><script>
        function check(value, message) { if (!value) throw new Error(message); }
        const outer = document.createElement('iframe');
        const inner = document.createElement('iframe');
        const node = document.createElement('div');
        node.id = 'adopted';
        document.body.append(outer);
        const child = outer.contentWindow;
        child.document.body.appendChild(inner);
        check(inner.ownerDocument === child.document, 'adopted frame owner');
        check(inner.contentWindow.parent === child, 'adopted frame parent');
        inner.contentDocument.body.appendChild(node);
        check(node.ownerDocument === inner.contentDocument, 'grandchild owner');
        check(inner.contentDocument.getElementById('adopted') === node, 'stable wrapper');
        check(node instanceof HTMLDivElement && !(node instanceof child.HTMLDivElement), 'original prototype');
        check(child.document.adoptNode(node) === node && node.ownerDocument === child.document, 'explicit adoption');
        child.document.body.append(node);
        check(child.document.body.lastChild === node, 'cross realm ParentNode');
        document.body.append(node);
        check(node.ownerDocument === document && node.parentNode === document.body, 'return to parent');
        let rejected = false;
        try { child.document.body.appendChild({__id: node.__id, nodeType: 1}); } catch (_) { rejected = true; }
        check(rejected, 'forged wrapper');
        const saved = inner.contentDocument;
        outer.remove();
        check(inner.isConnected && saved.body.isConnected, 'retained documents stay connected');
        check(inner.contentWindow === null, 'inactive child cannot recreate a navigable');
        document.querySelector('output').textContent = 'pass';
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pass"
    );
}

#[test]
fn initial_child_document_contract() {
    let (dom, outcome) = execute_html(include_str!(
        "../../../../tests/fixtures/iframe-initial-document.html"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let result = dom.elements_named("pre").next().unwrap().text_content();
    assert!(!result.contains("FAIL"), "{result}");
    assert_eq!(
        result
            .lines()
            .filter(|line| line.ends_with(": PASS"))
            .count(),
        20,
        "{result}"
    );
}

#[test]
fn child_realms_do_not_expose_the_trusted_storage_dispatcher() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><iframe></iframe><output>no</output><script>
        if (typeof document.querySelector('iframe').contentWindow.__dispatchStorageEvent === 'undefined')
            document.querySelector('output').textContent = 'yes';
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn iframe_elements_use_the_standard_interface_and_reflect_attributes() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><iframe></iframe><output>no</output><script>
            const frame = document.querySelector('iframe');
            frame.srcdoc = '<p>inside</p>';
            frame.sandbox.add('allow-scripts');
            const sandbox = frame.sandbox;
            frame.sandbox = 'allow-same-origin allow-presentation';
            frame.allowFullscreen = true;
            frame.referrerPolicy = 'no-referrer';
            const accepted = frame instanceof HTMLIFrameElement && frame instanceof HTMLElement &&
                frame.srcdoc === '<p>inside</p>' && frame.sandbox === sandbox &&
                frame.sandbox.value === 'allow-same-origin allow-presentation' &&
                frame.allowFullscreen && frame.referrerPolicy === 'no-referrer' &&
                frame.contentWindow === frame.contentDocument.defaultView &&
                frame.getSVGDocument() === null;
            if (accepted) document.querySelector('output').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn connected_iframes_own_live_html_documents() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output>no</output><script>
            const first = document.createElement('iframe');
            const second = document.createElement('iframe');
            const detachedDocument = first.contentDocument;
            document.body.append(first, second);
            const firstDocument = first.contentDocument;
            const secondDocument = second.contentDocument;
            const firstWindow = first.contentWindow;
            const secondWindow = second.contentWindow;
            const child = firstDocument.createElement('p');
            firstDocument.body.appendChild(child);
            first.remove();
            const accepted = detachedDocument === null &&
                firstDocument instanceof firstWindow.Document && secondDocument instanceof secondWindow.Document &&
                !(firstDocument instanceof Document) && firstWindow !== secondWindow &&
                firstDocument !== document && firstDocument !== secondDocument &&
                firstDocument.documentElement.localName === 'html' &&
                firstDocument.head.localName === 'head' && firstDocument.body.localName === 'body' &&
                child.ownerDocument === firstDocument && child.isConnected &&
                first.contentDocument === null && first.contentWindow === null &&
                second.contentWindow === secondDocument.defaultView;
            if (accepted) document.querySelector('output').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn removing_an_embedding_subtree_destroys_nested_navigables() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output>no</output><script>
        const container = document.createElement('div');
        document.body.append(container);
        container.innerHTML = '<iframe></iframe>';
        const outer = container.firstChild;
        const child = outer.contentWindow;
        child.document.body.innerHTML = '<iframe></iframe>';
        const inner = child.document.body.firstChild;
        const grandchild = inner.contentWindow;
        const savedDocument = grandchild.document;
        container.textContent = '';
        const afterRemoval = child.closed && grandchild.closed && outer.contentDocument === null &&
            inner.contentDocument === null && savedDocument.body !== null;
        const staleFrame = child.document.createElement('iframe');
        child.document.body.append(staleFrame);
        if (afterRemoval && staleFrame.contentWindow === null)
            document.querySelector('output').textContent = 'yes';
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
