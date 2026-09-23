use super::*;

#[test]
fn navigator_exposes_browser_identity_and_html_app_version() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        const ua = navigator.userAgent;
        if (!ua.startsWith('Mozilla/5.0 (') || !ua.includes('Chrome/') || !/Breeze\/\d+\.\d+\.\d+$/.test(ua))
            throw new Error('missing Breeze browser identity');
        if (navigator.appVersion !== ua.slice('Mozilla/'.length))
            throw new Error('legacy appVersion must follow the HTML algorithm');
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
