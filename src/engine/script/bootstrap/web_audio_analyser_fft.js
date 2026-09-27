    // The Web Audio analyser uses a Blackman-windowed forward FFT with 1/N
    // scaling, then smooths magnitudes (not their logarithms) across quanta.
    const analyserFft = (time, previous, smoothing) => {
        const size = time.length;
        const real = new Float64Array(size);
        const imag = new Float64Array(size);
        for (let i = 0; i < size; ++i) {
            const angle = 2 * Math.PI * i / size;
            const window = 0.42 - 0.5 * Math.cos(angle) +
                0.08 * Math.cos(2 * angle);
            real[i] = time[i] * window / size;
        }
        audioFft(real, imag, false);
        const result = new Float32Array(size / 2);
        for (let i = 0; i < result.length; ++i) {
            const magnitude = Math.hypot(real[i], imag[i]);
            result[i] = smoothing * previous[i] + (1 - smoothing) * magnitude;
        }
        return result;
    };
