    // Web Audio 1.0 §1.31.2 requires upsample -> shape -> downsample for 2x/4x.
    // Filter design is deliberately left to the UA. A 17-tap windowed-sinc
    // reconstruction filter retains high-frequency input content; four
    // low-pass biquads reject newly created harmonics before decimation.
    // Both filters keep their history across render quanta.
    const waveShaperOversampleCoefficients = factor => {
        // The cutoff is 0.9 of the original-rate Nyquist. Expressing it at
        // the oversampled rate leaves a transition band before decimation.
        const k = Math.tan(Math.PI * 0.45 / factor);
        const sections = [];
        for (let section = 0; section < 4; section++) {
            const q = 1 / (2 * Math.cos((2 * section + 1) * Math.PI / 16));
            const norm = 1 / (1 + k / q + k * k);
            const b0 = k * k * norm;
            sections.push({
                b0, b1: 2 * b0, b2: b0,
                a1: 2 * (k * k - 1) * norm,
                a2: (1 - k / q + k * k) * norm
            });
        }
        return sections;
    };
    const waveShaperLowpassSections = {
        2: waveShaperOversampleCoefficients(2),
        4: waveShaperOversampleCoefficients(4)
    };
    const WAVE_SHAPER_HISTORY = 17;
    const waveShaperReconstruction = factor => {
        const sinc = x => Math.abs(x) < 1e-12 ? 1 :
            Math.sin(Math.PI * x) / (Math.PI * x);
        return Array.from({ length: factor }, (_, phaseIndex) => {
            // Causal reconstruction is delayed by eight input frames. At the
            // final phase the target lies exactly on an input sample.
            const phase = (phaseIndex + 1) / factor;
            const weights = Array.from({ length: WAVE_SHAPER_HISTORY }, (_, tap) => {
                const distance = tap - 9 + phase;
                const window = Math.abs(distance) <= 8.5 ?
                    0.5 + 0.5 * Math.cos(Math.PI * distance / 8.5) : 0;
                return sinc(distance) * window;
            });
            const sum = weights.reduce((total, weight) => total + weight, 0);
            return weights.map(weight => weight / sum);
        });
    };
    const waveShaperReconstructionWeights = {
        2: waveShaperReconstruction(2),
        4: waveShaperReconstruction(4)
    };

    const waveShaperSample = (curve, sample) => {
        if (!Number.isFinite(sample)) sample = 0;
        const last = curve.length - 1;
        const position = (sample + 1) * last / 2;
        if (position < 0) return curve[0];
        if (position >= last) return curve[last];
        const lower = Math.floor(position);
        const fraction = position - lower;
        return (1 - fraction) * curve[lower] + fraction * curve[lower + 1];
    };

    const renderOversampledWaveShaper = (input, curve, factor, state) => {
        let dsp = state.oversampleDsp;
        if (!dsp || dsp.factor !== factor)
            dsp = state.oversampleDsp = { factor, lanes: [] };
        const coefficients = waveShaperLowpassSections[factor];
        const reconstruction = waveShaperReconstructionWeights[factor];
        for (let channelIndex = 0; channelIndex < input.length; channelIndex++) {
            const channel = input[channelIndex];
            let lane = dsp.lanes[channelIndex];
            if (!lane) lane = dsp.lanes[channelIndex] = {
                history: new Float64Array(WAVE_SHAPER_HISTORY), historyIndex: -1,
                filters: coefficients.map(() => ({ z1: 0, z2: 0 }))
            };
            for (let i = 0; i < channel.length; i++) {
                // An audio graph must not let non-finite samples poison the
                // filter state for every later quantum.
                const current = Number.isFinite(channel[i]) ? channel[i] : 0;
                lane.historyIndex = (lane.historyIndex + 1) % WAVE_SHAPER_HISTORY;
                lane.history[lane.historyIndex] = current;
                let filtered = 0;
                for (const weights of reconstruction) {
                    let reconstructed = 0;
                    for (let tap = 0; tap < WAVE_SHAPER_HISTORY; tap++) {
                        let index = lane.historyIndex - tap;
                        if (index < 0) index += WAVE_SHAPER_HISTORY;
                        reconstructed += weights[tap] * lane.history[index];
                    }
                    filtered = waveShaperSample(curve, reconstructed);
                    if (!Number.isFinite(filtered)) filtered = 0;
                    for (let section = 0; section < coefficients.length; section++) {
                        const c = coefficients[section], s = lane.filters[section];
                        const output = c.b0 * filtered + s.z1;
                        s.z1 = c.b1 * filtered - c.a1 * output + s.z2;
                        s.z2 = c.b2 * filtered - c.a2 * output;
                        filtered = output;
                    }
                }
                channel[i] = Math.fround(filtered);
            }
        }
        // A retired output channel cannot carry an old filter tail back into
        // the graph when a later connection increases channel count again.
        dsp.lanes.length = input.length;
        return input;
    };
