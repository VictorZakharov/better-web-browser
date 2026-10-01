    const analyserState = new WeakMap();
    const ANALYSER_HISTORY = 32768;
    const MAX_ANALYSER_NODES = 32;

    const validAnalyserSize = value => {
        const size = Number(value);
        if (!Number.isInteger(size) || size < 32 || size > 32768 ||
            (size & (size - 1)) !== 0)
            throw new AudioDOMException('fftSize must be a power of two from 32 to 32768',
                'IndexSizeError');
        return size;
    };
    const validAnalyserSmoothing = value => {
        value = finite(value, 'smoothingTimeConstant');
        if (value < 0 || value > 1)
            throw new AudioDOMException('smoothingTimeConstant is outside [0, 1]',
                'IndexSizeError');
        return value;
    };
    const validAnalyserRange = (minimum, maximum) => {
        minimum = finite(minimum, 'minDecibels');
        maximum = finite(maximum, 'maxDecibels');
        if (minimum >= maximum)
            throw new AudioDOMException('minDecibels must be less than maxDecibels',
                'IndexSizeError');
        return { minimum, maximum };
    };

    const analyserTimeData = state => {
        const samples = new Float32Array(state.size);
        for (let i = 0; i < samples.length; ++i)
            samples[i] = state.history[(state.cursor - state.size + i +
                ANALYSER_HISTORY) % ANALYSER_HISTORY];
        return samples;
    };
    const analyserFrequencyData = state => {
        if (state.frequencyFrame === state.frame) return state.frequency;
        const time = analyserTimeData(state);
        state.frequency = analyserFft(time, state.frequency, state.smoothing);
        state.frequencyFrame = state.frame;
        return state.frequency;
    };

    class AnalyserNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            options = channels.options;
            const size = audioOption(options, 'fftSize', 2048, validAnalyserSize);
            const max = audioOption(options, 'maxDecibels', -30,
                value => finite(value, 'maxDecibels'));
            const min = audioOption(options, 'minDecibels', -100,
                value => finite(value, 'minDecibels'));
            const smoothing = audioOption(options, 'smoothingTimeConstant', 0.8,
                validAnalyserSmoothing);
            const { minimum, maximum } = validAnalyserRange(min, max);
            if (!(context instanceof BaseAudioContext))
                throw new TypeError('AnalyserNode requires a BaseAudioContext');
            const contextState = audioContextState.get(context);
            if ((contextState.analyserNodes ?? 0) >= MAX_ANALYSER_NODES)
                throw new AudioDOMException('AnalyserNode limit reached',
                    'NotSupportedError');
            super(audioNodeToken, context, 1, 1, audioChannelSettings(context, channels));
            contextState.analyserNodes = (contextState.analyserNodes ?? 0) + 1;
            const state = { size, minimum, maximum, smoothing,
                history: new Float32Array(ANALYSER_HISTORY), cursor: 0,
                frame: -1, frequencyFrame: -2,
                frequency: new Float32Array(size / 2) };
            analyserState.set(this, state);
            audioNodeState.get(this).render = (frame, frames, cache) => {
                const samples = mixAudioInputs(this.context, this, frame, frames, cache);
                // §1.8.5 always uses the speaker mono matrix for analysis,
                // independently of the node's pass-through interpretation.
                const mono = mixAudioBuses([samples], 1, frames, 'speakers')[0];
                for (let i = 0; i < frames; ++i) {
                    state.history[state.cursor] = mono[i];
                    state.cursor = (state.cursor + 1) % ANALYSER_HISTORY;
                }
                state.frame = frame;
                return samples;
            };
        }
        get fftSize() { return analyserState.get(this).size; }
        set fftSize(value) {
            const size = validAnalyserSize(value);
            const state = analyserState.get(this);
            if (size === state.size) return;
            state.size = size;
            state.frequency = new Float32Array(size / 2);
            state.frequencyFrame = -2;
        }
        get frequencyBinCount() { return this.fftSize / 2; }
        get minDecibels() { return analyserState.get(this).minimum; }
        set minDecibels(value) {
            const state = analyserState.get(this);
            state.minimum = validAnalyserRange(value, state.maximum).minimum;
        }
        get maxDecibels() { return analyserState.get(this).maximum; }
        set maxDecibels(value) {
            const state = analyserState.get(this);
            state.maximum = validAnalyserRange(state.minimum, value).maximum;
        }
        get smoothingTimeConstant() { return analyserState.get(this).smoothing; }
        set smoothingTimeConstant(value) {
            analyserState.get(this).smoothing = validAnalyserSmoothing(value);
        }
        getFloatTimeDomainData(array) {
            if (!(array instanceof Float32Array))
                throw new TypeError('getFloatTimeDomainData requires Float32Array');
            array.set(analyserTimeData(analyserState.get(this)).subarray(0,
                Math.min(array.length, this.fftSize)));
        }
        getByteTimeDomainData(array) {
            if (!(array instanceof Uint8Array))
                throw new TypeError('getByteTimeDomainData requires Uint8Array');
            const data = analyserTimeData(analyserState.get(this));
            for (let i = 0; i < Math.min(array.length, data.length); ++i)
                array[i] = Math.max(0, Math.min(255, Math.floor(128 * (1 + data[i]))));
        }
        getFloatFrequencyData(array) {
            if (!(array instanceof Float32Array))
                throw new TypeError('getFloatFrequencyData requires Float32Array');
            const data = analyserFrequencyData(analyserState.get(this));
            for (let i = 0; i < Math.min(array.length, data.length); ++i)
                array[i] = 20 * Math.log10(data[i]);
        }
        getByteFrequencyData(array) {
            if (!(array instanceof Uint8Array))
                throw new TypeError('getByteFrequencyData requires Uint8Array');
            const state = analyserState.get(this);
            const data = analyserFrequencyData(state);
            for (let i = 0; i < Math.min(array.length, data.length); ++i) {
                const decibels = 20 * Math.log10(data[i]);
                const scaled = 255 * (decibels - state.minimum) /
                    (state.maximum - state.minimum);
                array[i] = Math.max(0, Math.min(255, Math.floor(scaled)));
            }
        }
    }
