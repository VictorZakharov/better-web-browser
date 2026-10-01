use super::*;

#[test]
fn owned_chromium_fixture_uses_the_same_pcm_oracles_in_unit_tests() {
    let html = format!(
        "<body><p id='fixture-status'></p><p id='audio-result'></p>\
        <div id='audio-failures'></div><div id='audio-observations'></div><script>\
        {}\n{}\n{}\n{}\n{}\nAudioRoutingChecks.run();</script>",
        include_str!("../../../../benchmarks/alpha/fixtures/audio-routing-contracts.js"),
        include_str!("../../../../benchmarks/alpha/fixtures/audio-routing-matrices.js"),
        include_str!("../../../../benchmarks/alpha/fixtures/audio-routing-feedback.js"),
        include_str!("../../../../benchmarks/alpha/fixtures/audio-routing-processing.js"),
        include_str!("../../../../benchmarks/alpha/fixtures/audio-routing-buffers.js"),
    );
    let dom = dom::parse_with_scripting(&html, true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#routing-fixture".into(),
        code: node.text_content(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut console = initial.console;
    let summary = dom
        .elements_named("p")
        .find(|node| node.attr("id").as_deref() == Some("audio-result"))
        .unwrap();
    // The convenience execute_html drain has a short wall-clock observation
    // window. Eighty independent asynchronous graphs may outlive it under
    // parallel-suite load. Drain bounded media tasks until the real fixture
    // publishes its completion marker; never replace or relax its PCM oracles.
    for _ in 0..128 {
        if !summary.text_content().is_empty() {
            break;
        }
        let outcome = runtime.advance_time(std::time::Duration::ZERO, 64);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        console.extend(outcome.console);
    }
    assert_eq!(summary.text_content(), "80/80 passed", "{console:?}");
    let failures = dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("audio-failures"))
        .unwrap()
        .text_content();
    assert!(failures.is_empty(), "{failures}");
    assert_eq!(console, ["log: Audio routing: 80/80 passed"]);
}
