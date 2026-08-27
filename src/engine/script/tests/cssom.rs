use super::*;

fn result(dom: &super::super::super::dom::Dom) -> Option<String> {
    dom.elements_named("body")
        .next()
        .and_then(|body| body.attr("data-result"))
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
