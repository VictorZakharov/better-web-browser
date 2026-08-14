use super::*;

#[test]
fn url_search_params_preserve_order_and_support_value_overloads() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            const params = new URLSearchParams('a=b&c=d&a=e');
            params.set('a', 'B');
            params.append('space', 'a b');
            params.append('first', 'one');
            params.append('first', 'two');
            params.delete('first', 'one');
            if (
                params.toString() === 'a=B&c=d&space=a+b&first=two' &&
                params.has('a', 'B') && !params.has('a', 'b') &&
                params.has('a', undefined) && params.values().next().value === 'B' &&
                params.size === 4
            ) document.getElementById('status').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn queue_microtask_validates_and_invokes_without_arguments() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            let threw = false;
            try { queueMicrotask(); } catch (error) { threw = error instanceof TypeError; }
            queueMicrotask(function() {
                if (threw && arguments.length === 0) document.getElementById('status').textContent = 'yes';
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn exposes_namespace_aware_nodes_fragments_and_html_interfaces() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><div id=""></div><div id="status">no</div><script>
            const svg = document.createElementNS('http://www.w3.org/2000/svg', 's:mixedCase');
            const html = document.createElementNS('http://www.w3.org/1999/xhtml', 'div');
            const fragment = document.createDocumentFragment();
            const fixture = document.createElement('div');
            fixture.id = 'old';
            document.body.appendChild(fixture);
            fixture.outerHTML = '<div id="replacement"></div>';
            if (
                svg.tagName === 's:mixedCase' && svg.nodeName === svg.tagName &&
                svg.localName === 'mixedCase' && svg.prefix === 's' &&
                svg.namespaceURI === 'http://www.w3.org/2000/svg' &&
                html.tagName === 'DIV' && html instanceof HTMLDivElement &&
                fragment.nodeType === 11 && fragment.nodeName === '#document-fragment' &&
                fragment.ownerDocument === document && document.doctype.nodeName === 'html' &&
                document.getElementById('') === null && document.getElementById('old') === null &&
                document.getElementById('replacement') instanceof HTMLDivElement
            ) document.getElementById('status').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}

#[test]
fn documents_clone_and_import_nodes_with_the_target_documents_semantics() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><table id="fixture"><tbody><tr><td id="target">text</td></tr></tbody></table>
        <div id="status">no</div><script>
            const xmlDocument = document.implementation.createDocument(
                'http://www.w3.org/1999/xhtml', 'foo:div', null);
            const imported = document.importNode(xmlDocument.documentElement, true);
            const documentClone = document.cloneNode(true);
            const newDocument = new Document();
            const clonedRoot = document.documentElement.cloneNode(true);
            const appendedRoot = newDocument.appendChild(clonedRoot);
            const htmlDocument = document.implementation.createHTMLDocument('copy');
            htmlDocument.body.appendChild(document.getElementById('fixture').cloneNode(true));
            const checks = [
                ['xml-view', xmlDocument.defaultView === null],
                ['xml-tag', xmlDocument.documentElement.tagName === 'foo:div'],
                ['import-owner', imported.ownerDocument === document],
                ['import-tag', imported.tagName === 'FOO:DIV'],
                ['clone-view', documentClone.defaultView === null],
                ['clone-tree', documentClone.getElementById('target').textContent === 'text'],
                ['new-identity', appendedRoot === clonedRoot],
                ['new-owner', newDocument.documentElement.ownerDocument === newDocument],
                ['new-tree', newDocument.getElementById('target').textContent === 'text'],
                ['html-title', htmlDocument.title === 'copy'],
                ['html-tree', htmlDocument.getElementById('target').textContent === 'text']
            ];
            const failures = checks.filter(([, passed]) => !passed).map(([name]) => name);
            document.getElementById('status').textContent = failures.join(',') || 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}

#[test]
fn document_write_preserves_one_tokenizer_stream_and_url_serialization() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body>
        <div><a href='?notin=&notin;&not;&;& &'>Link</a><p>Text: &notin;&not;</p></div>
        <script>
            const markup = "<div><a href='?notin=&notin;&not;&;& &'>Link</a><p>Text: &notin;&not;</p></div>";
            for (let index = 0; index < markup.length; index++) document.write(markup.charAt(index));
        </script>
        <p id="status">no</p><script>
            const divs = document.getElementsByTagName('div');
            const writtenHref = divs[1].firstChild.href;
            const query = writtenHref.substring(writtenHref.indexOf('?'));
            if (divs.length === 2 && divs[1].childNodes.length === 2 &&
                query === '?notin=%E2%88%89%C2%AC&;&%20&' &&
                divs[1].lastChild.textContent === 'Text: \u2209\u00AC') {
                document.getElementById('status').textContent = 'yes';
            }
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}

#[test]
fn window_named_access_and_cssom_use_the_computed_cascade() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><style>
            .outer { opacity: 0.5 !important; font-size: 18px !important; line-height: 2em; }
            html { z-index: inherit; position: inherit; overflow: inherit; background-color: inherit; }
        </style><body><p class="outer" id="el" style="opacity: 1; font-size: 36px">text</p>
        <div id="status">no</div><script>
            const style = getComputedStyle(el);
            const root = getComputedStyle(document.documentElement);
            if (
                el === document.getElementById('el') && style.opacity === '0.5' &&
                style.fontSize === '18px' && style.lineHeight === '36px' &&
                root.zIndex === 'auto' && root.position === 'static' &&
                root.overflow === 'visible' && root.backgroundColor === 'rgba(0, 0, 0, 0)'
            ) document.getElementById('status').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}
