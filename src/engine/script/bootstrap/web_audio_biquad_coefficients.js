    // Web Audio 1.0 §1.13.5 normative transfer functions. Coefficients remain
    // unnormalized here; both PCM and frequency response divide by a[0].
    const audioBiquadCoefficients = (type, frequency, detune, q, gain, rate) => {
        const computedFrequency = Math.max(0, Math.min(rate / 2,
            frequency * Math.pow(2, detune / 1200)));
        const omega = 2 * Math.PI * computedFrequency / rate;
        const cosine = Math.cos(omega), sine = Math.sin(omega);
        const alphaQ = sine / (2 * Math.max(Number.EPSILON, q));
        const alphaDb = sine / (2 * Math.pow(10,
            Math.max(-770, Math.min(770, q)) / 20));
        const A = Math.pow(10, Math.max(-1540, Math.min(1540, gain)) / 40);
        const shelf = Math.SQRT2 * sine * Math.sqrt(A);
        let b, a;
        if (type === 'lowpass') {
            b = [(1 - cosine) / 2, 1 - cosine, (1 - cosine) / 2];
            a = [1 + alphaDb, -2 * cosine, 1 - alphaDb];
        } else if (type === 'highpass') {
            b = [(1 + cosine) / 2, -(1 + cosine), (1 + cosine) / 2];
            a = [1 + alphaDb, -2 * cosine, 1 - alphaDb];
        } else if (type === 'bandpass') {
            b = [alphaQ, 0, -alphaQ];
            a = [1 + alphaQ, -2 * cosine, 1 - alphaQ];
        } else if (type === 'notch') {
            b = [1, -2 * cosine, 1];
            a = [1 + alphaQ, -2 * cosine, 1 - alphaQ];
        } else if (type === 'allpass') {
            b = [1 - alphaQ, -2 * cosine, 1 + alphaQ];
            a = [1 + alphaQ, -2 * cosine, 1 - alphaQ];
        } else if (type === 'peaking') {
            b = [1 + alphaQ * A, -2 * cosine, 1 - alphaQ * A];
            a = [1 + alphaQ / A, -2 * cosine, 1 - alphaQ / A];
        } else if (type === 'lowshelf') {
            b = [A * ((A + 1) - (A - 1) * cosine + shelf),
                2 * A * ((A - 1) - (A + 1) * cosine),
                A * ((A + 1) - (A - 1) * cosine - shelf)];
            a = [(A + 1) + (A - 1) * cosine + shelf,
                -2 * ((A - 1) + (A + 1) * cosine),
                (A + 1) + (A - 1) * cosine - shelf];
        } else {
            b = [A * ((A + 1) + (A - 1) * cosine + shelf),
                -2 * A * ((A - 1) + (A + 1) * cosine),
                A * ((A + 1) + (A - 1) * cosine - shelf)];
            a = [(A + 1) - (A - 1) * cosine + shelf,
                2 * ((A - 1) - (A + 1) * cosine),
                (A + 1) - (A - 1) * cosine - shelf];
        }
        return { b, a };
    };
