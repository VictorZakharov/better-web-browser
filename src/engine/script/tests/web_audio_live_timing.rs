use super::*;
use std::time::Duration;

fn evaluate(runtime: &mut ScriptRuntime, node: &NodeRef, code: &str) -> ScriptOutcome {
    runtime.execute_additional_with_loader(&[input(node, code)], None)
}

fn input(node: &NodeRef, code: &str) -> ScriptInput {
    ScriptInput {
        source_url: "https://example.com/#audio-clock".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }
}

fn next_pcm(runtime: &mut ScriptRuntime) -> (u32, Vec<u8>) {
    for _ in 0..30 {
        let outcome = runtime.advance_time(Duration::from_millis(10), 8);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        for action in outcome.graph_audio_actions {
            if let ScriptGraphAudioAction::Queue { stream_id, pcm, .. } = action {
                return (stream_id, pcm);
            }
        }
    }
    panic!("audio produced no PCM");
}

fn running_context() -> (ScriptRuntime, NodeRef, u32) {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let started = runtime.execute_initial(&[input(
        &node,
        "window.context = new AudioContext();
         const bed = context.createConstantSource(); bed.offset.value = .125;
         bed.connect(context.destination); bed.start();",
    )]);
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    runtime.set_audio_activation(true);
    let activated = evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    assert!(activated.errors.is_empty(), "{:?}", activated.errors);
    let (id, _) = next_pcm(&mut runtime);
    (runtime, node, id)
}

#[test]
fn a_starved_output_refills_in_a_bounded_batch_before_yielding() {
    let (mut runtime, _, stream_id) = running_context();
    // Initial submission + two immediate refills. A third acknowledgement
    // queues a media task instead of recursively filling an unbounded output.
    for inline in 0..3 {
        let accepted =
            runtime.deliver_graph_audio_status_with_queue(stream_id, "accepted", Some(0));
        assert!(accepted.errors.is_empty(), "{:?}", accepted.errors);
        let chunks = accepted
            .graph_audio_actions
            .iter()
            .filter(|action| matches!(action, ScriptGraphAudioAction::Queue { .. }))
            .count();
        assert_eq!(chunks, usize::from(inline < 2));
    }
    let (next_id, _) = next_pcm(&mut runtime);
    assert_eq!(next_id, stream_id);
}

#[test]
fn new_short_sound_starts_at_the_unrendered_quantum_after_backpressure() {
    let (mut runtime, node, stream_id) = running_context();
    let accepted = runtime.deliver_graph_audio_status_with_queue(stream_id, "accepted", Some(6000));
    assert!(accepted.errors.is_empty(), "{:?}", accepted.errors);
    let state_change = runtime.advance_time(Duration::ZERO, 8);
    assert!(state_change.errors.is_empty(), "{:?}", state_change.errors);
    assert!(state_change.graph_audio_actions.is_empty());
    runtime
        .host
        .borrow_mut()
        .pending_graph_audio_actions
        .push(ScriptGraphAudioAction::Queue {
            stream_id: 99,
            format: crate::media_protocol::GraphPcmFormat {
                sample_rate: 48000,
                channels: 2,
            },
            pcm: vec![0; 4],
        });
    let refused = runtime.advance_time(Duration::from_millis(10), 1);
    assert!(refused.errors.is_empty(), "{:?}", refused.errors);
    assert!(matches!(
        refused.graph_audio_actions.as_slice(),
        [ScriptGraphAudioAction::Queue { stream_id: 99, .. }]
    ));
    let created = evaluate(
        &mut runtime,
        &node,
        "const short = context.createConstantSource(); short.offset.value = .25;
         short.connect(context.destination); short.start();
         short.stop(context.currentTime + .5 / context.sampleRate);",
    );
    assert!(created.errors.is_empty(), "{:?}", created.errors);
    let (id, retained) = next_pcm(&mut runtime);
    assert_eq!(id, stream_id);
    assert!(
        retained
            .chunks_exact(2)
            .all(|value| value == 4096_i16.to_le_bytes())
    );
    runtime.deliver_graph_audio_status_with_queue(stream_id, "accepted", Some(6000));
    let (_, next) = next_pcm(&mut runtime);
    assert_eq!(i16::from_le_bytes([next[0], next[1]]), 12288);
    assert_eq!(i16::from_le_bytes([next[2], next[3]]), 12288);
    assert_eq!(i16::from_le_bytes([next[4], next[5]]), 4096);
}
