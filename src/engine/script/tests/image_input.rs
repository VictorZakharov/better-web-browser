use super::*;

#[test]
fn image_button_dimensions_and_alt_reflect_without_changing_other_inputs() {
    let (dom, outcome) = execute_html(
        r#"<body><p id=status></p><script>
            const image = document.createElement('input');
            image.type = 'image';
            image.src = '/submit.png';
            image.width = 80;
            image.height = 32;
            image.alt = 'Send';
            const text = document.createElement('input');
            text.type = 'text';
            text.width = 42;
            document.getElementById('status').textContent = String(
                image instanceof HTMLInputElement && image.width === 80 &&
                image.height === 32 && image.alt === 'Send' &&
                image.getAttribute('width') === '80' && text.width === 0
            );
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn image_submitter_records_click_coordinates_but_direct_formdata_defaults_to_zero() {
    let (dom, outcome) = execute_html(
        r#"<body><form id=form><input id=send type=image name=send src=/send.png
             width=100 height=40></form><p id=status></p><script>
            const form = document.getElementById('form');
            const send = document.getElementById('send');
            const direct = new FormData(form, send);
            let passed = direct.get('send.x') === '0' && direct.get('send.y') === '0';
            form.addEventListener('submit', event => {
                event.preventDefault();
                const entries = new FormData(form, event.submitter);
                passed = passed && entries.get('send.x') === '7' &&
                    entries.get('send.y') === '9' && event.submitter === send;
            });
            const rect = send.getBoundingClientRect();
            send.dispatchEvent(new MouseEvent('click', {
                bubbles:true, cancelable:true, clientX:rect.left + 7, clientY:rect.top + 9
            }));
            const after = new FormData(form, send);
            passed = passed && after.get('send.x') === '0' && after.get('send.y') === '0';
            document.getElementById('status').textContent = String(passed);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "true"
    );
}
