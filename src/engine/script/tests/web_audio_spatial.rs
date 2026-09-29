use super::*;

#[test]
fn panner_and_listener_positions_render_equal_power_and_distance_pcm() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 512, 8000);
        const source = context.createConstantSource();
        const panner = context.createPanner();
        panner.setPosition(0, 0, -1);
        source.connect(panner).connect(context.destination);
        source.start();
        const first = context.suspend(128 / 8000);
        const second = context.suspend(256 / 8000);
        const third = context.suspend(384 / 8000);
        first.then(() => { panner.setPosition(1, 0, 0); return context.resume(); });
        second.then(() => {
            context.listener.setPosition(1, 0, 0);
            return context.resume();
        });
        third.then(() => { panner.setPosition(3, 0, 0); return context.resume(); });
        context.startRendering().then(result => {
            const left = result.getChannelData(0), right = result.getChannelData(1);
            const close = (value, expected) => Math.abs(value - expected) < 0.001;
            if (!close(left[100], Math.SQRT1_2) ||
                !close(right[100], Math.SQRT1_2) ||
                !close(left[180], 0) || !close(right[180], 1) ||
                !close(left[300], Math.SQRT1_2) ||
                !close(right[300], Math.SQRT1_2) ||
                !close(left[400], 0) || !close(right[400], 0.5))
                throw Error('spatial equal-power/distance PCM: ' +
                    [left[100], right[100], left[180], right[180],
                        left[300], right[300], left[400], right[400]].join(','));
            console.log('spatial position and distance PCM passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: spatial position and distance PCM passed"]
    );
}

#[test]
fn panner_distance_models_and_sound_cone_change_pcm() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        function render(model, orientation, cone) {
            const context = new OfflineAudioContext(2, 128, 8000);
            const source = context.createConstantSource();
            const panner = new PannerNode(context, {
                distanceModel: model, positionZ: -2, rolloffFactor: 2,
                maxDistance: 3, coneInnerAngle: cone ? 90 : 360,
                coneOuterAngle: cone ? 180 : 360, coneOuterGain: 0.2,
                orientationZ: orientation
            });
            source.connect(panner).connect(context.destination);
            source.start();
            return context.startRendering().then(result => result.getChannelData(0)[80]);
        }
        Promise.all([
            render('linear', 1, false), render('inverse', 1, false),
            render('exponential', 1, false), render('inverse', -1, true)
        ]).then(values => {
            const expected = [Math.SQRT1_2 / 2, Math.SQRT1_2 / 3,
                Math.SQRT1_2 / 4,
                Math.SQRT1_2 / 3 * 0.2];
            if (values.some((value, i) => Math.abs(value - expected[i]) > 0.001))
                throw Error('distance/cone PCM: ' + values.join(','));
            console.log('spatial distance and cone PCM passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: spatial distance and cone PCM passed"]
    );
}

#[test]
fn listener_audio_param_automation_and_stereo_routing_affect_output() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 256, 8000);
        const buffer = context.createBuffer(2, 256, 8000);
        buffer.getChannelData(0).fill(1);
        const source = context.createBufferSource();
        source.buffer = buffer;
        const panner = context.createPanner();
        context.listener.positionX.setValueAtTime(-1, 0);
        context.listener.positionX.setValueAtTime(1, 128 / 8000);
        source.connect(panner).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const left = result.getChannelData(0), right = result.getChannelData(1);
            if (Math.abs(left[64]) > 0.001 || Math.abs(right[64] - 1) > 0.001 ||
                Math.abs(left[192] - 1) > 0.001 || Math.abs(right[192]) > 0.001)
                throw Error('listener automation or stereo routing: ' +
                    [left[64], right[64], left[192], right[192]].join(','));
            console.log('spatial listener automation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: spatial listener automation passed"]);
}

#[test]
fn panner_position_ramp_is_sampled_at_audio_rate() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const source = context.createConstantSource();
        const panner = context.createPanner();
        panner.positionZ.value = -1;
        panner.positionX.setValueAtTime(-1, 0);
        panner.positionX.linearRampToValueAtTime(1, 128 / 8000);
        source.connect(panner).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const left = result.getChannelData(0), right = result.getChannelData(1);
            if (!(left[0] > right[0] + 0.2 &&
                Math.abs(left[64] - right[64]) < 0.02 &&
                right[127] > left[127] + 0.2))
                throw Error('a-rate panner position stayed fixed for the quantum: ' +
                    [left[0], right[0], left[64], right[64],
                        left[127], right[127]].join(','));
            console.log('spatial a-rate panner ramp passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: spatial a-rate panner ramp passed"]);
}

#[test]
fn listener_position_modulation_changes_panner_pcm_without_a_cycle() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 256, 8000);
        const source = context.createConstantSource();
        const offset = context.createConstantSource();
        const panner = context.createPanner();
        panner.positionZ.value = -1;
        source.connect(panner).connect(context.destination);
        offset.connect(context.listener.positionX);
        source.start();
        offset.start();
        context.suspend(128 / 8000).then(() => {
            offset.disconnect(context.listener.positionX);
            return context.resume();
        });
        context.startRendering().then(result => {
            const left = result.getChannelData(0), right = result.getChannelData(1);
            if (!(left[80] > right[80] + 0.2 &&
                Math.abs(left[192] - right[192]) < 0.02))
                throw Error('listener modulation did not reach panner PCM: ' +
                    [left[80], right[80], left[192], right[192]].join(','));
            console.log('spatial listener modulation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: spatial listener modulation passed"]);
}

#[test]
fn panner_rejects_unsupported_hrtf_and_listener_feedback_cycles() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const panner = context.createPanner();
        const errors = [];
        for (const run of [
            () => new PannerNode(context, {panningModel: 'HRTF'}),
            () => { panner.panningModel = 'HRTF'; },
            () => { panner.distanceModel = 'other'; },
            () => { panner.refDistance = -1; },
            () => { panner.maxDistance = 0; },
            () => { panner.coneOuterGain = 2; },
            () => panner.connect(context.listener.positionX),
            () => new AudioListener()
        ]) {
            try { run(); errors.push('none'); }
            catch (error) { errors.push(error.name); }
        }
        if (errors.join(',') !== 'NotSupportedError,NotSupportedError,' +
            'TypeError,RangeError,RangeError,InvalidStateError,' +
            'NotSupportedError,TypeError')
            throw Error('spatial validation: ' + errors.join(','));
        if (panner.panningModel !== 'equalpower' || panner.refDistance !== 1)
            throw Error('invalid setter changed spatial state');
        let reads = 0;
        const options = {};
        Object.defineProperty(options, 'orientationX', {get() {
            if (++reads > 1) throw Error('dictionary member read twice');
            return 0;
        }});
        if (new PannerNode(context, options).orientationX.value !== 0 || reads !== 1)
            throw Error('dictionary conversion did not read member once');
        console.log('spatial validation passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: spatial validation passed"]);
}

#[test]
fn panner_downmixes_quad_speakers_before_summing_mono_connection() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const quad = context.createBuffer(4, 128, 8000);
        [0.2, 0.4, 0.6, 0.8].forEach((level, channel) =>
            quad.getChannelData(channel).fill(level));
        const quadSource = context.createBufferSource();
        quadSource.buffer = quad;
        const monoSource = context.createConstantSource();
        monoSource.offset.value = 0.1;
        const panner = context.createPanner();
        panner.setPosition(0, 0, -1);
        quadSource.connect(panner);
        monoSource.connect(panner);
        panner.connect(context.destination);
        quadSource.start();
        monoSource.start();
        context.startRendering().then(result => {
            const left = result.getChannelData(0)[64];
            const right = result.getChannelData(1)[64];
            // Quad L=.5*(front L+surround L), R=.5*(front R+surround R).
            // The other mono connection is replicated after input count clamps to 2.
            if (Math.abs(left - 0.5) > 0.001 || Math.abs(right - 0.7) > 0.001)
                throw Error('quad speaker downmix: ' + left + ',' + right);
            console.log('quad speaker downmix passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: quad speaker downmix passed"]);
}

#[test]
fn panner_downmixes_five_point_one_speakers_without_lfe() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const surround = context.createBuffer(6, 128, 8000);
        [0.2, 0.1, 0.1, 0.9, 0.2, 0.3].forEach((level, channel) =>
            surround.getChannelData(channel).fill(level));
        const source = context.createBufferSource();
        source.buffer = surround;
        const panner = context.createPanner();
        panner.setPosition(0, 0, -1);
        source.connect(panner).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const left = result.getChannelData(0)[64];
            const right = result.getChannelData(1)[64];
            const expectedLeft = 0.2 + Math.SQRT1_2 * (0.1 + 0.2);
            const expectedRight = 0.1 + Math.SQRT1_2 * (0.1 + 0.3);
            if (Math.abs(left - expectedLeft) > 0.001 ||
                Math.abs(right - expectedRight) > 0.001)
                throw Error('5.1 speaker downmix or LFE leak: ' + left + ',' + right);
            console.log('5.1 speaker downmix passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: 5.1 speaker downmix passed"]);
}
