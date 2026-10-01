    // Web Audio 1.0 §1.31: the curve is sampled over [-1, 1] with linear
    // interpolation. A copy is acquired when set, so later author writes to
    // the exposed Float32Array cannot change audio already configured here.
    const audioWaveShaperState = new WeakMap();
    const MAX_WAVESHAPER_CURVE_SAMPLES = 1_048_576;
    const MAX_WAVESHAPER_CONTEXT_BYTES = 16 * 1024 * 1024;

    const waveShaperState = node => {
        const state = audioWaveShaperState.get(node);
        if (!state) throw new TypeError('Illegal WaveShaperNode invocation');
        return state;
    };

    const waveShaperOptionCurve = value => {
        if (value == null || typeof value[Symbol.iterator] !== 'function')
            throw new TypeError('WaveShaper curve must be a sequence');
        const samples = [];
        for (const item of value) {
            if (samples.length >= MAX_WAVESHAPER_CURVE_SAMPLES)
                throw new AudioDOMException('WaveShaper curve is too large', 'NotSupportedError');
            const sample = +item;
            if (!Number.isFinite(sample) || !Number.isFinite(Math.fround(sample)))
                throw new TypeError('WaveShaper curve samples must be finite floats');
            samples.push(Math.fround(sample));
        }
        if (samples.length < 2)
            throw new AudioDOMException('WaveShaper curve requires two samples',
                'InvalidStateError');
        return Float32Array.from(samples);
    };

    const validateWaveShaperOversample = value => {
        value = `${value}`;
        if (value !== 'none' && value !== '2x' && value !== '4x')
            throw new TypeError('Invalid WaveShaper oversample mode');
        return value;
    };

    class WaveShaperNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            options = channels.options;
            const curve = audioOption(options, 'curve', null, waveShaperOptionCurve);
            const oversample = audioOption(options, 'oversample', 'none',
                validateWaveShaperOversample);
            const contextState = audioContextState.get(context);
            if (curve && (!contextState ||
                (contextState.waveCurveBytes || 0) + curve.byteLength >
                    MAX_WAVESHAPER_CONTEXT_BYTES))
                throw new AudioDOMException('WaveShaper curve budget exceeded',
                    'NotSupportedError');
            super(audioNodeToken, context, 1, 1, audioChannelSettings(context, channels));
            audioWaveShaperState.set(this, {
                curve: null, samples: null, curveSet: false, oversample,
                oversampleDsp: null
            });
            if (curve) this.curve = curve;
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderWaveShaper(this, frame, frames, cache);
        }
        get curve() { return waveShaperState(this).curve; }
        set curve(value) {
            const state = waveShaperState(this);
            // Web IDL converts both null and undefined to this nullable value.
            if (value == null) {
                if (state.samples)
                    audioContextState.get(this.context).waveCurveBytes -=
                        state.samples.byteLength;
                state.curve = null;
                state.samples = null;
                return;
            }
            if (!(value instanceof Float32Array))
                throw new TypeError('WaveShaper curve must be a Float32Array or null');
            if (state.curveSet)
                throw new AudioDOMException('WaveShaper curve was already set',
                    'InvalidStateError');
            if (value.length < 2)
                throw new AudioDOMException('WaveShaper curve requires two samples',
                    'InvalidStateError');
            if (value.length > MAX_WAVESHAPER_CURVE_SAMPLES)
                throw new AudioDOMException('WaveShaper curve is too large',
                    'NotSupportedError');
            const contextState = audioContextState.get(this.context);
            if ((contextState.waveCurveBytes || 0) + value.byteLength >
                MAX_WAVESHAPER_CONTEXT_BYTES)
                throw new AudioDOMException('WaveShaper curve budget exceeded',
                    'NotSupportedError');
            const samples = new Float32Array(value);
            contextState.waveCurveBytes =
                (contextState.waveCurveBytes || 0) + samples.byteLength;
            state.curve = value;
            state.samples = samples;
            state.curveSet = true;
        }
        get oversample() { return waveShaperState(this).oversample; }
        set oversample(value) {
            const state = waveShaperState(this);
            const mode = audioEnumAttribute(value, ['none', '2x', '4x']);
            if (mode === null) return;
            if (mode !== state.oversample) state.oversampleDsp = null;
            state.oversample = mode;
        }
    }

    const renderWaveShaper = (node, frame, frames, cache) => {
        const input = mixAudioInputs(node.context, node, frame, frames, cache);
        const state = audioWaveShaperState.get(node);
        const curve = state.samples;
        if (!curve) return input;
        if (state.oversample !== 'none') return renderOversampledWaveShaper(input,
            curve, state.oversample === '2x' ? 2 : 4, state);
        for (const channel of input) {
            for (let i = 0; i < frames; ++i)
                channel[i] = waveShaperSample(curve, channel[i]);
        }
        return input;
    };
