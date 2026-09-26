use super::cssom::result;
use super::media_queries::media_runtime;
use super::*;
use crate::engine::MediaEnvironment;

#[test]
fn match_media_serializes_queries_and_recovers_each_invalid_entry() {
    let (dom, _runtime) = media_runtime(
        r#"<body><script>
            const failures = [];
            const check = (name, yes) => { if (!yes) failures.push(name); };
            const canonical = matchMedia('ONLY SCREEN and (MIN-WIDTH:800px)');
            check('canonical', canonical.media === 'screen and (min-width: 800px)' &&
                canonical.matches);
            const empty = matchMedia('');
            check('empty', empty.media === '' && empty.matches);

            // Unknown conditions stay unknown under negation; malformed queries become "not all".
            // https://drafts.csswg.org/mediaqueries-4/#evaluating
            const unknown = matchMedia('not (future-feature)');
            check('unknown-not', unknown.media === 'not (future-feature)' && !unknown.matches);
            const malformed = matchMedia('not (width >= ');
            check('malformed-not', malformed.media === 'not all' && !malformed.matches);
            const invalidFirst = matchMedia('screen or (width > 900px), print');
            check('invalid-first', invalidFirst.media === 'not all, print' &&
                !invalidFirst.matches);
            const validLater = matchMedia('screen or (width > 900px), screen');
            check('valid-later', validLater.media === 'not all, screen' &&
                validLater.matches);
            const nestedComma = matchMedia('(width: bogus(10px, 20px)), print');
            check('nested-comma', nestedComma.media === '(width: bogus(10px, 20px)), print' &&
                !nestedComma.matches);
            document.body.setAttribute('data-result', failures.join(',') || 'pass');
        </script></body>"#,
        800.0,
        600.0,
        1.0,
        false,
    );
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn match_media_evaluates_range_boundaries_and_grouped_conditions() {
    let (dom, _runtime) = media_runtime(
        r#"<body><script>
            const failures = [];
            const check = (name, source, expected) => {
                const query = matchMedia(source);
                if (query.matches !== expected) failures.push(name + ':' + query.media);
            };
            check('inclusive', '(width >= 800px)', true);
            check('exclusive', '(width > 800px)', false);
            check('value-first', '(400px < width)', true);
            check('chained-inclusive', '(400px < width <= 800px)', true);
            check('chained-exclusive', '(400px < width < 800px)', false);
            check('reversed-chain', '(900px > width >= 800px)', true);
            check('group', '(height >= 600px) and ((width > 900px) or (orientation: landscape))', true);
            check('or', '(width > 900px) or (height >= 600px)', true);
            check('condition-not', 'not (width > 900px)', true);
            for (const source of [
                '(width > 900px) or (height >= 600px) and (orientation: landscape)',
                'screen or (width > 900px)'
            ]) {
                const query = matchMedia(source);
                if (query.media !== 'not all' || query.matches) failures.push('invalid:' + source);
            }
            check('mixed-direction-chain', '(400px < width > 900px)', false);
            document.body.setAttribute('data-result', failures.join(',') || 'pass');
        </script></body>"#,
        800.0,
        600.0,
        1.0,
        false,
    );
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn range_query_changes_dispatch_only_for_changed_results() {
    let (dom, mut runtime) = media_runtime(
        r#"<body><script>
            const range = matchMedia('(700px < width <= 900px)');
            const grouped = matchMedia('((width >= 900px) and (height >= 600px)) or (prefers-color-scheme: dark)');
            const invalid = matchMedia('not (future-feature)');
            const events = [];
            const listen = (name, query) => query.addEventListener('change', event => {
                events.push([name, event.matches, event.media === query.media,
                    event.isTrusted].join(':'));
                document.body.setAttribute('data-events', events.join(','));
            });
            listen('range', range);
            listen('grouped', grouped);
            listen('invalid', invalid);
            document.body.setAttribute('data-initial',
                [range.matches, grouped.matches, invalid.matches].join(','));
        </script></body>"#,
        800.0,
        600.0,
        1.0,
        false,
    );
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-initial").as_deref(),
        Some("true,false,false")
    );

    runtime.set_media_environment(MediaEnvironment::new(950.0, 600.0, 1.0, false));
    let resize = runtime.dispatch_user_input(UserInputEvent::Viewport {
        width: 950.0,
        height: 600.0,
        layout_width: 950.0,
        layout_height: 600.0,
        scale: 1.0,
    });
    assert!(
        resize.outcome.errors.is_empty(),
        "{:?}",
        resize.outcome.errors
    );
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("range:false:true:true,grouped:true:true:true")
    );

    runtime.dispatch_user_input(UserInputEvent::Viewport {
        width: 950.0,
        height: 600.0,
        layout_width: 950.0,
        layout_height: 600.0,
        scale: 1.0,
    });
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("range:false:true:true,grouped:true:true:true")
    );
}

