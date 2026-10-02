    // WebCodecs sample buffers are immutable snapshots owned by the realm.
    // Format conversion and layout are independent of any codec or container.
    const audioDataStates = new WeakMap();
    const encodedAudioStates = new WeakMap();
    const audioSampleFormats = ['u8', 's16', 's32', 'f32',
        'u8-planar', 's16-planar', 's32-planar', 'f32-planar'];
    const audioFormatInfo = format => ({
        kind: format.split('-')[0], planar: format.endsWith('-planar'),
        size: {u8: 1, s16: 2, s32: 4, f32: 4}[format.split('-')[0]]
    });
    const audioFloat = value => {
        const result = Math.fround(bitmapNumber(value));
        if (!Number.isFinite(result)) throw new TypeError('Audio sample rate must be finite');
        return result;
    };
    const audioDataInit = value => {
        const init = frameDictionary(value);
        // Web IDL dictionary members are read in lexicographic order. Snapshot
        // every getter once before validation or transfer has any side effect.
        const data = frameBuffer(frameRequired(init.data, 'data'));
        const format = frameEnum(frameRequired(init.format, 'format'), audioSampleFormats);
        const channels = frameUint(frameRequired(init.numberOfChannels, 'numberOfChannels'));
        const frames = frameUint(frameRequired(init.numberOfFrames, 'numberOfFrames'));
        const rate = audioFloat(frameRequired(init.sampleRate, 'sampleRate'));
        const timestamp = frameTime(frameRequired(init.timestamp, 'timestamp'));
        const transfer = frameTransfers(init.transfer);
        if (rate <= 0 || !channels || !frames) throw new TypeError('Audio dimensions must be positive');
        const size = frames * channels * audioFormatInfo(format).size;
        if (!Number.isSafeInteger(size) || data.length < size)
            throw new TypeError('Audio source is shorter than its declared sample layout');
        // A resource limit is not a codec capability claim. Reject before any
        // allocation or detachment, including products with huge channel counts.
        if (size > 16 * 1024 * 1024)
            throw frameError('AudioData exceeds its 16 MiB sample budget', 'NotSupportedError');
        if (!Number.isFinite(frames * 1000000 / rate) || frames * 1000000 / rate >= 2 ** 64)
            throw frameError('AudioData duration exceeds unsigned 64-bit metadata', 'NotSupportedError');
        return {data, format, channels, frames, rate, timestamp, transfer, size};
    };
    const audioCopyDictionary = value => {
        const options = frameDictionary(value);
        const format = frameEnum(options.format, audioSampleFormats);
        const rawCount = options.frameCount;
        const count = rawCount === undefined ? undefined : frameUint(rawCount);
        const rawOffset = options.frameOffset;
        const offset = rawOffset === undefined ? 0 : frameUint(rawOffset);
        const plane = frameUint(frameRequired(options.planeIndex, 'planeIndex'));
        return {format, count, offset, plane};
    };
    const audioCopyLayout = (state, options) => {
        const format = options.format ?? state.format;
        const info = audioFormatInfo(format);
        if (options.plane >= (info.planar ? state.channels : 1))
            throw new RangeError('Audio plane index exceeds the destination format');
        if (options.offset >= state.frames)
            throw new RangeError('Audio frame offset exceeds the source');
        const count = options.count ?? state.frames - options.offset;
        if (count > state.frames - options.offset)
            throw new RangeError('Audio copy exceeds the remaining source frames');
        return {...options, format, info, count,
            size: count * (info.planar ? 1 : state.channels) * info.size};
    };
    const audioDataState = value => {
        const state = audioDataStates.get(value);
        if (!state) throw new TypeError('Illegal AudioData receiver');
        return state;
    };
    const activeAudioData = value => {
        const state = audioDataState(value);
        if (state.bytes === null) throw frameError('AudioData is closed');
        return state;
    };
    const encodedAudioState = value => {
        const state = encodedAudioStates.get(value);
        if (!state) throw new TypeError('Illegal EncodedAudioChunk receiver');
        return state;
    };
