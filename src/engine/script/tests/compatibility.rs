use super::*;

#[test]
fn global_object_exposes_the_window_interface_prototype_chain() {
    let (dom, outcome) = execute_html(
        r#"<body><output>no</output><script>
            let illegalConstructor = false;
            try { new Window(); } catch (error) { illegalConstructor = error instanceof TypeError; }
            if (window === self && window === globalThis && window instanceof Window &&
                Window.prototype instanceof EventTarget && window.constructor === Window &&
                document.defaultView === window && illegalConstructor)
                document.querySelector('output').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn window_named_properties_track_scoped_tree_and_attribute_mutations() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="initial"></div><output id="status">no</output><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            check('initial', window.initial === document.getElementById('initial'));
            const container = document.createElement('section');
            container.innerHTML = '<div id="inserted"></div><form name="namedForm"></form>';
            document.body.appendChild(container);
            check('insert', window.inserted === container.firstChild && window.namedForm === container.lastChild);
            container.firstChild.id = 'renamed';
            check('rename-old', window.inserted === undefined);
            check('rename-new', window.renamed === container.firstChild);
            container.remove();
            check('remove-id', window.renamed === undefined);
            check('remove-name', window.namedForm === undefined);
            document.getElementById('status').textContent = failures.length ? failures.join(',') : 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn contextual_of_is_valid_as_a_lexical_binding() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            "use strict";
            let of = "yes";
            document.getElementById("status").textContent = of;
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn nested_arrow_parameter_destructuring_keeps_each_callback_argument() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            class CardCollector {
                constructor() { this.cards = []; }
                add(sections) {
                    sections.map(section => {
                        section.cards?.map(({ type: section }) => {
                            if (section) this.cards.push(section);
                        });
                    });
                }
            }
            const collector = new CardCollector();
            collector.add([
                { cards: [{ type: 'article' }, { type: 'video' }] },
                { cards: [{ type: 'infopane' }] }
            ]);
            document.getElementById('status').textContent = collector.cards.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "article,video,infopane"
    );
}

