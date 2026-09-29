use super::*;

#[test]
fn convolver_renders_exact_pcm_across_head_and_fft_tail_partitions() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 2304, 8000);
        const impulse = context.createBuffer(1, 2051, 8000);
        const taps = [[0, 0.5], [1, 0.25], [130, -0.4],
            [1024, 0.75], [1030, -0.2], [2048, 0.33], [2050, -0.15]];
        for (const [index, value] of taps)
            impulse.getChannelData(0)[index] = value;
        const sourceBuffer = context.createBuffer(1, 2, 8000);
        sourceBuffer.getChannelData(0).set([1, 0.5]);
        const source = context.createBufferSource();
        source.buffer = sourceBuffer;
        const convolver = context.createConvolver();
        convolver.normalize = false;
        convolver.buffer = impulse;
        source.connect(convolver).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            const expected = new Float64Array(pcm.length);
            for (const [index, value] of taps) {
                expected[index] += value;
                expected[index + 1] += value * 0.5;
            }
            for (let i = 0; i < pcm.length; ++i)
                if (Math.abs(pcm[i] - expected[i]) > 0.00002)
                    throw Error('Convolver impulse/tail mismatch at sample ' + i + ': ' + pcm[i]);
            console.log('convolver head and tail passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: convolver head and tail passed"]);
}

#[test]
fn dormant_convolver_wakes_for_a_late_source_without_losing_its_tail() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 2560, 8000);
        const impulse = context.createBuffer(1, 2, 8000);
        impulse.getChannelData(0).set([0.5, 0.25]);
        const sourceBuffer = context.createBuffer(1, 1, 8000);
        sourceBuffer.getChannelData(0)[0] = 1;
        const source = context.createBufferSource();
        source.buffer = sourceBuffer;
        const convolver = new ConvolverNode(context,
            {buffer: impulse, disableNormalization: true});
        source.connect(convolver).connect(context.destination);
        source.start(2048 / 8000);
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            for (let i = 0; i < pcm.length; ++i) {
                const expected = i === 2048 ? 0.5 : i === 2049 ? 0.25 : 0;
                if (Math.abs(pcm[i] - expected) > 0.00001)
                    throw Error('Late convolver source/tail mismatch at ' + i);
            }
            console.log('convolver late source passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: convolver late source passed"]);
}

#[test]
fn convolver_routes_mono_stereo_and_true_stereo_impulses() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        async function render(input, response, interpretation, mode) {
            const context = new OfflineAudioContext(2, 128, 8000);
            const sourceBuffer = context.createBuffer(input.length, 1, 8000);
            input.forEach((sample, channel) =>
                sourceBuffer.getChannelData(channel)[0] = sample);
            const impulse = context.createBuffer(response.length, 1, 8000);
            response.forEach((sample, channel) =>
                impulse.getChannelData(channel)[0] = sample);
            const source = context.createBufferSource();
            source.buffer = sourceBuffer;
            const convolver = new ConvolverNode(context,
                {buffer: impulse, disableNormalization: true});
            if (interpretation) convolver.channelInterpretation = interpretation;
            if (mode) convolver.channelCountMode = mode;
            source.connect(convolver).connect(context.destination);
            source.start();
            const result = await context.startRendering();
            return [result.getChannelData(0)[0], result.getChannelData(1)[0]];
        }
        Promise.all([
            render([1], [2]), render([1], [2, 3]),
            render([5, 7], [1, 2, 3, 4]), render([1], [1, 2, 3, 4]),
            render([1], [1, 2, 3, 4], 'discrete'),
            render([1], [1, 2, 3, 4], 'discrete', 'explicit')
        ]).then(values => {
            const expected = [[2, 2], [2, 3], [26, 38], [4, 6], [4, 6], [1, 2]];
            values.forEach((pair, index) => pair.forEach((sample, channel) => {
                if (Math.abs(sample - expected[index][channel]) > 0.00002)
                    throw Error('Convolver channel matrix mismatch ' + index);
            }));
            console.log('convolver channel matrix passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: convolver channel matrix passed"]);
}

#[test]
fn convolver_normalizes_at_assignment_and_acquires_buffer_content() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 44100);
        const impulse = context.createBuffer(1, 1, 44100);
        impulse.getChannelData(0)[0] = 1;
        const convolver = new ConvolverNode(context, {buffer: impulse});
        if (!convolver.normalize || convolver.buffer !== impulse)
            throw Error('ConvolverNode initial attributes are wrong');
        convolver.normalize = false;
        impulse.getChannelData(0)[0] = 0;
        const source = context.createBufferSource();
        const sourceBuffer = context.createBuffer(1, 1, 44100);
        sourceBuffer.getChannelData(0)[0] = 1;
        source.buffer = sourceBuffer;
        source.connect(convolver).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            if (Math.abs(pcm[0] - 0.00125) > 0.000001 ||
                Math.abs(pcm[1]) > 0.000001)
                throw Error('Convolver normalized or acquired response changed');
            console.log('convolver normalization and acquisition passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: convolver normalization and acquisition passed"]
    );
}

