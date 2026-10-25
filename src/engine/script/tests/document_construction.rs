use super::*;

fn status_after_script(body: &str) -> String {
    let html = format!(r#"<body>{body}<output id="status"></output></body>"#);
    let (dom, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    dom.elements_named("output").next().unwrap().text_content()
}

#[test]
fn create_document_treats_a_null_qualified_name_as_empty() {
    // DOMImplementation.createDocument's qualifiedName is [LegacyNullToEmptyString].
    let status = status_after_script(
        r#"<script>
            const doc = document.implementation.createDocument(null, null, null);
            const before = doc.childNodes.length;
            doc.appendChild(doc.createElement('root'));
            const named = document.implementation.createDocument(null, 'root', null);
            document.getElementById('status').textContent =
                [before, doc.documentElement.nodeName, named.documentElement.nodeName].join(':');
        </script>"#,
    );
    assert_eq!(status, "0:root:root");
}

#[test]
fn create_document_type_validates_names_and_is_adopted_by_create_document() {
    let status = status_after_script(
        r#"<script>
            const implementation = document.implementation;
            const doctype = implementation.createDocumentType('html', '-//W3C//DTD XHTML 1.0 Strict//EN', 'about:dtd');
            const initialOwner = doctype.ownerDocument === document;
            const doc = implementation.createDocument('http://www.w3.org/1999/xhtml', 'html', doctype);
            const errors = ['a>b', 'a b', 'a\0b'].map(name => {
                try { implementation.createDocumentType(name, '', ''); return 'none'; }
                catch (error) { return error.name; }
            });
            let typeError = false;
            try { implementation.createDocument(null, 'x', {}); } catch (error) { typeError = error instanceof TypeError; }
            document.getElementById('status').textContent = [
                doctype.nodeType, doctype.name, doctype.publicId, doctype.systemId, initialOwner,
                doctype.ownerDocument === doc, doc.firstChild === doctype, doc.doctype === doctype,
                doc.documentElement.localName, errors.join('/'), typeError,
                implementation.createDocumentType('', '', '').name === ''
            ].join(':');
        </script>"#,
    );
    assert_eq!(
        status,
        "10:html:-//W3C//DTD XHTML 1.0 Strict//EN:about:dtd:true:true:true:true:html:\
         InvalidCharacterError/InvalidCharacterError/InvalidCharacterError:true:true"
    );
}

#[test]
fn create_element_rejects_names_that_are_not_valid_element_local_names() {
    // https://dom.spec.whatwg.org/#valid-element-local-name
    let status = status_after_script(
        r#"<script>
            const outcome = name => {
                try { return document.createElement(name).localName; }
                catch (error) { return error.name + error.code; }
            };
            document.getElementById('status').textContent =
                ['form\0div', '', 'a b', 'a>b', '1a', '-a', 'a:b', '_x', 'my-el', 'é1'].map(outcome).join(',');
        </script>"#,
    );
    assert_eq!(
        status,
        "InvalidCharacterError5,InvalidCharacterError5,InvalidCharacterError5,\
         InvalidCharacterError5,InvalidCharacterError5,InvalidCharacterError5,a:b,_x,my-el,é1"
    );
}

#[test]
fn extracting_a_range_inside_the_body_leaves_the_doctype_alone() {
    let status = status_after_script(
        r#"<script>
            const doc = document.implementation.createHTMLDocument('');
            const h1 = doc.createElement('h1');
            const t1 = doc.createTextNode('Hello '), t2 = doc.createTextNode('Wonderful');
            const em = doc.createElement('em');
            em.appendChild(t2);
            h1.append(t1, em, doc.createTextNode(' Kitty'));
            const p = doc.createElement('p');
            p.textContent = 'How are you?';
            doc.body.append(h1, p);
            const range = doc.createRange();
            range.selectNodeContents(doc);
            const whole = range.toString();
            range.setStart(t2, 6);
            range.setEnd(p, 0);
            const text = range.toString();
            const fragment = range.extractContents();
            document.getElementById('status').textContent = [whole, text, fragment.childNodes.length,
                fragment.firstChild.firstChild.firstChild.data, fragment.ownerDocument === doc].join(':');
        </script>"#,
    );
    assert_eq!(
        status,
        "Hello Wonderful KittyHow are you?:ful Kitty:2:ful:true"
    );
}

#[test]
fn create_element_ns_validates_and_extracts_the_qualified_name() {
    // https://dom.spec.whatwg.org/#validate-and-extract
    let status = status_after_script(
        r#"<script>
            const html = 'http://www.w3.org/1999/xhtml', xmlns = 'http://www.w3.org/2000/xmlns/';
            const outcome = (namespace, name) => {
                try { const element = document.createElementNS(namespace, name); return element.prefix + '|' + element.localName; }
                catch (error) { return error.name; }
            };
            document.getElementById('status').textContent = [
                outcome(html, '<div>'), outcome(html, '0div'), outcome(html, 'di v'), outcome(html, 'di<v'),
                outcome(html, ':a'), outcome(null, 'p:a'), outcome(html, 'xml:a'), outcome(html, 'xmlns'),
                outcome(xmlns, 'xmlns:a'), outcome(html, 'p:a')
            ].join(',');
        </script>"#,
    );
    assert_eq!(
        status,
        "InvalidCharacterError,InvalidCharacterError,InvalidCharacterError,null|di<v,\
         InvalidCharacterError,NamespaceError,NamespaceError,NamespaceError,xmlns|a,p|a"
    );
}

#[test]
fn xhtml_documents_from_create_document_expose_title_body_and_forms() {
    let status = status_after_script(
        r#"<script>
            const ns = 'http://www.w3.org/1999/xhtml';
            const doc = document.implementation.createDocument(ns, 'html',
                document.implementation.createDocumentType('html', '', ''));
            doc.documentElement.appendChild(doc.createElementNS(ns, 'head'));
            doc.documentElement.appendChild(doc.createElementNS(ns, 'body'));
            const title = doc.createElementNS(ns, 'title');
            doc.documentElement.firstChild.appendChild(title);
            const initial = doc.title;
            title.textContent = 'Sparrow';
            doc.body.appendChild(doc.createElementNS(ns, 'form'));
            document.getElementById('status').textContent =
                [JSON.stringify(initial), doc.title, doc.forms.length].join(':');
        </script>"#,
    );
    assert_eq!(status, r#""":Sparrow:1"#);
}
