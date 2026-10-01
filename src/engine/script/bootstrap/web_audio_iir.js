    const audioIirState = new WeakMap();
    const iirCoefficients = (value, name) => {
        if (value == null || typeof value[Symbol.iterator] !== 'function')
            throw new TypeError(name + ' must be a sequence of coefficients');
        const coefficients = [];
        for (const coefficient of value) {
            if (coefficients.length === 20)
                throw new DOMException(name + ' exceeds 20 coefficients',
                    'NotSupportedError');
            coefficients.push(finite(coefficient, name + ' coefficient'));
        }
        if (!coefficients.length)
            throw new DOMException(name + ' needs at least one coefficient',
                'NotSupportedError');
        return coefficients;
    };
    const fillAudioFrequencyResponse = (b, a, rate, frequencyHz,
        magResponse, phaseResponse) => {
        if (!(frequencyHz instanceof Float32Array) ||
            !(magResponse instanceof Float32Array) ||
            !(phaseResponse instanceof Float32Array) ||
            frequencyHz.length !== magResponse.length ||
            frequencyHz.length !== phaseResponse.length)
            throw new DOMException('Frequency response arrays must have equal lengths',
                'InvalidAccessError');
        // The three arrays may alias; snapshot input frequencies before writing.
        const frequencies = Float32Array.from(frequencyHz);
        const nyquist = rate / 2;
        for (let i = 0; i < frequencies.length; ++i) {
            const frequency = frequencies[i];
            if (frequency < 0 || frequency > nyquist || !Number.isFinite(frequency)) {
                magResponse[i] = NaN;
                phaseResponse[i] = NaN;
                continue;
            }
            const omega = 2 * Math.PI * frequency / rate;
            const response = coefficients => {
                let real = 0, imaginary = 0;
                for (let k = 0; k < coefficients.length; ++k) {
                    real += coefficients[k] * Math.cos(omega * k);
                    imaginary -= coefficients[k] * Math.sin(omega * k);
                }
                return [real, imaginary];
            };
            const [br, bi] = response(b);
            const [ar, ai] = response(a);
            const denominator = ar * ar + ai * ai;
            const real = (br * ar + bi * ai) / denominator;
            const imaginary = (bi * ar - br * ai) / denominator;
            magResponse[i] = Math.hypot(real, imaginary);
            phaseResponse[i] = Math.atan2(imaginary, real);
        }
    };

    // Web Audio 1.0 §1.21.4: sum(a[k] * y[n-k]) = sum(b[k] * x[n-k]).
    // Coefficients are copied at construction and cannot change afterwards.
    class IIRFilterNode extends AudioNode {
        constructor(context, options) {
            const channels = readAudioChannelOptions(context, options);
            options = channels.options;
            if (options == null || typeof options !== 'object')
                throw new TypeError('IIRFilterNode requires coefficient options');
            const a = iirCoefficients(options.feedback, 'feedback');
            const b = iirCoefficients(options.feedforward, 'feedforward');
            if (b.every(value => value === 0) || a[0] === 0)
                throw new DOMException('IIR coefficients have no valid transfer function',
                    'InvalidStateError');
            super(audioNodeToken, context, 1, 1, audioChannelSettings(context, channels));
            audioIirState.set(this, { b, a, position: 0,
                input: [], output: [] });
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderIir(this, frame, frames, cache);
        }
        getFrequencyResponse(frequencyHz, magResponse, phaseResponse) {
            const state = audioIirState.get(this);
            if (!state) throw new TypeError('Illegal IIRFilterNode invocation');
            fillAudioFrequencyResponse(state.b, state.a, this.context.sampleRate,
                frequencyHz, magResponse, phaseResponse);
        }
    }
    const renderIir = (node, frame, frames, cache) => {
        const state = audioIirState.get(node);
        const mixed = mixAudioInputs(node.context, node, frame, frames, cache);
        const input = audioFilterInput(node, mixed, state.output,
            (history, channel) => iirTailActive(history) || iirTailActive(state.input[channel]),
            () => new Float64Array(20));
        const channels = input.length;
        resizeAudioHistories(state.input, channels, () => new Float64Array(20));
        resizeAudioHistories(state.output, channels, () => new Float64Array(20));
        const output = silence(channels, frames);
        for (let i = 0; i < frames; ++i) {
            const position = state.position++ % 20;
            for (let channel = 0; channel < channels; ++channel) {
                const previousInput = state.input[channel];
                const previousOutput = state.output[channel];
                previousInput[position] = input[channel][i];
                let value = 0;
                for (let k = 0; k < state.b.length; ++k)
                    value += state.b[k] * previousInput[(position - k + 20) % 20];
                for (let k = 1; k < state.a.length; ++k)
                    value -= state.a[k] * previousOutput[(position - k + 20) % 20];
                value /= state.a[0];
                previousOutput[position] = value;
                output[channel][i] = value;
            }
        }
        return output;
    };
