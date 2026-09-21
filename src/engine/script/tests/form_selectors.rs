use super::*;

// Phase D coverage: validation selectors through the public script surface
// (`matches`, `querySelector(All)`, author stylesheets). Selector matching
// itself is unit-tested in `engine::css::selector_validity`.

fn check(html: &str) -> (dom::Dom, ScriptOutcome) {
    let (dom, outcome) = execute_html(html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, outcome)
}

#[test]
fn matches_query_and_cascade_agree_on_validity() {
    let (dom, _) = check(
        r#"<head><style>input:invalid { display: block; }</style></head><body>
            <form><input id=empty required><input id=full required value=x></form>
            <output></output><script>
                const empty = document.getElementById('empty');
                const full = document.getElementById('full');
                const invalid = document.querySelectorAll('input:invalid');
                document.querySelector('output').textContent = [
                    empty.matches(':invalid'), full.matches(':invalid'),
                    full.matches(':valid'), empty.matches('input:not(:valid)'),
                    empty.matches(':is(:invalid, :required)'),
                    invalid.length, invalid[0] === empty,
                    getComputedStyle(empty).display, getComputedStyle(full).display
                ].join('|');
            </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|false|true|true|true|1|true|block|inline-block"
    );
}

#[test]
fn scripted_edit_updates_matches_and_author_style() {
    let (dom, _) = check(
        r#"<head><style>input:invalid { display: block; }</style></head><body>
            <form id=f><input required></form><output></output><script>
                const input = document.querySelector('input');
                const form = document.getElementById('f');
                const seen = [
                    input.matches(':invalid'), form.matches(':invalid'),
                    getComputedStyle(input).display
                ];
                input.value = 'fixed';
                seen.push(
                    input.matches(':valid'), form.matches(':valid'),
                    document.querySelectorAll('input:invalid').length,
                    getComputedStyle(input).display
                );
                document.querySelector('output').textContent = seen.join('|');
            </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|block|true|true|0|inline-block"
    );
}

#[test]
fn fieldset_and_external_controls_aggregate() {
    let (dom, _) = check(
        r#"<body><form id=f></form>
            <fieldset id=s><input form=f required></fieldset>
            <fieldset id=t><input required value=x></fieldset>
            <output></output><script>
                const form = document.getElementById('f');
                const s = document.getElementById('s');
                const t = document.getElementById('t');
                const lone = document.querySelector('fieldset input');
                document.querySelector('output').textContent = [
                    form.matches(':invalid'), s.matches(':invalid'), t.matches(':valid'),
                    lone.matches(':invalid')
                ].join('|');
                lone.value = 'fixed';
                document.querySelector('output').textContent += '|' + [
                    form.matches(':valid'), s.matches(':valid')
                ].join('|');
            </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|true|true|true|true"
    );
}

#[test]
fn required_optional_and_range_selectors() {
    let (dom, _) = check(
        r#"<body><form>
            <input id=a required><input id=b>
            <input id=c type=number min=1 max=10 value=20>
            <input id=d type=number min=1 max=10 value=5>
            <input id=e type=text min=1 value=0>
            </form><output></output><script>
                const byId = id => document.getElementById(id);
                document.querySelector('output').textContent = [
                    byId('a').matches(':required'), byId('b').matches(':optional'),
                    byId('c').matches(':out-of-range'), byId('d').matches(':in-range'),
                    byId('e').matches(':in-range'), byId('e').matches(':out-of-range'),
                    document.querySelectorAll(':out-of-range').length
                ].join('|');
            </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|true|true|false|false|1"
    );
}

#[test]
fn pattern_mismatch_reaches_matches_through_the_verdict_cache() {
    let (dom, _) = check(
        r#"<body><form><input id=p pattern="a+" value=""></form><output></output><script>
                const input = document.getElementById('p');
                const seen = [input.matches(':valid')];
                // The value setter refreshes the stored verdict on this realm,
                // so scopeless selector matching observes the mismatch.
                input.value = 'bbb';
                seen.push(input.matches(':invalid'), input.matches(':valid'));
                input.value = 'aaa';
                seen.push(input.matches(':valid'), input.matches(':invalid'));
                document.querySelector('output').textContent = seen.join('|');
            </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|false|true|false"
    );
}

#[test]
fn radio_group_check_updates_every_peer_selector() {
    let (dom, _) = check(
        r#"<body><form>
            <input type=radio name=g required value=1>
            <input type=radio name=g value=2>
            </form><output></output><script>
                const [first, second] = document.querySelectorAll('input');
                const seen = [first.matches(':invalid'), second.matches(':valid')];
                second.click();
                seen.push(
                    first.matches(':valid'), second.matches(':valid'),
                    document.querySelector('form').matches(':valid')
                );
                document.querySelector('output').textContent = seen.join('|');
            </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|false|true|true|true"
    );
}
