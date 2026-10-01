    // Web Audio §1.17: the impulse is acquired when assigned, and normalize
    // changes apply only on the next assignment. FFT state lives with the node.
    const audioConvolverState = new WeakMap();
    const convolverError = message => new AudioDOMException(message, 'NotSupportedError');
    const convolverImpulse = (buffer, contextState, normalize, releasedBytes) => {
        if (buffer === null) return { bank: null, bytes: 0 };
        if (!(buffer instanceof AudioBuffer))
            throw new TypeError('ConvolverNode buffer must be an AudioBuffer or null');
        const channels = buffer.numberOfChannels;
        if (![1, 2, 4].includes(channels) || buffer.sampleRate !== contextState.sampleRate)
            throw convolverError('Impulse response needs 1, 2, or 4 channels at the context sample rate');
        if (buffer.length > MAX_CONVOLVER_IR_FRAMES)
            throw convolverError('Impulse response exceeds the 131072-frame processing limit');
        const bytes = convolverBankBytes(buffer.length, channels);
        if (bytes > MAX_CONVOLVER_BYTES - (contextState.convolverBytes || 0) + releasedBytes)
            throw convolverError('Impulse responses exceed the context convolution memory limit');
        // Snapshot before RMS analysis and FFT setup. Later writes to AudioBuffer
        // cannot change this response until the author assigns buffer again.
        const impulse = Array.from({ length: channels }, (_, channel) =>
            Float32Array.from(buffer.getChannelData(channel), sample =>
                Number.isFinite(sample) ? sample : 0));
        let scale = 1;
        if (normalize) {
            let power = 0;
            for (const channel of impulse)
                for (const sample of channel) power += sample * sample;
            power = Math.sqrt(power / (channels * buffer.length));
            if (!Number.isFinite(power) || power < 0.000125) power = 0.000125;
            scale = 0.00125 / power * (44100 / buffer.sampleRate);
            if (channels === 4) scale *= 0.5;
        }
        return { bank: createConvolverBank(impulse, scale), bytes };
    };

    const mixConvolverInput = (node, state, frame, frames, cache) => {
        const mixed = mixAudioInputs(node.context, node, frame, frames, cache);
        const channels = mixed.length;
        // Non-finite source data must not persist in FFT history or overlap.
        for (const channel of mixed)
            for (let i = 0; i < frames; ++i)
                if (!Number.isFinite(channel[i])) channel[i] = 0;
        // A single-channel *input bus* is used for both virtual sources of a
        // four-channel IR. `discrete` affects connection up-mixing only when
        // channelCountMode actually creates a two-channel input bus.
        return { input: [mixed[0], mixed[1] || mixed[0]], channels };
    };
    const renderConvolver = (node, frame, frames, cache) => {
        const state = audioConvolverState.get(node);
        const { input, channels } = mixConvolverInput(node, state, frame, frames, cache);
        if (!state.bank) return silence(1, frames);
        const outputWidth = retainAudioTailWidth(state, channels, frame, frames,
            state.buffer.length);
        const output = renderConvolverBank(state.bank, input, frames);
        if (!output) return silence(1, frames);
        return outputWidth === 1 && state.buffer.numberOfChannels === 1 ?
            [output[0]] : output;
    };

    class ConvolverNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            options = channels.options;
            const contextState = audioContextState.get(context);
            if (!contextState) throw new TypeError('ConvolverNode requires an audio context');
            const buffer = audioOption(options, 'buffer', null, value => {
                if (value !== null && !(value instanceof AudioBuffer))
                    throw new TypeError('Convolver buffer must be an AudioBuffer or null');
                return value;
            });
            const normalize = !Boolean(options.disableNormalization);
            const settings = audioChannelSettings(context, channels, 'convolver');
            const prepared = convolverImpulse(buffer, contextState, normalize, 0);
            super(audioNodeToken, context, 1, 1, settings);
            audioConvolverState.set(this, { buffer, normalize, ...prepared });
            contextState.convolverBytes = (contextState.convolverBytes || 0) + prepared.bytes;
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderConvolver(this, frame, frames, cache);
        }
        get buffer() { return audioConvolverState.get(this).buffer; }
        set buffer(value) {
            const state = audioConvolverState.get(this);
            if (!state) throw new TypeError('Illegal ConvolverNode invocation');
            const contextState = audioContextState.get(this.context);
            const prepared = convolverImpulse(value, contextState, state.normalize, state.bytes);
            contextState.convolverBytes += prepared.bytes - state.bytes;
            state.buffer = value;
            state.bank = prepared.bank;
            state.bytes = prepared.bytes;
            state.channelDeadlines = null;
            contextState.graphRevision++;
        }
        get normalize() { return audioConvolverState.get(this).normalize; }
        set normalize(value) { audioConvolverState.get(this).normalize = Boolean(value); }
    }
