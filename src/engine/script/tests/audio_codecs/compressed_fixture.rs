//! Execute the exact offline browser contracts through V8 during unit testing.
use super::*;
use crate::engine::script::audio_codecs::test_packets;

#[test]
fn offline_elementary_audio_fixture_checks_real_packets_and_lossless_roundtrips() {
    let source = format!(
        r#"
        const summary=document.createElement('p');summary.id='summary';document.body.append(summary);
        const results=document.createElement('tbody');results.id='results';document.body.append(results);
        {}
        {}
        {}
        {}
        {}
        await runAudioProbes();
        assert(summary.dataset.total==='21','all offline contracts selected');
        assert(summary.dataset.passed===summary.dataset.total,summary.dataset.failures);
        "#,
        include_str!("../../../../../tests/audio-codec-fixtures/probe.js"),
        test_packets::javascript_cases(),
        include_str!("../../../../../tests/elementary-audio-fixtures/decode.js"),
        include_str!("../../../../../tests/elementary-audio-fixtures/encode.js"),
        include_str!("../../../../../tests/elementary-audio-fixtures/lifecycle.js"),
    );
    check(&source);
}
