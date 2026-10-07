use super::*;
use std::sync::Arc;

const CONCURRENCY: &str = r#"
    const value = navigator.hardwareConcurrency;
    if (!Number.isInteger(value) || value < 1 || value > 16)
        throw Error('invalid admitted processor count');
    const descriptor = Object.getOwnPropertyDescriptor(navigator, 'hardwareConcurrency');
    if (typeof descriptor.get !== 'function' || descriptor.set !== undefined || !descriptor.enumerable)
        throw Error('hardwareConcurrency is not readonly');
    try { (() => { 'use strict'; navigator.hardwareConcurrency = 0; })(); }
    catch (error) { if (!(error instanceof TypeError)) throw error; }
    if (navigator.hardwareConcurrency !== value) throw Error('capacity was author mutable');
"#;

#[test]
fn navigator_hardware_concurrency_reports_real_bounded_readonly_capacity() {
    let (dom, outcome) = execute_html(&format!(
        "<body><script>{CONCURRENCY};document.body.dataset.count=String(value);</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-count"),
        Some(super::super::runtime::platform_info::hardware_concurrency().to_string())
    );
}

#[test]
fn worker_reports_the_same_admitted_capacity_as_window() {
    let (_, outcome) = WorkerRuntime::start(
        "https://example.test/worker.js",
        &format!("{CONCURRENCY};postMessage(String(value));"),
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected fetch {url}"))),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.messages,
        [format!(
            "\"{}\"",
            super::super::runtime::platform_info::hardware_concurrency()
        )]
    );
}

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
