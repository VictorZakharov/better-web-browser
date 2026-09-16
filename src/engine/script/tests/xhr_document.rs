use super::network::{pending_runtime, test_response};

#[test]
fn xhr_documents_use_the_response_url_and_native_parser() {
    for (mime, source, response_type, root) in [
        (
            "Application/XML; charset=utf-8",
            "<root/>",
            "document",
            "root",
        ),
        ("application/example+xml", "<root/>", "", "root"),
        ("", "<root/>", "", "root"),
        (
            "text/html; charset=utf-8",
            "<a href='next'>link</a>",
            "document",
            "HTML",
        ),
        (
            "application/xml",
            "<parsererror xmlns='http://www.mozilla.org/newlayout/xml/parsererror.xml'/>",
            "document",
            "parsererror",
        ),
    ] {
        let (dom, mut runtime, id) = pending_runtime(&format!(
            r#"
            const xhr = new XMLHttpRequest(); xhr.open('GET','/source');
            xhr.responseType = '{response_type}';
            window.DOMParser = class {{ constructor() {{ throw Error('author replacement'); }} }};
            xhr.onload = () => {{
                const d = xhr.responseXML;
                if (!d || d.URL !== xhr.responseURL || d.location !== null || d.defaultView !== null) throw Error('document metadata');
                if (xhr.responseType === 'document' && xhr.response !== d) throw Error('response identity');
                if (d.querySelector('a')?.href && d.querySelector('a').href !== 'https://example.com/next') throw Error('base URL');
                document.querySelector('div').textContent = d.documentElement.nodeName;
            }};
            xhr.send();
        "#
        ));
        let mut response = test_response(source.as_bytes());
        response.headers = crate::fetch::HeaderList::new();
        if !mime.is_empty() {
            response.headers.append("content-type", mime).unwrap();
        }
        let outcome = runtime.complete_fetch_with_loader(id, Ok(response), None);
        assert!(outcome.errors.is_empty(), "{mime}: {:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            root,
            "{mime}"
        );
    }
}

#[test]
fn xhr_rejects_malformed_xml_and_non_document_mime_types() {
    for (mime, body, response_type) in [
        ("text/xml", "<broken>", "document"),
        ("text/plain", "<root/>", "document"),
        ("application/json", "{}", "document"),
        ("text/html", "<p>html", ""),
    ] {
        let (dom, mut runtime, id) = pending_runtime(&format!(
            r#"
            const xhr = new XMLHttpRequest(); xhr.open('GET','/source'); xhr.responseType = '{response_type}';
            xhr.onload = () => document.querySelector('div').textContent = String(xhr.responseXML === null);
            xhr.send();
        "#
        ));
        let mut response = test_response(body.as_bytes());
        response.headers = crate::fetch::HeaderList::new();
        response.headers.append("content-type", mime).unwrap();
        let outcome = runtime.complete_fetch_with_loader(id, Ok(response), None);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "true",
            "{mime}"
        );
    }
}
