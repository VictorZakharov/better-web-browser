    const encodedVideoStates = new WeakMap();
    const encodedVideoState = value => {
        const state = encodedVideoStates.get(value);
        if (!state) throw new TypeError('Illegal encoded video receiver');
        return state;
    };
    class EncodedVideoChunk {
        constructor(init) {
            if (!arguments.length) throw new TypeError('EncodedVideoChunk requires an initializer');
            const options = frameDictionary(init);
            const data = frameBuffer(frameRequired(options.data, 'data'));
            const rawDuration = options.duration;
            const duration = rawDuration === undefined ? null : frameLongTime(rawDuration, true);
            const timestamp = frameLongTime(frameRequired(options.timestamp, 'timestamp'));
            const transfer = frameTransfers(options.transfer);
            const type = frameEnum(frameRequired(options.type, 'type'), ['key','delta']);
            if (data.byteLength > 8 * 1024 * 1024)
                throw frameError('Encoded video exceeds its 8 MiB chunk budget','NotSupportedError');
            frameValidateTransfers(transfer);
            encodedVideoStates.set(this,{type,timestamp,duration,bytes:new Uint8Array(data)});
            for (const buffer of transfer) host('arrayBufferDetach',buffer);
        }
        get type() { return encodedVideoState(this).type; }
        get timestamp() { return encodedVideoState(this).timestamp; }
        get duration() { return encodedVideoState(this).duration; }
        get byteLength() { return encodedVideoState(this).bytes.byteLength; }
        copyTo(destination) {
            const state = encodedVideoState(this);
            if (!arguments.length) throw new TypeError('Chunk destination is required');
            const bytes = frameBuffer(destination);
            if (bytes.byteLength < state.bytes.byteLength) throw new TypeError('Chunk destination is too small');
            bytes.set(state.bytes);
        }
    }
    Object.defineProperty(EncodedVideoChunk.prototype,Symbol.toStringTag,
        {value:'EncodedVideoChunk',configurable:true});
    frameIDL([EncodedVideoChunk]);
    Object.assign(globalThis,{EncodedVideoChunk});
