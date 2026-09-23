use super::*;

#[test]
fn navigator_defaults_to_honest_breeze_identity() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        const ua = navigator.userAgent;
        if (!/^Breeze\/\d+\.\d+\.\d+$/.test(ua))
            throw new Error('Breeze must be the default identity');
        if (navigator.appVersion !== '' || navigator.product !== 'Gecko')
            throw new Error('legacy NavigatorID values are inconsistent');
        document.body.dataset.result = 'passed';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("passed")
    );
}
