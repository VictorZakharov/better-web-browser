use super::*;

#[test]
fn offline_oscillator_renders_gain_across_a_quantum_boundary() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const oscillator = context.createOscillator();
        oscillator.frequency.value = 1000;
        const gain = context.createGain();
        gain.gain.value = 0.5;
        oscillator.connect(gain).connect(context.destination);
        oscillator.start(0);
        let complete = false;
        context.oncomplete = event => {
            complete = event.renderedBuffer instanceof AudioBuffer && context.state === 'closed';
        };
        context.startRendering().then(buffer => {
            const samples = buffer.getChannelData(0);
            const close = (actual, expected) => Math.abs(actual - expected) < 0.0001;
            if (complete || context.state !== 'closed' || buffer.length !== 256 ||
                !close(samples[0], 0) || !close(samples[2], 0.5) ||
                !close(samples[6], -0.5) || !close(samples[130], 0.5))
                throw Error('offline oscillator samples or completion state');
            console.log('offline oscillator passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: offline oscillator passed"]);
}

#[test]
fn buffer_source_preserves_stereo_samples_and_scheduled_offset() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const buffer = context.createBuffer(2, 8, 8000);
        buffer.getChannelData(0).fill(0.25);
        buffer.getChannelData(1).fill(0.75);
        const source = context.createBufferSource();
        source.buffer = buffer;
        source.connect(context.destination);
        source.start(2 / 8000, 1 / 8000, 4 / 8000);
        context.startRendering().then(rendered => {
            const left = rendered.getChannelData(0), right = rendered.getChannelData(1);
            if (left[0] !== 0 || right[1] !== 0 || left[2] !== 0.25 ||
                right[2] !== 0.75 || left[5] !== 0.25 || right[6] !== 0)
                throw Error('buffer source sample placement');
            console.log('stereo buffer passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: stereo buffer passed"]);
}

#[test]
fn scheduled_gain_automation_changes_rendered_pcm() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 128, 8000);
        buffer.getChannelData(0).fill(1);
        const source = context.createBufferSource();
        source.buffer = buffer;
        const gain = context.createGain();
        gain.gain.setValueAtTime(0, 0);
        gain.gain.linearRampToValueAtTime(1, 8 / 8000);
        source.connect(gain).connect(context.destination);
        source.start();
        context.startRendering().then(rendered => {
            const data = rendered.getChannelData(0);
            if (data[0] !== 0 || Math.abs(data[4] - 0.5) > 0.00001 ||
                data[8] !== 1 || data[64] !== 1)
                throw Error('sample-accurate gain automation');
            console.log('gain automation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: gain automation passed"]);
}

#[test]
fn buffer_source_uses_k_rate_and_a_looped_duration_counts_buffer_content() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 256, 8000);
        buffer.getChannelData(0).set([0, 1, 2, 3]);
        const source = context.createBufferSource();
        source.buffer = buffer;
        source.loop = true;
        source.loopStart = 1 / 8000;
        source.loopEnd = 3 / 8000;
        source.connect(context.destination);
        source.start(0, 0, 6 / 8000);
        let fixedRate = false, assignedOnce = false;
        try { source.playbackRate.automationRate = 'a-rate'; }
        catch (error) { fixedRate = error.name === 'InvalidStateError'; }
        try { source.buffer = buffer; }
        catch (error) { assignedOnce = error.name === 'InvalidStateError'; }
        context.startRendering().then(rendered => {
            const samples = rendered.getChannelData(0);
            if (!fixedRate || !assignedOnce || source.playbackRate.automationRate !== 'k-rate' ||
                samples.slice(0, 7).join(',') !== '0,1,2,1,2,1,0')
                throw Error('loop duration or fixed-rate contract');
            console.log('loop duration passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: loop duration passed"]);
}

#[test]
fn k_rate_playback_automation_samples_once_per_quantum() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 256, 8000);
        for (let i = 0; i < buffer.length; ++i) buffer.getChannelData(0)[i] = i;
        const source = context.createBufferSource();
        source.buffer = buffer;
        source.playbackRate.setValueAtTime(1, 0);
        source.playbackRate.linearRampToValueAtTime(2, 128 / 8000);
        source.connect(context.destination);
        source.start();
        context.startRendering().then(rendered => {
            const samples = rendered.getChannelData(0);
            if (samples[127] !== 127 || samples[128] !== 128 || samples[129] !== 130)
                throw Error('playback rate was not sampled per quantum');
            console.log('k-rate passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: k-rate passed"]);
}

#[test]
fn constant_source_uses_scheduled_a_rate_offset() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        source.offset.setValueAtTime(0, 2 / 8000);
        source.offset.linearRampToValueAtTime(1, 6 / 8000);
        source.connect(context.destination);
        source.start(2 / 8000);
        source.stop(6 / 8000);
        context.startRendering().then(rendered => {
            const values = rendered.getChannelData(0);
            if (source.offset.automationRate !== 'a-rate' || values[1] !== 0 ||
                values[2] !== 0 || Math.abs(values[4] - 0.5) > 0.00001 ||
                Math.abs(values[5] - 0.75) > 0.00001 || values[6] !== 0)
                throw Error('constant source schedule or a-rate output');
            console.log('constant source passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: constant source passed"]);
}

#[test]
fn stereo_panner_renders_mono_and_stereo_equal_power_equations() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const mono = new OfflineAudioContext(2, 128, 8000);
        const constant = mono.createConstantSource();
        const pan = mono.createStereoPanner();
        constant.connect(pan).connect(mono.destination);
        constant.start();
        pan.pan.setValueAtTime(-1, 0);
        pan.pan.setValueAtTime(0, 16 / 8000);
        pan.pan.setValueAtTime(1, 32 / 8000);
        const stereo = new OfflineAudioContext(2, 128, 8000);
        const buffer = stereo.createBuffer(2, 128, 8000);
        buffer.getChannelData(0).fill(1);
        buffer.getChannelData(1).fill(0.5);
        const source = stereo.createBufferSource();
        source.buffer = buffer;
        const stereoPan = stereo.createStereoPanner();
        source.connect(stereoPan).connect(stereo.destination);
        source.start();
        stereoPan.pan.setValueAtTime(-1, 0);
        stereoPan.pan.setValueAtTime(1, 16 / 8000);
        Promise.all([mono.startRendering(), stereo.startRendering()]).then(([a, b]) => {
            const l = a.getChannelData(0), r = a.getChannelData(1);
            const sl = b.getChannelData(0), sr = b.getChannelData(1);
            const near = (actual, expected) => Math.abs(actual - expected) < 0.00001;
            if (!near(l[0], 1) || !near(r[0], 0) ||
                !near(l[16], Math.SQRT1_2) || !near(r[16], Math.SQRT1_2) ||
                !near(l[32], 0) || !near(r[32], 1) ||
                !near(sl[0], 1.5) || !near(sr[0], 0) ||
                !near(sl[16], 0) || !near(sr[16], 1.5))
                throw Error('stereo panner equal-power samples');
            console.log('stereo panner passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: stereo panner passed"]);
}

#[test]
fn graph_rejects_cycles_and_foreign_contexts() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const other = new OfflineAudioContext(1, 128, 8000);
        const first = context.createGain(), second = context.createGain();
        first.connect(second);
        let cycle = false, foreign = false, missing = false;
        try { second.connect(first); } catch (error) { cycle = error.name === 'NotSupportedError'; }
        try { first.connect(other.destination); }
        catch (error) { foreign = error.name === 'InvalidAccessError'; }
        first.disconnect(second);
        try { first.disconnect(second); }
        catch (error) { missing = error.name === 'InvalidAccessError'; }
        if (!cycle || !foreign || !missing) throw Error('graph connection contract');
        console.log('graph validation passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: graph validation passed"]);
}

#[test]
fn audio_buffer_copies_samples_without_aliasing_and_enforces_limits() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const buffer = new AudioBuffer({numberOfChannels: 1, length: 8, sampleRate: 8000});
        const input = new Float32Array([0.25, 0.5, 0.75]);
        buffer.copyToChannel(input, 0, 2);
        input[0] = 1;
        const result = new Float32Array(3);
        buffer.copyFromChannel(result, 0, 2);
        const unchanged = new Float32Array([9, 9]);
        buffer.copyFromChannel(unchanged, 0, buffer.length);
        let limit = false, channel = false;
        try { new AudioBuffer({numberOfChannels: 32, length: 1 << 20,
            sampleRate: 8000}); } catch (error) { limit = error.name === 'NotSupportedError'; }
        try { buffer.getChannelData(1); }
        catch (error) { channel = error.name === 'IndexSizeError'; }
        if (result.join(',') !== '0.25,0.5,0.75' || unchanged.join(',') !== '9,9' ||
            !limit || !channel)
            throw Error('buffer copy or allocation contract');
        console.log('audio buffer passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: audio buffer passed"]);
}

#[test]
fn long_offline_render_yields_to_a_document_timer() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 8192, 8000);
        const source = context.createOscillator();
        source.frequency.value = 500;
        source.connect(context.destination);
        source.start();
        const order = [];
        context.startRendering().then(buffer => {
            if (order.join(',') !== 'timer' || buffer.getChannelData(0)[2] !==
                Math.fround(Math.sin(Math.PI / 4)))
                throw Error('offline renderer monopolized the event loop or returned wrong PCM');
            console.log('offline render yielded');
        });
        setTimeout(() => order.push('timer'), 0);
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: offline render yielded"]);
}

#[test]
fn author_timer_replacement_cannot_block_audio_completion() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 8192, 8000);
        const source = context.createOscillator();
        source.connect(context.destination);
        source.start();
        const originalTimeout = setTimeout;
        const originalMicrotask = queueMicrotask;
        originalTimeout(() => {
            for (let id = 1; id < 100; ++id) clearTimeout(id);
        }, 0);
        context.startRendering().then(buffer => {
            globalThis.setTimeout = originalTimeout;
            globalThis.queueMicrotask = originalMicrotask;
            if (buffer.length !== 8192 || context.state !== 'closed')
                throw Error('audio render failed after author scheduler replacement');
            console.log('captured audio scheduler passed');
        });
        globalThis.setTimeout = () => { throw Error('author timer called'); };
        globalThis.queueMicrotask = () => { throw Error('author microtask called'); };
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: captured audio scheduler passed"]);
}

#[test]
fn document_teardown_discards_pending_offline_render_tasks() {
    let dom = crate::engine::dom::parse_with_scripting(
        r#"<body data-audio="pending"><script>
            const context = new OfflineAudioContext(1, 1_048_576, 8000);
            const source = context.createOscillator();
            source.connect(context.destination);
            source.start();
            context.startRendering().then(() => document.body.dataset.audio = 'stale');
        </script></body>"#,
        true,
    );
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial_before_document_completion(&scripts, None);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(runtime.next_timer_delay(), Some(std::time::Duration::ZERO));

    // Navigation destroys the owning realm; its queued quanta and bounded PCM
    // leave with that realm, and no completion callback can reach the old DOM.
    runtime.cancel_document();
    assert_eq!(runtime.next_timer_delay(), None);
    let cancelled = runtime.advance_time(std::time::Duration::from_secs(1), 128);
    assert!(cancelled.runtime_stopped);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-audio")
            .as_deref(),
        Some("pending")
    );
}

#[test]
fn disconnected_scheduled_source_still_fires_ended() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        source.onended = () => console.log('disconnected ended');
        source.start();
        source.stop(4 / 8000);
        context.startRendering().then(buffer => {
            if (buffer.getChannelData(0).some(sample => sample !== 0))
                throw Error('disconnected source reached destination');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: disconnected ended"]);
}
