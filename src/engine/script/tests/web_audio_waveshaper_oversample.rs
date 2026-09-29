use super::*;

#[test]
fn oversampled_waveshaper_keeps_channel_state_and_continuity_across_quanta() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 512, 8000);
        const buffer = context.createBuffer(2, 512, 8000);
        buffer.getChannelData(0).fill(0.5);
        buffer.getChannelData(1).fill(-0.25);
        const source = context.createBufferSource();
        source.buffer = buffer;
        const shaper = new WaveShaperNode(context, {
            curve: new Float32Array([-1, 0, 1]), oversample: '4x'
        });
        source.connect(shaper).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const left = result.getChannelData(0), right = result.getChannelData(1);
            for (let i = 100; i < 500; i++) {
                if (Math.abs(left[i] - 0.5) > 0.01 ||
                    Math.abs(right[i] + 0.25) > 0.01)
                    throw Error('oversampled steady-state PCM or channel isolation: ' + i +
                        ' ' + left[i] + ' ' + right[i]);
            }
            for (const boundary of [128, 256, 384]) {
                if (Math.abs(left[boundary] - left[boundary - 1]) > 0.001 ||
                    Math.abs(right[boundary] - right[boundary - 1]) > 0.001)
                    throw Error('oversampling filter history reset at a quantum boundary');
            }
            console.log('WaveShaper oversampling state passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: WaveShaper oversampling state passed"]
    );
}

#[test]
fn oversampled_waveshaper_filters_nonlinear_harmonics_before_decimation() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const curve = Float32Array.from({length: 1025}, (_, i) => {
            const x = i / 512 - 1;
            return x * x * x;
        });
        function render(mode) {
            const context = new OfflineAudioContext(1, 4000, 8000);
            const buffer = context.createBuffer(1, 4000, 8000);
            const samples = buffer.getChannelData(0);
            for (let i = 0; i < samples.length; i++)
                samples[i] = Math.sin(2 * Math.PI * 1600 * i / 8000);
            const source = context.createBufferSource();
            source.buffer = buffer;
            const shaper = new WaveShaperNode(context, {curve, oversample: mode});
            source.connect(shaper).connect(context.destination);
            source.start();
            return context.startRendering().then(result => result.getChannelData(0));
        }
        function aliasAmplitude(samples) {
            let cosine = 0, sine = 0;
            // Discard the startup transient. The 3900-sample window contains
            // an integer number of cycles at both 1600 and 3200 Hz.
            for (let i = 100; i < 4000; i++) {
                const angle = 2 * Math.PI * 3200 * i / 8000;
                cosine += samples[i] * Math.cos(angle);
                sine += samples[i] * Math.sin(angle);
            }
            return 2 * Math.hypot(cosine, sine) / 3900;
        }
        render('none').then(plain => render('2x').then(twice =>
            render('4x').then(four => {
                const base = aliasAmplitude(plain);
                const at2x = aliasAmplitude(twice);
                const at4x = aliasAmplitude(four);
                if (base < 0.15 || at2x >= base * 0.4 || at4x >= base * 0.4)
                    throw Error('nonlinear alias was not filtered: ' +
                        [base, at2x, at4x].join(','));
                console.log('WaveShaper alias rejection passed');
            })));
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WaveShaper alias rejection passed"]);
}

#[test]
fn oversample_without_a_curve_remains_a_passthrough() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        source.offset.value = 0.375;
        const shaper = context.createWaveShaper();
        shaper.oversample = '2x';
        source.connect(shaper).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const data = result.getChannelData(0);
            if (data[0] !== 0.375 || data[127] !== 0.375)
                throw Error('null-curve oversample mode altered PCM');
            console.log('WaveShaper null-curve oversampling passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: WaveShaper null-curve oversampling passed"]
    );
}

#[test]
fn oversampling_filter_tail_survives_source_completion_and_then_decays() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 512, 8000);
        const buffer = context.createBuffer(1, 1, 8000);
        buffer.getChannelData(0)[0] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const shaper = new WaveShaperNode(context, {
            curve: new Float32Array([-1, 0, 1]), oversample: '2x'
        });
        source.connect(shaper).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            let earlyTail = 0, lateTail = 0;
            for (let i = 1; i < 60; i++) earlyTail += Math.abs(samples[i]);
            for (let i = 256; i < samples.length; i++)
                lateTail = Math.max(lateTail, Math.abs(samples[i]));
            if (earlyTail < 0.1 || lateTail > 0.0001)
                throw Error('oversampling filter tail lost or did not decay: ' +
                    earlyTail + ',' + lateTail);
            console.log('WaveShaper filter tail passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WaveShaper filter tail passed"]);
}

