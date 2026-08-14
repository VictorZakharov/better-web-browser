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
