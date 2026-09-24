use super::super::*;

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
