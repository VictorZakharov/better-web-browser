    const audioBiquadState = new WeakMap();
    const biquadTypes = ['lowpass', 'highpass', 'bandpass', 'lowshelf',
        'highshelf', 'peaking', 'notch', 'allpass'];
    const biquadParametersAt = (node, frame, offset, cache) => {
        const rate = node.context.sampleRate;
        return audioBiquadCoefficients(node.type,
            audioParamValueForFrame(node.frequency, frame, offset, rate, cache),
            audioParamValueForFrame(node.detune, frame, offset, rate, cache),
            audioParamValueForFrame(node.Q, frame, offset, rate, cache),
            audioParamValueForFrame(node.gain, frame, offset, rate, cache), rate);
    };

    class BiquadFilterNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            options = channels.options;
            const q = audioOption(options, 'Q', 1, value => finiteFloat(value, 'Q'));
            const detune = audioOption(options, 'detune', 0,
                value => finiteFloat(value, 'detune'));
            const frequency = audioOption(options, 'frequency', 350,
                value => finiteFloat(value, 'frequency'));
            const gain = audioOption(options, 'gain', 0, value => finiteFloat(value, 'gain'));
            const type = audioOption(options, 'type', 'lowpass', value => `${value}`);
            if (!biquadTypes.includes(type)) throw new TypeError('Invalid biquad filter type');
            super(audioNodeToken, context, 1, 1, audioChannelSettings(context, channels));
            const rate = context.sampleRate;
            const parameter = (value, min, max) =>
                new AudioParam(audioParamToken, context, value, min, max);
            Object.defineProperties(this, {
                frequency: { enumerable: true,
                    value: parameter(frequency, 0, rate / 2) },
                detune: { enumerable: true,
                    value: parameter(detune, -153600, 153600) },
                Q: { enumerable: true,
                    value: parameter(q,
                        -3.4028234663852886e38, 3.4028234663852886e38) },
                gain: { enumerable: true,
                    value: parameter(gain, -3.4028234663852886e38, 1540) }
            });
            for (const param of [this.frequency, this.detune, this.Q, this.gain])
                audioParamState.get(param).owner = this;
            audioBiquadState.set(this, { type, channels: [] });
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderBiquad(this, frame, frames, cache);
        }
        get type() {
            const state = audioBiquadState.get(this);
            if (!state) throw new TypeError('Illegal BiquadFilterNode invocation');
            return state.type;
        }
        set type(value) {
            const state = audioBiquadState.get(this);
            if (!state) throw new TypeError('Illegal BiquadFilterNode invocation');
            value = audioEnumAttribute(value, biquadTypes);
            if (value === null) return;
            state.type = value;
        }
        getFrequencyResponse(frequencyHz, magResponse, phaseResponse) {
            if (!audioBiquadState.has(this))
                throw new TypeError('Illegal BiquadFilterNode invocation');
            const rate = this.context.sampleRate;
            const frame = Math.floor(this.context.currentTime * rate / AUDIO_QUANTUM) *
                AUDIO_QUANTUM;
            const { b, a } = biquadParametersAt(this, frame, 0);
            fillAudioFrequencyResponse(b, a, rate,
                frequencyHz, magResponse, phaseResponse);
        }
    }

    const renderBiquad = (node, frame, frames, cache) => {
        const histories = audioBiquadState.get(node).channels;
        const mixed = mixAudioInputs(node.context, node, frame, frames, cache);
        const input = audioFilterInput(node, mixed, histories, biquadTailActive,
            () => ({ x1: 0, x2: 0, y1: 0, y2: 0 }));
        const output = silence(input.length, frames);
        for (let i = 0; i < frames; ++i) {
            const { b, a } = biquadParametersAt(node, frame, i, cache);
            for (let channel = 0; channel < histories.length; ++channel) {
                const state = histories[channel];
                const x = input[channel][i];
                const y = (b[0] * x + b[1] * state.x1 + b[2] * state.x2 -
                    a[1] * state.y1 - a[2] * state.y2) / a[0];
                state.x2 = state.x1;
                state.x1 = x;
                state.y2 = state.y1;
                state.y1 = y;
                output[channel][i] = y;
            }
        }
        return output;
    };
