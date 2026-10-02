    class EncodedAudioChunk {
        constructor(init) {
            if (arguments.length === 0) throw new TypeError('EncodedAudioChunk requires an initializer');
            const options = frameDictionary(init);
            const data = frameBuffer(frameRequired(options.data, 'data'));
            const rawDuration = options.duration;
            const duration = rawDuration === undefined ? null : frameLongTime(rawDuration, true);
            const timestamp = frameLongTime(frameRequired(options.timestamp, 'timestamp'));
            const transfer = frameTransfers(options.transfer);
            const type = frameEnum(frameRequired(options.type, 'type'), ['key', 'delta']);
            if (data.byteLength > 8 * 1024 * 1024)
                throw frameError('Encoded audio exceeds its 8 MiB chunk budget', 'NotSupportedError');
            frameValidateTransfers(transfer);
            encodedAudioStates.set(this, {type, timestamp, duration, bytes: new Uint8Array(data)});
            for (const buffer of transfer) host('arrayBufferDetach', buffer);
        }
        get type() { return encodedAudioState(this).type; }
        get timestamp() { return encodedAudioState(this).timestamp; }
        get duration() { return encodedAudioState(this).duration; }
        get byteLength() { return encodedAudioState(this).bytes.byteLength; }
        copyTo(destination) {
            const state = encodedAudioState(this);
            if (arguments.length === 0) throw new TypeError('Chunk copy requires a destination');
            const bytes = frameBuffer(destination);
            if (bytes.byteLength < state.bytes.byteLength) throw new TypeError('Chunk destination is too small');
            bytes.set(state.bytes);
        }
    }
    Object.defineProperty(EncodedAudioChunk.prototype, Symbol.toStringTag,
        {value: 'EncodedAudioChunk', configurable: true});
    frameIDL([EncodedAudioChunk]);
    Object.assign(globalThis, {EncodedAudioChunk});
