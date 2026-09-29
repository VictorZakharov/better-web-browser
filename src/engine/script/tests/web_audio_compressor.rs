use super::*;

#[test]
fn compressor_exposes_specified_k_rate_parameters_and_factory() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const defaults = context.createDynamicsCompressor();
        const nullOptions = new DynamicsCompressorNode(context, null);
        const configured = new DynamicsCompressorNode(context, {
            threshold: -35, knee: 8, ratio: 4, attack: 0.01, release: 0.5
        });
        const names = ['threshold', 'knee', 'ratio', 'attack', 'release'];
        const expectedDefaults = [-24, 30, 12, 0.003, 0.25];
        const expectedOptions = [-35, 8, 4, 0.01, 0.5];
        if (defaults.numberOfInputs !== 1 || defaults.numberOfOutputs !== 1 ||
            nullOptions.threshold.value !== -24 ||
            defaults.reduction !== 0 || configured.context !== context ||
            names.some((name, index) =>
                defaults[name].defaultValue !== expectedDefaults[index] ||
                configured[name].defaultValue !== expectedDefaults[index] ||
                Math.abs(configured[name].value - expectedOptions[index]) > 0.000001 ||
                configured[name].automationRate !== 'k-rate'))
            throw Error('compressor defaults/options differ from Web Audio: ' +
                JSON.stringify(names.map(name => [name, defaults[name].defaultValue,
                    configured[name].defaultValue, configured[name].value,
                    configured[name].automationRate])));
        let rejectedRate = false, rejectedNonfinite = false;
        try { configured.threshold.automationRate = 'a-rate'; }
        catch (error) { rejectedRate = error.name === 'InvalidStateError'; }
        try { new DynamicsCompressorNode(context, {ratio: NaN}); }
        catch (error) { rejectedNonfinite = error instanceof TypeError; }
        if (!rejectedRate || !rejectedNonfinite)
            throw Error('compressor accepted invalid automation or options');
        console.log('compressor interface passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: compressor interface passed"]);
}

#[test]
fn compressor_has_fixed_lookahead_and_preserves_the_delayed_tail() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 1, 8000);
        buffer.getChannelData(0)[0] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const compressor = new DynamicsCompressorNode(context, {
            threshold: 0, knee: 0, ratio: 1, attack: 0, release: 0
        });
        source.connect(compressor).connect(context.destination);
        source.start();
        context.startRendering().then(rendered => {
            const samples = rendered.getChannelData(0);
            if (samples.slice(0, 48).some(value => value !== 0) ||
                samples[48] !== 1 || samples.slice(49).some(value => value !== 0) ||
                compressor.reduction !== 0)
                throw Error('compressor lost its 6ms lookahead tail');
            console.log('compressor lookahead passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: compressor lookahead passed"]);
}

#[test]
fn compressor_reduces_loud_signal_with_linked_stereo_detection() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 512, 8000);
        const buffer = context.createBuffer(2, 512, 8000);
        buffer.getChannelData(0).fill(0.8);
        buffer.getChannelData(1).fill(0.1);
        const source = context.createBufferSource();
        source.buffer = buffer;
        const compressor = new DynamicsCompressorNode(context, {
            threshold: -30, knee: 0, ratio: 20, attack: 0, release: 0.01
        });
        source.connect(compressor).connect(context.destination);
        source.start();
        context.startRendering().then(rendered => {
            const left = rendered.getChannelData(0);
            const right = rendered.getChannelData(1);
            if (left[47] !== 0 || right[47] !== 0 ||
                !(left[100] > 0.05 && left[100] < 0.7) ||
                Math.abs(left[100] / right[100] - 8) > 0.001 ||
                !(compressor.reduction < -8 && compressor.reduction > -11))
                throw Error('compressor did not apply linked stereo gain reduction');
            console.log('compressor linked stereo passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: compressor linked stereo passed"]);
}

#[test]
fn compressor_samples_parameter_automation_at_quantum_boundaries() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 256, 8000);
        buffer.getChannelData(0).fill(0.8);
        const source = context.createBufferSource();
        source.buffer = buffer;
        const compressor = new DynamicsCompressorNode(context, {
            threshold: 0, knee: 0, ratio: 20, attack: 0, release: 0
        });
        compressor.threshold.setValueAtTime(-30, 128 / 8000);
        source.connect(compressor).connect(context.destination);
        source.start();
        context.startRendering().then(rendered => {
            const samples = rendered.getChannelData(0);
            if (Math.abs(samples[127] - 0.8) > 0.00001 ||
                !(samples[128] < 0.7 && samples[128] > 0.05))
                throw Error('compressor k-rate change missed the render quantum');
            console.log('compressor automation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: compressor automation passed"]);
}

#[test]
fn compressor_identity_below_threshold_and_meter_includes_makeup_gain() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const inputLevel = Math.pow(10, -25 / 20);
        const buffer = context.createBuffer(1, 128, 8000);
        buffer.getChannelData(0).fill(inputLevel);
        const source = context.createBufferSource();
        source.buffer = buffer;
        const compressor = new DynamicsCompressorNode(context, {
            threshold: -20, knee: 20, ratio: 4, attack: 0, release: 0
        });
        source.connect(compressor).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            // The specified curve is the identity through -20 dB. At 0 dB,
            // its continuous knee/ratio sections yield -7.5 dB; makeup is
            // the inverse full-range gain raised to the 0.6 power.
            const makeup = Math.pow(10, 4.5 / 20);
            const pcm = result.getChannelData(0);
            if (Math.abs(pcm[48] - inputLevel * makeup) > 0.00001 ||
                Math.abs(compressor.reduction - 4.5) > 0.00001)
                throw Error('compressor knee identity or reduction meter differs');
            console.log('compressor threshold and meter passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: compressor threshold and meter passed"]
    );
}

#[test]
fn compressor_nonfinite_input_does_not_poison_its_persistent_detector() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 256, 8000);
        buffer.getChannelData(0).fill(0.8);
        buffer.getChannelData(0)[0] = NaN;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const compressor = context.createDynamicsCompressor();
        source.connect(compressor).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            if (!pcm.every(Number.isFinite) || !(pcm[100] > 0) ||
                !Number.isFinite(compressor.reduction))
                throw Error('nonfinite input poisoned the compressor detector');
            console.log('compressor nonfinite input passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: compressor nonfinite input passed"]);
}
