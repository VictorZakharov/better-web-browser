    // Web Audio §1.17: the impulse is acquired when assigned, and normalize
    // changes apply only on the next assignment. FFT state lives with the node.
    const audioConvolverState = new WeakMap();
    const convolverError = message => new AudioDOMException(message, 'NotSupportedError');
    const convolverChannelCount = value => {
        value = Number(value);
        if (!Number.isInteger(value) || value < 1 || value > 2)
            throw convolverError('ConvolverNode channelCount must be 1 or 2');
        return value;
    };
    const convolverCountMode = value => {
        value = String(value);
        if (value !== 'clamped-max' && value !== 'explicit')
            throw convolverError('ConvolverNode channelCountMode cannot be max');
        return value;
    };
    const convolverInterpretation = value => {
        value = String(value);
        if (value !== 'speakers' && value !== 'discrete')
            throw new TypeError('Invalid channelInterpretation');
        return value;
    };
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
        const buses = [...audioNodeState.get(node).inputs[0]].map(edge =>
            renderAudioNode(node.context, edge.source, frame, frames, cache, edge.output));
        const sourceChannels = Math.max(1, ...buses.map(bus => bus.length));
        const channels = state.channelCountMode === 'explicit' ? state.channelCount :
            Math.min(state.channelCount, sourceChannels);
        const mixed = silence(channels, frames);
        for (const bus of buses) for (let i = 0; i < frames; ++i) {
            let left = bus[0]?.[i] ?? 0;
            let right = bus[1]?.[i] ?? 0;
            if (state.channelInterpretation === 'speakers') {
                if (bus.length === 1) right = left;
                else if (bus.length === 3) {
                    left += Math.SQRT1_2 * bus[2][i];
                    right += Math.SQRT1_2 * bus[2][i];
                } else if (bus.length === 4) {
                    left += Math.SQRT1_2 * bus[2][i];
                    right += Math.SQRT1_2 * bus[3][i];
                } else if (bus.length >= 6) {
                    left += Math.SQRT1_2 * (bus[2][i] + bus[4][i]);
                    right += Math.SQRT1_2 * (bus[2][i] + bus[5][i]);
                }
            }
            // Non-finite source data must not persist in the FFT history and
            // overlap buffers after a malformed sample has passed the node.
            if (!Number.isFinite(left)) left = 0;
            if (!Number.isFinite(right)) right = 0;
            if (channels === 1)
                mixed[0][i] += bus.length === 1 ? left :
                    state.channelInterpretation === 'speakers' ? (left + right) * 0.5 : left;
            else {
                mixed[0][i] += left;
                mixed[1][i] += right;
            }
        }
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
        const output = renderConvolverBank(state.bank, input, frames);
        if (!output) return silence(1, frames);
        return channels === 1 && state.buffer.numberOfChannels === 1 ?
            [output[0]] : output;
    };

    class ConvolverNode extends AudioNode {
        constructor(context, options = {}) {
            options = options == null ? {} : Object(options);
            const contextState = audioContextState.get(context);
            if (!contextState) throw new TypeError('ConvolverNode requires an audio context');
            const channelCount = options.channelCount === undefined ? 2 :
                convolverChannelCount(options.channelCount);
            const channelCountMode = options.channelCountMode === undefined ?
                'clamped-max' : convolverCountMode(options.channelCountMode);
            const channelInterpretation = options.channelInterpretation === undefined ?
                'speakers' : convolverInterpretation(options.channelInterpretation);
            const normalize = !Boolean(options.disableNormalization);
            const buffer = options.buffer === undefined ? null : options.buffer;
            const prepared = convolverImpulse(buffer, contextState, normalize, 0);
            super(audioNodeToken, context, 1, 1);
            audioConvolverState.set(this, { buffer, normalize, ...prepared,
                channelCount, channelCountMode, channelInterpretation });
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
        }
        get normalize() { return audioConvolverState.get(this).normalize; }
        set normalize(value) { audioConvolverState.get(this).normalize = Boolean(value); }
        get channelCount() { return audioConvolverState.get(this).channelCount; }
        set channelCount(value) {
            audioConvolverState.get(this).channelCount = convolverChannelCount(value);
        }
        get channelCountMode() { return audioConvolverState.get(this).channelCountMode; }
        set channelCountMode(value) {
            audioConvolverState.get(this).channelCountMode = convolverCountMode(value);
        }
        get channelInterpretation() {
            return audioConvolverState.get(this).channelInterpretation;
        }
        set channelInterpretation(value) {
            audioConvolverState.get(this).channelInterpretation =
                convolverInterpretation(value);
        }
    }
