use super::*;

#[test]
fn analyser_passes_audio_and_retains_unconnected_time_domain_history() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const source = context.createConstantSource();
        const analyzer = context.createAnalyser();
        analyzer.fftSize = 256;
        source.connect(analyzer);
        source.connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const floatData = new Float32Array(257).fill(-7);
            const byteData = new Uint8Array(257).fill(7);
            analyzer.getFloatTimeDomainData(floatData);
            analyzer.getByteTimeDomainData(byteData);
            if (!(analyzer instanceof AnalyserNode) ||
                result.getChannelData(0)[200] !== 1 ||
                floatData[0] !== 1 || floatData[255] !== 1 || floatData[256] !== -7 ||
                byteData[0] !== 255 || byteData[255] !== 255 || byteData[256] !== 7)
                throw Error('disconnected analyser input history');
            console.log('analyser history passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: analyser history passed"]);
}

#[test]
fn analyser_fft_reports_blackman_windowed_frequency_bins() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createOscillator();
        source.frequency.value = 1000;
        const analyzer = new AnalyserNode(context,
            { fftSize: 32, smoothingTimeConstant: 0 });
        source.connect(analyzer).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const db = new Float32Array(analyzer.frequencyBinCount);
            const bytes = new Uint8Array(analyzer.frequencyBinCount);
            analyzer.getFloatFrequencyData(db);
            analyzer.getByteFrequencyData(bytes);
            const samples = result.getChannelData(0);
            if (analyzer.frequencyBinCount !== 16 ||
                Math.abs(samples[2] - 1) > 0.00001 ||
                db[4] < -16 || db[4] > -10 || db[4] < db[3] || db[4] < db[5] ||
                bytes[4] < 255 || bytes[0] !== 0)
                throw Error('Blackman FFT bin or passthrough');
            console.log('analyser FFT passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: analyser FFT passed"]);
}

#[test]
fn analyser_validates_options_and_resets_smoothing_without_losing_history() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const expected = (action, name) => {
            try { action(); } catch (error) { return error.name === name; }
            return false;
        };
        const analyzer = context.createAnalyser();
        const valid = analyzer.fftSize === 2048 && analyzer.frequencyBinCount === 1024 &&
            analyzer.minDecibels === -100 && analyzer.maxDecibels === -30 &&
            analyzer.smoothingTimeConstant === 0.8 &&
            expected(() => analyzer.fftSize = 31, 'IndexSizeError') &&
            expected(() => new AnalyserNode(context, { fftSize: null }),
                'IndexSizeError') &&
            expected(() => analyzer.fftSize = 65536, 'IndexSizeError') &&
            expected(() => analyzer.minDecibels = -30, 'IndexSizeError') &&
            expected(() => analyzer.maxDecibels = -100, 'IndexSizeError') &&
            expected(() => analyzer.smoothingTimeConstant = 1.1, 'IndexSizeError') &&
            expected(() => analyzer.getFloatFrequencyData([]), 'TypeError') &&
            expected(() => new AnalyserNode(context,
                { minDecibels: -20, maxDecibels: -30 }), 'IndexSizeError');
        if (!valid) throw Error('analyser option validation');
        analyzer.fftSize = 32;
        const source = context.createConstantSource();
        source.connect(analyzer);
        source.start();
        context.startRendering().then(() => {
            analyzer.fftSize = 256;
            const samples = new Float32Array(256);
            analyzer.getFloatTimeDomainData(samples);
            if (analyzer.frequencyBinCount !== 128 || samples.some(x => x !== 1))
                throw Error('analyser discarded prior time-domain history');
            console.log('analyser validation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: analyser validation passed"]);
}

#[test]
fn analyser_smooths_once_per_render_quantum_and_scales_bytes() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const source = context.createConstantSource();
        const analyzer = new AnalyserNode(context, {
            fftSize: 128, smoothingTimeConstant: 0.5,
            minDecibels: -100, maxDecibels: 0
        });
        source.connect(analyzer);
        source.start();
        source.stop(128 / 8000);
        let first;
        context.suspend(128 / 8000).then(() => {
            const a = new Float32Array(1), b = new Float32Array(1);
            analyzer.getFloatFrequencyData(a);
            analyzer.getFloatFrequencyData(b);
            if (a[0] !== b[0]) throw Error('analyser smoothed twice in one quantum');
            first = a[0];
            context.resume();
        });
        context.startRendering().then(() => {
            const decibels = new Float32Array(65).fill(123);
            const bytes = new Uint8Array(65).fill(123);
            analyzer.getFloatFrequencyData(decibels);
            analyzer.getByteFrequencyData(bytes);
            if (Math.abs((decibels[0] - first) + 6.0206) > 0.001 ||
                decibels[64] !== 123 || bytes[64] !== 123 ||
                bytes[0] !== Math.floor(255 * (decibels[0] + 100) / 100))
                throw Error('analyser smoothing or byte scaling');
            console.log('analyser smoothing passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: analyser smoothing passed"]);
}
