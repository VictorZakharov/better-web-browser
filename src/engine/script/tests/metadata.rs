//! HTML title metadata follows DOM mutation paths, not paint text or application headings.
use super::*;

#[test]
fn title_elements_keep_html_branding_and_direct_child_text_semantics() {
    let (dom, outcome) = execute_html(
        r#"<title id=subject>Parser title</title><body><script>
        const check = (name, value) => { if (!value) throw new Error(name); };
        const html = 'http://www.w3.org/1999/xhtml';
        const svg = 'http://www.w3.org/2000/svg';
        const title = document.getElementById('subject');
        check('parsed brand', title instanceof HTMLTitleElement && title.constructor === HTMLTitleElement);
        check('prototype', Object.getPrototypeOf(HTMLTitleElement.prototype) === HTMLElement.prototype);
        check('tag', Object.prototype.toString.call(title) === '[object HTMLTitleElement]');
        check('createElement', document.createElement('TITLE') instanceof HTMLTitleElement);
        check('HTML namespace', document.createElementNS(html, 'title') instanceof HTMLTitleElement);
        check('SVG namespace', !(document.createElementNS(svg, 'title') instanceof HTMLTitleElement));
        check('other namespace', !(document.createElementNS('urn:metadata', 'title') instanceof HTMLTitleElement));
        check('namespace case', !(document.createElementNS(html, 'TITLE') instanceof HTMLTitleElement));
        const descriptor = Object.getOwnPropertyDescriptor(HTMLTitleElement.prototype, 'text');
        check('descriptor', descriptor.enumerable && descriptor.configurable && descriptor.get && descriptor.set);
        for (const operation of [
            () => new HTMLTitleElement(),
            () => new HTMLTitleElement(1),
            () => descriptor.get.call(document.createElement('div')),
            () => descriptor.set.call(Object.create(HTMLTitleElement.prototype), 'fake'),
            () => { title.text = Symbol('no conversion'); }
        ]) {
            let threw = false;
            try { operation(); } catch (error) { threw = error instanceof TypeError; }
            check('illegal operation', threw);
        }
        check('failed set preserves title', title.text === 'Parser title');
        title.text = null;
        check('DOMString null', title.text === 'null');
        title.text = ' \t first\n';
        const nested = document.createElement('span'); nested.textContent = 'not child text';
        title.appendChild(nested);
        title.appendChild(document.createComment('not text'));
        title.appendChild(document.createTextNode(' second\u00a0value \r'));
        check('raw child text', title.text === ' \t first\n second\u00a0value \r');
        check('ASCII whitespace only', document.title === 'first second\u00a0value');
        const decoy = document.createElementNS(svg, 'title'); decoy.textContent = 'SVG decoy';
        document.head.insertBefore(decoy, title);
        check('HTML title selection ignores SVG title', document.title === 'first second\u00a0value');
        document.body.dataset.result = 'passed';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("passed")
    );
    assert_eq!(dom.title(), "first second\u{00a0}value");
}

#[test]
fn title_text_replacements_notify_observers_and_disconnect_children_synchronously() {
    let (dom, outcome) = execute_html(
        r#"<title id=subject>Old</title><body><script>
        const check = (name, value) => { if (!value) throw new Error(name); };
        const order = [];
        customElements.define('x-title-child', class extends HTMLElement {
            disconnectedCallback() { order.push('disconnected'); }
        });
        const title = document.getElementById('subject');
        const child = document.createElement('x-title-child');
        title.appendChild(child);
        const removed = [...title.childNodes];
        const observer = new MutationObserver(() => {});
        observer.observe(title, {childList:true, subtree:true, characterData:true});
        title.text = 'New';
        order.push('returned');
        const first = observer.takeRecords();
        check('one replace-all record', first.length === 1 && first[0].type === 'childList' && first[0].target === title);
        check('removed identities', first[0].removedNodes.length === 2 && first[0].removedNodes[0] === removed[0] && first[0].removedNodes[1] === child);
        check('new text identity', first[0].addedNodes.length === 1 && first[0].addedNodes[0] === title.firstChild && title.firstChild.data === 'New');
        check('reaction boundary', order.join(',') === 'disconnected,returned');
        const previousText = title.firstChild;
        title.text = 'New';
        const same = observer.takeRecords();
        check('same string still replaces children', same.length === 1 && same[0].removedNodes[0] === previousText && title.firstChild !== previousText);
        title.text = '';
        check('empty removes old child', observer.takeRecords().length === 1 && title.childNodes.length === 0);
        title.text = '';
        check('empty to empty has no child-list mutation', observer.takeRecords().length === 0);
        document.title = ' \t Final\n title \r';
        check('document setter uses DOM mutation', observer.takeRecords().length === 1 && title.text === ' \t Final\n title \r');
        document.body.dataset.result = 'passed';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("passed")
    );
    assert_eq!(dom.title(), "Final title");
    assert!(outcome.invalidation.impact.affects_style());
    assert!(outcome.render_requested);
}

#[test]
fn document_title_creation_selection_and_whitespace_follow_document_kind() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        const check = (name, value) => { if (!value) throw new Error(name); };
        const html = 'http://www.w3.org/1999/xhtml';
        const svg = 'http://www.w3.org/2000/svg';
        const copy = document.implementation.createHTMLDocument('Initial');
        const title = copy.querySelector('title');
        title.remove();
        copy.title = ' \n Created\t title \r';
        check('head creates title', copy.head.lastElementChild instanceof HTMLTitleElement && copy.title === 'Created title');
        copy.querySelector('title').remove();
        copy.head.remove();
        copy.title = 'Must not create under html';
        check('headless no-op', copy.title === '' && copy.querySelector('title') === null);
        const existing = copy.createElement('title');
        copy.body.appendChild(existing);
        copy.title = 'Existing title without head';
        check('existing title updated', existing.text === 'Existing title without head');
        const unicode = '\u00a0A\vB\u00a0';
        copy.title = unicode;
        check('non-ASCII spaces and vertical tab preserved', copy.title === unicode);

        const svgDocument = document.implementation.createDocument(svg, 'svg');
        const group = svgDocument.createElementNS(svg, 'g');
        const nested = svgDocument.createElementNS(svg, 'title');
        nested.textContent = 'Nested is not the document title';
        group.appendChild(nested); svgDocument.documentElement.appendChild(group);
        check('SVG direct children only', svgDocument.title === '');
        svgDocument.title = ' \t SVG\n title ';
        const svgTitle = svgDocument.documentElement.firstChild;
        check('SVG title inserted first', svgTitle.localName === 'title' && svgTitle.namespaceURI === svg && svgTitle.nextSibling === group);
        check('SVG interface is not HTML', svgTitle instanceof SVGElement && !(svgTitle instanceof HTMLTitleElement));
        check('SVG whitespace', svgDocument.title === 'SVG title');
        const span = svgDocument.createElementNS(html, 'span'); span.textContent = 'ignored descendant';
        svgTitle.appendChild(span);
        check('SVG title child text', svgDocument.title === 'SVG title');

        const other = document.implementation.createDocument('urn:metadata', 'root');
        other.title = 'Not created';
        check('other document setter no-op', other.documentElement.childNodes.length === 0 && other.title === '');
        document.body.dataset.result = 'passed';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("passed")
    );
}
