use super::*;

fn check(code: &str) {
    let (dom, outcome) = execute_html(&format!("<body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("pass")
    );
}

#[test]
fn template_contents_have_a_shared_inert_owner_including_nested_templates() {
    check(
        r#"
        const a = document.createElement('template'), b = document.createElement('template');
        a.innerHTML = '<section><template><span>nested</span></template></section>';
        const owner = a.content.ownerDocument;
        const nested = a.content.querySelector('template');
        const other = document.implementation.createHTMLDocument('other');
        other.adoptNode(a);
        const adoptedOwner = a.content.ownerDocument;
        const valid = owner !== document && owner.defaultView === null &&
            b.content.ownerDocument === owner && adoptedOwner !== owner && adoptedOwner !== other &&
            nested.content.ownerDocument === adoptedOwner && a.content.firstChild.ownerDocument === adoptedOwner;
        document.body.dataset.result = valid ? 'pass' : 'fail';
    "#,
    );
}

#[test]
fn reusable_template_movement_and_cloning_do_not_run_custom_element_constructors() {
    check(
        r#"
        let count = 0;
        class Item extends HTMLElement {
            constructor() { super(); count++; this.insertBefore(document.createComment('upgrade'), this.firstChild); }
        }
        customElements.define('x-item', Item);
        const t = document.createElement('template');
        t.innerHTML = '<x-item><span>binding target</span></x-item>';
        const owner = t.content.ownerDocument;
        const cached = owner.createDocumentFragment();
        cached.appendChild(t.content);
        const copy = cached.cloneNode(true);
        const createdInert = owner.createElement('x-item');
        const stillInert = count === 0 && cached.firstChild.firstChild.localName === 'span' &&
            !(copy.firstChild instanceof Item) && !(createdInert instanceof Item);
        const imported = document.importNode(cached, true);
        document.body.dataset.result = stillInert && count === 1 && imported.firstChild instanceof Item &&
            cached.firstChild.firstChild.localName === 'span' ? 'pass' : 'fail:' + count;
    "#,
    );
}

#[test]
fn inert_document_type_and_nested_template_ownership_survive_import_and_clone() {
    check(
        r#"
        const xml = document.implementation.createDocument(null, 'root');
        const template = xml.createElementNS('http://www.w3.org/1999/xhtml', 'template');
        const inert = template.content.ownerDocument;
        const htmlTemplate = document.createElement('template');
        htmlTemplate.innerHTML = '<template><span></span></template>';
        const imported = document.importNode(htmlTemplate, true);
        const clone = imported.cloneNode(true);
        const owner = imported.content.ownerDocument;
        const nested = clone.content.firstChild;
        const qualified = inert.createElement('prefix:MixedCase');
        document.body.dataset.result = inert !== xml && inert.defaultView === null &&
            inert.createElement('MixedCase').localName === 'MixedCase' &&
            qualified.localName === 'prefix:MixedCase' && qualified.namespaceURI === null && qualified.prefix === null &&
            owner.createElement('MixedCase').localName === 'mixedcase' &&
            owner !== document && clone.content.ownerDocument === owner &&
            nested.content.ownerDocument === owner && nested.content.firstChild.ownerDocument === owner
            ? 'pass' : 'fail';
    "#,
    );
}

#[test]
fn adopting_unupgraded_content_into_a_detached_fragment_waits_for_connection() {
    check(
        r#"
        let count = 0;
        class Item extends HTMLElement { constructor() { super(); count++; } }
        customElements.define('x-item', Item);
        const t = document.createElement('template');
        t.innerHTML = '<x-item></x-item>';
        const fragment = document.createDocumentFragment();
        const item = t.content.firstChild;
        fragment.appendChild(item);
        const before = count;
        document.body.appendChild(fragment);
        document.body.dataset.result = before === 0 && count === 1 && item instanceof Item ? 'pass' : 'fail';
    "#,
    );
}
