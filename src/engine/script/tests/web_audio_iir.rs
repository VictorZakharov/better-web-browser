use super::*;

#[test]
fn iir_fir_impulse_response_crosses_a_render_quantum() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 128, 8000);
        buffer.getChannelData(0)[127] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const filter = context.createIIRFilter([0.5, 0.5], [1]);
        source.connect(filter).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            if (pcm[126] !== 0 || pcm[127] !== 0.5 ||
                pcm[128] !== 0.5 || pcm[129] !== 0)
                throw Error('IIR feedforward tail failed across quantum');
            console.log('IIR FIR impulse passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: IIR FIR impulse passed"]);
}

#[test]
fn iir_feedback_tail_persists_after_source_ends() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 128, 8000);
        buffer.getChannelData(0)[127] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const filter = context.createIIRFilter([1], [1, -0.5]);
        source.connect(filter).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            if (pcm[127] !== 1 || pcm[128] !== 0.5 ||
                pcm[129] !== 0.25 || pcm[130] !== 0.125)
                throw Error('IIR recursive tail failed across quantum');
            console.log('IIR feedback tail passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: IIR feedback tail passed"]);
}

#[test]
fn iir_filter_keeps_stereo_channel_histories_separate() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const buffer = context.createBuffer(2, 1, 8000);
        buffer.getChannelData(0)[0] = 1;
        buffer.getChannelData(1)[0] = 0.5;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const filter = context.createIIRFilter([0.5, 0.5], [1]);
        source.connect(filter).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const left = result.getChannelData(0);
            const right = result.getChannelData(1);
            if (left[0] !== 0.5 || left[1] !== 0.5 ||
                right[0] !== 0.25 || right[1] !== 0.25)
                throw Error('IIR channel histories leaked into each other');
            console.log('IIR stereo history passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: IIR stereo history passed"]);
}

#[test]
fn iir_frequency_response_reports_magnitude_phase_and_invalid_frequencies() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const filter = context.createIIRFilter([0.5, 0.5], [1]);
        const frequency = new Float32Array([0, 2000, 4000, -1, 4001]);
        const magnitude = new Float32Array(5);
        const phase = new Float32Array(5);
        filter.getFrequencyResponse(frequency, magnitude, phase);
        const close = (actual, expected) => Math.abs(actual - expected) < 0.00001;
        if (!close(magnitude[0], 1) || !close(phase[0], 0) ||
            !close(magnitude[1], Math.SQRT1_2) || !close(phase[1], -Math.PI / 4) ||
            magnitude[2] > 0.00001 || !Number.isNaN(magnitude[3]) ||
            !Number.isNaN(phase[3]) || !Number.isNaN(magnitude[4]))
            throw Error('IIR complex frequency response mismatch');
        let errorName;
        try { filter.getFrequencyResponse(frequency, new Float32Array(1), phase); }
        catch (error) { errorName = error.name; }
        if (errorName !== 'InvalidAccessError')
            throw Error('frequency response accepted mismatched arrays');
        const aliased = new Float32Array([0, 2000]);
        const aliasedPhase = new Float32Array(2);
        filter.getFrequencyResponse(aliased, aliased, aliasedPhase);
        if (!close(aliased[0], 1) || !close(aliased[1], Math.SQRT1_2))
            throw Error('frequency response corrupted aliased input');
        console.log('IIR frequency response passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: IIR frequency response passed"]);
}

#[test]
fn iir_coefficients_are_validated_and_copied_at_construction() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const cases = [[[], [1]], [[1], []], [Array(21).fill(1), [1]],
            [[0, 0], [1]], [[1], [0]], [[Infinity], [1]]];
        const errors = cases.map(([b, a]) => {
            try { context.createIIRFilter(b, a); return 'accepted'; }
            catch (error) { return error.name; }
        });
        if (errors.join(',') !== 'NotSupportedError,NotSupportedError,' +
            'NotSupportedError,InvalidStateError,InvalidStateError,TypeError')
            throw Error('IIR coefficient validation mismatch: ' + errors);
        const b = [0.5], a = [1];
        const filter = new IIRFilterNode(context, {feedforward: b, feedback: a});
        b[0] = 2; a[0] = 2;
        const frequency = new Float32Array([0]);
        const magnitude = new Float32Array(1), phase = new Float32Array(1);
        filter.getFrequencyResponse(frequency, magnitude, phase);
        if (magnitude[0] !== 0.5 || phase[0] !== 0)
            throw Error('IIR coefficients changed after construction');
        console.log('IIR coefficients passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: IIR coefficients passed"]);
}
