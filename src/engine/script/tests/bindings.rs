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

mod html_elements;

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
                parsed.content.ownerDocument !== document &&
                parsed.content.ownerDocument.defaultView === null &&
                parsed.content.ownerDocument === created.content.ownerDocument &&
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
            reflectedScript.integrity = 'sha384-example';
            reflectedScript.crossOrigin = 'anonymous';
            const linked = document.createElement('link');
            linked.integrity = 'sha256-example';
            linked.crossOrigin = 'use-credentials';
            linked.as = 'script';
            setTimeout(() => {
                if (
                    anchor instanceof HTMLAnchorElement && anchor.download === 'report.txt' &&
                    anchor.ping.includes('/audit-two') && anchor.relList.contains('noopener') &&
                    details instanceof HTMLDetailsElement && details.open &&
                    closedDisplay === 'none' && openDisplay === 'block' &&
                    dialog instanceof HTMLDialogElement && !dialog.open && dialog.returnValue === 'accepted' &&
                    closedDialogDisplay === 'none' && openDialogDisplay === 'block' && closeEvents === 1 &&
                    reflectedScript instanceof HTMLScriptElement && reflectedScript.async && reflectedScript.defer &&
                    reflectedScript.text.includes('answer') &&
                    reflectedScript.integrity === 'sha384-example' &&
                    reflectedScript.crossOrigin === 'anonymous' &&
                    linked.integrity === 'sha256-example' &&
                    linked.crossOrigin === 'use-credentials' && linked.as === 'script' &&
                    linked.relList.supports('preload') && linked.relList.supports('stylesheet') &&
                    !linked.relList.supports('prefetch') &&
                    anchor.relList.supports('noopener') &&
                    (() => { try { anchor.classList.supports('anything'); return false; }
                              catch (error) { return error instanceof TypeError; } })()
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
