    const audioCompressorState = new WeakMap();
    const compressorDelaySeconds = 0.006;

    // Web Audio §1.19.4 permits the implementation to choose a continuously
    // differentiable knee and envelope shape. The identity section extends to
    // threshold; this quadratic knee then joins the 1/ratio-slope section with
    // matching value and derivative at threshold + knee.
    const compressorOutputDb = (inputDb, threshold, knee, ratio) => {
        if (inputDb <= threshold) return inputDb;
        if (inputDb >= threshold + knee || knee === 0)
            return threshold + (inputDb - threshold) / ratio +
                (1 - 1 / ratio) * knee / 2;
        const intoKnee = inputDb - threshold;
        return inputDb + (1 / ratio - 1) * intoKnee * intoKnee / (2 * knee);
    };
    const compressorGain = (level, threshold, knee, ratio) => {
        if (level < 0.0001) return 1;
        const inputDb = 20 * Math.log10(level);
        const outputDb = compressorOutputDb(inputDb, threshold, knee, ratio);
        return Math.min(1, Math.pow(10, (outputDb - inputDb) / 20));
    };
    const compressorSmoothing = (seconds, sampleRate) => seconds === 0 ? 1 :
        1 - Math.exp(-Math.LN10 / (2 * seconds * sampleRate));

    class DynamicsCompressorNode extends AudioNode {
        constructor(context, options = {}) {
            options = options == null ? {} : Object(options);
            const values = [
                ['threshold', -24, -100, 0], ['knee', 30, 0, 40],
                ['ratio', 12, 1, 20], ['attack', 0.003, 0, 1],
                ['release', 0.25, 0, 1]
            ];
            const initial = values.map(([name, defaultValue]) =>
                options[name] === undefined ? defaultValue :
                    finiteFloat(options[name], name));
            const contextState = audioContextState.get(context);
            if (!contextState)
                throw new TypeError('DynamicsCompressorNode requires an audio context');
            const channels = contextState.channels;
            const delayFrames = Math.max(1,
                Math.round(context.sampleRate * compressorDelaySeconds));
            const delayBytes = delayFrames * channels * Float32Array.BYTES_PER_ELEMENT;
            if (delayBytes > MAX_AUDIO_DELAY_BYTES - contextState.delayBytes)
                throw new DOMException('Audio delay lines exceed the context memory limit',
                    'NotSupportedError');
            super(audioNodeToken, context, 1, 1);
            for (let i = 0; i < values.length; ++i) {
                const [name, defaultValue, min, max] = values[i];
                const param = new AudioParam(audioParamToken, context,
                    defaultValue, min, max, 'k-rate', true);
                if (initial[i] !== defaultValue) param.value = initial[i];
                audioParamState.get(param).owner = this;
                Object.defineProperty(this, name, { enumerable: true, value: param });
            }
            contextState.delayBytes += delayBytes;
            audioCompressorState.set(this, {
                gain: 1, reduction: 0, position: 0, delayFrames,
                delayed: Array.from({ length: channels },
                    () => new Float32Array(delayFrames))
            });
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderDynamicsCompressor(this, frame, frames, cache);
        }
        get reduction() {
            const state = audioCompressorState.get(this);
            if (!state) throw new TypeError('Illegal DynamicsCompressorNode invocation');
            return state.reduction;
        }
    }

    const renderDynamicsCompressor = (node, frame, frames, cache) => {
        const state = audioCompressorState.get(node);
        const channels = state.delayed.length;
        const input = mixAudioInputs(node.context, node, frame, frames,
            cache, channels);
        const output = silence(channels, frames);
        const rate = node.context.sampleRate;
        const value = param => audioParamValueForFrame(param, frame, 0, rate, cache);
        const threshold = value(node.threshold);
        const knee = value(node.knee);
        const ratio = value(node.ratio);
        const attackRate = compressorSmoothing(value(node.attack), rate);
        const releaseRate = compressorSmoothing(value(node.release), rate);
        // The makeup stage is determined by the curve at full-scale input,
        // independent of the momentary detector level (Web Audio §1.19.4).
        const fullScaleDb = compressorOutputDb(0, threshold, knee, ratio);
        const makeup = Math.pow(10, -0.6 * fullScaleDb / 20);
        for (let i = 0; i < frames; ++i) {
            let level = 0;
            for (const channel of input) {
                const sample = channel[i];
                if (Number.isFinite(sample)) level = Math.max(level, Math.abs(sample));
            }
            const target = compressorGain(level, threshold, knee, ratio);
            const smoothing = target < state.gain ? attackRate : releaseRate;
            state.gain += (target - state.gain) * smoothing;
            for (let channel = 0; channel < channels; ++channel) {
                const delayed = state.delayed[channel];
                output[channel][i] = delayed[state.position] * state.gain * makeup;
                delayed[state.position] = Number.isFinite(input[channel][i]) ?
                    input[channel][i] : 0;
            }
            state.position = (state.position + 1) % state.delayFrames;
        }
        // The specified meter reports the gain applied after the fixed makeup
        // stage, sampled once at the end of the render quantum (§1.19.4).
        state.reduction = 20 * Math.log10(state.gain * makeup);
        return output;
    };
