use super::*;

#[test]
fn classic_script_top_level_this_is_the_window_global() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status"></div><script>
            "use strict";
            document.getElementById('status').textContent = String(
                this === window && this === globalThis &&
                'IntersectionObserver' in this
            );
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn loads_dynamically_inserted_external_scripts_in_the_same_realm() {
    let dom = dom::parse_with_scripting(
        r#"<html><head></head><body><div id="status">waiting</div><script>
            window.initialValue = 40;
            const loader = document.createElement('script');
            loader.src = '/dynamic.js';
            loader.onload = () => {
                document.getElementById('status').textContent = String(window.dynamicAnswer);
            };
            document.head.appendChild(loader);
        </script></body></html>"#,
        true,
    );
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut requested = Vec::new();
    let mut loader = |url: &str, _kind: ScriptKind, _options: ScriptFetchOptions| {
        requested.push(url.to_string());
        Ok("window.dynamicAnswer = window.initialValue + 2;".to_string())
    };
    let outcome = execute_with_loader(
        dom.document.clone(),
        "https://example.com/",
        &scripts,
        &mut loader,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 2);
    assert_eq!(requested, ["https://example.com/dynamic.js"]);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "42"
    );
}

#[test]
fn moving_an_already_started_external_script_does_not_execute_it_again() {
    let dom = dom::parse_with_scripting(
        r#"<html><body><script src="/app.js"></script></body></html>"#,
        true,
    );
    let node = dom.elements_named("script").next().unwrap();
    let script = ScriptInput {
        source_url: "https://example.com/app.js".into(),
        code: r#"
            window.executionCount = (window.executionCount || 0) + 1;
            document.body.setAttribute('data-executions', String(window.executionCount));
            document.body.appendChild(document.currentScript);
        "#
        .into(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut requested = Vec::new();
    let dynamic_code = script.code.clone();
    let mut loader = |url: &str, _kind: ScriptKind, _options: ScriptFetchOptions| {
        requested.push(url.to_string());
        Ok(dynamic_code.clone())
    };
    let outcome = execute_with_loader(
        dom.document.clone(),
        "https://example.com/",
        &[script],
        &mut loader,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    assert!(requested.is_empty());
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-executions")
            .as_deref(),
        Some("1")
    );
}

#[test]
fn image_constructor_reports_invalid_urls_asynchronously() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            const image = new Image();
            image.onerror = () => {
                document.getElementById('status').textContent = 'unsupported';
            };
            image.src = 'http://:invalid';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "unsupported"
    );
}