#[test]
fn disconnected_oversampled_shaper_advances_before_reconnection() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 512, 8000);
        const source = context.createConstantSource();
        source.offset.value = -1;
        const shaper = new WaveShaperNode(context, {
            curve: new Float32Array([-1, 0, 1]), oversample: '4x'
        });
        source.connect(shaper).connect(context.destination);
        source.start();
        const first = context.suspend(128 / 8000);
        const second = context.suspend(384 / 8000);
        first.then(() => {
            shaper.disconnect(context.destination);
            source.offset.value = 1;
            return context.resume();
        });
        second.then(() => {
            shaper.connect(context.destination);
            return context.resume();
        });
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (samples[100] > -0.95 || samples[320] !== 0 ||
                samples[384] < 0.95)
                throw Error('disconnected WaveShaper history froze: ' +
                    [samples[100], samples[320], samples[384]].join(','));
            console.log('WaveShaper disconnected history passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: WaveShaper disconnected history passed"]
    );
}

#[test]
fn nonfinite_curve_point_does_not_permanently_poison_filter_history() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 384, 8000);
        const source = context.createConstantSource();
        source.offset.setValueAtTime(-1, 0);
        source.offset.setValueAtTime(0, 128 / 8000);
        source.offset.setValueAtTime(1, 256 / 8000);
        const shaper = new WaveShaperNode(context, {oversample: '2x'});
        shaper.curve = new Float32Array([-1, -0.5, NaN, 0.5, 1]);
        source.connect(shaper).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (samples[100] > -0.95 || !Number.isFinite(samples[140]) ||
                !Number.isFinite(samples[300]) || samples[300] < 0.95)
                throw Error('nonfinite curve poisoned oversampling history');
            console.log('WaveShaper nonfinite recovery passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: WaveShaper nonfinite recovery passed"],
        "{:?}",
        outcome.diagnostics
    );
}

#[test]
fn retired_channel_does_not_revive_its_old_filter_tail() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 512, 8000);
        function stereo(rightValue) {
            const buffer = context.createBuffer(2, 512, 8000);
            buffer.getChannelData(1).fill(rightValue);
            const source = context.createBufferSource();
            source.buffer = buffer;
            source.start();
            return source;
        }
        const first = stereo(1), last = stereo(0);
        const mono = context.createConstantSource();
        mono.offset.value = 0;
        mono.start();
        const shaper = new WaveShaperNode(context, {
            curve: new Float32Array([-1, 0, 1]), oversample: '4x'
        });
        first.connect(shaper).connect(context.destination);
        const firstPause = context.suspend(128 / 8000);
        const secondPause = context.suspend(384 / 8000);
        firstPause.then(() => {
            first.disconnect(shaper);
            mono.connect(shaper);
            return context.resume();
        });
        secondPause.then(() => {
            mono.disconnect(shaper);
            last.connect(shaper);
            return context.resume();
        });
        context.startRendering().then(result => {
            const right = result.getChannelData(1);
            if (right[100] < 0.95 || Math.abs(right[384]) > 0.001)
                throw Error('retired right channel revived stale filter state: ' +
                    right[100] + ',' + right[384]);
            console.log('WaveShaper retired channel passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WaveShaper retired channel passed"]);
}

#[test]
fn identity_curve_oversampling_preserves_high_frequency_level() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        function render(mode, frequency) {
            const context = new OfflineAudioContext(1, 4096, 8000);
            const buffer = context.createBuffer(1, 4096, 8000);
            const data = buffer.getChannelData(0);
            for (let i = 0; i < data.length; i++)
                data[i] = Math.sin(2 * Math.PI * frequency * i / 8000);
            const source = context.createBufferSource();
            source.buffer = buffer;
            const shaper = new WaveShaperNode(context, {
                curve: new Float32Array([-1, 0, 1]), oversample: mode
            });
            source.connect(shaper).connect(context.destination);
            source.start();
            return context.startRendering().then(result => {
                const rendered = result.getChannelData(0);
                let energy = 0;
                for (let i = 128; i < 4096; i++) energy += rendered[i] ** 2;
                return Math.sqrt(energy / (4096 - 128));
            });
        }
        Promise.all([render('none', 2400), render('2x', 2400),
            render('4x', 2400), render('none', 3200),
            render('2x', 3200), render('4x', 3200)]).then(v => {
            if (v[1] / v[0] < 0.9 || v[2] / v[0] < 0.9 ||
                v[4] / v[3] < 0.8 || v[5] / v[3] < 0.8)
                throw Error('identity-curve frequency loss: ' + v.join(','));
            console.log('WaveShaper identity passband passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: WaveShaper identity passband passed"]
    );
}
