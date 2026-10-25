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

#[test]
fn rendered_image_dimensions_report_the_used_content_box() {
    // https://html.spec.whatwg.org/multipage/embedded-content.html#dom-img-width
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><html><body>
          <img id=shown width=1 height=1 style="padding: 2px; border: 3px solid">
          <script>
            const shown = document.getElementById('shown');
            const detached = document.createElement('img');
            detached.height = 7;
            document.body.setAttribute('data-result',
                [shown.width, shown.height, detached.height].join(','));
          </script></body></html>"#,
        true,
    );
    let image = dom.elements_named("img").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_geometry(&HashMap::from([(
        image.id(),
        RectF {
            width: 30.0,
            height: 20.0,
            ..RectF::default()
        },
    )]));
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#image".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(body.attr("data-result").as_deref(), Some("20,10,7"));
}
