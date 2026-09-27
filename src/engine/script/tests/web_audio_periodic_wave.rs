use super::*;

#[test]
fn periodic_wave_renders_normalized_and_raw_fourier_coefficients() {
    let (_, outcome) = execute_html(
        r#"<script>
        const near = (actual, expected) => Math.abs(actual - expected) < 0.0001;
        const normalized = new OfflineAudioContext(1, 128, 8000);
        const raw = new OfflineAudioContext(1, 128, 8000);
        const large = new OfflineAudioContext(1, 128, 8000);
        const real = new Float32Array([9, 0]);
        const imag = new Float32Array([9, 0.25]);
        const wave = normalized.createPeriodicWave(real, imag);
        const rawWave = raw.createPeriodicWave(real, imag,
            { disableNormalization: true });
        real[1] = 100;
        imag[1] = 0;
        const first = new OscillatorNode(normalized,
            { type: 'custom', periodicWave: wave, frequency: 1000 });
        const second = raw.createOscillator();
        const third = large.createOscillator();
        second.setPeriodicWave(rawWave);
        third.setPeriodicWave(large.createPeriodicWave([0, 0], [0, 3e38]));
        second.frequency.value = 1000;
        third.frequency.value = 1000;
        first.connect(normalized.destination);
        second.connect(raw.destination);
        third.connect(large.destination);
        first.start(); second.start(); third.start();
        Promise.all([normalized.startRendering(), raw.startRendering(),
            large.startRendering()]).then(([a, b, c]) => {
            const x = a.getChannelData(0), y = b.getChannelData(0),
                z = c.getChannelData(0);
            if (first.type !== 'custom' || second.type !== 'custom' ||
                !near(x[0], 0) || !near(x[2], 1) || !near(x[6], -1) ||
                !near(y[2], 0.25) || !near(y[6], -0.25) || !near(z[2], 1))
                throw Error('PeriodicWave normalization, DC, or coefficient copy');
            console.log('periodic coefficients passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: periodic coefficients passed"]);
}

#[test]
fn periodic_wave_band_limits_harmonics_above_nyquist() {
    let (_, outcome) = execute_html(
        r#"<script>
        const low = new OfflineAudioContext(1, 128, 8000);
        const high = new OfflineAudioContext(1, 128, 8000);
        const lowOsc = low.createOscillator();
        const highOsc = high.createOscillator();
        lowOsc.setPeriodicWave(low.createPeriodicWave([0, 0, 0], [0, 0, 1],
            { disableNormalization: true }));
        highOsc.setPeriodicWave(high.createPeriodicWave([0, 0, 0], [0, 0, 1],
            { disableNormalization: true }));
        lowOsc.frequency.value = 1000;
        highOsc.frequency.value = 3000;
        lowOsc.connect(low.destination);
        highOsc.connect(high.destination);
        lowOsc.start(); highOsc.start();
        Promise.all([low.startRendering(), high.startRendering()]).then(([a, b]) => {
            const audible = a.getChannelData(0), filtered = b.getChannelData(0);
            if (Math.abs(audible[1] - 1) > 0.0001 ||
                filtered.some(sample => sample !== 0))
                throw Error('PeriodicWave harmonic cutoff');
            console.log('periodic Nyquist passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: periodic Nyquist passed"]);
}

#[test]
fn periodic_wave_keeps_every_audible_harmonic_at_mid_band_cutoff() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const oscillator = context.createOscillator();
        // At 1500 Hz the second harmonic (3000 Hz) is below Nyquist, but
        // the third (4500 Hz) is not. Both coefficients are present.
        oscillator.setPeriodicWave(context.createPeriodicWave(
            [0, 0, 0, 0], [0, 0, 1, 1], { disableNormalization: true }));
        oscillator.frequency.value = 1500;
        oscillator.connect(context.destination);
        oscillator.start();
        context.startRendering().then(result => {
            const sample = result.getChannelData(0)[1];
            if (Math.abs(sample - Math.SQRT1_2) > 0.0001)
                throw Error('audible second harmonic was dropped');
            console.log('periodic mid-band passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: periodic mid-band passed"]);
}

#[test]
fn periodic_wave_bounded_table_cache_falls_back_to_exact_synthesis() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const real = new Array(17).fill(0), imag = new Array(17).fill(0);
        imag[1] = 1;
        const oscillator = context.createOscillator();
        oscillator.setPeriodicWave(context.createPeriodicWave(real, imag));
        const frequencies = Array.from({length: 10}, (_, i) => 4000 / (16 - i));
        for (let i = 0; i < frequencies.length; ++i)
            oscillator.frequency.setValueAtTime(frequencies[i], i / 8000);
        oscillator.connect(context.destination);
        oscillator.start();
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            let phase = 0;
            for (let i = 0; i < frequencies.length; ++i) {
                if (Math.abs(samples[i] - Math.sin(2 * Math.PI * phase)) > 0.001)
                    throw Error('dynamic harmonic cutoff or bounded-table fallback');
                phase += frequencies[i] / 8000;
            }
            console.log('periodic fallback passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: periodic fallback passed"]);
}

#[test]
fn periodic_wave_validates_lengths_context_and_custom_type() {
    let (_, outcome) = execute_html(
        r#"<script>
        const a = new OfflineAudioContext(1, 128, 8000);
        const b = new OfflineAudioContext(1, 128, 8000);
        const expected = (action, name) => {
            try { action(); } catch (error) { return error.name === name; }
            return false;
        };
        const wave = new PeriodicWave(a);
        const oscillator = a.createOscillator();
        const valid = expected(() => new PeriodicWave(a, { real: [0] }), 'IndexSizeError') &&
            expected(() => a.createPeriodicWave([0, 1], [0, 1, 2]), 'IndexSizeError') &&
            expected(() => new PeriodicWave(a, { real: [0, Infinity] }), 'TypeError') &&
            expected(() => new PeriodicWave(a, { real: [0, 1e100] }), 'TypeError') &&
            expected(() => new PeriodicWave(a, { real: new Float32Array(8193) }),
                'NotSupportedError') &&
            expected(() => oscillator.type = 'custom', 'InvalidStateError') &&
            expected(() => new OscillatorNode(a, { type: 'custom' }), 'InvalidStateError') &&
            expected(() => b.createOscillator().setPeriodicWave(wave), 'InvalidAccessError') &&
            expected(() => oscillator.setPeriodicWave({}), 'TypeError');
        if (!valid) throw Error('PeriodicWave validation');
        oscillator.setPeriodicWave(wave);
        oscillator.type = 'sine';
        if (oscillator.type !== 'sine') throw Error('custom-to-builtin transition');
        console.log('periodic validation passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: periodic validation passed"]);
}

#[test]
fn periodic_wave_phase_survives_waveform_change_during_suspension() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const oscillator = context.createOscillator();
        oscillator.frequency.value = 1000;
        oscillator.connect(context.destination);
        oscillator.start();
        context.suspend(128 / 8000).then(() => {
            oscillator.setPeriodicWave(context.createPeriodicWave([0, 0], [0, 0.5],
                { disableNormalization: true }));
            context.resume();
        });
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (Math.abs(samples[127] + Math.SQRT1_2) > 0.0001 ||
                Math.abs(samples[128]) > 0.0001 ||
                Math.abs(samples[130] - 0.5) > 0.0001)
                throw Error('waveform change reset oscillator phase');
            console.log('periodic phase passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: periodic phase passed"]);
}