#[test]
fn performance_timeline_records_retrieves_and_clears_user_timing_entries() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            const start = performance.mark('start', { startTime: 2, detail: { phase: 1 } });
            const end = performance.mark('end', { startTime: 7 });
            const span = performance.measure('span', 'start', 'end');
            check('constructors', performance instanceof Performance &&
                start instanceof PerformanceMark && span instanceof PerformanceMeasure &&
                start instanceof PerformanceEntry);
            check('mark', start.entryType === 'mark' && start.duration === 0 &&
                start.detail.phase === 1);
            check('measure', span.startTime === 2 && span.duration === 5 &&
                performance.getEntriesByName('span', 'measure')[0] === span);
            check('filters', performance.getEntriesByType('mark').length === 2 &&
                performance.getEntriesByName('start').length === 1 &&
                performance.getEntries().length === 3);
            let missing = false;
            try { performance.measure('missing', 'absent'); }
            catch (error) { missing = error.name === 'SyntaxError'; }
            check('missing-mark', missing);
            performance.clearMarks('start');
            performance.clearMeasures();
            check('clear', performance.getEntriesByType('mark').length === 1 &&
                performance.getEntriesByType('measure').length === 0);
            performance.setResourceTimingBufferSize(300);
            document.getElementById('status').textContent = failures.length ? failures.join(',') : 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn url_search_params_preserve_order_and_support_value_overloads() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            const params = new URLSearchParams('a=b&c=d&a=e');
            params.set('a', 'B');
            params.append('space', 'a b');
            params.append('first', 'one');
            params.append('first', 'two');
            params.delete('first', 'one');
            if (
                params.toString() === 'a=B&c=d&space=a+b&first=two' &&
                params.has('a', 'B') && !params.has('a', 'b') &&
                params.has('a', undefined) && params.values().next().value === 'B' &&
                params.size === 4
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
fn queue_microtask_validates_and_invokes_without_arguments() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">no</div><script>
            let threw = false;
            try { queueMicrotask(); } catch (error) { threw = error instanceof TypeError; }
            queueMicrotask(function() {
                if (threw && arguments.length === 0) document.getElementById('status').textContent = 'yes';
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn exposes_namespace_aware_nodes_fragments_and_html_interfaces() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><div id=""></div><div id="status">no</div><script>
            const svg = document.createElementNS('http://www.w3.org/2000/svg', 's:mixedCase');
            const html = document.createElementNS('http://www.w3.org/1999/xhtml', 'div');
            const fragment = document.createDocumentFragment();
            const fixture = document.createElement('div');
            fixture.id = 'old';
            document.body.appendChild(fixture);
            fixture.outerHTML = '<div id="replacement"></div>';
            if (
                svg.tagName === 's:mixedCase' && svg.nodeName === svg.tagName &&
                svg.localName === 'mixedCase' && svg.prefix === 's' &&
                svg.namespaceURI === 'http://www.w3.org/2000/svg' &&
                html.tagName === 'DIV' && html instanceof HTMLDivElement &&
                fragment.nodeType === 11 && fragment.nodeName === '#document-fragment' &&
                fragment.ownerDocument === document && document.doctype.nodeName === 'html' &&
                document.getElementById('') === null && document.getElementById('old') === null &&
                document.getElementById('replacement') instanceof HTMLDivElement
            ) document.getElementById('status').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}

#[test]
fn documents_clone_and_import_nodes_with_the_target_documents_semantics() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><table id="fixture"><tbody><tr><td id="target">text</td></tr></tbody></table>
        <div id="status">no</div><script>
            const xmlDocument = document.implementation.createDocument(
                'http://www.w3.org/1999/xhtml', 'foo:div', null);
            const imported = document.importNode(xmlDocument.documentElement, true);
            const documentClone = document.cloneNode(true);
            const newDocument = new Document();
            const clonedRoot = document.documentElement.cloneNode(true);
            const appendedRoot = newDocument.appendChild(clonedRoot);
            const htmlDocument = document.implementation.createHTMLDocument('copy');
            htmlDocument.body.appendChild(document.getElementById('fixture').cloneNode(true));
            const checks = [
                ['xml-view', xmlDocument.defaultView === null],
                ['xml-tag', xmlDocument.documentElement.tagName === 'foo:div'],
                ['import-owner', imported.ownerDocument === document],
                ['import-tag', imported.tagName === 'FOO:DIV'],
                ['clone-view', documentClone.defaultView === null],
                ['clone-tree', documentClone.getElementById('target').textContent === 'text'],
                ['new-identity', appendedRoot === clonedRoot],
                ['new-owner', newDocument.documentElement.ownerDocument === newDocument],
                ['new-tree', newDocument.getElementById('target').textContent === 'text'],
                ['html-title', htmlDocument.title === 'copy'],
                ['html-tree', htmlDocument.getElementById('target').textContent === 'text']
            ];
            const failures = checks.filter(([, passed]) => !passed).map(([name]) => name);
            document.getElementById('status').textContent = failures.join(',') || 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}

#[test]
fn adopt_node_preserves_identity_detaches_and_updates_subtree_ownership() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><div id="status">no</div><script>
            const sourceDocument = document.implementation.createHTMLDocument('source');
            const section = sourceDocument.createElement('section');
            section.innerHTML = '<span id="adopted-child">text</span>';
            sourceDocument.body.appendChild(section);
            const adopted = document.adoptNode(section);
            const template = document.createElement('template');
            template.innerHTML = '<strong>template</strong>';
            const content = document.adoptNode(template.content);
            const checks = [
                adopted === section,
                adopted.parentNode === null,
                adopted.ownerDocument === document,
                adopted.firstChild.ownerDocument === document,
                sourceDocument.body.children.length === 0,
                content === template.content,
                content.ownerDocument === document
            ];
            document.body.appendChild(adopted);
            checks.push(document.getElementById('adopted-child') === adopted.firstChild);
            document.getElementById('status').textContent = checks.every(Boolean) ? 'yes' : 'no';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}

#[test]
fn document_write_preserves_one_tokenizer_stream_and_url_serialization() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body>
        <div><a href='?notin=&notin;&not;&;& &'>Link</a><p>Text: &notin;&not;</p></div>
        <script>
            const markup = "<div><a href='?notin=&notin;&not;&;& &'>Link</a><p>Text: &notin;&not;</p></div>";
            for (let index = 0; index < markup.length; index++) document.write(markup.charAt(index));
        </script>
        <p id="status">no</p><script>
            const divs = document.getElementsByTagName('div');
            const writtenHref = divs[1].firstChild.href;
            const query = writtenHref.substring(writtenHref.indexOf('?'));
            if (divs.length === 2 && divs[1].childNodes.length === 2 &&
                query === '?notin=%E2%88%89%C2%AC&;&%20&' &&
                divs[1].lastChild.textContent === 'Text: \u2209\u00AC') {
                document.getElementById('status').textContent = 'yes';
            }
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}