#[test]
fn css_supports_parses_selector_queries_and_rejects_malformed_conditions() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const failures = [];
            const check = (name, yes) => { if (!yes) failures.push(name); };
            check('selector', CSS.supports('selector(div > .child)'));
            check('selector-list', CSS.supports('selector(:is(.a, .b))'));
            check('invalid-selector', !CSS.supports('selector(div >)'));
            check('unsupported-selector', !CSS.supports('selector(:unsupported-pseudo)'));
            check('selector-not', CSS.supports('not selector(:unsupported-pseudo)'));
            check('implicit-parens', CSS.supports('display: grid'));
            check('and', CSS.supports('(display: grid) and (position: sticky)'));
            check('or', CSS.supports('(display: grid) or (position: invalid)'));
            check('grouped', CSS.supports('((display: grid) and (position: sticky)) or (color: invalid)'));
            check('invalid-mixed', !CSS.supports('(display: grid) and (position: invalid) or (color: red)'));
            check('invalid-tail', !CSS.supports('(display: grid) and'));
            check('invalid-not', !CSS.supports('not selector('));
            document.body.setAttribute('data-result', failures.join(',') || 'pass');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn media_list_compares_canonical_queries_and_keeps_nested_commas_together() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const failures = [];
            const check = (name, yes) => { if (!yes) failures.push(name); };
            const sheet = new CSSStyleSheet({media: 'ONLY SCREEN and (MIN-WIDTH:700px), print'});
            const media = sheet.media;
            check('initial', media === sheet.media && media.length === 2 &&
                media.mediaText === 'screen and (min-width: 700px), print' &&
                media.item(0) === 'screen and (min-width: 700px)' &&
                media.item(1) === 'print' && media.item(2) === null);
            media.appendMedium('screen and (min-width: 700px)');
            check('canonical-duplicate', media.length === 2);
            media.appendMedium('ALL and (MAX-WIDTH:900px)');
            check('canonical-append', media.length === 3 &&
                media.item(2) === '(max-width: 900px)');
            media.deleteMedium('(max-width: 900px)');
            check('canonical-delete', media.length === 2);
            media.appendMedium('screen, print');
            media.deleteMedium('screen, print');
            check('multi-query-argument', media.length === 2);

            // A comma inside a function is not a query-list separator.
            media.mediaText = '(width: bogus(10px, 20px)), screen';
            check('nested-comma', media.length === 2 &&
                media.mediaText === '(width: bogus(10px, 20px)), screen' &&
                media.item(0) === '(width: bogus(10px, 20px))');
            media.mediaText = 'screen or (width > 900px), print';
            check('invalid-entry', media.length === 2 &&
                media.mediaText === 'not all, print');
            check('same-object', sheet.media === media);
            document.body.setAttribute('data-result', failures.join(',') || 'pass');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn cssom_condition_rules_share_media_and_supports_evaluation() {
    let (dom, outcome) = execute_html(
        r#"<style id=sheet>
            #target { color: black; background-color: green; }
            @media screen and (1000px < width <= 1300px) { #target { color: red; } }
            @supports selector(#target) { #target { background-color: blue; } }
        </style><body><p id=target>test</p><script>
            const failures = [];
            const check = (name, yes) => { if (!yes) failures.push(name); };
            const target = document.getElementById('target');
            const sheet = document.getElementById('sheet').sheet;
            const rules = sheet.cssRules;
            const media = rules[1], supports = rules[2];
            const colors = () => {
                const style = getComputedStyle(target);
                return [style.color, style.backgroundColor].join('|');
            };
            check('media-condition', media instanceof CSSMediaRule &&
                media.media === media.media &&
                media.conditionText === media.media.mediaText &&
                media.conditionText === 'screen and (1000px < width <= 1300px)' &&
                media.matches);
            check('supports-condition', supports instanceof CSSSupportsRule &&
                supports.conditionText === 'selector(#target)' &&
                supports.matches === CSS.supports(supports.conditionText) &&
                supports.matches);
            const readOnly = (rule, replacement) => {
                const original = rule.conditionText;
                try { rule.conditionText = replacement; }
                catch (error) { if (!(error instanceof TypeError)) return false; }
                return rule.conditionText === original;
            };
            check('readonly-condition', readOnly(media, 'print') &&
                readOnly(supports, 'not (display: grid)'));
            check('initial', colors() === 'rgb(255, 0, 0)|rgb(0, 0, 255)');
            media.media.mediaText = 'screen and (width > 1300px)';
            check('media-disabled', !media.matches &&
                media.conditionText === 'screen and (width > 1300px)' &&
                colors() === 'rgb(0, 0, 0)|rgb(0, 0, 255)');
            media.media.mediaText = 'screen and (width <= 1300px)';
            check('media-reenabled', media.matches &&
                colors() === 'rgb(255, 0, 0)|rgb(0, 0, 255)');
            sheet.insertRule('@supports selector(div >) { #target { background-color: red; } }', 3);
            check('invalid-support-rule', !rules[3].matches &&
                rules[3].conditionText === 'selector(div >)' &&
                colors() === 'rgb(255, 0, 0)|rgb(0, 0, 255)');
            sheet.deleteRule(2);
            check('removed-support-rule', colors() === 'rgb(255, 0, 0)|rgb(0, 128, 0)');
            sheet.insertRule('@supports selector(#target) { #target { background-color: blue; } }', 3);
            check('restored-support-rule', rules[3].matches &&
                colors() === 'rgb(255, 0, 0)|rgb(0, 0, 255)');
            document.body.setAttribute('data-result', failures.join(',') || 'pass');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
