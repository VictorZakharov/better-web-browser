use super::*;

#[test]
fn iframe_elements_use_the_standard_interface_and_reflect_attributes() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><iframe></iframe><output>no</output><script>
            const frame = document.querySelector('iframe');
            frame.srcdoc = '<p>inside</p>';
            frame.sandbox.add('allow-scripts');
            frame.allowFullscreen = true;
            frame.referrerPolicy = 'no-referrer';
            const accepted = frame instanceof HTMLIFrameElement && frame instanceof HTMLElement &&
                frame.srcdoc === '<p>inside</p>' && frame.sandbox.contains('allow-scripts') &&
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
