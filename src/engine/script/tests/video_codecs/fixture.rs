//! Keep the owned browser fixture executable in ordinary V8 unit checks.
use super::*;

#[test]
fn offline_av1_fixture_checks_real_pixels_and_native_resource_contracts() {
    check(&format!(
        r#"
        const summary=document.createElement('p');summary.id='summary';document.body.append(summary);
        const results=document.createElement('tbody');results.id='results';document.body.append(results);
        {}
        {}
        {}
        await runAudioProbes();
        assert(summary.dataset.total==='7','all AV1 contracts selected');
        assert(summary.dataset.passed===summary.dataset.total,summary.dataset.failures);
        "#,
        include_str!("../../../../../tests/audio-codec-fixtures/probe.js"),
        test_packets::javascript_cases(),
        include_str!("../../../../../tests/video-codec-fixtures/video.js"),
    ));
}
