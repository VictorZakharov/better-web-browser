use super::*;
use std::time::Duration;

#[test]
fn collected_voice_automation_releases_its_budget_without_releasing_live_handles() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let script = |code: &str| ScriptInput {
        source_url: "https://example.com/#curve-budget".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let started = runtime.execute_initial(&[script(
        "window.context = new OfflineAudioContext(1, 128, 8000);
         (() => { const source = context.createConstantSource();
             const gain = context.createGain();
             const values = new Float32Array(65536);
             for (let i = 0; i < 128; i++) gain.gain.setValueCurveAtTime(values, 2 * i, 1);
             source.connect(gain).connect(context.destination);
             source.start(); source.stop(1 / 8000); })();
         context.startRendering();",
    )]);
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    for _ in 0..5 {
        let result = runtime.advance_time(Duration::ZERO, 64);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }
    for _ in 0..5 {
        runtime.collect_garbage_for_testing();
    }
    let checked = runtime.execute_additional_with_loader(&[script(
        "const retained = context.createGain();
         const values = new Float32Array(65536);
         for (let i = 0; i < 128; i++) retained.gain.setValueCurveAtTime(values, context.currentTime + 2 * i, 1);
         let rejected = false;
         try { context.createGain().gain.setValueCurveAtTime(new Float32Array(2), context.currentTime, 1); }
         catch (error) { rejected = error.name === 'NotSupportedError'; }
         if (!rejected) throw Error('live curve handle bypassed the memory budget');",
    )], None);
    assert!(checked.errors.is_empty(), "{:?}", checked.errors);
}

#[test]
fn completed_voice_keeps_author_connection_handles_and_gain_can_be_reused() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const first = context.createConstantSource();
        const gain = context.createGain(); gain.gain.value = .25;
        first.connect(gain).connect(context.destination);
        first.start(); first.stop(4 / 8000);
        context.suspend(128 / 8000).then(() => {
            // Retirement is internal: retained author handles keep overload
            // matching and can reconnect a processor to another live voice.
            first.disconnect(gain);
            first.connect(gain);
            const next = context.createConstantSource();
            next.connect(gain); next.start();
            gain.gain.value = .5;
            return context.resume();
        });
        context.startRendering().then(buffer => {
            const values = buffer.getChannelData(0);
            for (let i = 0; i < values.length; i++) {
                const expected = i < 4 ? .25 : i < 128 ? 0 : .5;
                if (values[i] !== expected) throw Error('voice reconnect at ' + i + ': ' + values[i]);
            }
            console.log('retired voice handles passed');
        });
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: retired voice handles passed"]);
}

#[test]
fn two_ten_minute_music_sources_crossfade_with_exact_stereo_samples() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 48000);
        const music = context.createBuffer(2, 600 * 48000, 48000);
        music.getChannelData(0).fill(.25, music.length - 128);
        music.getChannelData(1).fill(-.5, music.length - 128);
        for (let i = 0; i < 2; i++) {
            const source = context.createBufferSource();
            source.buffer = music;
            source.connect(context.destination);
            source.start(0, (music.length - 128) / music.sampleRate);
        }
        context.startRendering().then(buffer => {
            if (!buffer.getChannelData(0).every(value => value === .5) ||
                !buffer.getChannelData(1).every(value => value === -1))
                throw Error('music snapshots changed stereo PCM');
            console.log('music crossfade samples passed');
        });
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: music crossfade samples passed"]);
}

#[test]
fn repeated_completed_voices_do_not_exhaust_the_live_graph() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let script = |code: &str| ScriptInput {
        source_url: "https://example.com/#voices".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let initial = runtime.execute_initial(&[script(
        "window.context = new AudioContext(); window.ended = 0;",
    )]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    runtime.set_audio_activation(true);
    for voice in 0..500 {
        let created = runtime.execute_additional_with_loader(
            &[script(
                "(() => { const source = context.createConstantSource();
                 const gain = context.createGain();
                 gain.gain.value = .25;
                 source.connect(gain).connect(context.destination);
                 source.onended = () => ended++;
                 source.start(); source.stop(context.currentTime + .001);
                 __notifyAudioActivation(); })();",
            )],
            None,
        );
        assert!(
            created.errors.is_empty(),
            "voice {voice}: {:?}",
            created.errors
        );
        let mut actions = created.graph_audio_actions;
        let mut submitted = false;
        for _ in 0..20 {
            if actions.is_empty() {
                let rendered = runtime.advance_time(Duration::from_millis(10), 8);
                assert!(rendered.errors.is_empty(), "{:?}", rendered.errors);
                actions = rendered.graph_audio_actions;
            }
            for action in actions.drain(..) {
                if let ScriptGraphAudioAction::Queue { stream_id, pcm, .. } = action {
                    assert!(pcm.chunks_exact(2).any(|sample| sample != [0, 0]));
                    // A deterministic output queue stays just above the refill
                    // target; this test does not substitute wall time for DSP time.
                    let accepted = runtime.deliver_graph_audio_status_with_queue(
                        stream_id,
                        "accepted",
                        Some(6000),
                    );
                    assert!(accepted.errors.is_empty(), "{:?}", accepted.errors);
                    submitted = true;
                }
            }
            if submitted {
                break;
            }
        }
        assert!(submitted, "voice {voice} produced no PCM");
    }
    let checked = runtime.execute_additional_with_loader(
        &[script(
            "if (ended !== 500) throw Error('lost ended events: ' + ended); context.close();",
        )],
        None,
    );
    assert!(checked.errors.is_empty(), "{:?}", checked.errors);
}
