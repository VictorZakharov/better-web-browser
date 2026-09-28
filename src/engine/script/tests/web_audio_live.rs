use super::*;
use crate::media_protocol::GraphPcmFormat;
use std::time::Duration;

fn script(node: &NodeRef, code: &str) -> ScriptInput {
    ScriptInput {
        source_url: "https://example.com/#audio".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }
}

fn evaluate(runtime: &mut ScriptRuntime, node: &NodeRef, code: &str) -> ScriptOutcome {
    runtime.execute_additional_with_loader(&[script(node, code)], None)
}

fn next_graph_action(runtime: &mut ScriptRuntime) -> ScriptGraphAudioAction {
    for _ in 0..10 {
        let outcome = runtime.advance_time(Duration::from_millis(10), 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        if let Some(action) = outcome.graph_audio_actions.into_iter().next() {
            return action;
        }
    }
    panic!("live audio did not produce a graph action");
}

#[test]
fn live_context_waits_for_activation_and_advances_only_after_pcm_acceptance() {
    let dom = dom::parse_with_scripting("<body><script></script><output></output></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[script(
        &node,
        r#"
        let highRateRejected = false;
        try { new AudioContext({ sampleRate: 96000 }); }
        catch (error) { highRateRejected = error.name === 'NotSupportedError'; }
        if (!highRateRejected) throw Error('unreliable live sample rate was accepted');
        window.context = new AudioContext({ sampleRate: 48000 });
        const source = context.createConstantSource();
        const analyser = context.createAnalyser();
        source.offset.value = 0.25;
        source.connect(analyser).connect(context.destination);
        source.start();
        context.resume().then(() => document.querySelector('output').textContent = 'resumed');
        if (context.state !== 'suspended' || context.currentTime !== 0)
            throw Error('live context bypassed activation');
    "#,
    )]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.graph_audio_actions.is_empty());
    let waiting = runtime.advance_time(Duration::ZERO, 5);
    assert!(waiting.errors.is_empty(), "{:?}", waiting.errors);
    assert!(waiting.graph_audio_actions.is_empty());

    runtime.set_audio_activation(true);
    let notified = evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    assert!(notified.errors.is_empty(), "{:?}", notified.errors);
    let action = next_graph_action(&mut runtime);
    let (stream_id, format, pcm) = match action {
        ScriptGraphAudioAction::Queue {
            stream_id,
            format,
            pcm,
        } => (stream_id, format, pcm),
        other => panic!("expected graph PCM, got {other:?}"),
    };
    assert_eq!(format.sample_rate, 48000);
    assert_eq!(format.channels, 2);
    assert_eq!(pcm.len(), 896 * 2 * 2);
    assert!(
        pcm.chunks_exact(2)
            .all(|sample| sample == 8192i16.to_le_bytes())
    );
    let before = evaluate(
        &mut runtime,
        &node,
        "if (context.currentTime !== 0 || context.state !== 'suspended') throw Error('time advanced without output acknowledgement');",
    );
    assert!(before.errors.is_empty(), "{:?}", before.errors);

    let accepted = evaluate(
        &mut runtime,
        &node,
        &format!("__receiveAudioGraphStatus({stream_id}, 'accepted');"),
    );
    assert!(accepted.errors.is_empty(), "{:?}", accepted.errors);
    let running = evaluate(
        &mut runtime,
        &node,
        r#"
        if (context.state !== 'running' || Math.abs(context.currentTime - 896 / 48000) > 1e-12 ||
            document.querySelector('output').textContent !== 'resumed')
            throw Error('accepted PCM did not start the live context');
        context.close();
    "#,
    );
    assert!(running.errors.is_empty(), "{:?}", running.errors);
    let close = next_graph_action(&mut runtime);
    assert!(matches!(close, ScriptGraphAudioAction::Close { stream_id: id } if id == stream_id));
    let closed = evaluate(
        &mut runtime,
        &node,
        &format!(
            "__receiveAudioGraphStatus({stream_id}, 'closed'); if (context.state !== 'closed') throw Error('close did not retire stream');"
        ),
    );
    assert!(closed.errors.is_empty(), "{:?}", closed.errors);
}

