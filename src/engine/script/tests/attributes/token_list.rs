use super::*;

#[test]
fn optional_undefined_force_preserves_toggle_semantics_through_a_wrapper() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="target"></div><script>
        const classes = document.getElementById('target').classList;
        const wrapper = force => classes.toggle('open', force);
        const result = [wrapper(), classes.contains('open'), wrapper(undefined), classes.contains('open'),
            classes.toggle('open'), classes.toggle('open', undefined), classes.contains('open')];
        document.body.dataset.result = result.join('|');
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true|true|false|false|true|false|false")
    );
}

#[test]
fn explicit_force_keeps_boolean_conversion_and_token_validation_order() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="target"></div><script>
        const classes = document.getElementById('target').classList;
        const object = { valueOf() { throw Error('Boolean object conversion must not invoke valueOf'); } };
        const results = [classes.toggle('open', object), classes.toggle('open', true),
            classes.toggle('open', null), classes.toggle('open', false), classes.toggle('open', 0),
            classes.toggle('open', 1), classes.toggle('open', ''), classes.contains('open')];
        try { classes.toggle('', undefined); throw Error('missing token error'); }
        catch (error) { results.push(error.name); }
        try { classes.toggle('bad token', false); throw Error('missing token error'); }
        catch (error) { results.push(error.name); }
        document.body.dataset.result = results.join('|');
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true|true|false|false|false|true|false|false|SyntaxError|InvalidCharacterError")
    );
}
