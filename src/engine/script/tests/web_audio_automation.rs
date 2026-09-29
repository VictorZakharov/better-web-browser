use super::*;

#[test]
fn repeated_immediate_audio_param_writes_do_not_exhaust_automation_budget() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const panner = context.createPanner();
        for (let i = 0; i < 5000; i++)
            panner.positionX.value = i % 2;
        if (panner.positionX.value !== 1)
            throw Error('last immediate position write was not retained');
        console.log('AudioParam immediate compaction passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: AudioParam immediate compaction passed"]
    );
}

#[test]
fn exponential_ramp_has_exact_geometric_samples() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        source.offset.setValueAtTime(1, 0);
        source.offset.exponentialRampToValueAtTime(16, 4 / 8000);
        source.connect(context.destination);
        source.start();
        context.startRendering().then(buffer => {
            const samples = buffer.getChannelData(0);
            if (samples.slice(0, 6).join(',') !== '1,2,4,8,16,16')
                throw Error('exponential ramp samples');
            console.log('exponential ramp passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: exponential ramp passed"]);
}

#[test]
fn target_and_curve_automation_use_continuous_sample_values() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const target = new OfflineAudioContext(1, 128, 8000);
        const targetSource = target.createConstantSource();
        targetSource.offset.setValueAtTime(0, 0);
        targetSource.offset.setTargetAtTime(1, 2 / 8000, 2 / 8000);
        targetSource.connect(target.destination);
        targetSource.start();
        const curve = new OfflineAudioContext(1, 128, 8000);
        const curveSource = curve.createConstantSource();
        const values = [0, 1, 0];
        curveSource.offset.setValueCurveAtTime(values, 0, 4 / 8000);
        values[1] = 99;
        curveSource.connect(curve.destination);
        curveSource.start();
        Promise.all([target.startRendering(), curve.startRendering()]).then(([a, b]) => {
            const t = a.getChannelData(0), c = b.getChannelData(0);
            const near = (actual, expected) => Math.abs(actual - expected) < 0.00001;
            if (!near(t[2], 0) || !near(t[4], 1 - Math.exp(-1)) ||
                !near(t[6], 1 - Math.exp(-2)) ||
                !near(c[0], 0) || !near(c[1], 0.5) ||
                !near(c[2], 1) || !near(c[3], 0.5) || !near(c[4], 0))
                throw Error('target or curve automation samples');
            console.log('target and curve passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: target and curve passed"]);
}

#[test]
fn value_curve_reaches_its_endpoint_before_a_following_ramp() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        source.offset.setValueCurveAtTime([0, 1], 0, 4 / 8000);
        source.offset.linearRampToValueAtTime(0, 8 / 8000);
        source.connect(context.destination);
        source.start();
        context.startRendering().then(buffer => {
            const samples = buffer.getChannelData(0);
            const near = (frame, expected) =>
                Math.abs(samples[frame] - expected) < 0.00001;
            if (!near(0, 0) || !near(2, 0.5) || !near(4, 1) ||
                !near(6, 0.5) || !near(8, 0))
                throw Error('curve endpoint was not the following ramp anchor');
            console.log('curve-to-ramp anchor passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: curve-to-ramp anchor passed"]);
}

#[test]
fn zero_target_time_constant_jumps_to_target() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        source.offset.setTargetAtTime(0, 2 / 8000, 0);
        source.connect(context.destination);
        source.start();
        context.startRendering().then(buffer => {
            const samples = buffer.getChannelData(0);
            if (samples[0] !== 1 || samples[1] !== 1 ||
                samples[2] !== 0 || samples[3] !== 0)
                throw Error('zero time constant did not jump at the target time');
            console.log('zero target time constant passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: zero target time constant passed"]);
}

#[test]
fn cancel_and_hold_keeps_partial_ramp_and_curve_shaping() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const ramp = new OfflineAudioContext(1, 128, 8000);
        const rampSource = ramp.createConstantSource();
        rampSource.offset.setValueAtTime(0, 0);
        rampSource.offset.linearRampToValueAtTime(1, 8 / 8000);
        rampSource.offset.cancelAndHoldAtTime(4 / 8000);
        rampSource.connect(ramp.destination);
        rampSource.start();
        const curve = new OfflineAudioContext(1, 128, 8000);
        const curveSource = curve.createConstantSource();
        curveSource.offset.setValueCurveAtTime([0, 1, 0], 0, 8 / 8000);
        curveSource.offset.cancelAndHoldAtTime(4 / 8000);
        curveSource.connect(curve.destination);
        curveSource.start();
        Promise.all([ramp.startRendering(), curve.startRendering()]).then(([a, b]) => {
            const r = a.getChannelData(0), c = b.getChannelData(0);
            const near = (actual, expected) => Math.abs(actual - expected) < 0.00001;
            if (!near(r[0], 0) || !near(r[2], 0.25) ||
                !near(r[4], 0.5) || !near(r[8], 0.5) ||
                !near(c[0], 0) || !near(c[2], 0.5) ||
                !near(c[4], 1) || !near(c[8], 1))
                throw Error('cancelAndHold did not preserve active automation');
            console.log('cancel and hold passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: cancel and hold passed"]);
}

#[test]
fn cancel_scheduled_values_removes_an_overlapping_curve() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        source.offset.setValueCurveAtTime([0, 1, 0], 0, 8 / 8000);
        source.offset.cancelScheduledValues(4 / 8000);
        source.connect(context.destination);
        source.start();
        context.startRendering().then(buffer => {
            const samples = buffer.getChannelData(0);
            if (samples[0] !== 1 || samples[4] !== 1 || samples[8] !== 1)
                throw Error('overlapping curve was not removed');
            console.log('cancel scheduled values passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: cancel scheduled values passed"]);
}

#[test]
fn nominal_audio_param_limits_clamp_dsp_only() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        const modulator = new ConstantSourceNode(context, {offset: 3e38});
        const gain = context.createGain();
        gain.gain.setValueAtTime(3e38, 0);
        if (gain.gain.value !== Math.fround(3e38))
            throw Error('automation trajectory was incorrectly clamped');
        source.connect(gain).connect(context.destination);
        modulator.connect(gain.gain);
        source.start();
        modulator.start();
        context.startRendering().then(buffer => {
            const sample = buffer.getChannelData(0)[0];
            if (!Number.isFinite(sample) || sample !== gain.gain.maxValue)
                throw Error('DSP value was not clamped to nominal maximum: ' + sample);
            console.log('nominal DSP clamp passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: nominal DSP clamp passed"]);
}

#[test]
fn automation_curve_rejects_finite_doubles_outside_float_range() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        let rejected = false;
        try { source.offset.setValueCurveAtTime([0, 1e100], 0, 1); }
        catch (error) { rejected = error instanceof TypeError; }
        if (!rejected) throw Error('out-of-float-range curve sample was accepted');
        source.offset.setValueCurveAtTime([0, 1], 0, 1);
        console.log('automation curve float conversion passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: automation curve float conversion passed"]
    );
}

#[test]
fn float_valued_param_operations_and_node_options_reject_overflow() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const param = context.createGain().gain;
        const operations = [
            () => { param.value = 1e100; },
            () => param.setValueAtTime(1e100, 0),
            () => param.linearRampToValueAtTime(1e100, 1),
            () => param.exponentialRampToValueAtTime(1e100, 1),
            () => param.setTargetAtTime(1e100, 0, 1),
            () => new GainNode(context, {gain: 1e100}),
            () => new OscillatorNode(context, {frequency: 1e100}),
            () => new AudioBufferSourceNode(context, {detune: 1e100}),
            () => new ConstantSourceNode(context, {offset: 1e100}),
            () => new StereoPannerNode(context, {pan: 1e100}),
            () => new BiquadFilterNode(context, {Q: 1e100}),
        ];
        for (const operation of operations) {
            let rejected = false;
            try { operation(); }
            catch (error) { rejected = error instanceof TypeError; }
            if (!rejected) throw Error('float overflow was accepted');
        }
        if (param.value !== 1) throw Error('failed float conversion mutated AudioParam');
        console.log('Web IDL float overflow passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: Web IDL float overflow passed"]);
}
