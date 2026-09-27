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
        value = String(value);
        if (value !== 'none' && value !== '2x' && value !== '4x')
            throw new TypeError('Invalid WaveShaper oversample mode');
        // Neither ordinary interpolation nor output-rate averaging is the
        // upsample/filter/downsample chain required for 2x and 4x. Do not
        // report those modes as working until that signal path exists.
        if (value !== 'none')
            throw new AudioDOMException('WaveShaper oversampling is unavailable',
                'NotSupportedError');
        return value;
    };

    class WaveShaperNode extends AudioNode {
        constructor(context, options = {}) {
            if (options == null) options = {};
            const oversample = validateWaveShaperOversample(
                options.oversample === undefined ? 'none' : options.oversample);
            const curve = options.curve === undefined ? null :
                waveShaperOptionCurve(options.curve);
            const contextState = audioContextState.get(context);
            if (curve && (!contextState ||
                (contextState.waveCurveBytes || 0) + curve.byteLength >
                    MAX_WAVESHAPER_CONTEXT_BYTES))
                throw new AudioDOMException('WaveShaper curve budget exceeded',
                    'NotSupportedError');
            super(audioNodeToken, context, 1, 1);
            audioWaveShaperState.set(this, {
                curve: null, samples: null, curveSet: false, oversample
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
            waveShaperState(this).oversample = validateWaveShaperOversample(value);
        }
    }

    const renderWaveShaper = (node, frame, frames, cache) => {
        const edges = audioNodeState.get(node).inputs[0];
        let channels = 1;
        for (const edge of edges)
            channels = Math.max(channels, renderAudioNode(node.context,
                edge.source, frame, frames, cache, edge.output).length);
        const input = mixAudioInputs(node.context, node, frame, frames, cache,
            channels);
        const curve = audioWaveShaperState.get(node).samples;
        if (!curve) return input;
        const last = curve.length - 1;
        for (const channel of input) {
            for (let i = 0; i < frames; ++i) {
                const position = (channel[i] + 1) * last / 2;
                if (position < 0) channel[i] = curve[0];
                else if (position >= last) channel[i] = curve[last];
                else {
                    const lower = Math.floor(position);
                    const fraction = position - lower;
                    channel[i] = (1 - fraction) * curve[lower] +
                        fraction * curve[lower + 1];
                }
            }
        }
        return input;
    };
