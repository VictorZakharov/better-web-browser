use super::*;
use std::time::Duration;

fn evaluate(runtime: &mut ScriptRuntime, node: &NodeRef, code: &str) -> ScriptOutcome {
    runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.com/#processing".into(),
            code: code.into(),
            node: node.clone(),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }],
        None,
    )
}

fn next_action(runtime: &mut ScriptRuntime) -> ScriptGraphAudioAction {
    for _ in 0..20 {
        let outcome = runtime.advance_time(Duration::from_millis(5), 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        if let Some(action) = outcome.graph_audio_actions.into_iter().next() {
            return action;
        }
    }
    panic!("processing graph did not produce an action");
}

fn processing_runtime() -> (ScriptRuntime, NodeRef) {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let ready = runtime.execute_initial(&[]);
    assert!(ready.errors.is_empty(), "{:?}", ready.errors);
    let initial = evaluate(
        &mut runtime,
        &node,
        r#"
        window.context=new AudioContext({sampleRate:48000});
        window.processor=context.createScriptProcessor(256,0,1);
        window.calls=0;
        processor.onaudioprocess=event=> {
            calls++;
            if(event.outputBuffer.length!==256||event.outputBuffer.sampleRate!==48000)
                throw Error('live processing format');
            event.outputBuffer.getChannelData(0).fill(.25);
        };
        processor.connect(context.destination);
        "#,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.graph_audio_actions.is_empty());
    (runtime, node)
}

#[test]
fn live_processing_waits_for_activation_and_dispatches_before_the_next_pcm_chunk() {
    let (mut runtime, node) = processing_runtime();
    let waiting = runtime.advance_time(Duration::ZERO, 8);
    assert!(waiting.errors.is_empty(), "{:?}", waiting.errors);
    assert!(waiting.graph_audio_actions.is_empty());
    let check = evaluate(
        &mut runtime,
        &node,
        "if(calls!==0) throw Error('activation bypass');",
    );
    assert!(check.errors.is_empty(), "{:?}", check.errors);
    runtime.set_audio_activation(true);
    let activated = evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    assert!(activated.errors.is_empty(), "{:?}", activated.errors);
    let ScriptGraphAudioAction::Queue {
        stream_id,
        format,
        pcm,
    } = next_action(&mut runtime)
    else {
        panic!("expected priming PCM");
    };
    assert_eq!(format.channels, 2);
    assert_eq!(pcm.len(), 256 * 2 * 2);
    assert!(pcm.iter().all(|byte| *byte == 0));
    let accepted = evaluate(
        &mut runtime,
        &node,
        &format!("__receiveAudioGraphStatus({stream_id},'accepted');"),
    );
    assert!(accepted.errors.is_empty(), "{:?}", accepted.errors);
    let ScriptGraphAudioAction::Queue {
        stream_id: second_id,
        pcm,
        ..
    } = next_action(&mut runtime)
    else {
        panic!("expected author-produced PCM");
    };
    assert_eq!(second_id, stream_id);
    assert_eq!(pcm.len(), 256 * 2 * 2);
    assert!(
        pcm.chunks_exact(2)
            .all(|sample| sample == 8192i16.to_le_bytes())
    );
    let observed = evaluate(
        &mut runtime,
        &node,
        "if(calls<1||calls>2) throw Error('processing dispatch cadence '+calls);",
    );
    assert!(observed.errors.is_empty(), "{:?}", observed.errors);
}

#[test]
fn processing_backpressure_reuses_pcm_and_does_not_dispatch_a_block_twice() {
    let (mut runtime, node) = processing_runtime();
    runtime.set_audio_activation(true);
    let activated = evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    assert!(activated.errors.is_empty(), "{:?}", activated.errors);
    let ScriptGraphAudioAction::Queue { stream_id, pcm, .. } = next_action(&mut runtime) else {
        panic!("expected priming PCM");
    };
    let settle = runtime.advance_time(Duration::ZERO, 8);
    assert!(settle.errors.is_empty(), "{:?}", settle.errors);
    assert!(settle.graph_audio_actions.is_empty());
    let before = evaluate(
        &mut runtime,
        &node,
        "if(calls!==1) throw Error('pending output duplicated callbacks '+calls);context.suspend();",
    );
    assert!(before.errors.is_empty(), "{:?}", before.errors);
    assert!(
        matches!(next_action(&mut runtime), ScriptGraphAudioAction::Close { stream_id: id } if id == stream_id)
    );
    let resumed = evaluate(
        &mut runtime,
        &node,
        &format!("__receiveAudioGraphStatus({stream_id},'closed');context.resume();"),
    );
    assert!(resumed.errors.is_empty(), "{:?}", resumed.errors);
    let ScriptGraphAudioAction::Queue {
        stream_id: new_id,
        pcm: reused,
        ..
    } = next_action(&mut runtime)
    else {
        panic!("expected retained PCM");
    };
    assert!(new_id > stream_id);
    assert_eq!(reused, pcm);
    let check = evaluate(
        &mut runtime,
        &node,
        "if(calls!==1||context.currentTime!==0) throw Error('suspend rerendered pending block');",
    );
    assert!(check.errors.is_empty(), "{:?}", check.errors);
}

#[test]
fn closing_live_context_retires_processing_tasks_and_ignores_late_acknowledgements() {
    let (mut runtime, node) = processing_runtime();
    runtime.set_audio_activation(true);
    let activated = evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    assert!(activated.errors.is_empty(), "{:?}", activated.errors);
    let ScriptGraphAudioAction::Queue { stream_id, .. } = next_action(&mut runtime) else {
        panic!("expected priming PCM");
    };
    let closing = evaluate(&mut runtime, &node, "context.close();");
    assert!(closing.errors.is_empty(), "{:?}", closing.errors);
    assert!(
        matches!(next_action(&mut runtime), ScriptGraphAudioAction::Close { stream_id: id } if id == stream_id)
    );
    let closed = evaluate(
        &mut runtime,
        &node,
        &format!("__receiveAudioGraphStatus({stream_id},'closed');window.finalCalls=calls;"),
    );
    assert!(closed.errors.is_empty(), "{:?}", closed.errors);
    let idle = runtime.advance_time(Duration::from_secs(1), 30);
    assert!(idle.errors.is_empty(), "{:?}", idle.errors);
    assert!(idle.graph_audio_actions.is_empty());
    let stale = evaluate(
        &mut runtime,
        &node,
        &format!(
            "__receiveAudioGraphStatus({stream_id},'accepted');if(context.state!=='closed'||calls!==finalCalls) throw Error('late processing revived context');"
        ),
    );
    assert!(stale.errors.is_empty(), "{:?}", stale.errors);
}