#[test]
fn convolver_sanitizes_nonfinite_impulse_samples() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const impulse = context.createBuffer(1, 3, 8000);
        impulse.getChannelData(0).set([NaN, Infinity, 1]);
        const sourceBuffer = context.createBuffer(1, 1, 8000);
        sourceBuffer.getChannelData(0)[0] = 1;
        const source = context.createBufferSource();
        source.buffer = sourceBuffer;
        const convolver = new ConvolverNode(context,
            {buffer: impulse, disableNormalization: true});
        source.connect(convolver).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            if (!Number.isFinite(pcm[0]) || !Number.isFinite(pcm[1]) ||
                Math.abs(pcm[0]) > 0.00001 || Math.abs(pcm[1]) > 0.00001 ||
                Math.abs(pcm[2] - 1) > 0.00001)
                throw Error('Nonfinite impulse poisoned convolution');
            console.log('convolver nonfinite samples passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: convolver nonfinite samples passed"]);
}

#[test]
fn convolver_nonfinite_input_does_not_poison_later_quanta() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const impulse = context.createBuffer(1, 130, 8000);
        impulse.getChannelData(0)[129] = 1;
        const sourceBuffer = context.createBuffer(1, 2, 8000);
        sourceBuffer.getChannelData(0).set([NaN, 1]);
        const source = context.createBufferSource();
        source.buffer = sourceBuffer;
        const convolver = new ConvolverNode(context,
            {buffer: impulse, disableNormalization: true});
        source.connect(convolver).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            for (let i = 0; i < pcm.length; ++i)
                if (!Number.isFinite(pcm[i]) ||
                    Math.abs(pcm[i] - (i === 130 ? 1 : 0)) > 0.00001)
                    throw Error('Nonfinite input poisoned later convolution at ' + i);
            console.log('convolver nonfinite input passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: convolver nonfinite input passed"]);
}

#[test]
fn convolver_rejects_invalid_impulses_and_unbounded_work() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const node = context.createConvolver();
        if (node.buffer !== null || !node.normalize || node.channelCount !== 2 ||
            node.channelCountMode !== 'clamped-max')
            throw Error('ConvolverNode defaults mismatch');
        const errors = [];
        const check = action => {
            try { action(); errors.push('accepted'); }
            catch (error) { errors.push(error.name); }
        };
        check(() => { node.buffer = context.createBuffer(3, 1, 8000); });
        check(() => { node.buffer = context.createBuffer(1, 1, 44100); });
        check(() => { node.buffer = context.createBuffer(1, 131073, 8000); });
        check(() => { node.channelCount = 3; });
        check(() => { node.channelCountMode = 'max'; });
        check(() => { node.buffer = {}; });
        if (errors.join(',') !== 'NotSupportedError,NotSupportedError,' +
            'NotSupportedError,NotSupportedError,NotSupportedError,TypeError')
            throw Error('ConvolverNode validation mismatch: ' + errors);
        node.buffer = context.createBuffer(1, 1, 8000);
        node.buffer = null;
        if (node.buffer !== null) throw Error('ConvolverNode failed to release response');
        console.log('convolver validation passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: convolver validation passed"]);
}
