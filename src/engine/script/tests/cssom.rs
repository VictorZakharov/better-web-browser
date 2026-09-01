use super::*;

fn result(dom: &super::super::super::dom::Dom) -> Option<String> {
    dom.elements_named("body")
        .next()
        .and_then(|body| body.attr("data-result"))
}

fn execute_html_with_stylesheets(
    html: &str,
    stylesheets: Vec<(String, String)>,
) -> (super::super::super::dom::Dom, ScriptOutcome) {
    let dom = crate::engine::dom::parse_with_scripting(html, true);
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
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_document_stylesheets(&stylesheets);
    let outcome = runtime.execute_initial(&scripts);
    (dom, outcome)
}

#[test]
fn document_stylesheets_are_live_same_object_and_owned_by_style_elements() {
    let (dom, outcome) = execute_html(
        r#"<head><style id="first" media="screen">body { color: red; }</style></head>
        <body><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            const list = document.styleSheets;
            const owner = document.getElementById('first');
            const sheet = owner.sheet;
            check('constructors', owner instanceof HTMLStyleElement &&
                list instanceof StyleSheetList);
            check('same-object', list === document.styleSheets && sheet === list[0] &&
                list.item(0) === sheet);
            check('metadata', sheet.ownerNode === owner && sheet.href === null &&
                sheet.media.mediaText === 'screen');
            check('rules', sheet.cssRules.length === 1 &&
                sheet.cssRules[0].selectorText === 'body');
            owner.textContent = 'main { display: block; }';
            check('live-source', owner.sheet === sheet && list[0] === sheet &&
                sheet.cssRules[0].selectorText === 'main');
            const second = document.createElement('style');
            second.textContent = 'p { color: blue; }';
            document.head.appendChild(second);
            check('live-add', list.length === 2 && Array.from(list)[1] === second.sheet);
            second.remove();
            check('live-remove', list.length === 1 && second.sheet === null);
            document.body.setAttribute('data-result', failures.length ? failures.join(',') : 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn custom_properties_are_case_sensitive_and_expose_resolved_computed_values() {
    let (dom, outcome) = execute_html(
        r#"<style>
            :root { --Accent: 12px; --alias: var(--Accent); }
            #target { --Accent: 24px; }
        </style><body><div id="target" style="--Inline: red"></div><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            const target = document.getElementById('target');
            const computed = getComputedStyle(target);
            check('computed-own', computed.getPropertyValue('--Accent') === '24px');
            check('computed-case', computed.getPropertyValue('--accent') === '');
            check('computed-var', computed.getPropertyValue('--alias') === '24px');
            check('inline-read', target.style.getPropertyValue('--Inline') === 'red');
            check('inline-case', target.style.getPropertyValue('--inline') === '');
            target.style.setProperty('--MixedCase', 'blue');
            check('inline-write', target.style.getPropertyValue('--MixedCase') === 'blue');
            check('inline-remove', target.style.removeProperty('--MixedCase') === 'blue' &&
                target.style.getPropertyValue('--MixedCase') === '');

            const sheet = new CSSStyleSheet();
            sheet.replaceSync('#target { --RuleCase: green; }');
            const ruleStyle = sheet.cssRules[0].style;
            check('rule-read', ruleStyle.getPropertyValue('--RuleCase') === 'green');
            check('rule-case', ruleStyle.getPropertyValue('--rulecase') === '');
            ruleStyle.setProperty('--SecondCase', 'purple');
            check('rule-write', ruleStyle.getPropertyValue('--SecondCase') === 'purple');
            document.body.setAttribute('data-result', failures.length ? failures.join(',') : 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn linked_stylesheets_expose_loaded_same_origin_rules_and_guard_cross_origin_rules() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<head>
            <link id="local" rel="alternate STYLESHEET" href="/site.css" title="Site">
            <link id="remote" rel="stylesheet" href="https://cdn.example.net/site.css">
        </head><body><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            const local = document.getElementById('local');
            const remote = document.getElementById('remote');
            check('constructors', local instanceof HTMLLinkElement && remote instanceof HTMLLinkElement);
            check('list', document.styleSheets.length === 2 && document.styleSheets[0] === local.sheet);
            check('local-metadata', local.sheet.href === 'https://example.com/site.css' &&
                local.sheet.ownerNode === local && local.sheet.title === 'Site');
            check('local-rules', local.sheet.cssRules.length === 1 &&
                local.sheet.cssRules[0].selectorText === 'body');
            try { remote.sheet.cssRules; failures.push('cross-origin:missing'); }
            catch (error) { if (error.name !== 'SecurityError') failures.push('cross-origin:' + error.name); }
            try { local.sheet.replaceSync('body {}'); failures.push('replace:missing'); }
            catch (error) { if (error.name !== 'NotAllowedError') failures.push('replace:' + error.name); }
            document.body.setAttribute('data-result', failures.length ? failures.join(',') : 'pass');
        </script></body>"#,
        vec![
            (
                "https://example.com/site.css".into(),
                "body { color: green; }".into(),
            ),
            (
                "https://cdn.example.net/site.css".into(),
                "body { color: purple; }".into(),
            ),
        ],
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn constructed_stylesheets_expose_live_rules_and_apply_to_document_and_shadow_roots() {
    let (dom, outcome) = execute_html(
        r#"<style>#target { color: #010101; }</style>
        <body><p id="target">document</p><x-card id="host"></x-card><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            const sheet = new CSSStyleSheet({ baseURL: '/assets/', media: 'screen' });
            sheet.replaceSync('@import "/ignored.css"; #target { color: #123456; }');
            check('metadata', sheet.href === null && sheet.ownerNode === null &&
                sheet.media.mediaText === 'screen' && sheet.cssRules === sheet.rules);
            check('imports', sheet.cssRules.length === 1 &&
                sheet.cssRules[0] instanceof CSSStyleRule);
            check('rules', sheet.cssRules.item(0) === sheet.cssRules[0] &&
                sheet.cssRules[0].selectorText === '#target');
            document.adoptedStyleSheets = [sheet];
            check('document-style', getComputedStyle(document.getElementById('target')).color ===
                'rgb(18, 52, 86)');
            const override = new CSSStyleSheet();
            override.replaceSync('#target { color: #fedcba; }');
            document.adoptedStyleSheets.push(override);
            check('cascade-order', getComputedStyle(document.getElementById('target')).color ===
                'rgb(254, 220, 186)');
            document.adoptedStyleSheets.pop();
            check('removal', getComputedStyle(document.getElementById('target')).color ===
                'rgb(18, 52, 86)');

            sheet.cssRules[0].style.setProperty('color', '#abcdef');
            check('live-rule-style', getComputedStyle(document.getElementById('target')).color ===
                'rgb(171, 205, 239)');
            sheet.insertRule('#target { background-color: #010203; }', 1);
            check('insert-rule', sheet.cssRules.length === 2 &&
                getComputedStyle(document.getElementById('target')).backgroundColor ===
                    'rgb(1, 2, 3)');
            sheet.deleteRule(1);

            const root = document.getElementById('host').attachShadow({ mode: 'open' });
            root.innerHTML = '<p id="inside">shadow</p>';
            const shadowSheet = new CSSStyleSheet();
            shadowSheet.replaceSync('#inside { color: #654321; }');
            root.adoptedStyleSheets.push(shadowSheet);
            check('shadow-style', getComputedStyle(root.getElementById('inside')).color ===
                'rgb(101, 67, 33)');
            document.body.setAttribute('data-result', failures.length ? failures.join(',') : 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
    assert!(outcome.invalidation.rebuild_style_rules);
}

#[test]
fn adopted_stylesheet_array_mutations_validate_identity_and_document_ownership() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            const expect = (name, callback, expected) => {
                try { callback(); failures.push(name + ':missing'); }
                catch (error) { if (error.name !== expected) failures.push(name + ':' + error.name); }
            };
            const first = new CSSStyleSheet();
            const second = new CSSStyleSheet();
            const adopted = document.adoptedStyleSheets;
            check('same-object', adopted === document.adoptedStyleSheets && Array.isArray(adopted));
            adopted.push(first, second);
            check('push', adopted.length === 2 && adopted[0] === first && adopted[1] === second);
            adopted.reverse();
            check('reverse', adopted[0] === second && adopted[1] === first);
            adopted.fill(first, 0, 1);
            check('fill', adopted[0] === first && adopted[1] === first);

            let prototypeSetterCalls = 0;
            Object.defineProperty(Array.prototype, '2', {
                configurable: true,
                set() { prototypeSetterCalls++; }
            });
            try { adopted[2] = second; } finally { delete Array.prototype[2]; }
            check('prototype-setter', prototypeSetterCalls === 0 && adopted[2] === second);
            expect('type', () => adopted.push({}), 'TypeError');
            const otherDocument = document.implementation.createHTMLDocument('other');
            expect('document', () => { otherDocument.adoptedStyleSheets = [first]; },
                'NotAllowedError');
            check('failed-mutations-atomic', adopted.length === 3);
            document.body.setAttribute('data-result', failures.length ? failures.join(',') : 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn asynchronous_replace_settles_with_the_sheet_and_rejects_overlapping_replacement() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const sheet = new CSSStyleSheet();
            const first = sheet.replace('body { color: #102030; }');
            const second = sheet.replace('body { color: red; }');
            Promise.all([
                first.then(value => value === sheet ? 'resolved' : 'wrong'),
                second.then(() => 'missing', error => error.name)
            ]).then(results => document.body.setAttribute(
                'data-result',
                results.join(':') + ':' + sheet.cssRules.length
            ));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("resolved:NotAllowedError:1"));
}

#[test]
fn separate_document_realms_do_not_retain_adopted_stylesheets() {
    let (first, first_outcome) = execute_html(
        r#"<body><script>
            const sheet = new CSSStyleSheet();
            sheet.replaceSync('body { color: red; }');
            document.adoptedStyleSheets = [sheet];
            document.body.setAttribute('data-result', String(document.adoptedStyleSheets.length));
        </script></body>"#,
    );
    assert!(
        first_outcome.errors.is_empty(),
        "{:?}",
        first_outcome.errors
    );
    assert_eq!(result(&first).as_deref(), Some("1"));
    drop(first);

    let (next, next_outcome) = execute_html(
        r#"<body><script>
            document.body.setAttribute('data-result', String(document.adoptedStyleSheets.length));
        </script></body>"#,
    );
    assert!(next_outcome.errors.is_empty(), "{:?}", next_outcome.errors);
    assert_eq!(result(&next).as_deref(), Some("0"));
}

#[test]
fn css_supports_uses_the_same_conservative_capability_table_as_feature_queries() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            check('declaration-overload', CSS.supports('display', 'grid'));
            check('condition-overload', CSS.supports('(display: grid) and (opacity: 25%)'));
            check('custom-property', CSS.supports('--theme-accent', 'anything'));
            check('unsupported-property', !CSS.supports('box-shadow', '0 0 1px black'));
            check('unsupported-value', !CSS.supports('position', 'sticky'));
            let missingArgument = false;
            try { CSS.supports(); } catch (error) { missingArgument = error instanceof TypeError; }
            check('argument-conversion', missingArgument);
            document.body.setAttribute('data-result', failures.length ? failures.join(',') : 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
