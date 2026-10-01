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
    let (dom, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let summary = dom
        .elements_named("p")
        .find(|node| node.attr("id").as_deref() == Some("audio-result"))
        .unwrap()
        .text_content();
    assert_eq!(summary, "80/80 passed", "{:?}", outcome.console);
    let failures = dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("audio-failures"))
        .unwrap()
        .text_content();
    assert!(failures.is_empty(), "{failures}");
    assert_eq!(outcome.console, ["log: Audio routing: 80/80 passed"]);
}
