    const initializeAudioCodec = (owner, init, encode) => {
        const options = frameDictionary(init);
        const error = frameRequired(options.error, 'error');
        if (typeof error !== 'function') throw new TypeError('Audio codec error callback must be callable');
        const output = frameRequired(options.output, 'output');
        if (typeof output !== 'function') throw new TypeError('Audio codec output callback must be callable');
        const lifetime = {id:null};
        audioCodecFinalizer.register(owner, lifetime, owner);
        audioCodecStates.set(owner, {owner, encode, error, output, status:'unconfigured', lifetime,
            id:null, active:null, config:null, queue:[], bytes:0, size:0, timer:null,
            generation:0, flushes:new Set(), ondequeue:null, dequeueListener:null, dequeueScheduled:false,
            keyRequired:true, metadataPending:true});
    };
    const setAudioCodecDequeueHandler = (owner, value, encode) => {
        const state = audioCodecState(owner, encode);
        const handler = typeof value === 'function' ? value : null;
        if (!handler && state.dequeueListener) {
            EventTarget.prototype.removeEventListener.call(owner, 'dequeue', state.dequeueListener);
            state.dequeueListener = null;
        } else if (handler && !state.dequeueListener) {
            // Event-handler registration acquires its listener position on the
            // first non-null assignment; replacements keep that position.
            state.dequeueListener = event => state.ondequeue?.call(owner, event);
            EventTarget.prototype.addEventListener.call(owner, 'dequeue', state.dequeueListener);
        }
        state.ondequeue = handler;
    };
    const configureAudioCodec = (owner, value, encode) => {
        const state = audioCodecState(owner, encode);
        const config = audioCodecConfig(value, encode);
        if (state.status === 'closed') throw frameError('Audio codec is closed');
        enqueueAudioCodec(state, {kind:'configure', config});
        state.status = 'configured';
        if (!encode) state.keyRequired = true;
    };
    const flushAudioCodec = (owner, encode) => {
        try {
            const state = configuredAudioCodec(owner, encode);
            const request = {kind:'flush'};
            const promise = new Promise((resolve, reject) => { request.resolve = resolve; request.reject = reject; });
            enqueueAudioCodec(state, request);
            state.flushes.add(request);
            if (!encode) state.keyRequired = true;
            return promise;
        } catch (error) { return Promise.reject(error); }
    };
    const resetAudioCodec = (owner, encode, close) => {
        const state = audioCodecState(owner, encode);
        if (state.status === 'closed') throw frameError('Audio codec is already closed');
        audioCodecAbort(state);
        state.status = close ? 'closed' : 'unconfigured';
        if (close) audioCodecFinalizer.unregister(owner);
    };
    class AudioDecoder extends EventTarget {
        constructor(init) {
            super();
            if (!arguments.length) throw new TypeError('AudioDecoder requires callbacks');
            initializeAudioCodec(this, init, false);
        }
        get state() { return audioCodecState(this, false).status; }
        get decodeQueueSize() { return audioCodecState(this, false).size; }
        get ondequeue() { return audioCodecState(this, false).ondequeue; }
        set ondequeue(value) { setAudioCodecDequeueHandler(this, value, false); }
        configure(config) {
            if (!arguments.length) throw new TypeError('Decoder configuration is required');
            configureAudioCodec(this, config, false);
        }
        decode(chunk) {
            const input = encodedAudioState(chunk);
            const state = configuredAudioCodec(this, false);
            if (state.keyRequired && input.type !== 'key') throw frameError('A key audio chunk is required', 'DataError');
            if (input.bytes.length > 512 * 1024) throw frameError('Audio chunk exceeds its 512 KiB command budget', 'QuotaExceededError');
            enqueueAudioCodec(state, {kind:'input', bytes:new Uint8Array(input.bytes), timestamp:input.timestamp});
            state.keyRequired = false;
        }
        flush() { return flushAudioCodec(this, false); }
        reset() { resetAudioCodec(this, false, false); }
        close() { resetAudioCodec(this, false, true); }
        static isConfigSupported(config) {
            if (!arguments.length) return Promise.reject(new TypeError('Decoder configuration is required'));
            return audioCodecSupport(config, false);
        }
    }
    class AudioEncoder extends EventTarget {
        constructor(init) {
            super();
            if (!arguments.length) throw new TypeError('AudioEncoder requires callbacks');
            initializeAudioCodec(this, init, true);
        }
        get state() { return audioCodecState(this, true).status; }
        get encodeQueueSize() { return audioCodecState(this, true).size; }
        get ondequeue() { return audioCodecState(this, true).ondequeue; }
        set ondequeue(value) { setAudioCodecDequeueHandler(this, value, true); }
        configure(config) {
            if (!arguments.length) throw new TypeError('Encoder configuration is required');
            configureAudioCodec(this, config, true);
        }
        encode(data) {
            const input = audioDataState(data);
            // Detached AudioData is an argument validity failure, not an
            // encoder state failure (WebCodecs encode algorithm, step 1).
            if (input.bytes === null) throw new TypeError('Cannot encode closed AudioData');
            const state = configuredAudioCodec(this, true);
            if (input.frames * input.channels * 4 > 512 * 1024)
                throw frameError('Audio codec input exceeds its 512 KiB PCM command budget', 'QuotaExceededError');
            const bytes = audioInterleavedFloat(input);
            enqueueAudioCodec(state, {kind:'input', bytes, timestamp:input.timestamp,
                rate:input.rate, channels:input.channels});
        }
        flush() { return flushAudioCodec(this, true); }
        reset() { resetAudioCodec(this, true, false); }
        close() { resetAudioCodec(this, true, true); }
        static isConfigSupported(config) {
            if (!arguments.length) return Promise.reject(new TypeError('Encoder configuration is required'));
            return audioCodecSupport(config, true);
        }
    }
    for (const constructor of [AudioDecoder, AudioEncoder]) {
        Object.defineProperty(constructor.prototype, Symbol.toStringTag, {value:constructor.name, configurable:true});
        Object.defineProperty(constructor, 'isConfigSupported', {enumerable:true});
    }
    frameIDL([AudioDecoder, AudioEncoder]);
    if (host('audioCodecSecureContext')) Object.assign(globalThis, {AudioDecoder, AudioEncoder});
