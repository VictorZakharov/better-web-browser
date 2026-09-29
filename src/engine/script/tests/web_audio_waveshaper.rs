use super::*;

#[test]
fn wave_shaper_without_a_curve_passes_pcm_through() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        source.offset.value = 0.375;
        const shaper = context.createWaveShaper();
        source.connect(shaper).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (shaper.curve !== null || samples[0] !== 0.375 ||
                samples[127] !== 0.375)
                throw Error('WaveShaper null-curve passthrough failed');
            console.log('WaveShaper passthrough passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WaveShaper passthrough passed"]);
}

#[test]
fn wave_shaper_interpolates_odd_curves_and_clamps_outside_unit_input() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 7, 8000);
        buffer.getChannelData(0).set([-2, -1, -0.5, 0, 0.5, 1, 2]);
        const source = context.createBufferSource();
        source.buffer = buffer;
        const shaper = context.createWaveShaper();
        shaper.curve = new Float32Array([-1, 0.5, 1]);
        source.connect(shaper).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const actual = [...result.getChannelData(0).slice(0, 7)];
            const expected = [-1, -1, -0.25, 0.5, 0.75, 1, 1];
            if (actual.some((value, index) =>
                Math.abs(value - expected[index]) > 0.000001))
                throw Error('WaveShaper odd-curve interpolation or edge clamp failed');
            console.log('WaveShaper odd curve PCM passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WaveShaper odd curve PCM passed"]);
}

#[test]
fn wave_shaper_even_curve_produces_dc_from_unconnected_silence() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const shaper = new WaveShaperNode(context, {
            curve: new Float32Array([0, 1])
        });
        shaper.connect(context.destination);
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (samples[0] !== 0.5 || samples[127] !== 0.5)
                throw Error('WaveShaper even-curve midpoint or DC output failed');
            console.log('WaveShaper even curve DC passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WaveShaper even curve DC passed"]);
}

#[test]
fn wave_shaper_acquires_curve_and_does_not_reshape_after_author_mutation() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(1, 1, 8000);
        buffer.getChannelData(0)[0] = 0;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const shaper = context.createWaveShaper();
        const curve = new Float32Array([-1, 0.25, 1]);
        shaper.curve = curve;
        if (shaper.curve !== curve || shaper.oversample !== 'none')
            throw Error('WaveShaper curve reflection or default mismatch');
        curve[1] = 0.75;
        source.connect(shaper).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            if (result.getChannelData(0)[0] !== 0.25)
                throw Error('WaveShaper used mutable author curve instead of acquired copy');
            console.log('WaveShaper curve acquisition passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: WaveShaper curve acquisition passed"]
    );
}

#[test]
fn wave_shaper_shapes_each_input_channel_before_destination_downmix() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const buffer = context.createBuffer(2, 1, 8000);
        buffer.getChannelData(0)[0] = -1;
        buffer.getChannelData(1)[0] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const shaper = new WaveShaperNode(context, {
            curve: new Float32Array([-1, 1, 1])
        });
        source.connect(shaper).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            if (result.getChannelData(0)[0] !== 0)
                throw Error('WaveShaper incorrectly downmixed before shaping');
            console.log('WaveShaper channel mapping passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WaveShaper channel mapping passed"]);
}

#[test]
fn wave_shaper_accepts_oversampling_and_rejects_invalid_modes_or_second_curve() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const shaper = context.createWaveShaper();
        const errors = [];
        try { shaper.curve = new Float32Array(1); }
        catch (error) { errors.push(error.name); }
        shaper.curve = new Float32Array([-1, 1]);
        shaper.curve = null;
        try { shaper.curve = new Float32Array([-1, 1]); }
        catch (error) { errors.push(error.name); }
        shaper.oversample = '2x';
        if (shaper.oversample !== '2x') throw Error('2x mode was not stored');
        shaper.oversample = '4x';
        if (shaper.oversample !== '4x') throw Error('4x mode was not stored');
        try { shaper.oversample = 'invalid'; }
        catch (error) { errors.push(error.name); }
        if (shaper.oversample !== '4x') throw Error('invalid mode changed state');
        try { new WaveShaperNode(context, {curve: [1]}); }
        catch (error) { errors.push(error.name); }
        if (errors.join(',') !==
            'InvalidStateError,InvalidStateError,TypeError,InvalidStateError')
            throw Error('WaveShaper validation mismatch: ' + errors.join(','));
        console.log('WaveShaper validation passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WaveShaper validation passed"]);
}
