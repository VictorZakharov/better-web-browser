use super::*;

fn check_script(code: &str) {
    let (_, outcome) = execute_html(&format!("<!doctype html><body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn html_fragment_serialization_handles_void_raw_text_templates_and_attributes() {
    check_script(
        r#"
        const check = (actual, expected, label) => {
            if (actual !== expected) throw Error(label + ': ' + actual + ' != ' + expected);
        };
        const root = document.createElement('div');
        root.setAttribute('data-note', '"<&\u00a0');
        root.innerHTML = '<br><input value="x&amp;y"><span>A &amp; B</span>';
        check(root.outerHTML,
            '<div data-note="&quot;&lt;&amp;&nbsp;"><br><input value="x&amp;y"><span>A &amp; B</span></div>',
            'outerHTML includes attributes and omits void closing tags');
        const script = document.createElement('script');
        script.textContent = 'if (a < b && c > d) {}';
        check(script.innerHTML, 'if (a < b && c > d) {}', 'raw text');
        const template = document.createElement('template');
        template.innerHTML = '<em>x</em>';
        check(template.innerHTML, '<em>x</em>', 'template contents');
        check(template.outerHTML, '<template><em>x</em></template>', 'template outerHTML');
        check(document.createElement('br').outerHTML, '<br>', 'void outerHTML');
        "#,
    );
}

#[test]
fn xml_serializer_preserves_namespaces_attributes_cdata_and_document_nodes() {
    check_script(
        r#"
        const check = (condition, label) => { if (!condition) throw Error(label); };
        const xml = '<!DOCTYPE r><?go now?><r xmlns="urn:r" xmlns:p="urn:p" p:a="A &amp; B"><p:child><![CDATA[<raw>]]></p:child><!--note--></r>';
        const documentNode = new DOMParser().parseFromString(xml, 'application/xml');
        const serialized = new XMLSerializer().serializeToString(documentNode);
        check(serialized.includes('<!DOCTYPE r>'), 'doctype');
        check(serialized.includes('<?go now?>'), 'processing instruction');
        check(serialized.includes('xmlns="urn:r"') && serialized.includes('xmlns:p="urn:p"'), 'namespace declarations');
        check(serialized.includes('p:a="A &amp; B"'), 'namespaced attribute');
        check(serialized.includes('<p:child><![CDATA[<raw>]]></p:child><!--note-->'), 'child nodes');
        const roundTrip = new DOMParser().parseFromString(serialized, 'application/xml');
        check(roundTrip.documentElement.namespaceURI === 'urn:r', 'root namespace round trip');
        check(roundTrip.documentElement.firstChild.namespaceURI === 'urn:p', 'child namespace round trip');
        check(roundTrip.documentElement.getAttributeNS('urn:p', 'a') === 'A & B', 'attribute round trip');
        "#,
    );
}

#[test]
fn xml_serializer_fixes_up_default_namespaces_and_unprefixed_namespaced_attributes() {
    check_script(
        r#"
        const doc = document.implementation.createDocument('urn:root', 'root');
        const root = doc.documentElement;
        root.setAttributeNS('urn:attribute', 'data', 'value');
        const unqualified = doc.createElementNS(null, 'plain');
        root.appendChild(unqualified);
        const source = new XMLSerializer().serializeToString(doc);
        const parsed = new DOMParser().parseFromString(source, 'application/xml');
        if (parsed.documentElement.localName !== 'root' ||
            parsed.documentElement.namespaceURI !== 'urn:root' ||
            parsed.documentElement.getAttributeNS('urn:attribute', 'data') !== 'value' ||
            parsed.documentElement.firstChild.namespaceURI !== null)
            throw Error('namespace fixup failed: ' + source);
        "#,
    );
}

#[test]
fn xml_serializer_escapes_line_breaks_and_preserves_html_namespace() {
    check_script(
        r#"
        const element = document.createElement('div');
        element.setAttribute('note', 'a\tb\nc\rd&"<');
        element.textContent = 'a&< >\r';
        const source = new XMLSerializer().serializeToString(element);
        if (!source.startsWith('<div xmlns="http://www.w3.org/1999/xhtml" note="a&#x9;b&#xA;c&#xD;d&amp;&quot;&lt;">') ||
            !source.endsWith('a&amp;&lt; &gt;&#xD;</div>'))
            throw Error('incorrect XML escapes: ' + source);
        "#,
    );
}

#[test]
fn xml_serializer_keeps_empty_non_void_html_elements_open_and_closed() {
    check_script(
        r#"
        const root = document.createElement('div');
        root.innerHTML = '<span></span><br><i></i>';
        const serialized = new XMLSerializer().serializeToString(root);
        if (!serialized.includes('<span></span><br />') ||
            !serialized.includes('<i></i></div>'))
            throw Error('incorrect empty HTML element XML form: ' + serialized);
        const parsed = new DOMParser().parseFromString(serialized, 'application/xhtml+xml');
        if (parsed.documentElement.children.length !== 3 ||
            parsed.documentElement.children[2].localName !== 'i')
            throw Error('empty HTML XML round trip lost sibling structure');
        "#,
    );
}

#[test]
fn serializer_rejects_non_nodes_and_invalid_receivers() {
    check_script(
        r#"
        const serializer = new XMLSerializer();
        for (const invalid of [null, undefined, {}, 1, 'text']) {
            let error; try { serializer.serializeToString(invalid); } catch (caught) { error = caught; }
            if (!(error instanceof TypeError)) throw Error('accepted non-node');
        }
        let error; try { serializer.serializeToString(); } catch (caught) { error = caught; }
        if (!(error instanceof TypeError)) throw Error('accepted no argument');
        error = null;
        try { XMLSerializer.prototype.serializeToString.call({}, document.body); }
        catch (caught) { error = caught; }
        if (!(error instanceof TypeError)) throw Error('accepted invalid receiver');
        "#,
    );
}

#[test]
fn serializer_reads_dom_state_not_author_overridden_accessors() {
    check_script(
        r#"
        const element = document.createElement('p');
        element.setAttribute('id', 'safe');
        element.textContent = 'real';
        element.getAttribute = () => 'forged';
        Object.defineProperty(element, 'childNodes', {get() { throw Error('forged children'); }});
        Object.defineProperty(element, 'localName', {get() { return 'fake'; }});
        if (element.outerHTML !== '<p id="safe">real</p>') throw Error('outerHTML used author accessors');
        if (!new XMLSerializer().serializeToString(element).includes('<p xmlns="http://www.w3.org/1999/xhtml" id="safe">real</p>'))
            throw Error('XMLSerializer used author accessors');
        "#,
    );
}
