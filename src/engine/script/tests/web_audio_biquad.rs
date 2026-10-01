use super::*;

#[test]
fn biquad_lowpass_and_highpass_render_distinct_impulse_pcm() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        async function impulse(type) {
            const context = new OfflineAudioContext(1, 256, 8000);
            const buffer = context.createBuffer(1, 1, 8000);
            buffer.getChannelData(0)[0] = 1;
            const source = context.createBufferSource();
            source.buffer = buffer;
            const filter = new BiquadFilterNode(context, {type, frequency: 1000});
            source.connect(filter).connect(context.destination);
            source.start();
            return (await context.startRendering()).getChannelData(0);
        }
        Promise.all([impulse('lowpass'), impulse('highpass')]).then(([low, high]) => {
            const close = (actual, expected) => Math.abs(actual - expected) < 0.000001;
            // b0/a0 from Web Audio §1.13.5 at 8 kHz, 1 kHz, Q=1 dB.
            if (!close(low[0], 0.111357369) || !close(high[0], 0.649038310) ||
                low[1] === 0 || high[1] === 0 ||
                !Number.isFinite(low[128]) || !Number.isFinite(high[128]))
                throw Error('biquad impulse response or tail mismatch');
            console.log('biquad impulse PCM passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: biquad impulse PCM passed"]);
}

#[test]
fn biquad_all_filter_families_have_expected_frequency_shapes() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const filter = context.createBiquadFilter();
        filter.frequency.value = 1000;
        filter.Q.value = 1;
        filter.gain.value = 6;
        const frequency = new Float32Array([0, 1000, 4000]);
        const response = type => {
            filter.type = type;
            const magnitude = new Float32Array(3), phase = new Float32Array(3);
            filter.getFrequencyResponse(frequency, magnitude, phase);
            return {magnitude, phase};
        };
        const low = response('lowpass').magnitude;
        const high = response('highpass').magnitude;
        const band = response('bandpass').magnitude;
        const notch = response('notch').magnitude;
        const all = response('allpass').magnitude;
        const peak = response('peaking').magnitude;
        const lowShelf = response('lowshelf').magnitude;
        const highShelf = response('highshelf').magnitude;
        const close = (actual, expected) => Math.abs(actual - expected) < 0.0001;
        const boost = Math.pow(10, 6 / 20);
        if (!close(low[0], 1) || low[2] > 0.0001 ||
            high[0] > 0.0001 || !close(high[2], 1) ||
            !close(band[1], 1) || notch[1] > 0.0001 ||
            ![...all].every(value => close(value, 1)) ||
            !close(peak[1], boost) ||
            !close(lowShelf[0], boost) || !close(lowShelf[2], 1) ||
            !close(highShelf[0], 1) || !close(highShelf[2], boost))
            throw Error('biquad filter-family transfer functions mismatch');
        console.log('biquad filter families passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: biquad filter families passed"]);
}

#[test]
fn biquad_automation_takes_effect_at_the_scheduled_sample() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 256, 8000);
        buffer.getChannelData(0)[128] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const filter = context.createBiquadFilter();
        filter.frequency.value = 1000;
        filter.frequency.setValueAtTime(2000, 128 / 8000);
        source.connect(filter).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            const alpha = 1 / (2 * Math.pow(10, 1 / 20));
            const expected = 0.5 / (1 + alpha); // 2 kHz lowpass b0/a0.
            if (pcm[127] !== 0 || Math.abs(pcm[128] - expected) > 0.000001)
                throw Error('biquad automation missed the quantum boundary');
            console.log('biquad automation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: biquad automation passed"]);
}

#[test]
fn biquad_frequency_response_validates_arrays_and_type() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const filter = context.createBiquadFilter();
        if (filter.type !== 'lowpass' || filter.frequency.defaultValue !== 350 ||
            filter.detune.defaultValue !== 0 || filter.Q.defaultValue !== 1 ||
            filter.gain.defaultValue !== 0)
            throw Error('biquad defaults mismatch');
        const names = [];
        try { new BiquadFilterNode(context, {type: 'bogus'}); }
        catch (error) { names.push(error.name); }
        try { filter.getFrequencyResponse(new Float32Array(2),
            new Float32Array(1), new Float32Array(2)); }
        catch (error) { names.push(error.name); }
        const frequencies = new Float32Array([-1, 4001]);
        const magnitude = new Float32Array(2), phase = new Float32Array(2);
        filter.getFrequencyResponse(frequencies, magnitude, phase);
        if (names.join(',') !== 'TypeError,InvalidAccessError' ||
            ![...magnitude, ...phase].every(Number.isNaN))
            throw Error('biquad frequency response validation mismatch');
        console.log('biquad validation passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: biquad validation passed"]);
}