#[test]
fn exposes_html_element_identity_namespaces_and_ua_defaults() {
    let (dom, outcome) = execute_html(
        r##"<body><div id="status">no</div><script>
            const container = document.createElement('div');
            container.innerHTML = '<svg><circle></circle></svg><math><mi>x</mi></math>';
            const section = document.createElement('section');
            const unknown = document.createElement('madeupelement');
            const time = document.createElement('time');
            const data = document.createElement('data');
            const image = document.createElement('img');
            const picture = document.createElement('picture');
            const source = document.createElement('source');
            const input = document.createElement('input');
            const mark = document.createElement('mark');
            const rp = document.createElement('rp');
            const parent = document.createElement('div');
            const translatedChild = document.createElement('span');
            const list = document.createElement('ol');
            const select = document.createElement('select');
            const fieldset = document.createElement('fieldset');
            const field = document.createElement('input');
            const form = document.createElement('form');
            const externalField = document.createElement('input');
            const label = document.createElement('label');
            parent.translate = false;
            parent.appendChild(translatedChild);
            parent.accessKey = 'x';
            list.reversed = true;
            fieldset.appendChild(field);
            form.id = 'owner';
            externalField.id = 'owned-field';
            externalField.setAttribute('form', 'owner');
            label.htmlFor = 'owned-field';
            document.body.appendChild(form);
            document.body.appendChild(externalField);
            document.body.appendChild(label);
            time.dateTime = '2026-08-13';
            data.value = '42';
            image.srcset = 'small.png 1x, large.png 2x';
            image.sizes = '100vw';
            source.srcset = 'wide.png 2x';
            source.sizes = '50vw';
            source.media = '(min-width: 600px)';
            input.placeholder = 'Search';
            if (
                section instanceof HTMLElement &&
                !(section instanceof HTMLUnknownElement) &&
                unknown instanceof HTMLUnknownElement &&
                time instanceof HTMLTimeElement && time.getAttribute('datetime') === '2026-08-13' &&
                data instanceof HTMLDataElement && data.getAttribute('value') === '42' &&
                image instanceof HTMLImageElement && image.getAttribute('srcset').includes('large.png') &&
                image.sizes === '100vw' && picture instanceof HTMLPictureElement &&
                source instanceof HTMLSourceElement && source.srcset === 'wide.png 2x' &&
                source.sizes === '50vw' && source.media === '(min-width: 600px)' &&
                input instanceof HTMLInputElement && input.getAttribute('placeholder') === 'Search' &&
                'onerror' in image &&
                getComputedStyle(section).display === 'block' &&
                getComputedStyle(mark).backgroundColor === 'rgb(255, 255, 0)' &&
                getComputedStyle(rp).display === 'none' &&
                translatedChild.translate === false &&
                parent.getAttribute('translate') === 'no' && parent.accessKey === 'x' &&
                typeof parent.accessKeyLabel === 'string' &&
                list instanceof HTMLOrderedListElement && list.hasAttribute('reversed') &&
                select instanceof HTMLSelectElement &&
                fieldset instanceof HTMLFieldSetElement && fieldset.elements[0] === field &&
                externalField.form === form && label instanceof HTMLLabelElement &&
                label.control === externalField &&
                container.firstChild.namespaceURI === 'http://www.w3.org/2000/svg' &&
                container.lastChild.namespaceURI === 'http://www.w3.org/1998/Math/MathML'
            ) document.getElementById('status').textContent = 'yes';
        </script></body>"##,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn exposes_dataset_and_core_html_form_interfaces() {
    let (dom, outcome) = execute_html(
        r##"<body><div id="status">no</div><script>
            const data = document.createElement('div');
            data.setAttribute('data-user-id', '41');
            const sameDataset = data.dataset === data.dataset;
            data.dataset.userId = '42';
            data.dataset.displayName = 'Ada';
            const datasetKeys = Object.keys(data.dataset).sort().join(',');
            delete data.dataset.displayName;

            const form = document.createElement('form');
            form.id = 'owner';
            const input = document.createElement('input');
            input.id = 'email';
            input.type = 'email';
            input.required = true;
            input.value = 'not-an-email';
            input.selectionDirection = 'backward';
            input.formAction = '/submit';
            input.formMethod = 'post';
            input.formNoValidate = true;
            let inputEvents = 0;
            let invalidEvents = 0;
            input.oninput = () => inputEvents++;
            input.onchange = () => inputEvents++;
            input.oninvalid = () => invalidEvents++;
            input.dispatchEvent(new Event('input'));
            input.dispatchEvent(new Event('change'));
            form.appendChild(input);
            document.body.appendChild(form);
            const label = document.createElement('label');
            label.htmlFor = 'email';
            document.body.appendChild(label);

            const datalist = document.createElement('datalist');
            datalist.id = 'choices';
            datalist.appendChild(document.createElement('option'));
            input.setAttribute('list', 'choices');
            document.body.appendChild(datalist);

            const textarea = document.createElement('textarea');
            textarea.minLength = 2;
            textarea.maxLength = 20;
            textarea.wrap = 'hard';
            const select = document.createElement('select');
            select.required = true;
            const fieldset = document.createElement('fieldset');
            fieldset.disabled = true;
            const output = document.createElement('output');
            output.value = 'ready';
            const progress = document.createElement('progress');
            progress.max = 10;
            progress.value = 4;
            const meter = document.createElement('meter');
            meter.min = 0;
            meter.max = 100;
            meter.value = 75;
            const formIsValid = form.checkValidity();

            if (
                data.dataset instanceof DOMStringMap && sameDataset && data.dataset.userId === '42' &&
                data.getAttribute('data-user-id') === '42' && !data.hasAttribute('data-display-name') &&
                datasetKeys === 'displayName,userId' && input instanceof HTMLInputElement &&
                input.selectionDirection === 'backward' && !input.validity.valid &&
                input.form === form && input.labels[0] === label && form.elements[0] === input &&
                input.formAction === 'https://example.com/submit' && input.formMethod === 'post' &&
                input.formNoValidate && datalist instanceof HTMLDataListElement &&
                input.list === datalist && datalist.options.length === 1 &&
                textarea instanceof HTMLTextAreaElement && textarea.minLength === 2 &&
                textarea.maxLength === 20 && textarea.wrap === 'hard' &&
                select instanceof HTMLSelectElement && select.required &&
                fieldset instanceof HTMLFieldSetElement && fieldset.disabled &&
                output instanceof HTMLOutputElement && output.value === 'ready' &&
                progress instanceof HTMLProgressElement && progress.position === 0.4 &&
                meter instanceof HTMLMeterElement && meter.value === 75 &&
                form instanceof HTMLFormElement && !formIsValid &&
                inputEvents === 2 && invalidEvents === 1
            ) document.getElementById('status').textContent = 'yes';
        </script></body>"##,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn exposes_template_contents_as_a_document_fragment() {
    let (dom, outcome) = execute_html(
        r#"<body><template id="parsed"><span id="inside">parsed</span></template><div id="status">no</div><script>
            const parsed = document.getElementById('parsed');
            const created = document.createElement('template');
            created.innerHTML = '<p data-value="42">created</p>';
            const paragraph = created.content.querySelector('p');
            if (
                parsed instanceof HTMLTemplateElement &&
                parsed.firstChild === null &&
                parsed.content instanceof DocumentFragment &&
                parsed.content.nodeType === 11 &&
                parsed.content.ownerDocument === document &&
                parsed.content.firstChild.textContent === 'parsed' &&
                document.getElementById('inside') === null &&
                paragraph.textContent === 'created' &&
                paragraph.dataset.value === '42' &&
                created.innerHTML.includes('<p data-value="42">created</p>') &&
                !created.content.isConnected
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
fn exposes_links_interactive_elements_and_script_reflection() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            const anchor = document.createElement('a');
            anchor.download = 'report.txt';
            anchor.ping = '/audit-one /audit-two';
            anchor.relList.add('noopener', 'noreferrer');

            const details = document.createElement('details');
            const summary = document.createElement('summary');
            summary.textContent = 'More';
            const content = document.createElement('p');
            content.textContent = 'Details';
            details.append(summary, content);
            document.body.appendChild(details);
            const closedDisplay = getComputedStyle(content).display;
            summary.click();
            const openDisplay = getComputedStyle(content).display;

            const dialog = document.createElement('dialog');
            document.body.appendChild(dialog);
            const closedDialogDisplay = getComputedStyle(dialog).display;
            let closeEvents = 0;
            dialog.addEventListener('close', () => closeEvents++);
            dialog.showModal();
            const openDialogDisplay = getComputedStyle(dialog).display;
            dialog.close('accepted');

            const reflectedScript = document.createElement('script');
            reflectedScript.async = true;
            reflectedScript.defer = true;
            reflectedScript.text = 'window.answer = 42';
            setTimeout(() => {
                if (
                    anchor instanceof HTMLAnchorElement && anchor.download === 'report.txt' &&
                    anchor.ping.includes('/audit-two') && anchor.relList.contains('noopener') &&
                    details instanceof HTMLDetailsElement && details.open &&
                    closedDisplay === 'none' && openDisplay === 'block' &&
                    dialog instanceof HTMLDialogElement && !dialog.open && dialog.returnValue === 'accepted' &&
                    closedDialogDisplay === 'none' && openDialogDisplay === 'block' && closeEvents === 1 &&
                    reflectedScript instanceof HTMLScriptElement && reflectedScript.async && reflectedScript.defer &&
                    reflectedScript.text.includes('answer')
                ) document.getElementById('status').textContent = 'yes';
            }, 0);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn encodes_utf8_and_delivers_cloned_window_messages() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            const bytes = new TextEncoder().encode('A¢😀');
            const decoded = new TextDecoder().decode(bytes);
            const destination = new Uint8Array(4);
            const progress = new TextEncoder().encodeInto('¢BC', destination);
            let messages = 0;
            window.addEventListener('message', event => {
                messages++;
                if (
                    decoded === 'A¢😀' && bytes.join(',') === '65,194,162,240,159,152,128' &&
                    progress.read === 3 && progress.written === 4 &&
                    event.origin === location.origin && event.source === window &&
                    event.data.nested.value === 42 && event.data !== payload
                ) document.getElementById('status').textContent = 'yes';
            });
            const payload = { nested: { value: 42 } };
            window.postMessage(payload, location.origin);
            payload.nested.value = 7;
            window.postMessage('discarded', 'https://other.example');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn exposes_tokenizer_results_through_dom_bindings() {
    let (dom, outcome) = execute_html(
        r##"<body><div id="status">no</div><script>
            let result = true;
            const failures = [];
            const check = (name, value) => { if (!value) failures.push(name); return value; };
            const e = document.createElement('div');
            e.innerHTML = '<div<div>';
            result &= check('tag-name', e.firstChild && e.firstChild.nodeName === 'DIV<DIV');
            e.innerHTML = "<div foo<bar=''>";
            result &= check('attribute-name', e.firstChild.attributes[0].name === 'foo<bar');
            e.innerHTML = '<div foo=`bar`>';
            result &= check('unquoted-attribute', e.firstChild.getAttribute('foo') === '`bar`');
            e.innerHTML = "<div \"foo=''>";
            result &= check('quoted-name', e.firstChild.attributes[0].name === '"foo');
            e.innerHTML = "<a href='\nbar'></a>";
            result &= check('attribute-newline', e.firstChild.getAttribute('href') === '\nbar');
            e.innerHTML = '<!DOCTYPE html>';
            result &= check('doctype', e.firstChild === null);
            e.innerHTML = '\r';
            result &= check('cr-normalization', e.firstChild.nodeValue === '\n');
            e.innerHTML = '&lang;&rang;&apos;&ImaginaryI;&Kopf;&notinva;';
            result &= check('entities', e.firstChild.nodeValue === '\u27E8\u27E9\'\u2148\uD835\uDD42\u2209');
            e.innerHTML = '<?import namespace="foo" implementation="#bar">';
            result &= check('processing-instruction', e.firstChild.nodeType === 8 && e.firstChild.nodeValue === '?import namespace="foo" implementation="#bar"');
            e.innerHTML = '<!--foo--bar-->';
            result &= check('comment', e.firstChild.nodeType === 8 && e.firstChild.nodeValue === 'foo--bar');
            e.innerHTML = '<![CDATA[x]]>';
            result &= check('cdata', e.firstChild.nodeType === 8 && e.firstChild.nodeValue === '[CDATA[x]]');
            e.innerHTML = '<textarea><!--</textarea>--></textarea>';
            result &= check('textarea', e.firstChild.firstChild.nodeValue === '<!--');
            e.innerHTML = '<style><!--</style>--></style>';
            result &= check('style', e.firstChild.firstChild.nodeValue === '<!--');
            document.getElementById('status').textContent = result ? 'yes' : failures.join(',');
        </script></body>"##,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}