#[test]
fn html_collections_are_live_indexed_named_and_same_object() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body>
        <img id="hero"><form name="search"></form><a id="linked" href="/target">link</a>
        <a id="not-linked">plain</a><embed name="viewer"><div id="fixture"></div>
        <div id="status">no</div><script>
            const initialScripts = document.scripts.length;
            const scripts = document.scripts;
            const addedScript = document.createElement('script');
            addedScript.id = 'loader';
            document.body.appendChild(addedScript);

            const spans = document.getElementById('fixture').getElementsByTagName('span');
            const span = document.createElement('span');
            span.id = 'live-span';
            document.getElementById('fixture').appendChild(span);

            const checks = [
                ['interface', scripts instanceof HTMLCollection &&
                    Object.prototype.toString.call(scripts) === '[object HTMLCollection]'],
                ['same-object', scripts === document.scripts && document.plugins === document.embeds],
                ['live-index', scripts.length === initialScripts + 1 &&
                    scripts.item(scripts.length - 1) === addedScript && scripts[scripts.length - 1] === addedScript],
                ['named', scripts.namedItem('loader') === addedScript && scripts.loader === addedScript],
                ['array-from', Array.from(scripts).includes(addedScript)],
                ['document-filters', document.images.hero === document.getElementById('hero') &&
                    document.forms.search === document.querySelector('form') &&
                    document.links.length === 1 && document.links.linked === document.getElementById('linked') &&
                    document.embeds.viewer === document.querySelector('embed')],
                ['element-live', spans.length === 1 && spans[0] === span]
            ];
            addedScript.remove();
            checks.push(['live-remove', scripts.length === initialScripts]);
            let illegalConstructor = false;
            try { new HTMLCollection(); } catch (error) { illegalConstructor = error instanceof TypeError; }
            checks.push(['constructor', illegalConstructor]);
            document.getElementById('status').textContent =
                checks.filter(([, passed]) => !passed).map(([name]) => name).join(',') || 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}

#[test]
fn caught_native_errors_expose_a_non_enumerable_stack_and_console_preserves_it() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><div id="status">no</div><script>
            function triggerFailure() { return missingBinding.member; }
            let caught;
            try { triggerFailure(); } catch (error) { caught = error; }
            const descriptor = Object.getOwnPropertyDescriptor(caught, 'stack');
            const checks = [
                caught instanceof ReferenceError,
                typeof caught.stack === 'string',
                caught.stack.includes('ReferenceError'),
                caught.stack.includes('triggerFailure'),
                descriptor && descriptor.enumerable === false
            ];
            console.error(caught);
            document.getElementById('status').textContent = checks.every(Boolean) ? 'yes' : 'no';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
    assert!(
        outcome.console.iter().any(|message| {
            message.starts_with("error: ReferenceError") && message.contains("triggerFailure")
        }),
        "{:?}",
        outcome.console
    );
}

#[test]
fn window_named_access_and_cssom_use_the_computed_cascade() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><style>
            .outer { opacity: 0.5 !important; font-size: 18px !important; line-height: 2em;
                     position: relative; z-index: -4; }
            html { z-index: inherit; position: inherit; overflow: inherit; background-color: inherit; }
        </style><body><p class="outer" id="el" style="opacity: 1; font-size: 36px">text</p>
        <div id="status">no</div><script>
            const style = getComputedStyle(el);
            const root = getComputedStyle(document.documentElement);
            if (
                el === document.getElementById('el') && style.opacity === '0.5' &&
                style.fontSize === '18px' && style.lineHeight === '36px' &&
                style.position === 'relative' && style.zIndex === '-4' &&
                root.zIndex === 'auto' && root.position === 'static' &&
                root.overflow === 'visible' && root.backgroundColor === 'rgba(0, 0, 0, 0)'
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
fn assigning_element_style_forwards_to_the_same_css_text_declaration() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><div id="target" style="color: red"></div><output>no</output>
        <script>
            const target = document.getElementById('target');
            const declaration = target.style;
            target.style = 'display: grid; color: blue';
            document.querySelector('output').textContent = [
                target.style === declaration,
                declaration.cssText,
                declaration.display,
                declaration.color,
                target.getAttribute('style')
            ].join('|');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|display: grid; color: blue|grid|blue|display: grid; color: blue"
    );
}
