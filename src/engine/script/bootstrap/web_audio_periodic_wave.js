    // PeriodicWave owns its Fourier coefficients and a family of band-limited
    // lookup tables. The inverse FFT keeps 8192-coefficient waves practical.
    const periodicWaveState = new WeakMap();
    const periodicWaveBytes = new WeakMap();
    const MAX_PERIODIC_COEFFICIENTS = 8192;
    const MAX_PERIODIC_BYTES = 16 * 1024 * 1024;
    const MAX_PERIODIC_CACHED_TABLES = 8;
    const MAX_PERIODIC_DIRECT_HARMONICS = 64_000_000;

    const periodicCoefficients = (source, name) => {
        if (source === undefined) return null;
        if (source === null || typeof source[Symbol.iterator] !== 'function')
            throw new TypeError(name + ' must be a sequence');
        const values = [];
        for (const item of source) {
            if (values.length >= MAX_PERIODIC_COEFFICIENTS)
                throw new AudioDOMException('PeriodicWave coefficient limit exceeded',
                    'NotSupportedError');
            const converted = Math.fround(finite(item, name));
            if (!Number.isFinite(converted))
                throw new TypeError(name + ' coefficient is outside float range');
            values.push(converted);
        }
        return Float32Array.from(values);
    };

    const periodicTable = (coefficients, size, harmonicLimit) => {
        const real = new Float64Array(size);
        const imag = new Float64Array(size);
        for (let harmonic = 1; harmonic <= harmonicLimit; ++harmonic) {
            const opposite = size - harmonic;
            real[harmonic] = real[opposite] = coefficients.real[harmonic] / 2;
            imag[harmonic] = -coefficients.imag[harmonic] / 2;
            imag[opposite] = coefficients.imag[harmonic] / 2;
        }
        // Each conjugate-symmetric coefficient contributes half of its
        // sinusoid's amplitude to the unscaled inverse transform.
        audioFft(real, imag, true);
        return real;
    };

    class PeriodicWave {
        constructor(context, options = {}) {
            if (!(context instanceof BaseAudioContext))
                throw new TypeError('PeriodicWave requires a BaseAudioContext');
            if (options == null) options = {};
            let real = periodicCoefficients(options.real, 'real');
            let imag = periodicCoefficients(options.imag, 'imag');
            if (!real && !imag) {
                real = new Float32Array(2);
                imag = new Float32Array([0, 1]);
            } else {
                const length = real?.length ?? imag.length;
                if (length < 2 || (real && imag && real.length !== imag.length))
                    throw new AudioDOMException('PeriodicWave coefficient lengths differ or are too short',
                        'IndexSizeError');
                real ??= new Float32Array(length);
                imag ??= new Float32Array(length);
            }
            // The DC coefficients are ignored by the Web Audio rendering model.
            real[0] = imag[0] = 0;
            let size = 2048;
            while (size < real.length * 2) size *= 2;
            const maximumHarmonic = real.length - 1;
            const bytes = (real.length + imag.length) * 4 + size * 8;
            const used = periodicWaveBytes.get(context) ?? 0;
            if (bytes > MAX_PERIODIC_BYTES - used)
                throw new AudioDOMException('PeriodicWave table budget exceeded',
                    'NotSupportedError');
            const coefficients = { real, imag };
            const fullTable = periodicTable(coefficients, size, maximumHarmonic);
            let peak = 0;
            for (const sample of fullTable) peak = Math.max(peak, Math.abs(sample));
            const scale = options.disableNormalization || peak === 0 ? 1 : 1 / peak;
            periodicWaveState.set(this, { context, coefficients, size,
                maximumHarmonic, tables: new Map([[maximumHarmonic, fullTable]]),
                directHarmonics: 0, scale });
            periodicWaveBytes.set(context, used + bytes);
        }
    }

    const periodicWaveSample = (wave, phase, frequency, sampleRate) => {
        const state = periodicWaveState.get(wave);
        const cutoff = frequency === 0 ? state.maximumHarmonic : Math.min(
            state.maximumHarmonic,
            Math.floor(sampleRate / (2 * Math.abs(frequency))));
        if (cutoff < 1) return 0;
        let table = state.tables.get(cutoff);
        const tableBytes = state.size * 8;
        const used = periodicWaveBytes.get(state.context);
        if (!table && state.tables.size < MAX_PERIODIC_CACHED_TABLES &&
            tableBytes <= MAX_PERIODIC_BYTES - used) {
            table = periodicTable(state.coefficients, state.size, cutoff);
            state.tables.set(cutoff, table);
            periodicWaveBytes.set(state.context, used + tableBytes);
        }
        if (!table) {
            // Automation can cross a new Nyquist boundary every sample. Do not
            // allocate an unbounded number of FFT tables or silently omit any
            // still-audible partial. The explicit work limit protects the task.
            state.directHarmonics += cutoff;
            if (state.directHarmonics > MAX_PERIODIC_DIRECT_HARMONICS)
                throw new AudioDOMException('PeriodicWave automation work limit exceeded',
                    'NotSupportedError');
            const radians = 2 * Math.PI * phase;
            let sample = 0;
            for (let harmonic = 1; harmonic <= cutoff; ++harmonic)
                sample += state.coefficients.real[harmonic] *
                    Math.cos(harmonic * radians) +
                    state.coefficients.imag[harmonic] * Math.sin(harmonic * radians);
            return sample * state.scale;
        }
        const location = phase * table.length;
        const first = Math.floor(location) % table.length;
        const fraction = location - Math.floor(location);
        const second = (first + 1) % table.length;
        return (table[first] * (1 - fraction) +
            table[second] * fraction) * state.scale;
    };
