use super::*;

#[test]
fn buffer_source_acquires_content_at_start_across_suspension() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 256, 8000);
        buffer.getChannelData(0).fill(1);
        const source = context.createBufferSource();
        source.buffer = buffer;
        source.connect(context.destination);
        source.start();
        // Neither immediate author mutation nor writes during a suspended
        // render may change content acquired by AudioBufferSourceNode.start().
        buffer.getChannelData(0).fill(2);
        const pause = context.suspend(128 / 8000);
        const rendering = context.startRendering();
        pause.then(() => {
            buffer.copyToChannel(new Float32Array(128).fill(3), 0, 128);
            return context.resume();
        });
        rendering.then(result => {
            const samples = result.getChannelData(0);
            if (samples[0] !== 1 || samples[127] !== 1 ||
                samples[128] !== 1 || samples[255] !== 1)
                throw Error('buffer source observed mutable author channel content');
            console.log('buffer source start acquisition passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: buffer source start acquisition passed"]
    );
}

#[test]
fn buffer_assignment_after_start_acquires_at_assignment() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 128, 8000);
        buffer.getChannelData(0).fill(0.25);
        const source = context.createBufferSource();
        source.connect(context.destination);
        source.start(0, 1 / 8000);
        source.buffer = buffer;
        buffer.getChannelData(0).fill(0.75);
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (samples[0] !== 0.25 || samples[126] !== 0.25 ||
                samples[127] !== 0)
                throw Error('late buffer assignment did not acquire content or offset');
            console.log('late buffer acquisition passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: late buffer acquisition passed"]);
}

#[test]
fn bufferless_source_cannot_restart_when_buffer_is_assigned_after_it_ends() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const source = context.createBufferSource();
        source.connect(context.destination);
        source.start(0);
        const paused = context.suspend(128 / 8000);
        const rendering = context.startRendering();
        paused.then(() => {
            const buffer = context.createBuffer(1, 128, 8000);
            buffer.getChannelData(0).fill(0.5);
            source.buffer = buffer;
            return context.resume();
        });
        rendering.then(result => {
            const samples = result.getChannelData(0);
            console.log(samples.every(sample => sample === 0) ?
                'ended bufferless source stayed silent' :
                'ended bufferless source restarted');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: ended bufferless source stayed silent"]
    );
}

#[test]
fn snapshot_budget_failure_does_not_schedule_the_source() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 4194304, 8000);
        for (let i = 0; i < 4; i++) {
            const source = context.createBufferSource();
            source.buffer = buffer;
            source.start();
        }
        const refused = context.createBufferSource();
        refused.buffer = buffer;
        let budget = false, unscheduled = false;
        try { refused.start(); }
        catch (error) { budget = error.name === 'NotSupportedError'; }
        try { refused.stop(); }
        catch (error) { unscheduled = error.name === 'InvalidStateError'; }
        if (!budget || !unscheduled)
            throw Error('failed snapshot acquisition scheduled its source');
        console.log('snapshot start failure atomicity passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: snapshot start failure atomicity passed"]
    );
}

#[test]
fn clearing_a_started_sources_buffer_outputs_silence() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 128, 8000);
        buffer.getChannelData(0).fill(1);
        const source = context.createBufferSource();
        source.buffer = buffer;
        source.connect(context.destination);
        source.start();
        source.buffer = null;
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (source.buffer !== null || samples[0] !== 0 || samples[127] !== 0)
                throw Error('null buffer still rendered acquired audio');
            console.log('cleared buffer silence passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: cleared buffer silence passed"]);
}

#[test]
fn loop_boundary_interpolates_into_the_loop_start_not_past_the_end() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 4, 8000);
        buffer.getChannelData(0).set([0, 1, 2, 10]);
        const source = new AudioBufferSourceNode(context, {
            buffer, loop: true, loopStart: 1 / 8000,
            loopEnd: 3 / 8000, playbackRate: 0.5
        });
        source.connect(context.destination);
        source.start(0, 2 / 8000);
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (samples[0] !== 2 || samples[1] !== 1.5 ||
                samples[2] !== 1 || samples[3] !== 1.5)
                throw Error('integer loop boundary interpolation failed');
            console.log('integer loop splice passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: integer loop splice passed"]);
}

#[test]
fn fractional_loop_endpoint_interpolates_to_fractional_loop_start() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 4, 8000);
        buffer.getChannelData(0).set([0, 0.5, 1, 0]);
        const source = new AudioBufferSourceNode(context, {
            buffer, loop: true, loopStart: 0.5 / 8000,
            loopEnd: 2.5 / 8000, playbackRate: 0.25
        });
        source.connect(context.destination);
        source.start(0, 2 / 8000);
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (samples[0] !== 1 || samples[1] !== 0.625 || samples[2] !== 0.25)
                throw Error('fractional loop endpoint interpolation failed');
            console.log('fractional loop splice passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: fractional loop splice passed"]);
}
