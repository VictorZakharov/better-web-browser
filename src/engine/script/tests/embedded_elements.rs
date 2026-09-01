use super::*;

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
            const child = firstDocument.createElement('p');
            firstDocument.body.appendChild(child);
            first.remove();
            const accepted = detachedDocument === null &&
                firstDocument instanceof Document && secondDocument instanceof Document &&
                firstDocument !== document && firstDocument !== secondDocument &&
                firstDocument.documentElement.localName === 'html' &&
                firstDocument.head.localName === 'head' && firstDocument.body.localName === 'body' &&
                child.ownerDocument === firstDocument && child.isConnected &&
                first.contentDocument === firstDocument &&
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
