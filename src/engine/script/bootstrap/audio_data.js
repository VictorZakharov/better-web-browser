    const audioDataToken = Symbol('AudioData resource');
    const makeAudioData = state => new AudioData(audioDataToken, state);
    const closeAudioData = value => {
        const state = audioDataState(value);
        state.bytes = null;
        state.format = null;
        state.rate = 0;
        state.frames = 0;
        state.channels = 0;
    };
    class AudioData {
        constructor(init, internal) {
            if (init === audioDataToken) { audioDataStates.set(this, internal); return; }
            if (arguments.length === 0) throw new TypeError('AudioData requires an initializer');
            const options = audioDataInit(init);
            frameValidateTransfers(options.transfer);
            const bytes = new Uint8Array(options.data.subarray(0, options.size));
            audioDataStates.set(this, {bytes, format: options.format, channels: options.channels,
                frames: options.frames, rate: options.rate, timestamp: options.timestamp});
            for (const buffer of options.transfer) host('arrayBufferDetach', buffer);
        }
        get format() { return audioDataState(this).format; }
        get sampleRate() { return audioDataState(this).rate; }
        get numberOfFrames() { return audioDataState(this).frames; }
        get numberOfChannels() { return audioDataState(this).channels; }
        get duration() {
            const state = audioDataState(this);
            return state.rate ? Math.floor(state.frames * 1000000 / state.rate) : 0;
        }
        get timestamp() { return audioDataState(this).timestamp; }
        allocationSize(options) {
            audioDataState(this);
            if (arguments.length === 0) throw new TypeError('Audio copy options are required');
            const converted = audioCopyDictionary(options);
            return audioCopyLayout(activeAudioData(this), converted).size;
        }
        copyTo(destination, options) {
            audioDataState(this);
            if (arguments.length < 2) throw new TypeError('Audio copy requires a destination and options');
            const bytes = frameBuffer(destination);
            const converted = audioCopyDictionary(options);
            const state = activeAudioData(this);
            copyAudioSamples(state, bytes, audioCopyLayout(state, converted));
        }
        clone() { return makeAudioData({...activeAudioData(this)}); }
        close() { closeAudioData(this); }
    }
    Object.defineProperty(AudioData, 'length', {value: 1, configurable: true});
    Object.defineProperty(AudioData.prototype, Symbol.toStringTag, {value: 'AudioData', configurable: true});
    frameIDL([AudioData]);
    Object.assign(globalThis, {AudioData});