#[test]
fn unaccepted_chunk_is_reused_after_suspend_and_resume() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_audio_activation(true);
    let initial = runtime.execute_initial(&[script(
        &node,
        r#"
        window.context = new AudioContext({ sampleRate: 8000 });
        const oscillator = context.createOscillator();
        oscillator.frequency.value = 700;
        oscillator.connect(context.destination);
        oscillator.start();
    "#,
    )]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let first = initial
        .graph_audio_actions
        .into_iter()
        .next()
        .unwrap_or_else(|| next_graph_action(&mut runtime));
    let (first_id, first_pcm) = match first {
        ScriptGraphAudioAction::Queue { stream_id, pcm, .. } => (stream_id, pcm),
        other => panic!("expected first graph PCM, got {other:?}"),
    };
    let suspended = evaluate(&mut runtime, &node, "context.suspend();");
    assert!(suspended.errors.is_empty(), "{:?}", suspended.errors);
    let close = next_graph_action(&mut runtime);
    assert!(matches!(close, ScriptGraphAudioAction::Close { stream_id } if stream_id == first_id));
    let ack = evaluate(
        &mut runtime,
        &node,
        &format!("__receiveAudioGraphStatus({first_id}, 'closed');"),
    );
    assert!(ack.errors.is_empty(), "{:?}", ack.errors);
    let resumed = evaluate(&mut runtime, &node, "context.resume();");
    assert!(resumed.errors.is_empty(), "{:?}", resumed.errors);
    let second = next_graph_action(&mut runtime);
    let (second_id, second_pcm) = match second {
        ScriptGraphAudioAction::Queue { stream_id, pcm, .. } => (stream_id, pcm),
        other => panic!("expected resumed graph PCM, got {other:?}"),
    };
    assert!(second_id > first_id);
    assert_eq!(
        first_pcm, second_pcm,
        "resume rerendered unaccepted DSP quanta"
    );
    let stale = evaluate(
        &mut runtime,
        &node,
        &format!(
            "__receiveAudioGraphStatus({first_id}, 'accepted'); if (context.currentTime !== 0) throw Error('retired stream advanced time');"
        ),
    );
    assert!(stale.errors.is_empty(), "{:?}", stale.errors);
}

#[test]
fn local_admission_backpressure_retries_the_same_dsp_output() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[script(
        &node,
        r#"
        window.context = new AudioContext({ sampleRate: 8000 });
        const oscillator = context.createOscillator();
        oscillator.frequency.value = 700;
        oscillator.connect(context.destination);
        oscillator.start();
    "#,
    )]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    runtime.set_audio_activation(true);
    let notified = evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    assert!(notified.errors.is_empty(), "{:?}", notified.errors);
    runtime
        .host
        .borrow_mut()
        .pending_graph_audio_actions
        .push(ScriptGraphAudioAction::Queue {
            stream_id: 99,
            format: GraphPcmFormat {
                sample_rate: 8000,
                channels: 2,
            },
            pcm: vec![0; 4],
        });
    // The dummy action occupies local admission during the first render task.
    let result = runtime.advance_time(Duration::ZERO, 1);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(matches!(
        result.graph_audio_actions.as_slice(),
        [ScriptGraphAudioAction::Queue { stream_id: 99, .. }]
    ));
    let retry = next_graph_action(&mut runtime);
    let ScriptGraphAudioAction::Queue { pcm, .. } = retry else {
        panic!("expected retried graph PCM");
    };
    assert_eq!(
        i16::from_le_bytes([pcm[0], pcm[1]]),
        0,
        "local refusal advanced oscillator phase"
    );
    assert_ne!(i16::from_le_bytes([pcm[4], pcm[5]]), 0);
}
